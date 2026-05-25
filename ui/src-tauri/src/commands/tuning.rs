//! Commands for tuning-bundle export and listing.

use crate::dto::ExportTuningBundleResponse;
use crate::error;
use crate::state::AppState;

/// Export a tuning bundle to `.i18n-harness/tuning/<timestamp>/`.
///
/// Calls `Project::export_tuning_bundle` under a brief project lock. The
/// bundle contains the curated example set, the active prompt template,
/// the latest evaluation run (if one exists), the locale config, and a copy
/// of the skill README. Returns a summary of what was written.
///
/// Errors:
/// - `"no project open"` when no project is loaded.
/// - `"no curated examples; promote some corrections first"` when the curated
///   set is empty.
/// - I/O error messages for filesystem failures.
#[tauri::command]
pub(crate) fn export_tuning_bundle_in_project(
    state: tauri::State<'_, AppState>,
) -> Result<ExportTuningBundleResponse, String> {
    let project_guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = project_guard.as_ref().ok_or_else(error::no_project)?;
    let summary = project.export_tuning_bundle().map_err(|e| e.to_string())?;
    Ok(ExportTuningBundleResponse {
        path: summary.path,
        examples_count: summary.examples_count,
        locales: summary.locales,
        has_score: summary.has_score,
        prompt_template_version: summary.prompt_template_version,
    })
}

/// List previously-exported tuning bundles for the open project, newest-first.
///
/// Reads the `.i18n-harness/tuning/` directory and returns one summary per
/// bundle subdirectory that contains a valid `examples.jsonl`. Returns an
/// empty array when no bundles have been exported or no project is open.
#[tauri::command]
pub(crate) fn list_tuning_bundles_in_project(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ExportTuningBundleResponse>, String> {
    let project_guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = project_guard.as_ref().ok_or_else(error::no_project)?;
    let bundles = project.list_tuning_bundles().map_err(|e| e.to_string())?;
    Ok(bundles
        .into_iter()
        .map(|s| ExportTuningBundleResponse {
            path: s.path,
            examples_count: s.examples_count,
            locales: s.locales,
            has_score: s.has_score,
            prompt_template_version: s.prompt_template_version,
        })
        .collect())
}
