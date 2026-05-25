//! Commands for managing corrections and the curated set.

use i18n_harness_core::UnitId;
use i18n_harness_project::{Correction, CorrectionProvenance, NewCorrection};

use crate::dto::{CorrectionIdResponse, ListCorrectionsFilter, RecordCorrectionRequest};
use crate::error;
use crate::services::project_paths::{parse_correction_id, resolve_catalog_path};
use crate::state::AppState;

/// Record an accepted human edit in the project's `corrections.jsonl`.
///
/// `catalog_path` may be absolute or manifest-relative; the command resolves
/// it against the project's catalog index and errors with
/// `"catalog not registered in project"` if no match is found. Returns the
/// content-addressed correction id.
#[tauri::command]
pub(crate) fn record_correction_in_project(
    req: RecordCorrectionRequest,
    state: tauri::State<'_, AppState>,
) -> Result<CorrectionIdResponse, String> {
    use i18n_harness_project::CorrectionFilter;

    let project_guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = project_guard.as_ref().ok_or_else(error::no_project)?;

    let (_, manifest_relative) = resolve_catalog_path(project, &req.catalog_path)?;

    let new_corr = NewCorrection {
        catalog: manifest_relative,
        locale: req.locale,
        unit_id: UnitId::from(req.unit_id),
        source: req.source,
        mt_proposal: req.mt_proposal,
        human_target: req.human_target,
        provenance: CorrectionProvenance {
            backend: req.provenance.backend,
            model: req.provenance.model,
            model_version: req.provenance.model_version,
            prompt_template_version: req.provenance.prompt_template_version,
            glossary_version: req.provenance.glossary_version,
        },
        flags_at_correction: req.flags_at_correction,
    };
    let _ = CorrectionFilter::default(); // suppress unused import warning
    let id = project
        .record_correction(new_corr)
        .map_err(|e| e.to_string())?;

    Ok(CorrectionIdResponse { id: id.to_string() })
}

/// List corrections stored in the project, optionally filtered.
///
/// `catalog_path` in the filter (if provided) may be absolute or
/// manifest-relative; it is resolved before the scan. The `curated_only` flag
/// post-filters to corrections that appear in the curated set.
#[tauri::command]
pub(crate) fn list_corrections_in_project(
    filter: ListCorrectionsFilter,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<Correction>, String> {
    use i18n_harness_project::CorrectionFilter;

    let project_guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = project_guard.as_ref().ok_or_else(error::no_project)?;

    let catalog_manifest = filter
        .catalog_path
        .as_deref()
        .map(|p| resolve_catalog_path(project, p).map(|(_, rel)| rel))
        .transpose()?;

    let cf = CorrectionFilter {
        catalog: catalog_manifest,
        locale: filter.locale,
        unit_id: filter.unit_id.map(UnitId::from),
        since: None,
    };

    let mut corrections = project.list_corrections(cf).map_err(|e| e.to_string())?;

    if filter.curated_only {
        let curated = project.curated();
        corrections.retain(|c| curated.contains(&c.id));
    }

    Ok(corrections)
}

/// Promote a correction to the project's curated set.
///
/// `id` must be a `"corr_<12-hex>"` string previously returned by
/// `record_correction_in_project`. Returns `"invalid correction id"` if the
/// string cannot be parsed.
#[tauri::command]
pub(crate) fn promote_correction_to_curated(
    id: String,
    note: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let corr_id = parse_correction_id(&id)?;
    let mut project_guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = project_guard.as_mut().ok_or_else(error::no_project)?;
    project
        .promote_to_curated(corr_id, note)
        .map_err(|e| e.to_string())
}

/// Remove a correction from the project's curated set.
///
/// Idempotent: returns `false` if the id was not in the curated set, `true`
/// if it was removed. Returns `"invalid correction id"` if `id` cannot be
/// parsed.
#[tauri::command]
pub(crate) fn un_curate_correction(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    let corr_id = parse_correction_id(&id)?;
    let mut project_guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = project_guard.as_mut().ok_or_else(error::no_project)?;
    project.un_curate(&corr_id).map_err(|e| e.to_string())
}

/// Return all entries in the project's curated set.
///
/// Each `CuratedExample` carries its `id`, an optional `note`, and the
/// resolved `correction` data (the full `Correction` record from
/// `corrections.jsonl`). When the underlying correction has been deleted
/// (file rotation, manual edit, project copy without state), the `correction`
/// field is `None` — the dangling entry is still returned so the UI can show
/// it and let the user remove it via `un_curate_correction`.
#[tauri::command]
pub(crate) fn list_curated_in_project(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<i18n_harness_project::CuratedExample>, String> {
    let project_guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = project_guard.as_ref().ok_or_else(error::no_project)?;
    Ok(project.curated().examples().cloned().collect())
}
