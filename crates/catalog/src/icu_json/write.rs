//! ICU-JSON writer. Splices new value strings into the original bytes;
//! reuses verbatim bytes for every unit the caller did not change.
//!
//! # Byte-stability strategy
//!
//! Identical shape to the PO writer (`crate::po::write`):
//!
//! 1. Compare the in-memory target text against the `original_value` captured
//!    during extract.
//! 2. If they match, do nothing — the original bytes for this value (and the
//!    surrounding key, whitespace, and structural punctuation) survive
//!    untouched.
//! 3. If they differ, render the new value via `serde_json::to_string` (the
//!    canonical JSON string serializer: produces `"…"` with all required
//!    escapes) and splice it over the captured value range — which itself
//!    includes the quotes.
//!
//! Edits are applied left-to-right by building a fresh byte buffer; only the
//! ranges between edits get copied verbatim. The PO writer applies edits
//! right-to-left into a mutable copy; this writer does it streaming-left-to-
//! right because the edit ranges never overlap (each one is one value
//! string).
//!
//! # What about Obsolete units?
//!
//! ICU-JSON has no native obsolete concept. A unit can land in `Obsolete`
//! state only via UI flows that mark it for deletion; on write we skip such
//! units exactly like PO does (their original bytes pass through). The
//! catalog format does not delete keys — that is the build tooling's job.

use std::fs;
use std::path::Path;

use i18n_harness_core::{Target, Unit, UnitState};

use crate::catalog::{Catalog, ExtractState};
use crate::error::CatalogError;

pub(super) fn render(catalog: &Catalog, units: &[Unit]) -> Result<Vec<u8>, CatalogError> {
    let edit = match &catalog.extract_state {
        ExtractState::IcuJson(s) => s,
        _ => {
            return Err(CatalogError::Apply {
                path: catalog.source_path.clone(),
                reason: "extract state is not ICU-JSON (catalog/format mismatch)".to_owned(),
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
        if matches!(unit.state, UnitState::Obsolete | UnitState::Vanished) {
            continue;
        }
        let new_target = match &unit.target {
            Target::Singular { text } => text.as_deref().unwrap_or(""),
            Target::Plural { .. } => {
                return Err(CatalogError::Apply {
                    path: catalog.source_path.clone(),
                    reason: format!(
                        "unit `{}`: ICU-JSON does not represent gettext-style plural targets; \
                         use an ICU `plural` expression inside a single string instead",
                        unit.id
                    ),
                });
            }
        };
        if new_target == edit_state.original_value {
            continue;
        }
        let escaped = format_json_string(new_target);
        edits.push((edit_state.value_range.0, edit_state.value_range.1, escaped));
    }

    edits.sort_by_key(|(start, _, _)| *start);
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

/// Render `s` as a JSON string literal (including the surrounding quotes).
///
/// Uses `serde_json::to_string` so the escape rules match the canonical JSON
/// spec for every codepoint — including control chars, `"`, `\`, and non-BMP
/// codepoints (emitted as their literal UTF-8 by serde_json's default, which
/// matches what most ICU-JSON catalogs in the wild contain). We intentionally
/// do NOT roll our own escaper here: the JSON escape rules are subtle enough
/// that hand-coding them invites subtle round-trip bugs.
fn format_json_string(s: &str) -> String {
    serde_json::to_string(s).expect("serde_json cannot fail on a &str")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_basic_string_round_trips_through_serde_json() {
        let s = "hello world";
        let out = format_json_string(s);
        assert_eq!(out, "\"hello world\"");
    }

    #[test]
    fn format_escapes_quotes_and_backslashes() {
        let s = r#"he said "hi" \ ok"#;
        let out = format_json_string(s);
        assert_eq!(out, r#""he said \"hi\" \\ ok""#);
    }

    #[test]
    fn format_escapes_newline_and_control_chars() {
        let out = format_json_string("a\nb\tc\u{0001}d");
        assert_eq!(out, "\"a\\nb\\tc\\u0001d\"");
    }

    #[test]
    fn format_passes_unicode_verbatim() {
        // serde_json defaults to passing non-BMP through as UTF-8, not as
        // \uXXXX surrogate pairs. That matches the way human-written ICU-JSON
        // is stored on disk.
        let out = format_json_string("é 漢 😀");
        assert_eq!(out, "\"é 漢 😀\"");
    }
}
