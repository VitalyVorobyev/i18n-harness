//! [`Catalog`] + edited [`Unit`]s → `.ts` writer.
//!
//! The strategy: start from [`crate::Catalog::source_bytes`] and apply a
//! minimal set of byte-range substitutions. Anything we do not touch comes
//! out byte-identical.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;

use i18n_harness_core::{Target, Unit, UnitState};

use crate::catalog::{Catalog, EditPoint, TypeAttr};
use crate::error::ApplyError;
use crate::placeholder::from_icu;

/// Write a catalog back to disk, applying any changes the caller made to the
/// units relative to what `extract` produced.
///
/// # Behavior
///
/// - Units whose `state` is [`UnitState::Vanished`] or [`UnitState::Obsolete`]
///   are never modified.
/// - Units the harness actually filled (target promoted from empty to
///   non-empty *and* state is currently writable) are promoted to
///   [`UnitState::Finished`] in the open tag (the `type="unfinished"`
///   attribute is removed), per the M0 contract.
/// - Units whose target is unchanged from what `extract` produced are left
///   alone — no spurious diffs, no spurious state transitions.
/// - The output file is written atomically: we write to `<out>.tmp` then
///   rename. On error the original `<out>` is left untouched if it existed.
///
/// # Errors
///
/// See [`ApplyError`].
pub fn apply(catalog: &Catalog, units: &[Unit], out: &Path) -> Result<(), ApplyError> {
    let new_bytes = render(catalog, units)?;

    let tmp = match out.extension() {
        Some(ext) => out.with_extension(format!("{}.tmp", ext.to_string_lossy())),
        None => out.with_extension("tmp"),
    };
    {
        let mut f = fs::File::create(&tmp).map_err(|source| ApplyError::Io {
            path: tmp.clone(),
            source,
        })?;
        f.write_all(&new_bytes).map_err(|source| ApplyError::Io {
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

/// In-memory render. Exposed for callers (e.g. the CLI's `round-trip`
/// subcommand) that want to diff bytes without touching the disk.
///
/// # Errors
///
/// See [`ApplyError`]. Only [`ApplyError::UnknownUnit`] can fire here;
/// `apply`'s [`ApplyError::Io`] variants are reachable only via `apply`.
pub fn render(catalog: &Catalog, units: &[Unit]) -> Result<Vec<u8>, ApplyError> {
    // Map each provided unit id to its index inside catalog.units.
    let mut overrides: HashMap<usize, &Unit> = HashMap::new();
    for u in units {
        match catalog.units.iter().position(|c| c.id == u.id) {
            Some(idx) => {
                overrides.insert(idx, u);
            }
            None => {
                return Err(ApplyError::UnknownUnit { id: u.id.0.clone() });
            }
        }
    }

    let mut edits: Vec<Edit> = Vec::new();
    for (idx, ep) in catalog.edit_points.iter().enumerate() {
        // `original` MUST be the pristine parse-time snapshot, not the live
        // (potentially in-place-mutated) `catalog.units[idx]`. The Tauri
        // frontend mutates `catalog.units` through `find_unit_mut` and then
        // passes the same slice as `units` to `apply`, which would make
        // `candidate.target == original.target` trivially true and silently
        // skip the bytes-on-disk update. The CLI path is unaffected because
        // it builds a fresh override slice without touching the catalog's
        // own `units`.
        let pristine = &catalog.original_units[idx];
        // The candidate's default when the caller didn't supply an override
        // for this unit is also the pristine snapshot: that preserves the
        // byte-identical round-trip for `render(&catalog, &[])`.
        let candidate = overrides.get(&idx).copied().unwrap_or(pristine);
        plan_edits_for_unit(candidate, pristine, ep, &catalog.source_bytes, &mut edits);
    }

    edits.sort_by_key(|e| e.start);

    // Sanity: edits must be non-overlapping. Per-unit ranges are disjoint by
    // construction.
    let mut last_end = 0usize;
    for e in &edits {
        debug_assert!(
            e.start >= last_end,
            "overlapping edits at {}..{} after end {}",
            e.start,
            e.end,
            last_end
        );
        last_end = e.end;
    }

    let mut out = Vec::with_capacity(catalog.source_bytes.len());
    let mut cursor = 0usize;
    for e in edits {
        out.extend_from_slice(&catalog.source_bytes[cursor..e.start]);
        out.extend_from_slice(&e.replacement);
        cursor = e.end;
    }
    out.extend_from_slice(&catalog.source_bytes[cursor..]);
    Ok(out)
}

#[derive(Debug)]
struct Edit {
    start: usize,
    end: usize,
    replacement: Vec<u8>,
}

fn plan_edits_for_unit(
    candidate: &Unit,
    original: &Unit,
    ep: &EditPoint,
    source: &[u8],
    edits: &mut Vec<Edit>,
) {
    // Vanished and obsolete are sacred — never touched by the harness.
    if matches!(ep.original_state, UnitState::Vanished | UnitState::Obsolete) {
        return;
    }

    // If the caller passed the unit through unchanged (target identical to
    // what `extract` produced), emit no edits — period. This is what makes
    // byte-stable round-trip on CDATA or anything else `extract` had to
    // simplify into a plain string actually work: we compare *intent*, not
    // *bytes*, for the "did the caller change this?" decision.
    if candidate.target == original.target {
        return;
    }

    let mut singular_body: Option<Vec<u8>> = None;
    let mut numerus_bodies: Vec<Option<Vec<u8>>> = vec![None; ep.numerus_bodies.len()];
    let mut any_target_change = false;
    let mut promote_to_finished;

    match &candidate.target {
        Target::Singular { text } => {
            let new_body = text
                .as_deref()
                .map(|t| escape_xml(&from_icu(t)).into_bytes())
                .unwrap_or_default();
            if new_body != ep.original_translation_body {
                any_target_change = true;
                singular_body = Some(new_body);
            }
            promote_to_finished = matches!(text.as_deref(), Some(s) if !s.is_empty());
        }
        Target::Plural { forms } => {
            promote_to_finished = !forms.is_empty();
            for (i, form) in forms.iter().enumerate() {
                let new_body = form
                    .as_deref()
                    .map(|t| escape_xml(&from_icu(t)).into_bytes())
                    .unwrap_or_default();
                let changed = i >= ep.original_numerus_bodies.len()
                    || new_body != ep.original_numerus_bodies[i];
                if changed && i < ep.numerus_bodies.len() {
                    numerus_bodies[i] = Some(new_body);
                    any_target_change = true;
                }
                if !matches!(form.as_deref(), Some(s) if !s.is_empty()) {
                    promote_to_finished = false;
                }
            }
        }
    }

    // Emit body edits.
    if let Some(new_body) = singular_body {
        edits.push(Edit {
            start: ep.translation_body.0,
            end: ep.translation_body.1,
            replacement: new_body,
        });
    }
    for (i, body) in numerus_bodies.into_iter().enumerate() {
        if let Some(new_body) = body {
            let (s, e) = ep.numerus_bodies[i];
            edits.push(Edit {
                start: s,
                end: e,
                replacement: new_body,
            });
        }
    }

    // State transition: only when we *actually* changed the target and the
    // unit's current state allows write-back. If the caller explicitly set
    // `candidate.state = Proposed`, keep `type="unfinished"` on disk so the
    // file carries the "needs human review" signal the gate produced. This
    // is the contract `harness translate` relies on: gate-clean units land
    // as Finished, gate-flagged units stay Proposed.
    if any_target_change
        && ep.original_state.is_writable()
        && promote_to_finished
        && candidate.state != UnitState::Proposed
    {
        if let Some(attr) = ep.type_attr.as_ref() {
            // Rewrite the open tag to strip `type="unfinished"`.
            edits.push(Edit {
                start: ep.translation_open_tag.0,
                end: ep.translation_open_tag.1,
                replacement: rebuild_open_tag_without_type(source, ep, attr),
            });
        }
    }
}

fn rebuild_open_tag_without_type(source: &[u8], ep: &EditPoint, attr: &TypeAttr) -> Vec<u8> {
    let (tag_start, tag_end) = ep.translation_open_tag;
    debug_assert!(attr.full_range.0 >= tag_start && attr.full_range.1 <= tag_end);
    let mut out = Vec::with_capacity(tag_end - tag_start);
    out.extend_from_slice(&source[tag_start..attr.full_range.0]);
    out.extend_from_slice(&source[attr.full_range.1..tag_end]);
    out
}

fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_xml_handles_specials() {
        assert_eq!(escape_xml("a < b & c > d"), "a &lt; b &amp; c &gt; d");
        // Quotes inside text bodies don't need escaping; only `<`, `>`, `&`.
        assert_eq!(escape_xml("\"hi\""), "\"hi\"");
    }
}
