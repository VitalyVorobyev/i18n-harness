//! Commands for opening, closing, and managing project manifests.

use std::path::{Path, PathBuf};

use i18n_harness_project::{CatalogRef, DraftManifest, Project, ProjectSummary};

use crate::dto::ProjectOpenResponse;
use crate::error;
use crate::state::AppState;

/// Open an existing project rooted at `root`.
///
/// Stashes the project in [`AppState`]; replaces any previously-open project
/// and clears the stand-alone catalog slot (the UI's old single-file session
/// is closed when a project takes over). As a side effect, if the project
/// declares a glossary, that glossary is also pinned in the glossary slot so
/// existing `translate_unit` calls benefit immediately.
#[tauri::command]
pub(crate) fn open_project(
    root: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectOpenResponse, String> {
    open_project_impl(Path::new(&root), &state)
}

/// State-only impl of [`open_project`] — same semantics, takes a borrowed
/// [`AppState`] so integration tests can drive the Tauri command surface
/// without spinning up a real Tauri runtime. The command wrapper above is
/// a one-liner that calls into this helper.
pub(crate) fn open_project_impl(
    root: &Path,
    state: &AppState,
) -> Result<ProjectOpenResponse, String> {
    let (project, warnings) = Project::open(root).map_err(|e| e.to_string())?;
    let summary = project.summary();
    let glossary_for_slot = project.glossary().cloned();

    {
        let mut current = state
            .project
            .lock()
            .map_err(error::lock_poisoned("project"))?;
        *current = Some(project);
    }
    {
        let mut g = state
            .glossary
            .lock()
            .map_err(error::lock_poisoned("glossary"))?;
        *g = glossary_for_slot;
    }
    {
        let mut c = state
            .catalog
            .lock()
            .map_err(error::lock_poisoned("catalog"))?;
        *c = None;
    }

    Ok(ProjectOpenResponse {
        summary,
        warnings: warnings.into_iter().map(|w| w.to_string()).collect(),
    })
}

/// Discover a project from a directory that has no manifest yet.
///
/// Returns the draft for the UI to confirm; never writes anything. The UI
/// follows up with `create_project` once the user has reviewed the draft.
#[tauri::command]
pub(crate) fn discover_project(root: String) -> Result<DraftManifest, String> {
    let root_path = PathBuf::from(&root);
    Project::discover(&root_path).map_err(|e| e.to_string())
}

/// Write `<root>/i18n-harness.toml` from `draft` and open the result.
///
/// Side effects mirror `open_project`: stashes the project, pre-populates the
/// glossary slot when declared, and clears the stand-alone catalog slot.
#[tauri::command]
pub(crate) fn create_project(
    root: String,
    draft: DraftManifest,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectOpenResponse, String> {
    let root_path = PathBuf::from(&root);
    let (project, warnings) =
        Project::create_from_draft(&root_path, draft).map_err(|e| e.to_string())?;
    let summary = project.summary();
    let glossary_for_slot = project.glossary().cloned();

    {
        let mut current = state
            .project
            .lock()
            .map_err(error::lock_poisoned("project"))?;
        *current = Some(project);
    }
    {
        let mut g = state
            .glossary
            .lock()
            .map_err(error::lock_poisoned("glossary"))?;
        *g = glossary_for_slot;
    }
    {
        let mut c = state
            .catalog
            .lock()
            .map_err(error::lock_poisoned("catalog"))?;
        *c = None;
    }

    Ok(ProjectOpenResponse {
        summary,
        warnings: warnings.into_iter().map(|w| w.to_string()).collect(),
    })
}

/// Drop the currently-open project. The stand-alone catalog slot, the
/// project-scoped multi-catalog store, and the glossary slot are also
/// cleared so the next "open file" starts from a clean slate.
///
/// Locks are acquired and released one at a time to avoid any lock-order
/// issue.
#[tauri::command]
pub(crate) fn close_project(state: tauri::State<'_, AppState>) -> Result<(), String> {
    *state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))? = None;
    *state
        .glossary
        .lock()
        .map_err(error::lock_poisoned("glossary"))? = None;
    *state
        .catalog
        .lock()
        .map_err(error::lock_poisoned("catalog"))? = None;
    state
        .project_catalogs
        .lock()
        .map_err(error::lock_poisoned("project_catalogs"))?
        .clear();
    Ok(())
}

/// Return the currently-open project's summary, or `None` if no project is
/// open. The UI calls this on launch to rehydrate (when persistence lands)
/// or to detect whether the home screen should be shown.
#[tauri::command]
pub(crate) fn current_project_summary(
    state: tauri::State<'_, AppState>,
) -> Result<Option<ProjectSummary>, String> {
    let current = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    Ok(current.as_ref().map(Project::summary))
}

/// List catalogs declared in the currently-open project.
///
/// Errors with `"no project open"` when the project slot is empty — the UI
/// should gate this command behind a successful `open_project`.
#[tauri::command]
pub(crate) fn list_catalogs(state: tauri::State<'_, AppState>) -> Result<Vec<CatalogRef>, String> {
    let current = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = current.as_ref().ok_or_else(error::no_project)?;
    Ok(project.catalogs().to_vec())
}

/// Persist the manifest's in-memory `toml_edit` document to disk.
///
/// Mutations applied through `Project::add_catalog`, `update_locale`,
/// `set_backend`, etc. update the document in memory; this command writes
/// the document atomically. Settings-tab edits in the UI will call this
/// after each batch of mutations.
#[tauri::command]
pub(crate) fn save_manifest(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let current = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = current.as_ref().ok_or_else(error::no_project)?;
    project.save_manifest().map_err(|e| e.to_string())
}
