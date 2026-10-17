//! Qt ↔ ICU placeholder normalization.
//!
//! # Mapping
//!
//! | Qt                          | ICU                | Notes                                         |
//! |-----------------------------|--------------------|-----------------------------------------------|
//! | `%n`                        | `{count}`          | plural count argument                         |
//! | `%1` … `%99`                | `{0}` … `{98}`     | positional; Qt is **1-indexed**, ICU is **0-indexed** |
//! | `%L1` … `%L99`              | `{L1}` … `{L99}`   | locale-aware integer; preserved as a named ICU placeholder so the round-trip is lossless |
//! | `%%`                        | `%`                | escaped literal `%` in Qt source becomes a plain `%` in ICU |
//!
//! # The indexing decision
//!
//! Qt placeholders are documented as 1-indexed (`%1` is the first argument).
//! ICU positional placeholders are 0-indexed (`{0}` is the first). We convert
//! at the boundary: **the ICU intermediate uses 0-indexed positionals.** The
//! reverse is deterministic and lossless for the supported range
//! (1..=99 maps to 0..=98). The gate works in ICU-form and compares
//! placeholder multisets, so it never has to know about Qt's indexing.
//!
//! # The `%L` decision
//!
//! Qt's `%L1` is a locale-aware integer (e.g. thousands separators). ICU
//! MessageFormat has no exact equivalent; the closest is `{0, number}` (with
//! a format type) which is a *different shape* than positional and requires
//! parser support. To keep the converter trivially lossless we encode `%L<N>`
//! as a named ICU placeholder `{L<N>}`. This sacrifices nothing in practice:
//! the backend prompt sees a named placeholder it must preserve verbatim,
//! the gate sees it as a placeholder occurrence in the multiset, and the
//! reverse converter restores `%L<N>` exactly. The collision risk
//! (a `{L7}` in a non-Qt catalog meaning something else) is zero for the
//! Qt-only path this module serves.
//!
//! # Known limitations
//!
//! - Literal apostrophes (`'`) are handled: `to_icu` escapes each as `''`
//!   (the ICU literal-apostrophe form) and `from_icu` collapses `''` back to
//!   `'`. This keeps placeholders adjacent to apostrophes (e.g. the common
//!   `'%1'` quoting in Romance-language targets) from being misread by ICU
//!   consumers as quoted literals.
//! - The source text must not contain unescaped ICU **brace** metacharacters
//!   (`{`, `}`) used as literal characters; ICU treats them as syntax. Qt UI
//!   strings essentially never use these as literals, but extracting from a
//!   pathological source would round-trip incorrectly. A `debug_assert!`
//!   could be added later if this becomes an issue in practice.
//! - `%` followed by any non-digit, non-`L`, non-`%`, non-`n` character is
//!   passed through unchanged. This covers the Qt usage we know about; if a
//!   project uses `%s`/`%d`-style printf placeholders in `.ts` source
//!   strings (uncommon — those should not appear in a Qt-extracted source),
//!   the converter will leave them alone and the gate will flag them later.

/// Convert a Qt-form placeholder string to its ICU-form equivalent.
///
/// See the module docs for the mapping. The function does not validate that
/// placeholder indices are within Qt's 1..=99 range; out-of-range values are
/// passed through unchanged.
pub fn to_icu(qt: &str) -> String {
    let bytes = qt.as_bytes();
    let mut out = String::with_capacity(qt.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\'' {
            // ICU treats `'` as the literal-escape marker: a lone `'` before a
            // syntax char (`{`, `}`, `#`, `|`) opens a quoted span, so a Qt
            // string like `'%1'` would otherwise normalize to `'{0}'`, which
            // every ICU consumer (the gate parser, the ICU-JSON serializer)
            // reads as the *literal text* `{0}` rather than a placeholder.
            // Doubling makes the apostrophe an unambiguous literal `'`, leaving
            // `{0}` a real placeholder. `from_icu` collapses `''` back to `'`.
            out.push_str("''");
            i += 1;
            continue;
        }
        if b != b'%' {
            // Fast path: copy a run of plain text verbatim, stopping at the
            // next byte that needs handling (`%` token, `'` escape).
            let start = i;
            while i < bytes.len() && bytes[i] != b'%' && bytes[i] != b'\'' {
                i += 1;
            }
            // Safety: `qt` is a `&str`, so its bytes are valid UTF-8; we never
            // split a multi-byte sequence because `%` and `'` are single-byte
            // ASCII.
            out.push_str(std::str::from_utf8(&bytes[start..i]).expect("utf-8 invariant"));
            continue;
        }

        // We're at a '%'. Look at the next byte to decide what kind of token.
        let next = bytes.get(i + 1).copied();
        match next {
            Some(b'%') => {
                out.push('%');
                i += 2;
            }
            Some(b'n') => {
                out.push_str("{count}");
                i += 2;
            }
            Some(b'L') => {
                // Locale-aware integer: %L<digits>. Encode as named placeholder.
                let digits_start = i + 2;
                let mut j = digits_start;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                if j > digits_start {
                    out.push_str("{L");
                    out.push_str(
                        std::str::from_utf8(&bytes[digits_start..j]).expect("ascii digits"),
                    );
                    out.push('}');
                    i = j;
                } else {
                    // `%L` not followed by a digit → leave as literal.
                    out.push('%');
                    i += 1;
                }
            }
            Some(c) if c.is_ascii_digit() => {
                // Positional: %<digits>. Subtract 1 for ICU 0-indexing.
                let digits_start = i + 1;
                let mut j = digits_start;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                let digits = std::str::from_utf8(&bytes[digits_start..j]).expect("ascii digits");
                let n: u32 = digits.parse().expect("ascii digits parse");
                // Qt 1-indexed → ICU 0-indexed. If n is 0 (which Qt does not
                // produce in normal usage), pass through as %0 unchanged.
                if n == 0 {
                    out.push('%');
                    i += 1;
                } else {
                    out.push('{');
                    out.push_str(&(n - 1).to_string());
                    out.push('}');
                    i = j;
                }
            }
            _ => {
                // `%` followed by something we don't recognize, or EOF.
                // Pass the `%` through and continue.
                out.push('%');
                i += 1;
            }
        }
    }
    out
}

/// Convert an ICU-form placeholder string back to its Qt-form equivalent.
///
/// This is the inverse of [`to_icu`] for the supported subset; see the
/// module docs.
pub fn from_icu(icu: &str) -> String {
    let bytes = icu.as_bytes();
    let mut out = String::with_capacity(icu.len() + icu.len() / 16);
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'\'' => {
                // Inverse of the `to_icu` escape: a doubled `''` is one literal
                // apostrophe. A lone `'` (from an external ICU source that
                // never went through `to_icu`) is passed through unchanged.
                if bytes.get(i + 1) == Some(&b'\'') {
                    out.push('\'');
                    i += 2;
                } else {
                    out.push('\'');
                    i += 1;
                }
            }
            b'%' => {
                // Literal `%` in ICU output came from an escaped `%%` in Qt source.
                out.push_str("%%");
                i += 1;
            }
            b'{' => {
                // Look ahead for the closing `}`.
                let body_start = i + 1;
                let mut j = body_start;
                while j < bytes.len() && bytes[j] != b'}' {
                    j += 1;
                }
                if j >= bytes.len() {
                    // Unclosed `{` — pass through literally; this is malformed
                    // ICU and the gate will catch it.
                    out.push('{');
                    i += 1;
                    continue;
                }
                let body = std::str::from_utf8(&bytes[body_start..j]).expect("utf-8 invariant");
                let after = j + 1;
                if body == "count" {
                    out.push_str("%n");
                    i = after;
                } else if let Some(rest) = body.strip_prefix('L')
                    && !rest.is_empty()
                    && rest.bytes().all(|b| b.is_ascii_digit())
                {
                    out.push_str("%L");
                    out.push_str(rest);
                    i = after;
                } else if !body.is_empty() && body.bytes().all(|b| b.is_ascii_digit()) {
                    let n: u32 = body.parse().expect("ascii digits parse");
                    out.push('%');
                    out.push_str(&(n + 1).to_string());
                    i = after;
                } else {
                    // Some other named placeholder — emit it verbatim (Qt
                    // would not have produced it via to_icu, but we want
                    // from_icu to be safe on inputs from other sources too).
                    out.push('{');
                    out.push_str(body);
                    out.push('}');
                    i = after;
                }
            }
            _ => {
                let start = i;
                while i < bytes.len() && bytes[i] != b'%' && bytes[i] != b'{' && bytes[i] != b'\'' {
                    i += 1;
                }
                out.push_str(std::str::from_utf8(&bytes[start..i]).expect("utf-8 invariant"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // ── Mapping examples ──────────────────────────────────────────────────

    #[test]
    fn positional_decrements_index() {
        assert_eq!(to_icu("%1"), "{0}");
        assert_eq!(to_icu("%2"), "{1}");
        assert_eq!(to_icu("%99"), "{98}");
        assert_eq!(from_icu("{0}"), "%1");
        assert_eq!(from_icu("{98}"), "%99");
    }

    #[test]
    fn plural_count_maps_to_named_count() {
        assert_eq!(to_icu("%n"), "{count}");
        assert_eq!(from_icu("{count}"), "%n");
    }

    #[test]
    fn locale_aware_int_round_trips_through_named_form() {
        assert_eq!(to_icu("%L1"), "{L1}");
        assert_eq!(to_icu("%L42"), "{L42}");
        assert_eq!(from_icu("{L1}"), "%L1");
        assert_eq!(from_icu("{L42}"), "%L42");
    }

    #[test]
    fn percent_escape() {
        assert_eq!(to_icu("%%"), "%");
        assert_eq!(from_icu("%"), "%%");
        assert_eq!(to_icu("100%%"), "100%");
        assert_eq!(from_icu("100%"), "100%%");
    }

    #[test]
    fn mixed_message() {
        let qt = "User %1 has %n new message(s) (%L2 total)";
        let icu = "User {0} has {count} new message(s) ({L2} total)";
        assert_eq!(to_icu(qt), icu);
        assert_eq!(from_icu(icu), qt);
    }

    #[test]
    fn passthrough_text_only() {
        assert_eq!(to_icu("Hello, world!"), "Hello, world!");
        assert_eq!(from_icu("Hello, world!"), "Hello, world!");
    }

    #[test]
    fn apostrophe_around_placeholder_is_escaped() {
        // The real-world case: a Romance-language target quotes a placeholder
        // with apostrophes. Without escaping, the ICU intermediate `'{0}'`
        // reads as the literal text `{0}`, so the gate reports a (false)
        // hard placeholder-mismatch. Doubling keeps `{0}` a real placeholder.
        assert_eq!(
            to_icu("Producto '%1' seleccionado"),
            "Producto ''{0}'' seleccionado"
        );
        assert_eq!(
            from_icu("Producto ''{0}'' seleccionado"),
            "Producto '%1' seleccionado"
        );
    }

    #[test]
    fn lone_apostrophe_round_trips() {
        // English contractions and Romance elisions are common in UI strings.
        assert_eq!(to_icu("doesn't exist"), "doesn''t exist");
        assert_eq!(from_icu("doesn''t exist"), "doesn't exist");
    }

    #[test]
    fn unknown_percent_is_passed_through() {
        // %s is not a Qt placeholder; we leave it intact.
        assert_eq!(to_icu("foo %s bar"), "foo %s bar");
    }

    #[test]
    fn trailing_percent_is_safe() {
        assert_eq!(to_icu("trailing %"), "trailing %");
    }

    #[test]
    fn percent_l_without_digits_is_literal() {
        assert_eq!(to_icu("%L"), "%L");
        assert_eq!(to_icu("%L foo"), "%L foo");
    }

    // ── Round-trip property ────────────────────────────────────────────────

    // A generator for Qt-shaped strings: mixed runs of safe text and Qt
    // placeholder tokens drawn from the supported set. We exclude the ICU
    // brace metacharacters (`{`, `}`) — Qt sources never use them literally —
    // but `'` is included: it is escaped/unescaped by the converter and must
    // round-trip.
    fn qt_token() -> impl Strategy<Value = String> {
        prop_oneof![
            // Plural count
            Just("%n".to_owned()),
            // Positional 1..=99
            (1u32..=99u32).prop_map(|n| format!("%{n}")),
            // Locale-aware integer 1..=99
            (1u32..=99u32).prop_map(|n| format!("%L{n}")),
            // Escaped percent
            Just("%%".to_owned()),
        ]
    }

    fn safe_text() -> impl Strategy<Value = String> {
        // ASCII letters, digits, space, common punctuation including `'`
        // (escaped/unescaped by the converter) — explicitly no `%`, `{`, `}`.
        "[a-zA-Z0-9 .,!?'-]{0,12}".prop_map(|s| s.to_owned())
    }

    fn qt_string() -> impl Strategy<Value = String> {
        prop::collection::vec(prop_oneof![qt_token(), safe_text()], 0..8)
            .prop_map(|parts| parts.concat())
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        #[test]
        fn from_icu_inverse_of_to_icu(s in qt_string()) {
            let icu = to_icu(&s);
            let back = from_icu(&icu);
            prop_assert_eq!(back, s);
        }

        #[test]
        fn to_icu_then_from_icu_preserves_length_change_only_for_known_tokens(s in qt_string()) {
            // Sanity: every transformation we apply has a defined inverse on
            // our corpus, so round-trip equality holds even when the
            // intermediate length changes.
            let icu = to_icu(&s);
            let back = from_icu(&icu);
            prop_assert_eq!(back.len(), s.len());
        }
    }
}
