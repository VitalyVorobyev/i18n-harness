//! Reference-reuse / remainder-split / merge helpers — the testable core behind
//! the Tauri reuse commands.
//!
//! Everything here is `AppState`-free so it can be unit-tested without a Tauri
//! runtime. Each function is a thin wrapper over `i18n-harness-reuse`: it calls
//! the deterministic engine, writes the result through the Qt adapter (keeping
//! every catalog write behind the round-trip contract), and returns a
//! serializable report plus the data the command layer needs to refresh
//! in-memory state and persist review status.
//!
//! These operations move *existing* text between catalogs by exact unit-id
//! match — no model, no network. They run synchronously like a single-unit
//! edit, not through the bulk-translate job machinery.

use std::path::{Path, PathBuf};

use i18n_harness_adapter_qt::{self as adapter_qt, Catalog};
use i18n_harness_core::{ReviewStatus, UnitId};
use i18n_harness_glossary::Glossary;
use i18n_harness_locales::Locale;
use i18n_harness_reuse::{
    ConflictText, ReferenceConflict, ReuseError, ReuseReport, merge_back, reuse_from_references,
    writable_untranslated_ids,
};

use crate::dto::{
    MergeReportDto, ReferenceConflictCandidateDto, ReferenceConflictDto, ReuseReportDto,
    SplitReportDto,
};

/// CLDR-form join separator used by `ConflictText::Plural`. Mirrors the
/// `i18n-harness-reuse` rendering so the React layer can split it back.
const UNIT_SEPARATOR: char = '\u{1F}';

/// A review-status write the command layer must persist after a reuse pass.
///
/// The service decides *what* to write (conflicts → `Conflict` with the
/// candidate list as a JSON note; copied-needs-review → `NeedsReview`); the
/// command layer owns the `Project` handle and does the durable append. Keeping
/// the decision here means the policy is tested once, away from `AppState`.
pub(crate) struct ReviewStatusWrite {
    /// The unit the status applies to.
    pub(crate) unit_id: UnitId,
    /// The status to record.
    pub(crate) status: ReviewStatus,
    /// Reviewer note. For conflicts this is the JSON-serialized candidate list
    /// so it survives a project reopen; for needs-review copies it is `None`.
    pub(crate) note: Option<String>,
}

/// Result of [`reuse_into_catalog`].
pub(crate) struct ReuseServiceResult {
    /// The re-extracted base catalog after the reuse apply. The command layer
    /// stashes this in the in-memory project-catalog store so the UI sees the
    /// copied translations without re-opening from disk.
    pub(crate) catalog: Catalog,
    /// The serializable report for the wire.
    pub(crate) report: ReuseReportDto,
    /// Review-status writes the command layer must persist.
    pub(crate) review_writes: Vec<ReviewStatusWrite>,
}

/// Run reuse from `reference_abs_paths` into the base catalog at `base_abs`,
/// write the merged units back in place via the Qt adapter, and return the
/// post-reuse catalog plus a report and the review-status writes to persist.
///
/// The apply is atomic (temp-then-rename, per the adapter contract). After the
/// write the base is re-extracted so the returned `Catalog` carries the fresh
/// source bytes needed for a subsequent byte-stable round-trip.
pub(crate) fn reuse_into_catalog(
    base_abs: &Path,
    reference_abs_paths: &[PathBuf],
    locale: &Locale,
    glossary: Option<&Glossary>,
) -> Result<ReuseServiceResult, String> {
    let outcome = reuse_from_references(base_abs, reference_abs_paths, locale, glossary)
        .map_err(reuse_error_to_string)?;

    adapter_qt::apply(&outcome.base, &outcome.units, base_abs)
        .map_err(|e| format!("apply failed: {e}"))?;

    // Re-extract so the in-memory catalog carries the on-disk source bytes the
    // round-trip contract depends on (apply consumes `outcome.base`'s bytes).
    let catalog = adapter_qt::extract(base_abs).map_err(|e| format!("re-extract failed: {e}"))?;

    let report = &outcome.report;
    let review_writes = build_review_writes(report);
    let report_dto = build_reuse_report_dto(base_abs, reference_abs_paths, report);

    Ok(ReuseServiceResult {
        catalog,
        report: report_dto,
        review_writes,
    })
}

/// Carve a remainder subset of the base catalog at `base_abs` into `out_abs`,
/// keeping only `keep_ids`. Used by the reuse→split flow (caller passes
/// `report.remaining_ids`) and the standalone split (caller computes the ids
/// with [`standalone_remainder_ids`]).
pub(crate) fn split_remainder(
    base_abs: &Path,
    keep_ids: &std::collections::HashSet<UnitId>,
    out_abs: &Path,
) -> Result<SplitReportDto, String> {
    let base = adapter_qt::extract(base_abs).map_err(|e| format!("extract failed: {e}"))?;
    adapter_qt::write_subset(&base, keep_ids, out_abs)
        .map_err(|e| format!("write_subset failed: {e}"))?;
    Ok(SplitReportDto {
        base_path: base_abs.to_string_lossy().into_owned(),
        out_path: out_abs.to_string_lossy().into_owned(),
        kept_count: keep_ids.len(),
    })
}

/// Compute the writable-untranslated id set for a standalone split (no reuse
/// pass in front). Extracts the base and delegates to
/// `i18n_harness_reuse::writable_untranslated_ids`.
pub(crate) fn standalone_remainder_ids(
    base_abs: &Path,
) -> Result<std::collections::HashSet<UnitId>, String> {
    let base = adapter_qt::extract(base_abs).map_err(|e| format!("extract failed: {e}"))?;
    Ok(writable_untranslated_ids(&base))
}

/// Fold the translated remainder at `with_abs` back into the base at `base_abs`,
/// writing the merged result to `out_abs`, then return a report.
///
/// The merged output is written to `out_abs` (which may differ from any
/// currently-open catalog), so this returns counts only; the command layer does
/// not refresh the in-memory store from it. Overlap / stray-id guard failures
/// surface as a structured `Err(String)` that names the offending ids (see
/// [`reuse_error_to_string`]) so the UI can show them rather than a bare
/// "merge failed".
pub(crate) fn merge_catalogs(
    base_abs: &Path,
    with_abs: &Path,
    out_abs: &Path,
) -> Result<MergeReportDto, String> {
    let outcome = merge_back(base_abs, with_abs).map_err(reuse_error_to_string)?;
    adapter_qt::apply(&outcome.base, &outcome.units, out_abs)
        .map_err(|e| format!("apply failed: {e}"))?;
    Ok(MergeReportDto {
        base_path: base_abs.to_string_lossy().into_owned(),
        with_path: with_abs.to_string_lossy().into_owned(),
        out_path: out_abs.to_string_lossy().into_owned(),
        merged: outcome.report.merged,
        merged_complete: outcome.report.merged_complete,
    })
}

// ── Internal helpers ───────────────────────────────────────────────────────────

/// Map a [`ReuseError`] to a UI-facing string. The merge guard variants carry
/// the offending ids; preserve them verbatim (the `Display` impl already lists
/// them) so the frontend can show which units broke the merge.
fn reuse_error_to_string(err: ReuseError) -> String {
    err.to_string()
}

/// Build the review-status writes a reuse pass implies:
///
/// - every conflicting unit → `Conflict`, with the candidate list serialized as
///   JSON in the note so the React review surface can rebuild it after a reopen;
/// - every copied-needs-review unit → `NeedsReview`, no note (the copied text is
///   already in the catalog; the flag/incomplete state is the signal).
///
/// Copied-finished units get no write — they are promoted to `Finished` in the
/// catalog itself and need no review event.
fn build_review_writes(report: &ReuseReport) -> Vec<ReviewStatusWrite> {
    let mut writes = Vec::with_capacity(report.conflicts.len() + report.copied_needs_review.len());

    for conflict in &report.conflicts {
        let note = serialize_conflict_note(conflict);
        writes.push(ReviewStatusWrite {
            unit_id: conflict.id.clone(),
            status: ReviewStatus::Conflict,
            note: Some(note),
        });
    }

    for id in &report.copied_needs_review {
        writes.push(ReviewStatusWrite {
            unit_id: id.clone(),
            status: ReviewStatus::NeedsReview,
            note: None,
        });
    }

    writes
}

/// Serialize a conflict's candidate list to the JSON shape the wire DTO uses, so
/// the persisted review note is the same structure the UI renders live. Falls
/// back to a plain string only if serialization somehow fails (it does not, for
/// these owned types — the `unwrap_or_else` keeps the function total).
fn serialize_conflict_note(conflict: &ReferenceConflict) -> String {
    let dto = conflict_to_dto(conflict);
    serde_json::to_string(&dto.candidates)
        .unwrap_or_else(|_| format!("{} conflicting candidates", dto.candidates.len()))
}

/// Convert a reuse `ReferenceConflict` to its wire DTO.
fn conflict_to_dto(conflict: &ReferenceConflict) -> ReferenceConflictDto {
    ReferenceConflictDto {
        unit_id: conflict.id.to_string(),
        candidates: conflict
            .candidates
            .iter()
            .map(|c| {
                let (is_plural, text) = match &c.text {
                    ConflictText::Singular(s) => (false, s.clone()),
                    ConflictText::Plural(forms) => (
                        true,
                        forms
                            .iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                            .join(&UNIT_SEPARATOR.to_string()),
                    ),
                };
                ReferenceConflictCandidateDto {
                    reference: c.reference.to_string_lossy().into_owned(),
                    also_from: c
                        .also_from
                        .iter()
                        .map(|p| p.to_string_lossy().into_owned())
                        .collect(),
                    is_plural,
                    text,
                }
            })
            .collect(),
    }
}

/// Build the full wire report from a reuse pass.
fn build_reuse_report_dto(
    base_abs: &Path,
    reference_abs_paths: &[PathBuf],
    report: &ReuseReport,
) -> ReuseReportDto {
    ReuseReportDto {
        catalog_path: base_abs.to_string_lossy().into_owned(),
        references: reference_abs_paths
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect(),
        copied_finished: report.copied_finished_count(),
        copied_needs_review: report.copied_needs_review_count(),
        conflict_count: report.conflict_count(),
        remaining_count: report.remaining_count(),
        conflicts: report.conflicts.iter().map(conflict_to_dto).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use i18n_harness_reuse::ConflictCandidate;

    #[test]
    fn conflict_to_dto_renders_singular_text() {
        let conflict = ReferenceConflict {
            id: UnitId::from("ctx::Hello"),
            candidates: vec![
                ConflictCandidate {
                    reference: PathBuf::from("/refs/a.ts"),
                    also_from: vec![PathBuf::from("/refs/c.ts")],
                    text: ConflictText::Singular("Hallo".to_owned()),
                },
                ConflictCandidate {
                    reference: PathBuf::from("/refs/b.ts"),
                    also_from: vec![],
                    text: ConflictText::Singular("Servus".to_owned()),
                },
            ],
        };
        let dto = conflict_to_dto(&conflict);
        assert_eq!(dto.unit_id, "ctx::Hello");
        assert_eq!(dto.candidates.len(), 2);
        assert_eq!(dto.candidates[0].reference, "/refs/a.ts");
        assert_eq!(dto.candidates[0].also_from, vec!["/refs/c.ts".to_owned()]);
        assert!(!dto.candidates[0].is_plural);
        assert_eq!(dto.candidates[0].text, "Hallo");
        assert_eq!(dto.candidates[1].text, "Servus");
    }

    #[test]
    fn conflict_to_dto_joins_plural_forms_with_unit_separator() {
        let conflict = ReferenceConflict {
            id: UnitId::from("ctx::file"),
            candidates: vec![ConflictCandidate {
                reference: PathBuf::from("/refs/a.ts"),
                also_from: vec![],
                text: ConflictText::Plural(vec!["Datei".to_owned(), "Dateien".to_owned()]),
            }],
        };
        let dto = conflict_to_dto(&conflict);
        assert!(dto.candidates[0].is_plural);
        assert_eq!(dto.candidates[0].text, "Datei\u{1F}Dateien");
    }

    #[test]
    fn build_review_writes_maps_conflicts_to_conflict_status_with_json_note() {
        let mut report = ReuseReport::default();
        report.conflicts.push(ReferenceConflict {
            id: UnitId::from("ctx::Hello"),
            candidates: vec![ConflictCandidate {
                reference: PathBuf::from("/refs/a.ts"),
                also_from: vec![],
                text: ConflictText::Singular("Hallo".to_owned()),
            }],
        });
        report.copied_needs_review.push(UnitId::from("ctx::Bye"));

        let writes = build_review_writes(&report);
        assert_eq!(writes.len(), 2);

        let conflict_write = writes
            .iter()
            .find(|w| w.unit_id.as_str() == "ctx::Hello")
            .expect("conflict write present");
        assert_eq!(conflict_write.status, ReviewStatus::Conflict);
        let note = conflict_write.note.as_deref().expect("conflict has a note");
        // The note must be valid JSON the UI can parse back into candidates.
        let parsed: serde_json::Value = serde_json::from_str(note).expect("note is valid JSON");
        assert!(parsed.is_array());
        assert_eq!(parsed[0]["reference"], "/refs/a.ts");
        assert_eq!(parsed[0]["text"], "Hallo");

        let needs_review_write = writes
            .iter()
            .find(|w| w.unit_id.as_str() == "ctx::Bye")
            .expect("needs-review write present");
        assert_eq!(needs_review_write.status, ReviewStatus::NeedsReview);
        assert!(needs_review_write.note.is_none());
    }
}
