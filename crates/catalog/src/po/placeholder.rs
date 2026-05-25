//! gettext (printf-style) ↔ ICU MessageFormat placeholder converter.
//!
//! # Mapping (canonical, used by the round-trip-stable path)
//!
//! | gettext                | ICU            | Notes                          |
//! |------------------------|----------------|--------------------------------|
//! | `%%`                   | `%`            | literal percent                |
//! | `%s`, `%d`, `%i`, `%u`, `%f`, `%c` (non-positional) | `{0}`, `{1}`, `{2}`, … assigned in source order | conversion specifier is dropped in the ICU view; the writer recovers it from the catalog's saved table |
//! | `%1$s`, `%2$d`, …      | `{0}`, `{1}`, … | gettext 1-indexed → ICU 0-indexed; conversion specifier dropped |
//! | `%(name)s`, `%(count)d`| `{name}`, `{count}` | Python-style named; conversion specifier dropped |
//!
//! # Inverse mapping (default, used when no per-unit table is available)
//!
//! When the converter has no per-unit memory of the original spelling — i.e.,
//! when [`from_icu`] is called via the trait surface without a context table —
//! it picks a safe default:
//!
//! - Named `{count}` → `%(count)s` (always `s`; we cannot know the gettext
//!   conversion specifier without the source).
//! - Positional `{0}`, `{1}`, … → `%1$s`, `%2$s`, … (positional, always `s`).
//! - Literal `%` → `%%`.
//!
//! That convention is round-trip-stable on its own restricted output domain
//! (positional `%N$s` and named `%(name)s` only), and it is enough for the
//! trait-level property test. The actual byte-stable PO writer uses a richer
//! path: for *unchanged* units the original bytes splice through verbatim,
//! and for *changed* units the writer recovers the original conversion
//! specifier from a per-unit placeholder table captured during extract.
//!
//! # Refused syntax
//!
//! - Width / precision specifiers like `%-10s`, `%.3f`: rejected with
//!   [`PlaceholderError::UnsupportedSyntax`]. The PO writer cannot
//!   round-trip an arbitrary printf format string through ICU.
//! - glibc `%n` (writes count of chars so far): rejected; this is a known
//!   gettext exploit vector and never appears in honest UI strings.
//! - Mixed positional and non-positional: rejected. Mixing creates ambiguous
//!   indexing rules that the inverse cannot resolve.

use crate::error::PlaceholderError;

/// One placeholder occurrence in a gettext string, captured during extract.
///
/// The PO writer keeps a `Vec<PoPlaceholder>` per unit so that when the
/// translator edits the target, the inverse converter knows whether each
/// index originally used `%s`, `%d`, `%f`, etc., and writes back the exact
/// spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PoPlaceholder {
    /// Which shape this placeholder takes in the gettext string.
    pub kind: PoPlaceholderKind,
    /// `s`, `d`, `i`, `u`, `f`, `c` — the printf conversion specifier byte.
    pub specifier: u8,
}

/// The three placeholder shapes the gettext converter recognizes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PoPlaceholderKind {
    /// `%s`, `%d`, … — bare placeholder; index is implicit from order.
    Bare,
    /// `%1$s`, `%2$d`, … — explicit positional, 1-indexed.
    Positional(u32),
    /// `%(name)s`, `%(count)d`, … — Python-style named.
    Named(String),
}

/// Convert a gettext-form placeholder string to its ICU equivalent.
///
/// See module docs for the mapping. Returns the converted string; the
/// gettext-side metadata (which conversion specifier each occurrence used)
/// must be recovered separately via [`parse_placeholders`] if the writer
/// needs to round-trip the exact original spelling.
pub(crate) fn to_icu(native: &str) -> Result<String, PlaceholderError> {
    let mut out = String::with_capacity(native.len());
    let mut next_implicit_index: u32 = 0;
    let mut saw_implicit = false;
    let mut saw_positional = false;

    let bytes = native.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b != b'%' {
            // Fast path: copy a run of non-% chars verbatim.
            let start = i;
            while i < bytes.len() && bytes[i] != b'%' {
                i += 1;
            }
            out.push_str(std::str::from_utf8(&bytes[start..i]).expect("utf-8 invariant"));
            continue;
        }

        let next = bytes.get(i + 1).copied();
        match next {
            None => {
                // Trailing `%`. Pass through as literal.
                out.push('%');
                i += 1;
            }
            Some(b'%') => {
                out.push('%');
                i += 2;
            }
            Some(b'(') => {
                // %(name)<spec>
                let name_start = i + 2;
                let mut j = name_start;
                while j < bytes.len() && bytes[j] != b')' {
                    j += 1;
                }
                if j >= bytes.len() || j == name_start {
                    return Err(PlaceholderError::UnsupportedSyntax(format!(
                        "unterminated or empty `%(...)` at offset {i}"
                    )));
                }
                let name = std::str::from_utf8(&bytes[name_start..j])
                    .map_err(|_| {
                        PlaceholderError::UnsupportedSyntax("non-utf8 name in `%(...)`".to_owned())
                    })?
                    .to_owned();
                if !is_valid_icu_name(&name) {
                    return Err(PlaceholderError::UnsupportedSyntax(format!(
                        "named placeholder `{name}` is not a valid ICU identifier"
                    )));
                }
                let spec_pos = j + 1;
                let specifier = bytes.get(spec_pos).copied().ok_or_else(|| {
                    PlaceholderError::UnsupportedSyntax(format!(
                        "named placeholder `%({name})` has no conversion specifier"
                    ))
                })?;
                if !is_supported_specifier(specifier) {
                    return Err(PlaceholderError::UnsupportedSyntax(format!(
                        "unsupported conversion specifier `{}` in `%({name})`",
                        char::from(specifier)
                    )));
                }
                out.push('{');
                out.push_str(&name);
                out.push('}');
                i = spec_pos + 1;
            }
            Some(c) if c.is_ascii_digit() => {
                // Could be %N$<spec> (positional) or width.
                let digits_start = i + 1;
                let mut j = digits_start;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                let digits =
                    std::str::from_utf8(&bytes[digits_start..j]).expect("ascii digits utf-8");
                if bytes.get(j) == Some(&b'$') {
                    // Positional: %<n>$<spec>
                    let n: u32 = digits.parse().map_err(|_| {
                        PlaceholderError::UnsupportedSyntax(format!(
                            "positional index `{digits}` out of range"
                        ))
                    })?;
                    if n == 0 {
                        return Err(PlaceholderError::UnsupportedSyntax(
                            "positional index `0$` is not valid in gettext".to_owned(),
                        ));
                    }
                    let spec_pos = j + 1;
                    let specifier = bytes.get(spec_pos).copied().ok_or_else(|| {
                        PlaceholderError::UnsupportedSyntax(format!(
                            "positional `%{n}$` has no conversion specifier"
                        ))
                    })?;
                    if !is_supported_specifier(specifier) {
                        return Err(PlaceholderError::UnsupportedSyntax(format!(
                            "unsupported conversion specifier `{}` in `%{n}$`",
                            char::from(specifier)
                        )));
                    }
                    saw_positional = true;
                    if saw_implicit {
                        return Err(PlaceholderError::UnsupportedSyntax(
                            "cannot mix positional `%N$x` and bare `%x` placeholders".to_owned(),
                        ));
                    }
                    out.push('{');
                    out.push_str(&(n - 1).to_string());
                    out.push('}');
                    i = spec_pos + 1;
                } else {
                    // Width specifier (e.g. `%10s`) is not supported.
                    return Err(PlaceholderError::UnsupportedSyntax(format!(
                        "width/precision specifier `%{digits}…` at offset {i}"
                    )));
                }
            }
            Some(c) if is_supported_specifier(c) => {
                // Bare `%<spec>`.
                saw_implicit = true;
                if saw_positional {
                    return Err(PlaceholderError::UnsupportedSyntax(
                        "cannot mix positional `%N$x` and bare `%x` placeholders".to_owned(),
                    ));
                }
                out.push('{');
                out.push_str(&next_implicit_index.to_string());
                out.push('}');
                next_implicit_index += 1;
                i += 2;
            }
            Some(c) => {
                // Width modifier (`-`, `+`, `.`, `*`, space) or unsupported.
                return Err(PlaceholderError::UnsupportedSyntax(format!(
                    "unsupported `%{ch}` at offset {i}",
                    ch = char::from(c)
                )));
            }
        }
    }
    Ok(out)
}

/// Convert an ICU-form placeholder string back to gettext form using the
/// trait-default convention (no per-unit context).
///
/// See module docs for the mapping. Implementations of
/// [`crate::CatalogFormat::apply`] use the richer
/// [`from_icu_with_table`] path when they have per-unit specifier info.
pub(crate) fn from_icu(icu: &str) -> Result<String, PlaceholderError> {
    from_icu_inner(icu, None)
}

/// Convert an ICU-form string to gettext form, using the per-unit
/// placeholder table captured during extract.
///
/// `table` is indexed by placeholder occurrence in the original gettext text.
/// Named placeholders are looked up by name; positional placeholders are
/// looked up by index.
pub(crate) fn from_icu_with_table(
    icu: &str,
    table: &[PoPlaceholder],
) -> Result<String, PlaceholderError> {
    from_icu_inner(icu, Some(table))
}

fn from_icu_inner(icu: &str, table: Option<&[PoPlaceholder]>) -> Result<String, PlaceholderError> {
    let bytes = icu.as_bytes();
    let mut out = String::with_capacity(icu.len() + icu.len() / 16);
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'%' => {
                // Literal `%` in ICU output rewrites back to `%%`.
                out.push_str("%%");
                i += 1;
            }
            b'{' => {
                let body_start = i + 1;
                let mut j = body_start;
                while j < bytes.len() && bytes[j] != b'}' {
                    j += 1;
                }
                if j >= bytes.len() {
                    // Unclosed `{`: malformed ICU; pass through.
                    out.push('{');
                    i += 1;
                    continue;
                }
                let body =
                    std::str::from_utf8(&bytes[body_start..j]).expect("utf-8 in ICU placeholder");
                let after = j + 1;
                if body.is_empty() {
                    return Err(PlaceholderError::UnsupportedSyntax(
                        "empty `{}` placeholder".to_owned(),
                    ));
                }
                if body.bytes().all(|b| b.is_ascii_digit()) {
                    let index: u32 = body.parse().map_err(|_| {
                        PlaceholderError::UnsupportedSyntax(format!(
                            "positional index `{body}` out of range"
                        ))
                    })?;
                    // Lookup policy for positional ICU placeholders:
                    //
                    // - If the table contains an explicit positional entry
                    //   matching `index + 1`, reuse its spelling.
                    // - Else if the table is "all bare" (no positional, no
                    //   named), index into it: the Nth bare slot maps to
                    //   `{N}`.
                    // - Else fall back to the default `%(index+1)$s`.
                    let positional_match = table.and_then(|t| {
                        t.iter().find(|p| {
                            matches!(p.kind, PoPlaceholderKind::Positional(n) if n == index + 1)
                        })
                    });
                    let bare_match = table.and_then(|t| {
                        let all_bare = t.iter().all(|p| matches!(p.kind, PoPlaceholderKind::Bare));
                        if all_bare {
                            t.get(index as usize)
                        } else {
                            None
                        }
                    });
                    let (use_positional, specifier) = match (positional_match, bare_match) {
                        (Some(p), _) => (true, p.specifier),
                        (None, Some(p)) => (false, p.specifier),
                        (None, None) => (true, b's'),
                    };
                    if use_positional {
                        out.push('%');
                        out.push_str(&(index + 1).to_string());
                        out.push('$');
                        out.push(char::from(specifier));
                    } else {
                        out.push('%');
                        out.push(char::from(specifier));
                    }
                    i = after;
                } else if is_valid_icu_name(body) {
                    let entry = table.and_then(|t| {
                        t.iter()
                            .find(|p| matches!(&p.kind, PoPlaceholderKind::Named(n) if n == body))
                    });
                    let specifier = entry.map_or(b's', |p| p.specifier);
                    out.push_str("%(");
                    out.push_str(body);
                    out.push(')');
                    out.push(char::from(specifier));
                    i = after;
                } else if table.is_some() {
                    // ICU expression like `{count, plural, ...}` cannot be
                    // rewritten back into gettext; PO does not represent
                    // ICU plurals.
                    return Err(PlaceholderError::UnknownNamedPlaceholder(body.to_owned()));
                } else {
                    // No context, complex body — pass through verbatim so
                    // the property test on the converter pair can still
                    // round-trip simple shapes; complex ICU is the writer's
                    // responsibility to refuse.
                    out.push('{');
                    out.push_str(body);
                    out.push('}');
                    i = after;
                }
            }
            _ => {
                let start = i;
                while i < bytes.len() && bytes[i] != b'%' && bytes[i] != b'{' {
                    i += 1;
                }
                out.push_str(std::str::from_utf8(&bytes[start..i]).expect("utf-8 invariant"));
            }
        }
    }
    Ok(out)
}

/// Scan a gettext-form string and return the placeholder occurrences in
/// source order. The PO parser invokes this to populate per-unit
/// [`PoPlaceholder`] tables so the writer can round-trip the original
/// conversion specifiers.
///
/// Rejects strings that reuse the same `%(name)` with different conversion
/// specifiers: in gettext such a string is ill-formed (the named-argument
/// dictionary maps each key to a single value), and supporting it would
/// break the round-trip table lookup (there is no way to disambiguate
/// `{k}` between two distinct specifiers).
pub(crate) fn parse_placeholders(native: &str) -> Result<Vec<PoPlaceholder>, PlaceholderError> {
    let bytes = native.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            i += 1;
            continue;
        }
        let next = bytes.get(i + 1).copied();
        match next {
            None => break,
            Some(b'%') => i += 2,
            Some(b'(') => {
                let name_start = i + 2;
                let mut j = name_start;
                while j < bytes.len() && bytes[j] != b')' {
                    j += 1;
                }
                if j >= bytes.len() || j == name_start {
                    return Err(PlaceholderError::UnsupportedSyntax(format!(
                        "unterminated or empty `%(...)` at offset {i}"
                    )));
                }
                let name = std::str::from_utf8(&bytes[name_start..j])
                    .map_err(|_| {
                        PlaceholderError::UnsupportedSyntax("non-utf8 name in `%(...)`".to_owned())
                    })?
                    .to_owned();
                let spec_pos = j + 1;
                let specifier = bytes.get(spec_pos).copied().ok_or_else(|| {
                    PlaceholderError::UnsupportedSyntax(format!(
                        "named placeholder `%({name})` has no conversion specifier"
                    ))
                })?;
                if !is_supported_specifier(specifier) {
                    return Err(PlaceholderError::UnsupportedSyntax(format!(
                        "unsupported conversion specifier `{}` in `%({name})`",
                        char::from(specifier)
                    )));
                }
                out.push(PoPlaceholder {
                    kind: PoPlaceholderKind::Named(name),
                    specifier,
                });
                i = spec_pos + 1;
            }
            Some(c) if c.is_ascii_digit() => {
                let digits_start = i + 1;
                let mut j = digits_start;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                let digits =
                    std::str::from_utf8(&bytes[digits_start..j]).expect("ascii digits utf-8");
                if bytes.get(j) == Some(&b'$') {
                    let n: u32 = digits.parse().map_err(|_| {
                        PlaceholderError::UnsupportedSyntax(format!(
                            "positional index `{digits}` out of range"
                        ))
                    })?;
                    let spec_pos = j + 1;
                    let specifier = bytes.get(spec_pos).copied().ok_or_else(|| {
                        PlaceholderError::UnsupportedSyntax(format!(
                            "positional `%{n}$` has no conversion specifier"
                        ))
                    })?;
                    if !is_supported_specifier(specifier) {
                        return Err(PlaceholderError::UnsupportedSyntax(format!(
                            "unsupported conversion specifier `{}` in `%{n}$`",
                            char::from(specifier)
                        )));
                    }
                    out.push(PoPlaceholder {
                        kind: PoPlaceholderKind::Positional(n),
                        specifier,
                    });
                    i = spec_pos + 1;
                } else {
                    return Err(PlaceholderError::UnsupportedSyntax(format!(
                        "width/precision specifier `%{digits}…` at offset {i}"
                    )));
                }
            }
            Some(c) if is_supported_specifier(c) => {
                out.push(PoPlaceholder {
                    kind: PoPlaceholderKind::Bare,
                    specifier: c,
                });
                i += 2;
            }
            Some(c) => {
                return Err(PlaceholderError::UnsupportedSyntax(format!(
                    "unsupported `%{ch}` at offset {i}",
                    ch = char::from(c)
                )));
            }
        }
    }
    // Named placeholders with the same name must agree on specifier; gettext
    // resolves names through a dictionary that cannot disambiguate
    // `%(k)s` vs `%(k)d`. Positional reuse is fine — same index, same arg.
    for (i, p) in out.iter().enumerate() {
        if let PoPlaceholderKind::Named(name) = &p.kind {
            for q in out.iter().skip(i + 1) {
                if let PoPlaceholderKind::Named(other) = &q.kind
                    && name == other
                    && p.specifier != q.specifier
                {
                    return Err(PlaceholderError::UnsupportedSyntax(format!(
                        "named placeholder `%({name})` reused with different conversion specifiers `{}` and `{}`",
                        char::from(p.specifier),
                        char::from(q.specifier),
                    )));
                }
            }
        }
    }
    Ok(out)
}

fn is_supported_specifier(b: u8) -> bool {
    matches!(b, b's' | b'd' | b'i' | b'u' | b'f' | b'c')
}

fn is_valid_icu_name(name: &str) -> bool {
    // ICU identifier rules: letter or `_`, followed by letters / digits / `_`.
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // ── Mapping examples ──────────────────────────────────────────────────

    #[test]
    fn bare_specifiers_are_numbered_in_order() {
        assert_eq!(to_icu("%s likes %s").unwrap(), "{0} likes {1}");
        assert_eq!(to_icu("%d of %d items").unwrap(), "{0} of {1} items");
    }

    #[test]
    fn positional_decrements_index() {
        assert_eq!(to_icu("%1$s %2$s").unwrap(), "{0} {1}");
        assert_eq!(to_icu("%2$d before %1$s").unwrap(), "{1} before {0}");
    }

    #[test]
    fn named_placeholders_drop_specifier() {
        assert_eq!(to_icu("%(user)s wrote").unwrap(), "{user} wrote");
        assert_eq!(to_icu("count=%(count)d").unwrap(), "count={count}");
    }

    #[test]
    fn percent_escape() {
        assert_eq!(to_icu("100%% done").unwrap(), "100% done");
        assert_eq!(from_icu("100% done").unwrap(), "100%% done");
    }

    #[test]
    fn from_icu_default_uses_positional_with_s() {
        assert_eq!(from_icu("{0} likes {1}").unwrap(), "%1$s likes %2$s");
        assert_eq!(from_icu("{user} wrote").unwrap(), "%(user)s wrote");
    }

    #[test]
    fn rejects_mixed_positional_and_bare() {
        assert!(to_icu("%1$s and %d").is_err());
        assert!(to_icu("%d and %2$s").is_err());
    }

    #[test]
    fn rejects_width_modifier() {
        let err = to_icu("hello %-10s").unwrap_err();
        assert!(matches!(err, PlaceholderError::UnsupportedSyntax(_)));
    }

    #[test]
    fn rejects_unknown_specifier() {
        // %n (chars-so-far) is intentionally not supported.
        let err = to_icu("oops %n").unwrap_err();
        assert!(matches!(err, PlaceholderError::UnsupportedSyntax(_)));
    }

    #[test]
    fn parse_placeholders_captures_specifiers() {
        let table = parse_placeholders("%s saw %d items as %(user)s").unwrap();
        assert_eq!(table.len(), 3);
        assert!(matches!(table[0].kind, PoPlaceholderKind::Bare));
        assert_eq!(table[0].specifier, b's');
        assert_eq!(table[1].specifier, b'd');
        assert!(matches!(&table[2].kind, PoPlaceholderKind::Named(n) if n == "user"));
    }

    #[test]
    fn from_icu_with_table_recovers_original_specifier() {
        let table = parse_placeholders("%s and %d").unwrap();
        // Translator's new message: same placeholders, possibly reordered.
        let icu = "we saw {0} along with {1}";
        let back = from_icu_with_table(icu, &table).unwrap();
        assert_eq!(back, "we saw %s along with %d");
    }

    #[test]
    fn from_icu_with_table_recovers_positional_form() {
        let table = parse_placeholders("%1$s of %2$d").unwrap();
        let icu = "{1} from {0}";
        let back = from_icu_with_table(icu, &table).unwrap();
        assert_eq!(back, "%2$d from %1$s");
    }

    #[test]
    fn from_icu_with_table_recovers_named_specifier() {
        let table = parse_placeholders("%(user)s wrote %(count)d").unwrap();
        let icu = "{user} edited {count} lines";
        let back = from_icu_with_table(icu, &table).unwrap();
        assert_eq!(back, "%(user)s edited %(count)d lines");
    }

    #[test]
    fn parse_rejects_same_name_with_different_specifiers() {
        // gettext's named-argument dictionary cannot map one key to two
        // distinct conversion specifiers; refuse explicitly instead of
        // silently producing a broken table.
        let err = parse_placeholders("%(k)s and %(k)d").unwrap_err();
        assert!(
            matches!(err, PlaceholderError::UnsupportedSyntax(ref m)
                if m.contains("reused with different conversion specifiers")),
            "got: {err:?}",
        );
    }

    #[test]
    fn parse_allows_same_name_with_same_specifier() {
        // Re-using the same named placeholder with the SAME specifier is
        // fine: gettext's dictionary maps the name to a single value, and
        // we can faithfully round-trip both occurrences.
        let table = parse_placeholders("%(user)s saw %(user)s").unwrap();
        assert_eq!(table.len(), 2);
        assert_eq!(table[0].specifier, b's');
        assert_eq!(table[1].specifier, b's');
    }

    // ── Round-trip property ───────────────────────────────────────────────

    fn po_token() -> impl Strategy<Value = String> {
        prop_oneof![
            // Bare specifiers
            Just("%s".to_owned()),
            Just("%d".to_owned()),
            // Escaped percent
            Just("%%".to_owned()),
        ]
    }

    fn po_token_named() -> impl Strategy<Value = String> {
        // Map each name to a single specifier by deriving the specifier from
        // the name's first letter. Avoids the gettext "same name, different
        // specifier" ill-formed case while still exercising both `s` and `d`.
        "[a-z][a-z0-9_]{0,8}".prop_map(|n| {
            let spec = if n.starts_with(|c: char| c <= 'm') {
                's'
            } else {
                'd'
            };
            format!("%({n}){spec}")
        })
    }

    fn safe_text() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 .,!?-]{0,12}".prop_map(|s| s.to_owned())
    }

    fn po_string_bare() -> impl Strategy<Value = String> {
        prop::collection::vec(prop_oneof![po_token(), safe_text()], 0..6)
            .prop_map(|parts| parts.concat())
    }

    fn po_string_named() -> impl Strategy<Value = String> {
        prop::collection::vec(prop_oneof![po_token_named(), safe_text()], 0..6)
            .prop_map(|parts| parts.concat())
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        /// `to_icu` then `from_icu_with_table` round-trips bare gettext strings.
        #[test]
        fn bare_to_icu_then_table_back_is_identity(s in po_string_bare()) {
            let table = parse_placeholders(&s).unwrap();
            let icu = to_icu(&s).unwrap();
            let back = from_icu_with_table(&icu, &table).unwrap();
            prop_assert_eq!(back, s);
        }

        /// `to_icu` then `from_icu_with_table` round-trips named gettext strings.
        #[test]
        fn named_to_icu_then_table_back_is_identity(s in po_string_named()) {
            let table = parse_placeholders(&s).unwrap();
            let icu = to_icu(&s).unwrap();
            let back = from_icu_with_table(&icu, &table).unwrap();
            prop_assert_eq!(back, s);
        }

        /// The trait-default `from_icu` round-trips through `to_icu` for
        /// ICU strings built from the conversion's own output domain
        /// (positional `%N$s` and named `%(name)s`).
        #[test]
        fn from_icu_then_to_icu_round_trips_default_form(s in po_string_named()) {
            // First reduce s to a "canonical" form by running it through
            // (to_icu, from_icu); that is what the writer produces on disk
            // for translator-supplied targets. Round-tripping that should
            // be the identity.
            let icu = to_icu(&s).unwrap();
            let canonical = from_icu(&icu).unwrap();
            let icu_again = to_icu(&canonical).unwrap();
            prop_assert_eq!(icu, icu_again);
        }
    }
}
