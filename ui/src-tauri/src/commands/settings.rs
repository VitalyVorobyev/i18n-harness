//! Commands for mutating project manifest settings (catalogs, locales, backend,
//! glossary, prompts).

use i18n_harness_project::{
    BackendConfig, CatalogEntry, GlossaryConfig, LocaleConfig, PromptsConfig,
};

use crate::dto::ProjectOpenResponse;
use crate::error;
use crate::state::AppState;

/// Add a new catalog entry to the project manifest and persist it.
///
/// Errors if the path already exists in the manifest (`DuplicateCatalogPath`)
/// or the file is not found on disk (`CatalogNotFound`). Returns a fresh
/// `ProjectOpenResponse` so the UI can re-render without an extra round trip.
#[tauri::command]
pub(crate) fn add_catalog_to_project(
    entry: CatalogEntry,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectOpenResponse, String> {
    let mut guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = guard.as_mut().ok_or_else(error::no_project)?;
    project.add_catalog(entry).map_err(|e| e.to_string())?;
    project.save_manifest().map_err(|e| e.to_string())?;
    let summary = project.summary();
    Ok(ProjectOpenResponse {
        summary,
        warnings: vec![],
    })
}

/// Remove the catalog at `path` (manifest-relative) from the project manifest
/// and persist it.
///
/// Also evicts the catalog from the in-memory `project_catalogs` store so
/// the UI cannot navigate to a catalog that no longer exists in the manifest.
/// Idempotent — returns successfully even if no entry matched (removed = false
/// is not surfaced to the UI; the fresh summary is sufficient).
#[tauri::command]
pub(crate) fn remove_catalog_from_project(
    path: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectOpenResponse, String> {
    let manifest_path = std::path::PathBuf::from(&path);

    // Acquire project lock, mutate, and release before taking project_catalogs.
    let (summary, abs_path) = {
        let mut guard = state
            .project
            .lock()
            .map_err(error::lock_poisoned("project"))?;
        let project = guard.as_mut().ok_or_else(error::no_project)?;

        // Resolve the absolute path before the mutation for the catalog eviction
        // step below — after removal the catalog() lookup would return None.
        let abs = project
            .catalog(&manifest_path)
            .map(|r| std::path::PathBuf::from(&r.absolute_path));

        project
            .remove_catalog(&manifest_path)
            .map_err(|e| e.to_string())?;
        project.save_manifest().map_err(|e| e.to_string())?;
        let summary = project.summary();
        (summary, abs)
    };

    // Evict from the open-catalog store (best-effort; no error if absent).
    if let Some(abs) = abs_path {
        if let Ok(mut store) = state.project_catalogs.lock() {
            store.remove(&abs);
        }
    }

    Ok(ProjectOpenResponse {
        summary,
        warnings: vec![],
    })
}

/// Upsert a locale config block in the project manifest and persist it.
///
/// Creates the `[locales.<id>]` block if absent; updates only the fields
/// present in `config`, leaving unknown sibling keys untouched (forward-compat).
#[tauri::command]
pub(crate) fn update_locale_in_project(
    id: String,
    config: LocaleConfig,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectOpenResponse, String> {
    let mut guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = guard.as_mut().ok_or_else(error::no_project)?;
    project
        .update_locale(&id, config)
        .map_err(|e| e.to_string())?;
    project.save_manifest().map_err(|e| e.to_string())?;
    let summary = project.summary();
    Ok(ProjectOpenResponse {
        summary,
        warnings: vec![],
    })
}

/// Remove a locale config block from the project manifest and persist it.
///
/// Idempotent — no error if the block did not exist.
#[tauri::command]
pub(crate) fn remove_locale_from_project(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectOpenResponse, String> {
    let mut guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = guard.as_mut().ok_or_else(error::no_project)?;
    project.remove_locale(&id).map_err(|e| e.to_string())?;
    project.save_manifest().map_err(|e| e.to_string())?;
    let summary = project.summary();
    Ok(ProjectOpenResponse {
        summary,
        warnings: vec![],
    })
}

/// Replace the `[backend.default]` block in the project manifest and persist it.
///
/// Creates the block if absent. Preserves unknown sibling keys.
#[tauri::command]
pub(crate) fn set_backend_in_project(
    config: BackendConfig,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectOpenResponse, String> {
    let mut guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = guard.as_mut().ok_or_else(error::no_project)?;
    project.set_backend(config).map_err(|e| e.to_string())?;
    project.save_manifest().map_err(|e| e.to_string())?;
    let summary = project.summary();
    Ok(ProjectOpenResponse {
        summary,
        warnings: vec![],
    })
}

/// Replace the `[glossary]` block in the project manifest and persist it.
///
/// Creates the block if absent.
#[tauri::command]
pub(crate) fn set_glossary_in_project(
    config: GlossaryConfig,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectOpenResponse, String> {
    let mut guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = guard.as_mut().ok_or_else(error::no_project)?;
    project.set_glossary(config).map_err(|e| e.to_string())?;
    project.save_manifest().map_err(|e| e.to_string())?;
    let summary = project.summary();
    Ok(ProjectOpenResponse {
        summary,
        warnings: vec![],
    })
}

/// Replace the `[prompts]` block in the project manifest and persist it.
///
/// Creates the block if absent.
#[tauri::command]
pub(crate) fn set_prompts_in_project(
    config: PromptsConfig,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectOpenResponse, String> {
    let mut guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = guard.as_mut().ok_or_else(error::no_project)?;
    project.set_prompts(config).map_err(|e| e.to_string())?;
    project.save_manifest().map_err(|e| e.to_string())?;
    let summary = project.summary();
    Ok(ProjectOpenResponse {
        summary,
        warnings: vec![],
    })
}
