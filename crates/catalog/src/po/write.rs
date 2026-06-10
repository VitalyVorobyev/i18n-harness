//! `.po` writer. Splices new `msgstr` blocks into the original bytes; reuses
//! verbatim bytes for every unit the caller did not change.
//!
//! # Byte-stability strategy
//!
//! Same shape as the Qt adapter: edits are byte-range substitutions on a
//! preserved copy of the source bytes. For each unit:
//!
//! 1. Compare the in-memory target text (ICU-normalized) against the
//!    `original_targets` table captured during extract.
//! 2. If they match, do nothing — the original bytes for this block survive
//!    untouched.
//! 3. If they differ, format a new `msgstr` block (using the original
//!    `prefix`, e.g. `msgstr ` or `msgstr[1] `) and splice it over the
//!    block's byte range.
//!
//! Edits are applied right-to-left so earlier offsets stay valid.

use std::fs;
use std::path::Path;

use i18n_harness_core::{Target, Unit, UnitState};

use super::parse::MsgstrBlock;
use super::placeholder::{PoPlaceholder, from_icu_with_table};
use crate::catalog::{Catalog, ExtractState};
use crate::error::CatalogError;

pub(super) fn render(catalog: &Catalog, units: &[Unit]) -> Result<Vec<u8>, CatalogError> {
    let edit = match &catalog.extract_state {
        ExtractState::Po(s) => s,
        _ => {
            return Err(CatalogError::Apply {
                path: catalog.source_path.clone(),
                reason: "extract state is not PO (catalog/format mismatch)".to_owned(),
            });
        }
    };

    if units.is_empty() {
        return Ok(catalog.source_bytes.clone());
    }

    if units.len() != edit.units.len() {
        return Err(CatalogError::Apply {
            path: catalog.source_path.clone(),
            reason: format!(
                "unit count changed: catalog had {}, got {}",
                edit.units.len(),
                units.len()
            ),
        });
    }

    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for (unit, edit_state) in units.iter().zip(edit.units.iter()) {
        // Obsolete / Vanished units are never rewritten — their msgstr_blocks
        // may be empty (gettext `#~` obsolete entries are not parsed into the
        // edit state; round-trip relies on source-byte splicing for them).
        // The corresponding bytes pass through unchanged because no edit
        // range is added for this unit. Codex P1 follow-up on PR #38.
        //
        // Note: this guard intentionally checks for Obsolete/Vanished
        // specifically rather than calling `is_writable()`. Finished units
        // MUST be rewritten when the user edits them — `is_writable` excludes
        // Finished for the CLI batch contract, but the PO apply path is the
        // UI write side and follows the `is_ui_editable` semantics.
        if matches!(unit.state, UnitState::Obsolete | UnitState::Vanished) {
            continue;
        }
        let new_targets = collect_target_strings(&unit.target);
        if new_targets.len() != edit_state.msgstr_blocks.len() {
            // Plural arity changed (e.g., locale arity reconciliation
            // landed a new slot). Re-render every block with the new arity.
            let block = edit_state
                .msgstr_blocks
                .first()
                .ok_or_else(|| CatalogError::Apply {
                    path: catalog.source_path.clone(),
                    reason: "no msgstr block in catalog state".to_owned(),
                })?;
            let rendered = format_blocks(&new_targets, block, &edit_state.source_placeholders)?;
            let (first_start, _) = edit_state.msgstr_blocks.first().unwrap().range;
            let (_, last_end) = edit_state.msgstr_blocks.last().unwrap().range;
            edits.push((first_start, last_end, rendered));
            continue;
        }

        for (idx, (block, new)) in edit_state
            .msgstr_blocks
            .iter()
            .zip(new_targets.iter())
            .enumerate()
        {
            let original = edit_state.original_targets.get(idx).cloned().flatten();
            if same_target(&original, new) {
                continue;
            }
            let rendered = format_block(block, new.as_deref(), &edit_state.source_placeholders)?;
            edits.push((block.range.0, block.range.1, rendered));
        }
    }

    edits.sort_by_key(|(start, _, _)| *start);
    // Ensure no overlap.
    for w in edits.windows(2) {
        if w[0].1 > w[1].0 {
            return Err(CatalogError::Apply {
                path: catalog.source_path.clone(),
                reason: "overlapping edit ranges".to_owned(),
            });
        }
    }

    let mut out = Vec::with_capacity(catalog.source_bytes.len());
    let mut cursor = 0;
    for (start, end, repl) in edits {
        out.extend_from_slice(&catalog.source_bytes[cursor..start]);
        out.extend_from_slice(repl.as_bytes());
        cursor = end;
    }
    out.extend_from_slice(&catalog.source_bytes[cursor..]);
    Ok(out)
}

pub(super) fn apply(catalog: &Catalog, units: &[Unit], out: &Path) -> Result<(), CatalogError> {
    let bytes = render(catalog, units)?;
    write_atomically(out, &bytes)
}

fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), CatalogError> {
    let dir = path.parent().ok_or_else(|| CatalogError::Io {
        op: "write",
        path: path.to_path_buf(),
        source: std::io::Error::other("output path has no parent directory"),
    })?;
    let tmp = dir.join(format!(
        ".{}.tmp-i18n-harness",
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("catalog")
    ));
    fs::write(&tmp, bytes).map_err(|source| CatalogError::Io {
        op: "write",
        path: tmp.clone(),
        source,
    })?;
    fs::rename(&tmp, path).map_err(|source| {
        let _ = fs::remove_file(&tmp);
        CatalogError::Io {
            op: "rename",
            path: path.to_path_buf(),
            source,
        }
    })
}

fn collect_target_strings(t: &Target) -> Vec<Option<String>> {
    match t {
        Target::Singular { text } => vec![text.clone()],
        Target::Plural { forms } => forms.clone(),
    }
}

fn same_target(original: &Option<String>, new: &Option<String>) -> bool {
    match (original, new) {
        (None, None) => true,
        (None, Some(s)) | (Some(s), None) => s.is_empty(),
        (Some(a), Some(b)) => a == b,
    }
}

fn format_block(
    block: &MsgstrBlock,
    new_target: Option<&str>,
    placeholders: &[PoPlaceholder],
) -> Result<String, CatalogError> {
    let icu = new_target.unwrap_or("");
    let native = if icu.is_empty() {
        String::new()
    } else {
        from_icu_with_table(icu, placeholders).map_err(CatalogError::PlaceholderConversion)?
    };
    let escaped = escape(&native);
    let mut out = String::with_capacity(block.prefix.len() + escaped.len() + 3);
    out.push_str(&block.prefix);
    out.push(' ');
    out.push('"');
    out.push_str(&escaped);
    out.push('"');
    Ok(out)
}

fn format_blocks(
    new_targets: &[Option<String>],
    template: &MsgstrBlock,
    placeholders: &[PoPlaceholder],
) -> Result<String, CatalogError> {
    let mut out = String::new();
    for (i, new) in new_targets.iter().enumerate() {
        let icu = new.as_deref().unwrap_or("");
        let native = if icu.is_empty() {
            String::new()
        } else {
            from_icu_with_table(icu, placeholders).map_err(CatalogError::PlaceholderConversion)?
        };
        let escaped = escape(&native);
        if i > 0 {
            out.push('\n');
        }
        // For arity-changed re-render we always emit `msgstr[N]` form,
        // matching the gettext convention for plural entries.
        if new_targets.len() > 1 {
            out.push_str(&format!("msgstr[{i}] \"{}\"", escaped));
        } else {
            out.push_str(&template.prefix);
            out.push(' ');
            out.push('"');
            out.push_str(&escaped);
            out.push('"');
        }
    }
    Ok(out)
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_handles_special_chars() {
        assert_eq!(escape("hello"), "hello");
        assert_eq!(escape("a\"b"), "a\\\"b");
        assert_eq!(escape("a\\b"), "a\\\\b");
        assert_eq!(escape("line\nbreak"), "line\\nbreak");
    }

    #[test]
    fn same_target_treats_none_and_empty_as_equivalent() {
        assert!(same_target(&None, &None));
        assert!(same_target(&None, &Some(String::new())));
        assert!(same_target(&Some(String::new()), &None));
        assert!(!same_target(&Some("x".into()), &None));
        assert!(!same_target(&Some("a".into()), &Some("b".into())));
    }
}
