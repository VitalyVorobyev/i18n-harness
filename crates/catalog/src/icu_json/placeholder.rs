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
/// Apostrophe handling follows ICU MessageFormat's actual rule (matching
/// the spec the JDK / ICU4J / format.js implementations all agree on):
///
/// - `''` is always a literal apostrophe.
/// - Outside quoted mode, a lone `'` starts quoting ONLY when it precedes
///   an ICU syntax character (`{`, `}`, `#`, `|`). A `'` followed by any
///   other character (or by end of input) is a literal apostrophe — so
///   `don't worry` does not enter quoted mode, but `don't {name}` would
///   not enter it either (the `'` precedes the letter `t`, not a syntax
///   char).
/// - Inside quoted mode, a lone `'` ends quoting (and `''` is still a
///   literal apostrophe).
///
/// Getting this rule right matters: a naive "every `'` toggles quoting"
/// approximation lets strings like `don't {name` slip through brace
/// validation — the apostrophe enters quoted mode, the unmatched `{` is
/// ignored, and malformed ICU reaches disk. Codex P1 on PR #39.
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
                if quoted {
                    // Inside quoted mode, a lone `'` ends the quote.
                    quoted = false;
                    i += 1;
                } else if let Some(&next) = bytes.get(i + 1)
                    && is_icu_syntax(next)
                {
                    // Starts quoting only when followed by a syntax char.
                    quoted = true;
                    i += 1;
                } else {
                    // Literal apostrophe.
                    i += 1;
                }
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

/// The ICU MessageFormat syntax characters that trigger apostrophe quoting.
/// Matches the set documented in the ICU4J `MessagePattern` source and the
/// format.js / react-intl tokenizer.
fn is_icu_syntax(b: u8) -> bool {
    matches!(b, b'{' | b'}' | b'#' | b'|')
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
    fn rejects_unmatched_brace_after_literal_apostrophe() {
        // Codex P1 on PR #39: a naive "every `'` toggles quoting"
        // approximation lets `don't {name` slip through — the apostrophe
        // in `don't` would enter quoted mode and the `{` would be
        // ignored. The correct ICU rule only starts quoting when `'`
        // precedes a syntax char (`{`, `}`, `#`, `|`).
        let err = to_icu("don't {name").unwrap_err();
        assert!(matches!(err, PlaceholderError::UnsupportedSyntax(_)));
    }

    #[test]
    fn doubled_apostrophe_is_literal_and_does_not_quote() {
        // `''` is always a literal apostrophe — must not affect brace
        // counting.
        assert!(to_icu("it's ''wonderful''").is_ok());
        assert!(to_icu("can't '' open '{x}'").is_ok());
    }

    #[test]
    fn lone_apostrophe_before_letter_is_literal() {
        // ICU rule: `'` before a non-syntax char is literal, NOT a quote
        // start.
        assert!(to_icu("it's a test").is_ok());
        assert!(to_icu("rock 'n' roll").is_ok());
        // And it must still catch a real unmatched brace later.
        let err = to_icu("rock 'n' roll }unmatched").unwrap_err();
        assert!(matches!(err, PlaceholderError::UnsupportedSyntax(_)));
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
