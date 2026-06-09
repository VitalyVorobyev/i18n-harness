//! Byte-subtraction subset writer for Qt `.ts`.
//!
//! Given a parsed [`Catalog`] and a set of [`UnitId`]s to **keep**, this
//! produces a new, valid `.ts` containing only those units, built by deleting
//! the byte spans of the dropped units (and any context that becomes empty)
//! from [`Catalog::source_bytes`]. It is the inverse discipline of
//! [`crate::apply`]: where `apply` splices *replacements* over edit points,
//! `render_subset` splices *deletions* over message / context spans.
//!
//! # What `render_subset` guarantees
//!
//! - **Kept messages are byte-identical.** Every kept `<message>…</message>`
//!   block appears in the output as a verbatim substring of the input; kept
//!   messages are never reserialized.
//! - **Id-exactness.** `extract(render_subset(c, keep))` yields exactly the
//!   units whose ids are in `keep` (intersected with the units actually
//!   present in `c`).
//! - **Empty-context pruning.** A `<context>` whose every member unit is
//!   dropped is removed in full — `<name>`, whitespace, and all — so no
//!   dangling empty contexts remain. A context keeping ≥1 member is left
//!   intact apart from its dropped messages.
//! - **Clean whitespace.** Deleting a block also absorbs the run of leading
//!   horizontal whitespace and the single newline that introduced it, so no
//!   blank lines accumulate.
//! - **Identity on keep-all.** `render_subset(c, <all ids>)` is byte-identical
//!   to the original file (no edits are emitted).
//! - **Re-parseable / round-trippable.** The output re-parses via
//!   [`crate::extract`] and itself round-trips byte-stably.
//!
//! # What `render_subset` does NOT guarantee
//!
//! - It does not reflow or re-indent surviving content. The header
//!   (`<?xml?>`, `<!DOCTYPE TS>`), the `<TS …>` root with its `language`
//!   attribute, and any document-level comments outside contexts pass through
//!   verbatim.
//! - It does not validate `keep` against ids not present in the catalog; ids
//!   in `keep` that the catalog does not contain are simply ignored (the
//!   result still contains every catalog unit that *is* in `keep`).
//! - A `<message>` that parsed to no [`Unit`] (e.g. one with no `<source>`)
//!   is not individually addressable; it survives unless the whole context it
//!   lives in is pruned.

use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::Path;

use i18n_harness_core::UnitId;

use crate::catalog::Catalog;
use crate::error::ApplyError;

/// One contiguous byte range to delete from the source.
#[derive(Debug, Clone, Copy)]
struct DeleteSpan {
    start: usize,
    end: usize,
}

/// Render a structural subset of `catalog` containing only the units whose
/// ids are in `keep`, by byte-subtraction from the original source bytes.
///
/// See the module docs for the full contract. This is the in-memory form;
/// [`write_subset`] is the atomic-write wrapper.
pub fn render_subset(catalog: &Catalog, keep: &HashSet<UnitId>) -> Vec<u8> {
    // Which catalog unit indices are dropped (= not kept).
    let drop_unit: Vec<bool> = catalog
        .units
        .iter()
        .map(|u| !keep.contains(&u.id))
        .collect();

    // A unit index belongs to a context that is being pruned in full. Such a
    // unit's per-message delete is redundant with the context-block delete, so
    // we skip it to avoid double-coverage. We mark those indices up front.
    let mut covered_by_context: Vec<bool> = vec![false; catalog.units.len()];
    let mut deletes: Vec<DeleteSpan> = Vec::new();

    for ctx in &catalog.contexts {
        let all_dropped = !ctx.member_unit_indices.is_empty()
            && ctx
                .member_unit_indices
                .iter()
                .all(|&idx| drop_unit.get(idx).copied().unwrap_or(false));
        // A context with no member units at all is pruned too: it would
        // otherwise survive as a dangling `<context><name>…</name></context>`
        // with nothing translatable inside, which the subset feature exists to
        // avoid. (Such a context is rare — it means every `<message>` inside
        // failed to parse to a unit — but the rule is "no empty contexts".)
        let empty_context = ctx.member_unit_indices.is_empty();
        if all_dropped || empty_context {
            for &idx in &ctx.member_unit_indices {
                covered_by_context[idx] = true;
            }
            deletes.push(absorb_leading_ws(&catalog.source_bytes, ctx.block));
        }
    }

    for (idx, dropped) in drop_unit.iter().enumerate() {
        if *dropped && !covered_by_context[idx] {
            deletes.push(absorb_leading_ws(
                &catalog.source_bytes,
                catalog.message_spans[idx],
            ));
        }
    }

    splice_out(&catalog.source_bytes, deletes)
}

/// Write a structural subset of `catalog` to `out`, keeping only the units in
/// `keep`. Same atomic temp-then-rename discipline as [`crate::apply`]: writes
/// to `<out>.tmp`, fsyncs, then renames over `out`.
///
/// # Errors
///
/// Returns [`ApplyError::Io`] if the temp file cannot be created, written,
/// flushed, or renamed. On any failure an existing `out` is left untouched.
pub fn write_subset(
    catalog: &Catalog,
    keep: &HashSet<UnitId>,
    out: &Path,
) -> Result<(), ApplyError> {
    let bytes = render_subset(catalog, keep);
    write_atomic(&bytes, out)
}

/// Atomic write helper, factored out of the subset/apply paths. Writes to a
/// sibling `.tmp` file, fsyncs it, then renames over `out`.
pub(crate) fn write_atomic(bytes: &[u8], out: &Path) -> Result<(), ApplyError> {
    let tmp = match out.extension() {
        Some(ext) => out.with_extension(format!("{}.tmp", ext.to_string_lossy())),
        None => out.with_extension("tmp"),
    };
    {
        let mut f = fs::File::create(&tmp).map_err(|source| ApplyError::Io {
            path: tmp.clone(),
            source,
        })?;
        f.write_all(bytes).map_err(|source| ApplyError::Io {
            path: tmp.clone(),
            source,
        })?;
        f.sync_all().map_err(|source| ApplyError::Io {
            path: tmp.clone(),
            source,
        })?;
    }
    fs::rename(&tmp, out).map_err(|source| ApplyError::Io {
        path: out.to_path_buf(),
        source,
    })?;
    Ok(())
}

/// Extend a `(start, end)` block backward over the run of leading horizontal
/// whitespace and the single newline that introduced it, so deleting the
/// block leaves no blank line behind.
///
/// The rule: from `start`, walk left over spaces and tabs; then, if the byte
/// immediately before that run is `\n`, consume it too (and a preceding `\r`
/// if present, to handle CRLF). This claims exactly the indentation + line
/// break of the line the block sits on. Consecutive sibling deletes tile
/// cleanly because the gap between two siblings is exactly one such run,
/// claimed by the later sibling.
fn absorb_leading_ws(bytes: &[u8], block: (usize, usize)) -> DeleteSpan {
    let (start, end) = block;
    let mut s = start;
    while s > 0 && (bytes[s - 1] == b' ' || bytes[s - 1] == b'\t') {
        s -= 1;
    }
    if s > 0 && bytes[s - 1] == b'\n' {
        s -= 1;
        if s > 0 && bytes[s - 1] == b'\r' {
            s -= 1;
        }
    }
    DeleteSpan { start: s, end }
}

/// Splice the deletes out of `source`, producing the subset bytes. Sorts the
/// deletes, merges any that touch or overlap (so absorbed whitespace can never
/// cause a double-copy), `debug_assert!`s non-overlap of the merged set, then
/// copies the surviving gaps — the same splicing discipline as
/// [`crate::render`].
fn splice_out(source: &[u8], mut deletes: Vec<DeleteSpan>) -> Vec<u8> {
    if deletes.is_empty() {
        return source.to_vec();
    }
    deletes.sort_by_key(|d| d.start);

    // Merge overlapping or adjacent spans. By construction (per-message and
    // whole-context deletes never double-cover, and absorbed whitespace tiles
    // edge-to-edge) the spans are already disjoint; merging only collapses the
    // edge-adjacent case into one copy boundary and makes the invariant total.
    let mut merged: Vec<DeleteSpan> = Vec::with_capacity(deletes.len());
    for d in deletes {
        match merged.last_mut() {
            Some(prev) if d.start <= prev.end => {
                prev.end = prev.end.max(d.end);
            }
            _ => merged.push(d),
        }
    }

    let mut last_end = 0usize;
    for d in &merged {
        debug_assert!(
            d.start >= last_end,
            "overlapping subset deletes at {}..{} after end {}",
            d.start,
            d.end,
            last_end
        );
        last_end = d.end;
    }

    let total: usize = merged.iter().map(|d| d.end - d.start).sum();
    let mut out = Vec::with_capacity(source.len().saturating_sub(total));
    let mut cursor = 0usize;
    for d in merged {
        out.extend_from_slice(&source[cursor..d.start]);
        cursor = d.end;
    }
    out.extend_from_slice(&source[cursor..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absorb_takes_indent_and_one_newline() {
        let bytes = b"</message>\n    <message>";
        // block starts at the second `<message>` (index of last `<`).
        let block_start = bytes.len() - "<message>".len();
        let span = absorb_leading_ws(bytes, (block_start, bytes.len()));
        // Should absorb back over "\n    " to land right after "</message>".
        assert_eq!(span.start, "</message>".len());
        assert_eq!(&bytes[..span.start], b"</message>");
    }

    #[test]
    fn absorb_handles_crlf() {
        let bytes = b"X\r\n    <message>";
        let block_start = bytes.len() - "<message>".len();
        let span = absorb_leading_ws(bytes, (block_start, bytes.len()));
        assert_eq!(span.start, 1, "should absorb \\r\\n + indent");
        assert_eq!(&bytes[..span.start], b"X");
    }

    #[test]
    fn absorb_at_start_of_file_is_noop() {
        let bytes = b"<message>";
        let span = absorb_leading_ws(bytes, (0, bytes.len()));
        assert_eq!(span.start, 0);
    }

    #[test]
    fn splice_empty_is_identity() {
        let src = b"hello world";
        assert_eq!(splice_out(src, Vec::new()), src.to_vec());
    }

    #[test]
    fn splice_merges_adjacent() {
        let src = b"AABBCC";
        let out = splice_out(
            src,
            vec![
                DeleteSpan { start: 2, end: 4 },
                DeleteSpan { start: 4, end: 6 },
            ],
        );
        assert_eq!(out, b"AA".to_vec());
    }
}
