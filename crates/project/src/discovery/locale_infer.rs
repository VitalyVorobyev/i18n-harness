//! Filename-based locale inference fallback.
//!
//! Adapter-derived locale (from XML attributes, PO headers, JSON keys) is
//! preferred. This module provides the fallback regex that fires when no
//! adapter-derived locale is available. See design §3.3.

use std::sync::OnceLock;

use regex::Regex;

/// The locale-inference regex.
///
/// Matches a locale token embedded in a file stem before the extension.
/// Examples that match: `app_de.ts`, `app-de_DE.ts`, `messages.zh_Hans.json`,
/// `de.po` (locale as the entire stem).
///
/// The alternation `(?:[_\-\.]|^)` lets the locale start either after a
/// separator character or at the very beginning of the filename (covering
/// single-locale filenames like `de.po`).
fn locale_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?:[_\-\.]|^)([a-z]{2,3}(?:[_-][A-Z][a-zA-Z]{1,3})?)\.(?:ts|po|json)$",
        )
        .expect("locale inference regex is valid")
    })
}

/// Attempt to infer a locale id from a filename stem.
///
/// Applies the regex to `filename` (the bare file name, not a full path).
/// Returns the first capture group with hyphens normalized to underscores,
/// or `None` if the regex does not match.
///
/// # Normalization
///
/// `de-DE` → `de_DE`; `zh-Hans` → `zh_Hans`. The workspace convention uses
/// underscores throughout ([`crates/locales`]).
pub(crate) fn infer_from_filename(filename: &str) -> Option<String> {
    locale_regex()
        .captures(filename)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().replace('-', "_"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_de_ts() {
        assert_eq!(infer_from_filename("app_de.ts").as_deref(), Some("de"));
    }

    #[test]
    fn app_de_de_ts() {
        assert_eq!(infer_from_filename("app-de_DE.ts").as_deref(), Some("de_DE"));
    }

    #[test]
    fn messages_zh_hans_json() {
        assert_eq!(
            infer_from_filename("messages.zh_Hans.json").as_deref(),
            Some("zh_Hans")
        );
    }

    #[test]
    fn de_po() {
        assert_eq!(infer_from_filename("de.po").as_deref(), Some("de"));
    }

    #[test]
    fn random_ts_no_match() {
        assert_eq!(infer_from_filename("random.ts"), None);
    }

    #[test]
    fn hyphen_normalized_to_underscore() {
        assert_eq!(
            infer_from_filename("app-de-DE.ts").as_deref(),
            Some("de_DE")
        );
    }

    #[test]
    fn three_letter_language_code() {
        assert_eq!(infer_from_filename("messages_zho.json").as_deref(), Some("zho"));
    }
}
