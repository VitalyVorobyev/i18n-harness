//! Content sniffer for gettext PO files.

use crate::format::{Confidence, SniffHit, strip_bom, utf8_prefix};

/// Recognize a gettext PO file from its first bytes.
///
/// Requires `msgid`/`msgstr` markers. A header block with a `Language:` field
/// is `High` confidence; a header without one, or bare pairs, is `Medium`.
pub(crate) fn sniff(head: &[u8]) -> Option<SniffHit> {
    let text = utf8_prefix(strip_bom(head));
    if !text.contains("msgid") || !text.contains("msgstr") {
        return None;
    }

    let has_content_type = text.contains("Content-Type:");
    let has_header_block =
        text.contains("msgid \"\"\nmsgstr \"\"") || text.contains("msgid \"\"\r\nmsgstr \"\"");
    if !has_header_block && !has_content_type {
        return Some(SniffHit {
            confidence: Confidence::Medium,
            language: None,
            reason: "msgid/msgstr pairs found without header — likely gettext PO".to_owned(),
        });
    }

    let language = header_language(text);
    let (confidence, reason) = match &language {
        Some(lang) => (
            Confidence::High,
            format!("gettext PO header with Language: {lang}"),
        ),
        None => (
            Confidence::Medium,
            "gettext PO header block found (no Language: header)".to_owned(),
        ),
    };
    Some(SniffHit {
        confidence,
        language,
        reason,
    })
}

/// Value of the header's `Language:` field (`"Language: de_DE\n"` inside the
/// header msgstr), normalized to `ll_CC`. Only plausible locale tags are
/// accepted: 2–3 letters, optionally followed by `_`/`-` and 2–4 letters.
fn header_language(text: &str) -> Option<String> {
    let start = text.find("Language:")? + "Language:".len();
    let rest = text[start..].trim_start_matches([' ', '\t']);
    let end = rest.find("\\n")?;
    let value = rest[..end].trim();
    let (lang, region) = match value.split_once(['_', '-']) {
        Some((l, r)) => (l, Some(r)),
        None => (value, None),
    };
    let lang_ok = (2..=3).contains(&lang.len()) && lang.bytes().all(|b| b.is_ascii_alphabetic());
    let region_ok = region
        .is_none_or(|r| (2..=4).contains(&r.len()) && r.bytes().all(|b| b.is_ascii_alphabetic()));
    (lang_ok && region_ok).then(|| value.replace('-', "_"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_with_language_is_high_confidence() {
        let po = b"msgid \"\"\nmsgstr \"\"\n\"Content-Type: text/plain; charset=UTF-8\\n\"\n\"Language: pt-BR\\n\"\n";
        let hit = sniff(po).expect("po");
        assert_eq!(hit.confidence, Confidence::High);
        assert_eq!(hit.language.as_deref(), Some("pt_BR"));
    }

    #[test]
    fn header_without_language_is_medium() {
        let po = b"msgid \"\"\nmsgstr \"\"\n\"Content-Type: text/plain; charset=UTF-8\\n\"\n";
        let hit = sniff(po).expect("po");
        assert_eq!(hit.confidence, Confidence::Medium);
        assert_eq!(hit.language, None);
    }

    #[test]
    fn bare_pairs_are_medium() {
        let hit = sniff(b"msgid \"Open\"\nmsgstr \"\"\n").expect("po");
        assert_eq!(hit.confidence, Confidence::Medium);
    }

    #[test]
    fn non_po_text_is_rejected() {
        assert!(sniff(b"hello world").is_none());
    }

    #[test]
    fn implausible_language_value_is_ignored() {
        let po = b"msgid \"\"\nmsgstr \"\"\n\"Language: not a locale\\n\"\n";
        assert_eq!(sniff(po).expect("po").language, None);
    }
}
