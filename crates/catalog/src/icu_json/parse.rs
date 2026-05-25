//! ICU-JSON reader.
//!
//! Hand-written single-pass JSON tokenizer focused on one job: for every leaf
//! string value, capture (a) its dot-joined path and (b) the exact byte range
//! of the value-string in the source (INCLUDING the surrounding quotes). The
//! writer splices new escaped strings into those ranges so untouched bytes
//! pass through verbatim.
//!
//! # What this parses
//!
//! - Top-level JSON object (the file must be an object — react-intl /
//!   i18next / Lingui all use object roots; an array root makes no sense as
//!   a catalog).
//! - Recursively nested objects: each nested key extends the dot-joined path.
//! - Leaf string values: become [`UnitId`] = path, source/target = the
//!   unescaped string.
//!
//! # What this skips
//!
//! - Non-string leaves (numbers, booleans, nulls, arrays). Emits a
//!   `tracing::warn!` and continues. Real-world ICU-JSON catalogs are
//!   string-only; these constructs typically mean the file was misclassified
//!   as a catalog (e.g. a config file slipped through). The sniffer is the
//!   first line of defense; this is the second.
//!
//! # What this rejects
//!
//! - Non-UTF-8 bytes.
//! - Malformed JSON (mismatched braces, trailing commas — strict JSON).
//! - Non-object root (an array, a bare string, etc.).
//! - Empty keys (`{"": "..."}`) — there is no meaningful unit id for them
//!   and they would collide with each other across nesting levels.
//! - Duplicate keys within the same object (an in-place duplicate produces
//!   two units with the same id; both round-trip and write paths assume id
//!   uniqueness — refusing is safer than silently dropping one).
//!
//! # Path construction
//!
//! Path joining uses `.` as separator (`app.menu.file`) — the standard
//! react-intl / format.js convention. Keys that themselves contain `.` are
//! kept verbatim; the resulting unit id matches what the build tooling
//! produces from a JSX `<FormattedMessage id="..." />`. The harness does not
//! enforce a "no `.` in keys" rule because that would refuse half the
//! ICU-JSON files in the wild.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use i18n_harness_core::{Provenance, Target, Unit, UnitId, UnitState, compute_source_hash};
use tracing::warn;

use super::placeholder::to_icu;
use crate::catalog::{Catalog, ExtractState};
use crate::error::CatalogError;

/// Per-unit edit state captured during extract; the writer consults this to
/// produce byte-stable round-trip output.
#[derive(Debug, Clone)]
pub(crate) struct ExtractStateIcuJson {
    /// In-order per-unit edit records, parallel to `Catalog::units`.
    pub(crate) units: Vec<UnitEditState>,
}

/// One leaf string's byte layout in the source file.
#[derive(Debug, Clone)]
pub(crate) struct UnitEditState {
    /// Byte range of the value string in the source bytes, INCLUDING the
    /// opening and closing `"` quotes. On rewrite the writer replaces this
    /// range with `serde_json::to_string(&new_value)` — itself a fully
    /// quoted+escaped JSON string — so quote handling stays consistent.
    pub(crate) value_range: (usize, usize),
    /// Original unescaped value text. The writer compares against this to
    /// decide whether to splice (changed) or skip (unchanged).
    pub(crate) original_value: String,
}

/// Read an ICU-JSON file into a [`Catalog`].
pub(super) fn extract(path: &Path) -> Result<Catalog, CatalogError> {
    let bytes = fs::read(path).map_err(|source| CatalogError::Io {
        op: "read",
        path: path.to_path_buf(),
        source,
    })?;
    let text = std::str::from_utf8(&bytes).map_err(|e| CatalogError::Parse {
        path: path.to_path_buf(),
        reason: format!("file is not valid UTF-8: {e}"),
    })?;

    let mut parser = Parser::new(text);
    let leaves = parser.run().map_err(|reason| CatalogError::Parse {
        path: path.to_path_buf(),
        reason,
    })?;

    let mut units = Vec::with_capacity(leaves.len());
    let mut edit_states = Vec::with_capacity(leaves.len());

    for leaf in leaves {
        // Validate ICU brace balance; rejects obviously broken input early.
        let source_icu = to_icu(&leaf.value).map_err(CatalogError::PlaceholderConversion)?;

        let state = if source_icu.is_empty() {
            UnitState::Untranslated
        } else {
            UnitState::Finished
        };
        let provenance = Provenance {
            file: path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_owned(),
            line: Some(leaf.line),
            byte_offset: Some(leaf.value_range.0 as u32),
        };
        let source_hash = if matches!(state, UnitState::Vanished | UnitState::Obsolete) {
            None
        } else {
            // ICU-JSON has no disambiguation comment and no developer
            // extracomment; pass empty strings.
            Some(compute_source_hash(&source_icu, "", "", false))
        };

        // ICU-JSON is single-locale by convention. The "target" for the file
        // IS the value the file already contains; treat it as Finished. The
        // UI overwrites the target on translate; the writer splices it back
        // when changed.
        let target = if source_icu.is_empty() {
            Target::Singular { text: None }
        } else {
            Target::Singular {
                text: Some(source_icu.clone()),
            }
        };

        let unit = Unit {
            id: UnitId::from(leaf.path),
            source: source_icu,
            target,
            placeholders: Vec::new(),
            plural_arity: None,
            flags: Default::default(),
            provenance,
            state,
            source_hash,
            review_status: None,
            source_changed_since_review: false,
            confidence: None,
            flag_notes: Default::default(),
        };
        edit_states.push(UnitEditState {
            value_range: leaf.value_range,
            original_value: leaf.value,
        });
        units.push(unit);
    }

    Ok(Catalog {
        source_path: path.to_path_buf(),
        language: None,
        source_bytes: bytes,
        units,
        extract_state: ExtractState::IcuJson(ExtractStateIcuJson { units: edit_states }),
    })
}

/// A single leaf string captured by the walker.
#[derive(Debug)]
struct Leaf {
    path: String,
    value: String,
    /// Byte range of the value INCLUDING the surrounding `"` quotes.
    value_range: (usize, usize),
    /// 1-based line number where the value's opening quote sits.
    line: u32,
}

/// Strict JSON tokenizer + path tracker. Walks the input once and emits one
/// [`Leaf`] per leaf string value. Object/array nesting is tracked so we can
/// build dot-joined paths and skip non-string leaves cleanly.
struct Parser<'a> {
    src: &'a str,
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            cursor: 0,
        }
    }

    fn run(&mut self) -> Result<Vec<Leaf>, String> {
        self.skip_ws();
        let mut leaves = Vec::new();
        match self.peek() {
            Some(b'{') => {
                self.parse_object(&mut Vec::new(), &mut leaves)?;
            }
            Some(b) => {
                return Err(format!(
                    "ICU-JSON catalog must be a JSON object at the root; found `{}` at byte {}",
                    char::from(b),
                    self.cursor
                ));
            }
            None => return Err("empty file (expected JSON object)".to_owned()),
        }
        self.skip_ws();
        if self.cursor < self.bytes.len() {
            return Err(format!(
                "unexpected trailing content at byte {} (after root object)",
                self.cursor
            ));
        }
        Ok(leaves)
    }

    fn parse_object(
        &mut self,
        path_stack: &mut Vec<String>,
        out: &mut Vec<Leaf>,
    ) -> Result<(), String> {
        self.expect(b'{')?;
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.cursor += 1;
            return Ok(());
        }
        // Track per-object key uniqueness; nested objects get their own set.
        let mut seen: HashSet<String> = HashSet::new();
        loop {
            self.skip_ws();
            // Parse key (must be a string).
            let key_start = self.cursor;
            if self.peek() != Some(b'"') {
                return Err(format!(
                    "expected string key at byte {}; found `{}`",
                    self.cursor,
                    self.peek_char_for_error()
                ));
            }
            let (key, _key_end) = self.parse_string_literal()?;
            if key.is_empty() {
                return Err(format!(
                    "empty key at byte {key_start}; ICU-JSON catalogs cannot use \"\" as a unit id"
                ));
            }
            if !seen.insert(key.clone()) {
                return Err(format!(
                    "duplicate key `{key}` at byte {key_start}; \
                     ICU-JSON catalogs must have unique keys per object"
                ));
            }
            self.skip_ws();
            self.expect(b':')?;
            self.skip_ws();

            path_stack.push(key);
            self.parse_value(path_stack, out)?;
            path_stack.pop();

            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.cursor += 1;
                    self.skip_ws();
                    // Trailing comma is invalid JSON.
                    if self.peek() == Some(b'}') {
                        return Err(format!("trailing comma in object at byte {}", self.cursor));
                    }
                }
                Some(b'}') => {
                    self.cursor += 1;
                    return Ok(());
                }
                Some(b) => {
                    return Err(format!(
                        "expected `,` or `}}` after object value at byte {}; found `{}`",
                        self.cursor,
                        char::from(b)
                    ));
                }
                None => {
                    return Err("unexpected end of input inside object".to_owned());
                }
            }
        }
    }

    fn parse_value(
        &mut self,
        path_stack: &mut [String],
        out: &mut Vec<Leaf>,
    ) -> Result<(), String> {
        match self.peek() {
            Some(b'{') => {
                let mut stack = path_stack.to_vec();
                self.parse_object(&mut stack, out)
            }
            Some(b'"') => {
                let start = self.cursor;
                let line = self.line_at(start);
                let (value, end) = self.parse_string_literal()?;
                let path = path_stack.join(".");
                out.push(Leaf {
                    path,
                    value,
                    value_range: (start, end),
                    line,
                });
                Ok(())
            }
            Some(b'[') => {
                let path = path_stack.join(".");
                warn!(
                    path = %path,
                    byte = self.cursor,
                    "ICU-JSON: array value skipped (ICU-JSON catalogs are string-only; \
                     consider moving array values to a separate config file)"
                );
                self.skip_value()?;
                Ok(())
            }
            Some(b) if is_value_start(b) => {
                let path = path_stack.join(".");
                warn!(
                    path = %path,
                    byte = self.cursor,
                    "ICU-JSON: non-string scalar value skipped (numbers/booleans/null \
                     are not translatable)"
                );
                self.skip_value()?;
                Ok(())
            }
            Some(b) => Err(format!(
                "unexpected `{}` at byte {} (expected JSON value)",
                char::from(b),
                self.cursor
            )),
            None => Err("unexpected end of input (expected JSON value)".to_owned()),
        }
    }

    /// Walk a value (any JSON value) without recording leaves. Used when we
    /// encounter a non-string leaf we want to skip but still validate as
    /// structurally well-formed JSON (so the writer's splice indices stay
    /// trustworthy).
    fn skip_value(&mut self) -> Result<(), String> {
        match self.peek() {
            Some(b'"') => {
                let _ = self.parse_string_literal()?;
                Ok(())
            }
            Some(b'{') => self.skip_object(),
            Some(b'[') => self.skip_array(),
            Some(b't') => self.expect_literal(b"true").map(|_| ()),
            Some(b'f') => self.expect_literal(b"false").map(|_| ()),
            Some(b'n') => self.expect_literal(b"null").map(|_| ()),
            Some(b) if b == b'-' || b.is_ascii_digit() => self.skip_number(),
            Some(b) => Err(format!(
                "unexpected `{}` at byte {} while skipping value",
                char::from(b),
                self.cursor
            )),
            None => Err("unexpected end of input while skipping value".to_owned()),
        }
    }

    fn skip_object(&mut self) -> Result<(), String> {
        self.expect(b'{')?;
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.cursor += 1;
            return Ok(());
        }
        loop {
            self.skip_ws();
            if self.peek() != Some(b'"') {
                return Err(format!(
                    "expected string key at byte {} while skipping object",
                    self.cursor
                ));
            }
            let _ = self.parse_string_literal()?;
            self.skip_ws();
            self.expect(b':')?;
            self.skip_ws();
            self.skip_value()?;
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.cursor += 1;
                }
                Some(b'}') => {
                    self.cursor += 1;
                    return Ok(());
                }
                _ => {
                    return Err(format!(
                        "expected `,` or `}}` at byte {} while skipping object",
                        self.cursor
                    ));
                }
            }
        }
    }

    fn skip_array(&mut self) -> Result<(), String> {
        self.expect(b'[')?;
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.cursor += 1;
            return Ok(());
        }
        loop {
            self.skip_ws();
            self.skip_value()?;
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.cursor += 1;
                }
                Some(b']') => {
                    self.cursor += 1;
                    return Ok(());
                }
                _ => {
                    return Err(format!(
                        "expected `,` or `]` at byte {} while skipping array",
                        self.cursor
                    ));
                }
            }
        }
    }

    fn skip_number(&mut self) -> Result<(), String> {
        // Accept any contiguous run of number-shaped chars; serde_json's
        // round-trip path is not invoked here (we only need to advance the
        // cursor past the value).
        let start = self.cursor;
        if self.peek() == Some(b'-') {
            self.cursor += 1;
        }
        while let Some(b) = self.peek() {
            if b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'E' | b'+' | b'-') {
                self.cursor += 1;
            } else {
                break;
            }
        }
        if self.cursor == start {
            return Err(format!("expected number at byte {start}; nothing consumed"));
        }
        Ok(())
    }

    /// Parse a `"..."` string literal starting at the current cursor. Returns
    /// `(unescaped_value, end_byte_exclusive)` where `end_byte_exclusive` is
    /// the byte AFTER the closing `"`.
    fn parse_string_literal(&mut self) -> Result<(String, usize), String> {
        debug_assert_eq!(self.bytes[self.cursor], b'"');
        let mut i = self.cursor + 1;
        let mut out = String::new();
        while i < self.bytes.len() {
            match self.bytes[i] {
                b'"' => {
                    self.cursor = i + 1;
                    return Ok((out, i + 1));
                }
                b'\\' => {
                    let next =
                        self.bytes.get(i + 1).copied().ok_or_else(|| {
                            "string ends with unfinished escape sequence".to_owned()
                        })?;
                    match next {
                        b'"' => {
                            out.push('"');
                            i += 2;
                        }
                        b'\\' => {
                            out.push('\\');
                            i += 2;
                        }
                        b'/' => {
                            out.push('/');
                            i += 2;
                        }
                        b'b' => {
                            out.push('\u{0008}');
                            i += 2;
                        }
                        b'f' => {
                            out.push('\u{000C}');
                            i += 2;
                        }
                        b'n' => {
                            out.push('\n');
                            i += 2;
                        }
                        b'r' => {
                            out.push('\r');
                            i += 2;
                        }
                        b't' => {
                            out.push('\t');
                            i += 2;
                        }
                        b'u' => {
                            let hex_end = i + 6;
                            if hex_end > self.bytes.len() {
                                return Err(format!("truncated \\uXXXX escape at byte {i}"));
                            }
                            let hex = std::str::from_utf8(&self.bytes[i + 2..hex_end])
                                .map_err(|_| format!("non-ascii \\uXXXX hex at byte {i}"))?;
                            let cp = u32::from_str_radix(hex, 16)
                                .map_err(|_| format!("invalid \\u escape `{hex}` at byte {i}"))?;
                            // Handle surrogate pairs: a high surrogate must be
                            // followed by a \u-escaped low surrogate.
                            if (0xD800..=0xDBFF).contains(&cp) {
                                if self.bytes.get(hex_end) != Some(&b'\\')
                                    || self.bytes.get(hex_end + 1) != Some(&b'u')
                                {
                                    return Err(format!(
                                        "high surrogate \\u{hex} not followed by low surrogate"
                                    ));
                                }
                                let lo_end = hex_end + 6;
                                if lo_end > self.bytes.len() {
                                    return Err(format!(
                                        "truncated low surrogate at byte {hex_end}"
                                    ));
                                }
                                let lo_hex = std::str::from_utf8(&self.bytes[hex_end + 2..lo_end])
                                    .map_err(|_| {
                                        format!("non-ascii low surrogate hex at byte {hex_end}")
                                    })?;
                                let lo = u32::from_str_radix(lo_hex, 16)
                                    .map_err(|_| format!("invalid low surrogate `{lo_hex}`"))?;
                                if !(0xDC00..=0xDFFF).contains(&lo) {
                                    return Err(format!("invalid low surrogate value U+{lo:04X}"));
                                }
                                let code = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                                let ch = char::from_u32(code).ok_or_else(|| {
                                    format!("invalid surrogate pair codepoint U+{code:X}")
                                })?;
                                out.push(ch);
                                i = lo_end;
                            } else {
                                let ch = char::from_u32(cp)
                                    .ok_or_else(|| format!("invalid \\u escape U+{cp:04X}"))?;
                                out.push(ch);
                                i = hex_end;
                            }
                        }
                        other => {
                            return Err(format!(
                                "unsupported escape `\\{ch}` at byte {i}",
                                ch = char::from(other)
                            ));
                        }
                    }
                }
                b'\n' => {
                    return Err(format!("unterminated string (newline at byte {i})"));
                }
                b if b < 0x20 => {
                    return Err(format!(
                        "control char 0x{b:02X} in string at byte {i} (must be escaped)"
                    ));
                }
                _ => {
                    // Fast path: copy a contiguous run of plain chars.
                    let run_start = i;
                    while i < self.bytes.len()
                        && self.bytes[i] != b'"'
                        && self.bytes[i] != b'\\'
                        && self.bytes[i] >= 0x20
                    {
                        i += 1;
                    }
                    out.push_str(
                        std::str::from_utf8(&self.bytes[run_start..i]).expect("utf-8 invariant"),
                    );
                }
            }
        }
        Err("unterminated string (eof before closing quote)".to_owned())
    }

    fn skip_ws(&mut self) {
        while let Some(b) = self.peek() {
            if matches!(b, b' ' | b'\t' | b'\r' | b'\n') {
                self.cursor += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.cursor).copied()
    }

    fn peek_char_for_error(&self) -> char {
        self.bytes
            .get(self.cursor)
            .map_or('\u{FFFD}', |b| char::from(*b))
    }

    fn expect(&mut self, b: u8) -> Result<(), String> {
        match self.peek() {
            Some(c) if c == b => {
                self.cursor += 1;
                Ok(())
            }
            Some(c) => Err(format!(
                "expected `{}` at byte {}; found `{}`",
                char::from(b),
                self.cursor,
                char::from(c)
            )),
            None => Err(format!(
                "expected `{}` at byte {}; found end of input",
                char::from(b),
                self.cursor
            )),
        }
    }

    fn expect_literal(&mut self, lit: &[u8]) -> Result<(), String> {
        if self.cursor + lit.len() > self.bytes.len()
            || &self.bytes[self.cursor..self.cursor + lit.len()] != lit
        {
            return Err(format!(
                "expected `{}` literal at byte {}",
                std::str::from_utf8(lit).unwrap_or("?"),
                self.cursor,
            ));
        }
        self.cursor += lit.len();
        Ok(())
    }

    fn line_at(&self, byte_offset: usize) -> u32 {
        // 1-based line number. Walk from the start; ICU-JSON files are
        // small (typically <100 KiB), so the O(n) cost is fine.
        let upto = byte_offset.min(self.bytes.len());
        let n = self.src[..upto].bytes().filter(|&b| b == b'\n').count();
        u32::try_from(n + 1).unwrap_or(u32::MAX)
    }
}

fn is_value_start(b: u8) -> bool {
    matches!(b, b'"' | b'{' | b'[' | b't' | b'f' | b'n' | b'-') || b.is_ascii_digit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flat_string_map() {
        let src = r#"{"greeting":"Hello","farewell":"Goodbye"}"#;
        let mut p = Parser::new(src);
        let leaves = p.run().expect("parse");
        assert_eq!(leaves.len(), 2);
        assert_eq!(leaves[0].path, "greeting");
        assert_eq!(leaves[0].value, "Hello");
        assert_eq!(leaves[1].path, "farewell");
        assert_eq!(leaves[1].value, "Goodbye");
    }

    #[test]
    fn parses_nested_object() {
        let src = r#"{"app":{"menu":{"file":"File"}}}"#;
        let mut p = Parser::new(src);
        let leaves = p.run().expect("parse");
        assert_eq!(leaves.len(), 1);
        assert_eq!(leaves[0].path, "app.menu.file");
        assert_eq!(leaves[0].value, "File");
    }

    #[test]
    fn captures_byte_range_includes_quotes() {
        let src = r#"{"k":"v"}"#;
        let mut p = Parser::new(src);
        let leaves = p.run().expect("parse");
        let (start, end) = leaves[0].value_range;
        assert_eq!(&src[start..end], r#""v""#);
    }

    #[test]
    fn handles_escape_sequences() {
        let src = r#"{"k":"line1\nline2 \"quoted\" é"}"#;
        let mut p = Parser::new(src);
        let leaves = p.run().expect("parse");
        assert_eq!(leaves[0].value, "line1\nline2 \"quoted\" é");
    }

    #[test]
    fn handles_surrogate_pair() {
        // U+1F600 = grinning face emoji = 😀
        let src = r#"{"k":"smile 😀"}"#;
        let mut p = Parser::new(src);
        let leaves = p.run().expect("parse");
        assert_eq!(leaves[0].value, "smile 😀");
    }

    #[test]
    fn empty_object_produces_no_units() {
        let src = "{}";
        let mut p = Parser::new(src);
        let leaves = p.run().expect("parse");
        assert!(leaves.is_empty());
    }

    #[test]
    fn skips_non_string_leaves() {
        let src = r#"{"a":"keep","b":42,"c":true,"d":null,"e":[1,2,3],"f":"also-keep"}"#;
        let mut p = Parser::new(src);
        let leaves = p.run().expect("parse");
        assert_eq!(leaves.len(), 2);
        assert_eq!(leaves[0].path, "a");
        assert_eq!(leaves[1].path, "f");
    }

    #[test]
    fn rejects_array_root() {
        let src = "[1,2,3]";
        let mut p = Parser::new(src);
        let err = p.run().unwrap_err();
        assert!(err.contains("object at the root"));
    }

    #[test]
    fn rejects_duplicate_keys() {
        let src = r#"{"k":"a","k":"b"}"#;
        let mut p = Parser::new(src);
        let err = p.run().unwrap_err();
        assert!(err.contains("duplicate key"));
    }

    #[test]
    fn rejects_empty_key() {
        let src = r#"{"":"x"}"#;
        let mut p = Parser::new(src);
        let err = p.run().unwrap_err();
        assert!(err.contains("empty key"));
    }

    #[test]
    fn rejects_trailing_comma() {
        let src = r#"{"k":"v",}"#;
        let mut p = Parser::new(src);
        assert!(p.run().is_err());
    }

    #[test]
    fn line_numbers_are_one_based() {
        let src = "{\n  \"first\": \"a\",\n  \"second\": \"b\"\n}";
        let mut p = Parser::new(src);
        let leaves = p.run().expect("parse");
        assert_eq!(leaves[0].line, 2);
        assert_eq!(leaves[1].line, 3);
    }
}
