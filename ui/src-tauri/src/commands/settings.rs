//! Commands for mutating project manifest settings (catalogs, locales, backend,
//! glossary, prompts).

use std::path::{Path, PathBuf};

use i18n_harness_project::{
    BackendConfig, CatalogEntry, CatalogFormat, GlossaryConfig, LocaleConfig, PromptsConfig,
};

use crate::dto::ProjectOpenResponse;
use crate::error;
use crate::state::AppState;

/// Guess a `CatalogFormat` from a file extension, or `None` for unsupported
/// extensions (the file is then ignored by the folder scan).
fn format_from_ext(path: &Path) -> Option<CatalogFormat> {
    match path
        .extension()
        .and_then(|e| e.to_str())?
        .to_ascii_lowercase()
        .as_str()
    {
        "ts" => Some(CatalogFormat::QtTs),
        "po" | "pot" => Some(CatalogFormat::GettextPo),
        "json" => Some(CatalogFormat::IcuJson),
        _ => None,
    }
}

/// Recursively collect catalog files (`.ts` / `.po` / `.json`) under `dir`.
/// Symlinks are not followed; unreadable subdirectories are skipped silently.
fn collect_catalog_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            collect_catalog_files(&path, out);
        } else if file_type.is_file() && format_from_ext(&path).is_some() {
            out.push(path);
        }
    }
}

/// Infer a locale id for `file_name` by matching each known locale id as a
/// `_<id>_` / `_<id>.` token in the name (e.g. `Acf_es_ES_s.ts` → `es_ES`).
/// Falls back to the first known locale id when no token matches.
fn infer_locale(file_name: &str, locale_ids: &[String]) -> Option<(String, bool)> {
    for id in locale_ids {
        if file_name.contains(&format!("_{id}_")) || file_name.contains(&format!("_{id}.")) {
            return Some((id.clone(), false));
        }
    }
    locale_ids.first().map(|id| (id.clone(), true))
}

/// Relativize `abs` against `root`; keep it absolute when not under root.
/// Mirrors the UI's `relativize` helper so manifest paths match either way.
fn relativize(abs: &Path, root: &Path) -> PathBuf {
    abs.strip_prefix(root)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| abs.to_path_buf())
}

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

/// Discover every `.ts` / `.po` / `.json` catalog under `folder` (recursively)
/// and add each new one to the project manifest, persisting **once** at the end.
///
/// Locale is inferred from each filename via a `_<locale>_` token; files with no
/// matching token fall back to the project's first locale (reported in
/// `warnings`). Files already present in the manifest are skipped. The returned
/// `warnings` summarise added / skipped / locale-defaulted files.
#[tauri::command]
pub(crate) fn add_catalogs_from_folder(
    folder: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectOpenResponse, String> {
    let mut guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = guard.as_mut().ok_or_else(error::no_project)?;

    let root = project.paths().root().to_path_buf();
    let locale_ids: Vec<String> = project.locale_ids().map(str::to_owned).collect();
    if locale_ids.is_empty() {
        return Err("project has no locales configured; add a locale first".to_string());
    }

    let mut files = Vec::new();
    collect_catalog_files(Path::new(&folder), &mut files);
    files.sort();

    let mut added = 0usize;
    let mut skipped = 0usize;
    let mut warnings: Vec<String> = Vec::new();

    for abs in files {
        let Some(format) = format_from_ext(&abs) else {
            continue;
        };
        let file_name = abs
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Some((locale, defaulted)) = infer_locale(&file_name, &locale_ids) else {
            continue;
        };
        let rel = relativize(&abs, &root);
        let entry = CatalogEntry {
            path: rel.clone(),
            format,
            locale: locale.clone(),
        };
        match project.add_catalog(entry) {
            Ok(()) => {
                added += 1;
                if defaulted {
                    warnings.push(format!(
                        "{}: no locale in filename, defaulted to {locale}",
                        rel.display()
                    ));
                }
            }
            Err(_) => {
                // Most commonly a duplicate path already in the manifest; treat
                // as a skip rather than aborting the whole batch.
                skipped += 1;
            }
        }
    }

    if added > 0 {
        project.save_manifest().map_err(|e| e.to_string())?;
    }

    warnings.insert(
        0,
        format!("Added {added} catalog(s); skipped {skipped} already-present."),
    );

    let summary = project.summary();
    Ok(ProjectOpenResponse { summary, warnings })
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
