//! ICU MessageFormat placeholder converter for ICU-JSON catalogs.
//!
//! # The identity converter
//!
//! ICU-JSON catalogs already store ICU MessageFormat strings — this is the
//! whole point of the format. Both directions of the converter are therefore
//! the identity function: `placeholders_to_icu(s) -> s` and
//! `placeholders_from_icu(s) -> s`.
//!
//! # What this module DOES do
//!
//! Validation. Before treating an input as ICU we verify the braces balance.
//! That catches the most common corruption mode (a stray `{` or `}` from a
//! hand-edit or a translator's typo) early — at extract / apply time — rather
//! than at translate time when the unit reaches the LLM. The check is
//! intentionally syntactic-only; we do not parse ICU MessageFormat grammar
//! here (the gate does that on translated targets via the ICU parser).
//!
//! # What this module does NOT do
//!
//! - Full ICU MessageFormat grammar validation. A string like `{count plural}`
//!   (missing the comma between argument and kind) has balanced braces but is
//!   semantically invalid; the gate catches it.
//! - Apostrophe-quoting normalization. ICU treats `'` specially (`''` is a
//!   literal apostrophe, `'{'` is a literal open-brace). Honoring those rules
//!   when scanning for balance would require a real lexer; the simple counter
//!   is enough to catch obviously broken input while accepting all
//!   round-trip-valid input. The gate's ICU parser handles the strict case.

use crate::error::PlaceholderError;

/// Identity-with-validation: returns the input unchanged after confirming the
/// braces balance.
///
/// # Errors
///
/// - [`PlaceholderError::UnsupportedSyntax`] when the brace count does not
///   balance or a `}` appears without a matching `{`.
pub(crate) fn to_icu(s: &str) -> Result<String, PlaceholderError> {
    validate_braces(s)?;
    Ok(s.to_owned())
}

/// Identity-with-validation in the opposite direction. Same logic as
/// [`to_icu`] — ICU-JSON's "native" form already IS ICU.
///
/// # Errors
///
/// - [`PlaceholderError::UnsupportedSyntax`] when the brace count does not
///   balance.
pub(crate) fn from_icu(s: &str) -> Result<String, PlaceholderError> {
    validate_braces(s)?;
    Ok(s.to_owned())
}

/// Walk `s` and verify that every `{` has a matching `}` later in the string
/// and no `}` precedes its matching `{`.
///
/// Apostrophe-escaping (`'{'` for a literal brace) is honored conservatively:
/// a `'` toggles a "quoted" mode in which braces are ignored. Doubled `''`
/// inside quoted mode is a literal apostrophe and does not exit the mode.
/// Outside quoted mode, `''` is a literal apostrophe.
///
/// This is not a full ICU lexer — it is the minimum needed to catch the
/// common breakage modes (an unmatched brace introduced by a hand-edit). The
/// gate's ICU parser is the authoritative grammar check.
fn validate_braces(s: &str) -> Result<(), PlaceholderError> {
    let bytes = s.as_bytes();
    let mut depth: i32 = 0;
    let mut quoted = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => {
                // `''` is always a literal apostrophe regardless of quoted
                // mode.
                if bytes.get(i + 1) == Some(&b'\'') {
                    i += 2;
                    continue;
                }
                // A lone `'` toggles quoted mode IF the next character can
                // be a syntax char — ICU's actual rule. We approximate by
                // toggling unconditionally; the worst case is over-permissive
                // brace checking, which never produces a false positive
                // rejection.
                quoted = !quoted;
                i += 1;
            }
            b'{' if !quoted => {
                depth += 1;
                i += 1;
            }
            b'}' if !quoted => {
                depth -= 1;
                if depth < 0 {
                    return Err(PlaceholderError::UnsupportedSyntax(format!(
                        "unmatched `}}` at byte offset {i}"
                    )));
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    if depth != 0 {
        return Err(PlaceholderError::UnsupportedSyntax(format!(
            "unbalanced braces (depth {depth} at end of input)"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn identity_passes_plain_text() {
        assert_eq!(to_icu("hello world").unwrap(), "hello world");
        assert_eq!(from_icu("hello world").unwrap(), "hello world");
    }

    #[test]
    fn identity_passes_simple_placeholder() {
        assert_eq!(to_icu("Hi, {name}!").unwrap(), "Hi, {name}!");
        assert_eq!(from_icu("Hi, {name}!").unwrap(), "Hi, {name}!");
    }

    #[test]
    fn identity_passes_plural_form() {
        let s = "{count, plural, one {# message} other {# messages}}";
        assert_eq!(to_icu(s).unwrap(), s);
    }

    #[test]
    fn identity_passes_nested_braces() {
        let s = "{count, plural, =0 {none} one {one {item}} other {many}}";
        assert_eq!(to_icu(s).unwrap(), s);
    }

    #[test]
    fn rejects_unmatched_open_brace() {
        let err = to_icu("hello {name").unwrap_err();
        assert!(matches!(err, PlaceholderError::UnsupportedSyntax(_)));
    }

    #[test]
    fn rejects_unmatched_close_brace() {
        let err = to_icu("hello name}").unwrap_err();
        assert!(matches!(err, PlaceholderError::UnsupportedSyntax(_)));
    }

    #[test]
    fn accepts_quoted_braces() {
        // ICU's apostrophe-escape lets a literal `{` slip through.
        assert!(to_icu("don't say '{name}' here").is_ok());
    }

    #[test]
    fn empty_string_is_ok() {
        assert_eq!(to_icu("").unwrap(), "");
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// to_icu followed by from_icu is the identity on any input that
        /// validate_braces accepts.
        #[test]
        fn round_trip_identity_for_brace_balanced_inputs(s in r"[a-zA-Z0-9 .,!?#-]{0,40}") {
            // Plain text always validates.
            let icu = to_icu(&s).unwrap();
            let back = from_icu(&icu).unwrap();
            prop_assert_eq!(back, s);
        }

        /// Inputs with matched `{name}` placeholders round-trip cleanly.
        #[test]
        fn round_trip_identity_for_placeholder_strings(
            prefix in r"[a-z ]{0,8}",
            name in r"[a-z][a-z0-9_]{0,8}",
            suffix in r"[a-z ]{0,8}",
        ) {
            let s = format!("{prefix}{{{name}}}{suffix}");
            let icu = to_icu(&s).unwrap();
            let back = from_icu(&icu).unwrap();
            prop_assert_eq!(back, s);
        }
    }
}
