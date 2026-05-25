//! Commands for the project-scoped per-catalog store.

use std::path::PathBuf;

use i18n_harness_core::{Unit, UnitId};

use crate::backing::extract_for_format;
use crate::dto::{CatalogResponse, SaveAllDirtyResponse, SaveSummary, TargetEdit};
use crate::error;
use crate::state::{AppState, OpenCatalogEntry};

use super::util::build_catalog_response_backing;

/// Open a catalog that belongs to the currently-open project.
///
/// The catalog must be declared in the project manifest — either by its
/// manifest-relative path (e.g. `"translations/app_de.ts"`) or by its
/// resolved absolute path. The extracted units have their `review_status`
/// and `source_changed_since_review` fields populated by
/// `Project::apply_review_state` before they are returned. The catalog is
/// stashed in the per-project multi-catalog store keyed by absolute path;
/// subsequent edit/save commands use that key.
///
/// Errors if no project is open, or if `catalog_path` does not match any
/// declared catalog.
#[tauri::command]
pub(crate) fn open_catalog_in_project(
    catalog_path: String,
    state: tauri::State<'_, AppState>,
) -> Result<CatalogResponse, String> {
    open_catalog_in_project_impl(&catalog_path, &state)
}

/// State-only impl of [`open_catalog_in_project`] — same semantics, takes a
/// borrowed [`AppState`] so integration tests can drive the open + edit + save
/// flow without spinning up a real Tauri runtime.
#[doc(hidden)]
pub fn open_catalog_in_project_impl(
    catalog_path: &str,
    state: &AppState,
) -> Result<CatalogResponse, String> {
    let path = PathBuf::from(catalog_path);

    let project_guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = project_guard.as_ref().ok_or_else(error::no_project)?;

    let catalog_ref = project
        .catalog(&path)
        .ok_or_else(|| format!("catalog not in project: {catalog_path}"))?;
    let abs = PathBuf::from(&catalog_ref.absolute_path);
    let format = catalog_ref.format;

    let mut catalog = extract_for_format(&abs, format)?;
    project.apply_review_state(&abs, catalog.units_mut());

    let response = build_catalog_response_backing(&abs, &catalog);
    drop(project_guard);

    state
        .project_catalogs
        .lock()
        .map_err(error::lock_poisoned("project_catalogs"))?
        .insert(
            abs,
            OpenCatalogEntry {
                catalog,
                dirty: false,
            },
        );

    Ok(response)
}

/// Write a target edit into a catalog that is open in the project store.
///
/// Mirrors the state-transition rules of `update_unit_target`: promotes
/// `Untranslated → Proposed` on first text, demotes `Proposed/Finished →
/// Untranslated` when the target is fully cleared, and refuses edits on
/// `Vanished`/`Obsolete` units. Sets `dirty = true` on the entry on any
/// successful mutation.
///
/// Errors if the catalog is not in the project store or the unit is not
/// found / not writable.
#[tauri::command]
pub(crate) fn update_unit_target_in_project(
    catalog_path: String,
    unit_id: String,
    edit: TargetEdit,
    state: tauri::State<'_, AppState>,
) -> Result<Unit, String> {
    update_unit_target_in_project_impl(&catalog_path, &unit_id, edit, &state)
}

/// State-only impl of [`update_unit_target_in_project`] — same semantics, takes
/// a borrowed [`AppState`] so integration tests can drive the open + edit +
/// save flow without spinning up a real Tauri runtime.
#[doc(hidden)]
pub fn update_unit_target_in_project_impl(
    catalog_path: &str,
    unit_id: &str,
    edit: TargetEdit,
    state: &AppState,
) -> Result<Unit, String> {
    let abs = PathBuf::from(catalog_path);
    let mut store = state
        .project_catalogs
        .lock()
        .map_err(error::lock_poisoned("project_catalogs"))?;
    let entry = store
        .get_mut(&abs)
        .ok_or_else(error::no_catalog_in_project)?;
    let id = UnitId::from(unit_id.to_owned());
    let unit = entry
        .catalog
        .find_unit_mut(&id)
        .ok_or_else(|| format!("unit not found: {id}"))?;

    crate::services::unit_edit::apply_target_edit(unit, edit)?;
    let result = unit.clone();
    entry.dirty = true;
    Ok(result)
}

/// Write a single project-catalog back to disk using the byte-stable adapter.
///
/// Clears `dirty` on success. Errors if the catalog is not in the project
/// store.
#[tauri::command]
pub(crate) fn save_catalog_in_project(
    catalog_path: String,
    state: tauri::State<'_, AppState>,
) -> Result<SaveSummary, String> {
    let abs = PathBuf::from(&catalog_path);
    let mut store = state
        .project_catalogs
        .lock()
        .map_err(error::lock_poisoned("project_catalogs"))?;
    let entry = store
        .get_mut(&abs)
        .ok_or_else(error::no_catalog_in_project)?;
    let units = entry.catalog.units().to_vec();
    entry.catalog.apply(&units, &abs)?;
    entry.dirty = false;
    Ok(SaveSummary {
        path: abs.to_string_lossy().into_owned(),
        unit_count: units.len(),
    })
}

/// Write every dirty catalog in the project store back to disk.
///
/// Catalogs are written in BTreeMap iteration order (absolute path order),
/// which is deterministic. On the first apply failure the function stops and
/// returns the catalogs written so far plus `failed_path` / `failed_reason`
/// describing the failure. Successfully-written entries have their `dirty`
/// flag cleared regardless of whether a later entry failed.
#[tauri::command]
pub(crate) fn save_all_dirty(
    state: tauri::State<'_, AppState>,
) -> Result<SaveAllDirtyResponse, String> {
    save_all_dirty_impl(&state)
}

/// State-only impl of [`save_all_dirty`] — same semantics, takes a borrowed
/// [`AppState`] so integration tests can drive the open + edit + save flow
/// without spinning up a real Tauri runtime.
#[doc(hidden)]
pub fn save_all_dirty_impl(state: &AppState) -> Result<SaveAllDirtyResponse, String> {
    let mut store = state
        .project_catalogs
        .lock()
        .map_err(error::lock_poisoned("project_catalogs"))?;
    let mut saved: Vec<SaveSummary> = Vec::new();
    for (abs, entry) in store.iter_mut() {
        if !entry.dirty {
            continue;
        }
        let units = entry.catalog.units().to_vec();
        match entry.catalog.apply(&units, abs) {
            Ok(()) => {
                entry.dirty = false;
                saved.push(SaveSummary {
                    path: abs.to_string_lossy().into_owned(),
                    unit_count: units.len(),
                });
            }
            Err(e) => {
                return Ok(SaveAllDirtyResponse {
                    saved,
                    failed_path: Some(abs.to_string_lossy().into_owned()),
                    failed_reason: Some(e),
                });
            }
        }
    }
    Ok(SaveAllDirtyResponse {
        saved,
        failed_path: None,
        failed_reason: None,
    })
}

/// Re-read a catalog from disk and fold the current review state back in.
///
/// Replaces the in-memory entry in the project store and clears `dirty`.
/// Returns the same shape as `open_catalog_in_project`. Errors if the
/// catalog is not in the project store or if no project is open.
#[tauri::command]
pub(crate) fn discard_changes_in_project(
    catalog_path: String,
    state: tauri::State<'_, AppState>,
) -> Result<CatalogResponse, String> {
    let abs = PathBuf::from(&catalog_path);

    let project_guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = project_guard.as_ref().ok_or_else(error::no_project)?;

    // Confirm the catalog is in the store before doing I/O, and capture its
    // format so we dispatch to the right reader.
    let format = {
        let store = state
            .project_catalogs
            .lock()
            .map_err(error::lock_poisoned("project_catalogs"))?;
        if !store.contains_key(&abs) {
            return Err(error::no_catalog_in_project());
        }
        project
            .catalog(&abs)
            .ok_or_else(|| "catalog not registered in project".to_string())?
            .format
    };

    let mut catalog = extract_for_format(&abs, format)?;
    project.apply_review_state(&abs, catalog.units_mut());

    let response = build_catalog_response_backing(&abs, &catalog);
    drop(project_guard);

    state
        .project_catalogs
        .lock()
        .map_err(error::lock_poisoned("project_catalogs"))?
        .insert(
            abs,
            OpenCatalogEntry {
                catalog,
                dirty: false,
            },
        );

    Ok(response)
}

/// Return the absolute paths of all catalogs currently open in the project
/// store, in BTreeMap order (i.e. lexicographic absolute-path order).
///
/// The UI uses this to render per-catalog dirty-state pills. Returns an
/// empty vec when no catalogs have been opened via `open_catalog_in_project`.
#[tauri::command]
pub(crate) fn list_open_catalogs(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    let store = state
        .project_catalogs
        .lock()
        .map_err(error::lock_poisoned("project_catalogs"))?;
    Ok(store
        .keys()
        .map(|p| p.to_string_lossy().into_owned())
        .collect())
}

/// Return whether the named catalog has unsaved edits.
///
/// Returns `false` if the catalog is not in the project store (treat
/// unknown = clean from the UI's perspective).
#[tauri::command]
pub(crate) fn is_catalog_dirty(
    catalog_path: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    is_catalog_dirty_impl(&catalog_path, &state)
}

/// State-only impl of [`is_catalog_dirty`] — same semantics, takes a borrowed
/// [`AppState`] so integration tests can drive the open + edit + save flow
/// without spinning up a real Tauri runtime.
#[doc(hidden)]
pub fn is_catalog_dirty_impl(catalog_path: &str, state: &AppState) -> Result<bool, String> {
    let abs = PathBuf::from(catalog_path);
    let store = state
        .project_catalogs
        .lock()
        .map_err(error::lock_poisoned("project_catalogs"))?;
    Ok(store.get(&abs).is_some_and(|e| e.dirty))
}
