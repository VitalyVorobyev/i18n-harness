//! Content sniffer for Qt Linguist `.ts` files.
//!
//! `.ts` is also the TypeScript extension, so the sniffer has to tell a
//! Linguist XML file from TypeScript source by content alone.

use i18n_harness_catalog::{Confidence, SniffHit, strip_bom, utf8_prefix};

/// How far into the file the `<TS` root element must appear.
const ROOT_WINDOW: usize = 1024;

/// Recognize a Qt Linguist file from its first bytes.
///
/// `<TS` in the first KiB without TypeScript keywords is `High` confidence;
/// `<TS` alongside such keywords is `Medium` (the XML element is the stronger
/// signal). TypeScript source, or text without `<TS`, is rejected.
pub(crate) fn sniff(head: &[u8]) -> Option<SniffHit> {
    let text = utf8_prefix(strip_bom(head));
    let window = &text[..floor_char_boundary(text, ROOT_WINDOW)];

    if !window.contains("<TS") {
        return None;
    }
    if has_js_keywords(window) {
        return Some(SniffHit {
            confidence: Confidence::Medium,
            language: None,
            reason: "<TS element seen alongside JS keywords — treating as Qt Linguist XML"
                .to_owned(),
        });
    }

    let language = root_language(text);
    let reason = match &language {
        Some(lang) => format!("<TS language=\"{lang}\"> seen — Qt Linguist XML"),
        None => "<TS root element seen — Qt Linguist XML (no language attribute)".to_owned(),
    };
    Some(SniffHit {
        confidence: Confidence::High,
        language,
        reason,
    })
}

/// Whether `text` contains a TypeScript/JavaScript keyword as a whole word.
pub(crate) fn has_js_keywords(text: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "import",
        "export",
        "const",
        "let",
        "function",
        "class",
        "interface",
        "type",
    ];
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))
        .any(|word| KEYWORDS.contains(&word))
}

/// The `language` attribute of the `<TS ...>` root element, as `ll_CC`.
fn root_language(text: &str) -> Option<String> {
    let start = text.find("<TS")?;
    let tag_end = start + text[start..].find('>')?;
    let tag = &text[start..tag_end];
    let attr = tag
        .match_indices("language=\"")
        .find(|(i, _)| tag[..*i].ends_with(char::is_whitespace))?
        .0
        + "language=\"".len();
    let value_end = attr + tag[attr..].find('"')?;
    let value = &tag[attr..value_end];
    (!value.is_empty()).then(|| value.replace('-', "_"))
}

/// Largest char boundary `<= index` (stable replacement for
/// `str::floor_char_boundary`).
fn floor_char_boundary(text: &str, index: usize) -> usize {
    if index >= text.len() {
        return text.len();
    }
    (0..=index)
        .rev()
        .find(|&i| text.is_char_boundary(i))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINGUIST: &[u8] = br#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="sk_SK">
<context><name>Main</name></context>
</TS>
"#;

    #[test]
    fn linguist_file_is_high_confidence_with_language() {
        let hit = sniff(LINGUIST).expect("qt");
        assert_eq!(hit.confidence, Confidence::High);
        assert_eq!(hit.language.as_deref(), Some("sk_SK"));
    }

    #[test]
    fn dashed_language_is_normalized() {
        let hit = sniff(br#"<TS version="2.1" language="pt-BR"></TS>"#).expect("qt");
        assert_eq!(hit.language.as_deref(), Some("pt_BR"));
    }

    #[test]
    fn missing_language_attribute_is_still_qt() {
        let hit = sniff(br#"<TS version="2.1"></TS>"#).expect("qt");
        assert_eq!(hit.language, None);
    }

    #[test]
    fn sourcelanguage_is_not_mistaken_for_language() {
        let hit = sniff(br#"<TS version="2.1" sourcelanguage="en"></TS>"#).expect("qt");
        assert_eq!(hit.language, None);
    }

    #[test]
    fn typescript_source_is_rejected() {
        assert!(sniff(b"import { x } from './y';\nexport const z = 1;\n").is_none());
    }

    #[test]
    fn bom_is_ignored() {
        let mut bytes = b"\xef\xbb\xbf".to_vec();
        bytes.extend_from_slice(LINGUIST);
        assert!(sniff(&bytes).is_some());
    }
}
