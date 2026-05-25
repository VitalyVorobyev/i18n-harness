//! GNU gettext `.po` reader.
//!
//! Hand-written line-oriented parser. Captures enough byte-level state on
//! every `msgstr` (or `msgstr[N]`) block that the writer can splice in new
//! target text without disturbing the surrounding bytes — comments, blank
//! lines, header, escape choices in untouched units all survive verbatim.
//!
//! # What this parses
//!
//! - The header entry (empty `msgid ""` plus key-value `msgstr`). We extract
//!   `Language:` and `Plural-Forms:` (only the `nplurals=N` number).
//! - Singular entries: optional `msgctxt`, required `msgid`, required `msgstr`.
//! - Plural entries: optional `msgctxt`, required `msgid` + `msgid_plural`,
//!   required `msgstr[N]` blocks for `N = 0..nplurals`.
//! - Comments: `# translator`, `#. extracted`, `#: source ref`, `#, flag`,
//!   `#| previous`. Source refs feed `Provenance.file`/`line`; the rest pass
//!   through via the byte-stable splice path.
//! - Multi-line string continuations: `"line one"\n"line two"`.
//!
//! # What this rejects
//!
//! - Non-UTF-8 bytes (PO files are conventionally UTF-8; the header's
//!   `Content-Type` field is consulted only informationally).
//! - Multi-byte placeholders the converter cannot represent (passed up via
//!   [`crate::PlaceholderError`]).
//!
//! # Encoding handling
//!
//! UTF-8 only. The header may declare a `charset=...` but we treat the
//! payload as UTF-8 regardless; the gettext community has converged on
//! UTF-8 for ~20 years and the harness's invariant ("ICU-normalized source")
//! requires a Unicode pipeline anyway.

use std::fs;
use std::path::Path;

use i18n_harness_core::{
    Placeholder, PlaceholderKind, Provenance, Target, Unit, UnitId, UnitState, compute_source_hash,
};

use super::placeholder::{PoPlaceholder, parse_placeholders, to_icu};
use crate::catalog::{Catalog, ExtractState};
use crate::error::CatalogError;

/// Per-unit edit state captured during extract; the writer consults this to
/// produce byte-stable round-trip output.
#[derive(Debug, Clone)]
pub(crate) struct ExtractStatePo {
    /// In-order per-unit edit records, parallel to `Catalog::units`.
    pub(crate) units: Vec<UnitEditState>,
    /// `nplurals` parsed from the header's `Plural-Forms:` declaration, if any.
    pub(crate) header_nplurals: Option<u32>,
}

#[derive(Debug, Clone)]
pub(crate) struct UnitEditState {
    /// Byte ranges of the `msgstr` / `msgstr\[N\]` content (the bytes
    /// between the `"..."` quotes, including continuation-line quotes).
    /// One entry for singular; `nplurals` entries for plural in
    /// `\[0\], \[1\], ...` order.
    pub(crate) msgstr_blocks: Vec<MsgstrBlock>,
    /// Original (already-unescaped, ICU-normalized) target text for each
    /// block. Used by the writer to detect "unchanged" units and skip the
    /// splice entirely.
    pub(crate) original_targets: Vec<Option<String>>,
    /// Placeholder table captured from the source (msgid). Used by the
    /// writer's `from_icu_with_table` path to preserve conversion specifiers.
    pub(crate) source_placeholders: Vec<PoPlaceholder>,
}

/// A single `msgstr` (or `msgstr[N]`) block's byte layout.
#[derive(Debug, Clone)]
pub(crate) struct MsgstrBlock {
    /// Byte range of the entire block in the source file, starting at the
    /// first character of `msgstr` and ending at the newline after the last
    /// continuation line (exclusive of the newline if there is none).
    ///
    /// On rewrite we replace this range with a freshly-formatted block, so
    /// any continuation-line layout the user had is preserved only when we
    /// skip the splice (unchanged units).
    pub(crate) range: (usize, usize),
    /// The full prefix (`msgstr ` or `msgstr[N] `). Reused verbatim on
    /// rewrite to preserve the user's exact spacing.
    pub(crate) prefix: String,
}

/// Read a `.po` file into a [`Catalog`].
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
    parser.run().map_err(|reason| CatalogError::Parse {
        path: path.to_path_buf(),
        reason,
    })?;

    let mut units = Vec::with_capacity(parser.entries.len());
    let mut edit_states = Vec::with_capacity(parser.entries.len());
    let mut language: Option<String> = None;
    let mut header_nplurals: Option<u32> = None;

    for entry in parser.entries {
        if entry.is_header {
            // Parse Language and Plural-Forms from the header msgstr body.
            let header_text = entry
                .msgstr_blocks
                .first()
                .map(|b| b.unescaped.as_str())
                .unwrap_or("");
            for line in header_text.split('\n') {
                if let Some(rest) = line.strip_prefix("Language:") {
                    let v = rest.trim();
                    if !v.is_empty() {
                        language = Some(v.to_owned());
                    }
                } else if let Some(rest) = line.strip_prefix("Plural-Forms:") {
                    header_nplurals = parse_nplurals(rest);
                }
            }
            continue;
        }

        let id = build_unit_id(entry.msgctxt.as_deref(), &entry.msgid);
        let source_icu = to_icu(&entry.msgid).map_err(CatalogError::PlaceholderConversion)?;
        let source_placeholders =
            parse_placeholders(&entry.msgid).map_err(CatalogError::PlaceholderConversion)?;
        let placeholders = synthesize_placeholders(&source_placeholders);

        let plural_arity = entry.msgid_plural.as_ref().map(|_| {
            // Source arity for plurals is fixed to 2 in gettext (msgid +
            // msgid_plural); the target locale's arity may differ.
            2
        });

        let (target, original_targets) = if let Some(_plural_source) = &entry.msgid_plural {
            let mut forms: Vec<Option<String>> = Vec::with_capacity(entry.msgstr_blocks.len());
            let mut originals: Vec<Option<String>> = Vec::with_capacity(entry.msgstr_blocks.len());
            for block in &entry.msgstr_blocks {
                let icu = if block.unescaped.is_empty() {
                    None
                } else {
                    Some(to_icu(&block.unescaped).map_err(CatalogError::PlaceholderConversion)?)
                };
                originals.push(icu.clone());
                forms.push(icu);
            }
            (Target::Plural { forms }, originals)
        } else {
            let block = entry
                .msgstr_blocks
                .first()
                .expect("non-plural entry must have one msgstr block");
            let icu = if block.unescaped.is_empty() {
                None
            } else {
                Some(to_icu(&block.unescaped).map_err(CatalogError::PlaceholderConversion)?)
            };
            (Target::Singular { text: icu.clone() }, vec![icu])
        };

        let state = compute_state(&entry, &target);
        let provenance = entry
            .source_refs
            .first()
            .map(|sref| Provenance {
                file: sref.file.clone(),
                line: sref.line,
                byte_offset: None,
            })
            .unwrap_or_default();

        let plural_for_hash = plural_arity.is_some();
        let extracomment = entry.extracted_comments.join("\n");
        let disambiguation = entry.msgctxt.clone().unwrap_or_default();
        let source_hash = if matches!(state, UnitState::Vanished | UnitState::Obsolete) {
            None
        } else {
            Some(compute_source_hash(
                &source_icu,
                &disambiguation,
                &extracomment,
                plural_for_hash,
            ))
        };

        let unit = Unit {
            id,
            source: source_icu,
            target,
            placeholders,
            plural_arity,
            flags: Default::default(),
            provenance,
            state,
            source_hash,
            review_status: None,
            source_changed_since_review: false,
            confidence: None,
            flag_notes: Default::default(),
        };
        let msgstr_blocks: Vec<MsgstrBlock> = entry
            .msgstr_blocks
            .iter()
            .map(|b| MsgstrBlock {
                range: b.range,
                prefix: b.prefix.clone(),
            })
            .collect();
        edit_states.push(UnitEditState {
            msgstr_blocks,
            original_targets,
            source_placeholders,
        });
        units.push(unit);
    }

    Ok(Catalog {
        source_path: path.to_path_buf(),
        language,
        source_bytes: bytes,
        units,
        extract_state: ExtractState::Po(ExtractStatePo {
            units: edit_states,
            header_nplurals,
        }),
    })
}

fn synthesize_placeholders(table: &[PoPlaceholder]) -> Vec<Placeholder> {
    let mut implicit_index: u32 = 0;
    table
        .iter()
        .map(|p| match &p.kind {
            super::placeholder::PoPlaceholderKind::Bare => {
                let i = implicit_index;
                implicit_index += 1;
                Placeholder::positional(i, 0)
            }
            super::placeholder::PoPlaceholderKind::Positional(n) => {
                Placeholder::positional(n.saturating_sub(1), 0)
            }
            super::placeholder::PoPlaceholderKind::Named(name) => {
                if name == "count" {
                    let mut ph = Placeholder::plural_count(0);
                    ph.kind = PlaceholderKind::Named;
                    ph
                } else {
                    Placeholder::named(name.clone(), 0)
                }
            }
        })
        .collect()
}

fn build_unit_id(msgctxt: Option<&str>, msgid: &str) -> UnitId {
    match msgctxt {
        Some(ctx) if !ctx.is_empty() => UnitId::from(format!("{ctx}\u{0004}{msgid}")),
        _ => UnitId::from(msgid.to_owned()),
    }
}

fn compute_state(entry: &Entry, target: &Target) -> UnitState {
    if entry.is_obsolete {
        return UnitState::Obsolete;
    }
    if entry.is_fuzzy {
        return UnitState::Proposed;
    }
    if target.is_empty() {
        UnitState::Untranslated
    } else if target.is_complete() {
        UnitState::Finished
    } else {
        UnitState::Proposed
    }
}

fn parse_nplurals(value: &str) -> Option<u32> {
    // Plural-Forms: nplurals=2; plural=(n != 1);
    for part in value.split(';') {
        let part = part.trim();
        if let Some(n_str) = part.strip_prefix("nplurals=") {
            return n_str.trim().parse().ok();
        }
    }
    None
}

#[derive(Debug, Default)]
struct Entry {
    is_header: bool,
    is_fuzzy: bool,
    is_obsolete: bool,
    msgctxt: Option<String>,
    msgid: String,
    msgid_plural: Option<String>,
    msgstr_blocks: Vec<RawMsgstrBlock>,
    extracted_comments: Vec<String>,
    source_refs: Vec<SourceRef>,
}

#[derive(Debug)]
struct RawMsgstrBlock {
    range: (usize, usize),
    prefix: String,
    unescaped: String,
}

#[derive(Debug)]
struct SourceRef {
    file: String,
    line: Option<u32>,
}

struct Parser<'a> {
    src: &'a str,
    cursor: usize,
    entries: Vec<Entry>,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            cursor: 0,
            entries: Vec::new(),
        }
    }

    fn run(&mut self) -> Result<(), String> {
        let mut header_seen = false;
        while self.cursor < self.src.len() {
            if self.peek_blank_line() {
                self.consume_line();
                continue;
            }
            let entry = self.parse_entry()?;
            if !header_seen && entry.msgid.is_empty() {
                let mut hdr = entry;
                hdr.is_header = true;
                self.entries.push(hdr);
                header_seen = true;
            } else {
                self.entries.push(entry);
            }
        }
        Ok(())
    }

    fn peek_blank_line(&self) -> bool {
        let mut i = self.cursor;
        let bytes = self.src.as_bytes();
        while i < bytes.len() {
            match bytes[i] {
                b'\n' => return true,
                b' ' | b'\t' | b'\r' => i += 1,
                _ => return false,
            }
        }
        false
    }

    fn consume_line(&mut self) {
        let bytes = self.src.as_bytes();
        while self.cursor < bytes.len() && bytes[self.cursor] != b'\n' {
            self.cursor += 1;
        }
        if self.cursor < bytes.len() {
            self.cursor += 1;
        }
    }

    fn parse_entry(&mut self) -> Result<Entry, String> {
        let mut entry = Entry::default();
        loop {
            if self.cursor >= self.src.len() {
                break;
            }
            let line = self.peek_line();
            let trimmed = line.trim_start();

            if trimmed.starts_with('#') {
                self.parse_comment_line(line, &mut entry)?;
                self.consume_line();
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("msgctxt") {
                let (value, _) = self.parse_string_block("msgctxt", rest)?;
                entry.msgctxt = Some(value);
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("msgid_plural") {
                let (value, _) = self.parse_string_block("msgid_plural", rest)?;
                entry.msgid_plural = Some(value);
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("msgid") {
                let (value, _) = self.parse_string_block("msgid", rest)?;
                entry.msgid = value;
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("msgstr") {
                let (prefix_kind, after_idx) = if let Some(stripped) = rest.strip_prefix('[') {
                    let end = stripped
                        .find(']')
                        .ok_or_else(|| "unterminated msgstr[N] index".to_owned())?;
                    let idx_str = &stripped[..end];
                    let idx: u32 = idx_str
                        .parse()
                        .map_err(|_| format!("invalid msgstr index `{idx_str}`"))?;
                    (format!("msgstr[{idx}]"), &stripped[end + 1..])
                } else {
                    ("msgstr".to_owned(), rest)
                };
                let (value, block) = self.parse_string_block(&prefix_kind, after_idx)?;
                entry.msgstr_blocks.push(RawMsgstrBlock {
                    range: block,
                    prefix: prefix_kind,
                    unescaped: value,
                });
                if let Some(rest_after) = self.src.get(self.cursor..)
                    && rest_after.starts_with('\n')
                {
                    // Block parsing ended right at the newline; the writer
                    // wants the range to NOT include the trailing newline so
                    // we don't double-emit it on splice.
                }
                continue;
            }
            // Empty line ends the entry.
            if trimmed.is_empty() {
                self.consume_line();
                break;
            }
            return Err(format!(
                "unexpected line at byte {}: `{}`",
                self.cursor,
                trim_for_error(line)
            ));
        }
        Ok(entry)
    }

    fn parse_comment_line(&mut self, line: &str, entry: &mut Entry) -> Result<(), String> {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("#~") {
            // Obsolete entry marker.
            entry.is_obsolete = true;
            // Treat the remainder of `#~ msgid "..."` etc. as a normal entry
            // line by recursing — gettext writes obsolete entries with each
            // line prefixed by `#~`. We adopt a simpler approach: capture
            // the bytes; the round-trip preserves them verbatim because
            // unchanged-unit splicing keeps the original bytes.
            let _ = rest;
            return Ok(());
        }
        if let Some(rest) = trimmed.strip_prefix("#,") {
            for flag in rest.split(',') {
                if flag.trim() == "fuzzy" {
                    entry.is_fuzzy = true;
                }
            }
            return Ok(());
        }
        if let Some(rest) = trimmed.strip_prefix("#.") {
            entry.extracted_comments.push(rest.trim().to_owned());
            return Ok(());
        }
        if let Some(rest) = trimmed.strip_prefix("#:") {
            for ref_tok in rest.split_whitespace() {
                if let Some((file, line_str)) = ref_tok.rsplit_once(':') {
                    let line_no = line_str.parse().ok();
                    entry.source_refs.push(SourceRef {
                        file: file.to_owned(),
                        line: line_no,
                    });
                } else {
                    entry.source_refs.push(SourceRef {
                        file: ref_tok.to_owned(),
                        line: None,
                    });
                }
            }
            return Ok(());
        }
        // Plain `# ...` translator comment: ignore (round-trip handles bytes).
        Ok(())
    }

    fn peek_line(&self) -> &'a str {
        let bytes = self.src.as_bytes();
        let mut end = self.cursor;
        while end < bytes.len() && bytes[end] != b'\n' {
            end += 1;
        }
        &self.src[self.cursor..end]
    }

    /// Parse a string block starting after the keyword (`msgid`, `msgstr`, ...).
    /// Handles multi-line continuations and returns the unescaped value plus
    /// the byte range covering the entire block (keyword + all string lines).
    fn parse_string_block(
        &mut self,
        keyword: &str,
        first_rest: &str,
    ) -> Result<(String, (usize, usize)), String> {
        let line = self.peek_line();
        let leading_ws = line.len() - line.trim_start().len();
        let kw_token_len = keyword_prefix_len(line, keyword);
        // Start of the block excludes the leading whitespace so the writer's
        // replacement string (which doesn't carry indent) cannot strip it.
        let first_line_offset = self.cursor + leading_ws;
        let after_kw = self.cursor + leading_ws + kw_token_len;
        // first_rest excludes the keyword + whatever index `[N]` followed.
        let _ = first_rest;
        let body_start = skip_spaces(self.src.as_bytes(), after_kw);
        if body_start >= self.src.len() || self.src.as_bytes()[body_start] != b'"' {
            return Err(format!(
                "expected `\"` after `{keyword}` at byte {body_start}"
            ));
        }
        let mut unescaped = String::new();
        let (mut cur_line_end, segment) = parse_quoted_string(self.src, body_start)
            .map_err(|e| format!("{keyword}: {e} at byte {body_start}"))?;
        unescaped.push_str(&segment);
        // Advance past trailing whitespace then newline.
        cur_line_end = skip_trailing_to_newline(self.src.as_bytes(), cur_line_end);
        let mut block_end = cur_line_end;
        // Move cursor past the newline of the first line for subsequent scanning.
        self.cursor = if cur_line_end < self.src.len() {
            cur_line_end + 1
        } else {
            cur_line_end
        };

        loop {
            if self.cursor >= self.src.len() {
                break;
            }
            let next_line = self.peek_line();
            let trimmed = next_line.trim_start();
            if !trimmed.starts_with('"') {
                break;
            }
            let cont_quote_start = self.cursor + (next_line.len() - trimmed.len());
            let (line_end, segment) = parse_quoted_string(self.src, cont_quote_start)
                .map_err(|e| format!("{keyword} continuation: {e} at byte {cont_quote_start}"))?;
            unescaped.push_str(&segment);
            let line_end = skip_trailing_to_newline(self.src.as_bytes(), line_end);
            block_end = line_end;
            self.cursor = if line_end < self.src.len() {
                line_end + 1
            } else {
                line_end
            };
        }
        Ok((unescaped, (first_line_offset, block_end)))
    }
}

fn keyword_prefix_len(line: &str, keyword: &str) -> usize {
    // Returns the byte length of the keyword token + any `[N]` after it
    // *within this line*, after leading whitespace.
    let trimmed = line.trim_start();
    debug_assert!(trimmed.starts_with(keyword));
    let after = &trimmed[keyword.len()..];
    if let Some(rest) = after.strip_prefix('[') {
        if let Some(end) = rest.find(']') {
            return keyword.len() + 1 + end + 1;
        }
    }
    keyword.len()
}

fn skip_spaces(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    i
}

fn skip_trailing_to_newline(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i] != b'\n' {
        i += 1;
    }
    i
}

/// Parse a `"..."` string starting at `start` (the opening quote). Returns
/// `(byte_after_closing_quote, unescaped_value)`.
fn parse_quoted_string(src: &str, start: usize) -> Result<(usize, String), String> {
    let bytes = src.as_bytes();
    debug_assert_eq!(bytes[start], b'"');
    let mut i = start + 1;
    let mut out = String::new();
    while i < bytes.len() {
        match bytes[i] {
            b'"' => return Ok((i + 1, out)),
            b'\\' => {
                let next = bytes
                    .get(i + 1)
                    .copied()
                    .ok_or_else(|| "string ends with unfinished escape sequence".to_owned())?;
                match next {
                    b'n' => {
                        out.push('\n');
                        i += 2;
                    }
                    b't' => {
                        out.push('\t');
                        i += 2;
                    }
                    b'r' => {
                        out.push('\r');
                        i += 2;
                    }
                    b'"' => {
                        out.push('"');
                        i += 2;
                    }
                    b'\\' => {
                        out.push('\\');
                        i += 2;
                    }
                    other => {
                        return Err(format!(
                            "unsupported escape `\\{ch}`",
                            ch = char::from(other)
                        ));
                    }
                }
            }
            b'\n' => {
                return Err("unterminated string (newline before closing quote)".to_owned());
            }
            _ => {
                // UTF-8 fast path: walk a contiguous run.
                let run_start = i;
                while i < bytes.len() && bytes[i] != b'"' && bytes[i] != b'\\' && bytes[i] != b'\n'
                {
                    i += 1;
                }
                out.push_str(std::str::from_utf8(&bytes[run_start..i]).expect("utf-8 invariant"));
            }
        }
    }
    Err("unterminated string (eof before closing quote)".to_owned())
}

fn trim_for_error(line: &str) -> String {
    let mut s = line.trim().to_owned();
    if s.len() > 80 {
        s.truncate(80);
        s.push('…');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_nplurals_handles_common_forms() {
        assert_eq!(parse_nplurals(" nplurals=2; plural=(n != 1);"), Some(2));
        assert_eq!(
            parse_nplurals(" nplurals=3; plural=(n%10==1 ? 0 : 1);"),
            Some(3)
        );
        assert_eq!(parse_nplurals("bogus"), None);
    }

    #[test]
    fn build_unit_id_with_context_uses_eot_separator() {
        // Gettext's convention is the EOT () separator between
        // msgctxt and msgid; keeping the same separator means cross-tool
        // hashes (msgctxt-aware tooling) match.
        let id = build_unit_id(Some("menu"), "&File");
        assert_eq!(id.as_str(), "menu\u{0004}&File");
        let id_no_ctx = build_unit_id(None, "Hello");
        assert_eq!(id_no_ctx.as_str(), "Hello");
        let id_empty_ctx = build_unit_id(Some(""), "Hello");
        assert_eq!(id_empty_ctx.as_str(), "Hello");
    }

    #[test]
    fn parse_quoted_string_handles_basic_escapes() {
        let s = "\"hello \\\"world\\\" line\\nbreak\"".to_owned();
        let (end, value) = parse_quoted_string(&s, 0).unwrap();
        assert_eq!(end, s.len());
        assert_eq!(value, "hello \"world\" line\nbreak");
    }
}
