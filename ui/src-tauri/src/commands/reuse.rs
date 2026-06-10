//! Reference-reuse / remainder-split / merge commands.
//!
//! These move *existing* translations between Qt catalogs by exact unit-id
//! match — no model, no network — so they run synchronously like a single-unit
//! edit rather than through the bulk-translate job machinery.
//!
//! Lock discipline mirrors `translate_unit_in_project`: clone the data needed
//! from the project under a brief lock, do the file work with **no locks held**,
//! then re-acquire brief locks to refresh the in-memory catalog and persist the
//! review-status events. The reuse/merge engine itself never sees `AppState`.

use std::collections::HashSet;
use std::path::PathBuf;

use i18n_harness_core::UnitId;
use i18n_harness_glossary::Glossary;
use i18n_harness_locales::Locale;
use i18n_harness_project::{CatalogFormat, ReferenceEntry, ReferenceRef};

use crate::backing::BackingCatalog;
use crate::dto::{MergeReportDto, ProjectOpenResponse, ReuseReportDto, SplitReportDto};
use crate::error;
use crate::services::reuse as svc;
use crate::state::{AppState, OpenCatalogEntry};

/// Resolve the base catalog's locale (a `&'static Locale`) and an owned glossary
/// clone for a reuse pass, holding the project lock only briefly.
///
/// Mirrors the locale-merge in `resolve_project_translate_context` but without
/// the backend resolution — reuse never calls a model. Available regardless of
/// the `ollama` feature.
fn resolve_reuse_context(
    state: &tauri::State<'_, AppState>,
    abs: &std::path::Path,
) -> Result<(&'static Locale, Option<Glossary>), String> {
    let project_guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = project_guard.as_ref().ok_or_else(error::no_project)?;

    let catalog_ref = project
        .catalog(abs)
        .ok_or_else(|| "catalog not open in project".to_string())?;
    let locale_id = catalog_ref.locale.clone();

    let locale = project
        .locale(&locale_id)
        .map(|r| r.workspace_locale())
        .or_else(|| Locale::by_id(&locale_id))
        .ok_or_else(|| format!("unknown locale `{locale_id}`; add it to crates/locales"))?;

    let glossary = project.glossary().cloned();
    Ok((locale, glossary))
}

/// Collect the absolute paths of all Qt references serving `locale_id`, in
/// manifest declaration order (which is reuse's priority order). Holds the
/// project lock only briefly. Returns the paths even for references whose
/// on-disk status is not `Ok`; reuse's own extract surfaces a bad reference as a
/// clear error.
///
/// Non-Qt references (e.g. a `gettext-po` entry for the same locale) are
/// filtered out: the reuse engine is Qt-only and would abort on a non-Qt file,
/// so passing only Qt references mirrors the CLI's selection.
fn references_for_locale(
    state: &tauri::State<'_, AppState>,
    locale_id: &str,
) -> Result<Vec<PathBuf>, String> {
    let project_guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = project_guard.as_ref().ok_or_else(error::no_project)?;
    Ok(project
        .references()
        .iter()
        .filter(|r: &&ReferenceRef| r.locale == locale_id && r.format == CatalogFormat::QtTs)
        .map(|r| PathBuf::from(&r.absolute_path))
        .collect())
}

/// Reuse expert translations from reference catalogs into a project catalog.
///
/// When `reference_paths` is `None` the manifest's `[[references]]` entries
/// matching the base catalog's locale are used (in declaration / priority
/// order); pass an explicit list to override that selection ad-hoc. The base
/// catalog's locale and the project glossary are threaded into the gate inside
/// the reuse pass.
///
/// On success the in-memory project-catalog entry for `catalog_path` is replaced
/// with the post-reuse catalog (so a subsequent `open_catalog_in_project` /
/// list returns the copied translations), and review-status events are persisted
/// for conflicts (`Conflict`, with the candidate list as the review note so it
/// survives a reopen) and copied-needs-review units (`NeedsReview`).
///
/// Available regardless of the `ollama` feature — reuse is local and
/// deterministic.
#[tauri::command]
pub(crate) fn reuse_references_in_project(
    catalog_path: String,
    reference_paths: Option<Vec<String>>,
    state: tauri::State<'_, AppState>,
) -> Result<ReuseReportDto, String> {
    let abs = PathBuf::from(&catalog_path);

    // 1. Resolve locale + glossary under a brief project lock.
    let (locale, glossary) = resolve_reuse_context(&state, &abs)?;

    // 2. Resolve reference paths: explicit override, or the manifest's
    //    references for this locale.
    let reference_abs: Vec<PathBuf> = match reference_paths {
        Some(paths) => paths.into_iter().map(PathBuf::from).collect(),
        None => references_for_locale(&state, locale.id)?,
    };
    if reference_abs.is_empty() {
        return Err(format!(
            "no references available for locale `{}`; declare some via add_project_reference \
             or pass reference_paths explicitly",
            locale.id,
        ));
    }

    // 3. Run the reuse pass + write-back with NO locks held.
    let result = svc::reuse_into_catalog(&abs, &reference_abs, locale, glossary.as_ref())?;
    let mut catalog = result.catalog;

    // 4. Fold current review state onto the re-extracted units (a brief project
    //    lock, then dropped), then refresh the in-memory catalog entry and pull
    //    the source hash of every review-write target while we hold the catalog
    //    lock. The two locks are taken in series, never nested, matching the
    //    project_catalogs-then-project discipline of `translate_one`.
    {
        let project_guard = state
            .project
            .lock()
            .map_err(error::lock_poisoned("project"))?;
        if let Some(project) = project_guard.as_ref() {
            project.apply_review_state(&abs, catalog.units_mut());
        }
    }

    // Collect (unit_id, status, note, source_hash) under the catalog lock; the
    // reuse engine is Qt-only so the refreshed entry is always `BackingCatalog::Qt`.
    let durable_writes: Vec<(UnitId, ReviewStatusTuple)> = {
        let mut store = state
            .project_catalogs
            .lock()
            .map_err(error::lock_poisoned("project_catalogs"))?;

        let hashes: Vec<(UnitId, String)> = result
            .review_writes
            .iter()
            .map(|w| {
                let hash = catalog
                    .units()
                    .iter()
                    .find(|u| u.id == w.unit_id)
                    .and_then(|u| u.source_hash.clone())
                    .unwrap_or_default();
                (w.unit_id.clone(), hash)
            })
            .collect();

        store.insert(
            abs.clone(),
            OpenCatalogEntry {
                catalog: BackingCatalog::Qt(catalog),
                dirty: false,
            },
        );

        result
            .review_writes
            .iter()
            .zip(hashes)
            .map(|(w, (_, hash))| {
                (
                    w.unit_id.clone(),
                    ReviewStatusTuple {
                        status: w.status,
                        note: w.note.clone(),
                        source_hash: hash,
                    },
                )
            })
            .collect()
    };

    // 5. Persist review-status events (conflicts + copied-needs-review) under
    //    the project lock only. The note for a conflict is the JSON candidate
    //    list so a reopen can rebuild the conflict view from review.jsonl.
    {
        let project_guard = state
            .project
            .lock()
            .map_err(error::lock_poisoned("project"))?;
        if let Some(project) = project_guard.as_ref() {
            for (unit_id, write) in &durable_writes {
                project
                    .set_review_status(
                        &abs,
                        unit_id,
                        Some(write.status),
                        write.source_hash.clone(),
                        write.note.clone(),
                    )
                    .map_err(|e| format!("set_review_status failed: {e}"))?;
            }
        }
    }

    Ok(result.report)
}

/// A resolved review-status write: the status, the reviewer note, and the
/// source hash pinned at reuse time. Built under the catalog lock so step 5 can
/// persist it with only the project lock held.
struct ReviewStatusTuple {
    status: i18n_harness_core::ReviewStatus,
    note: Option<String>,
    source_hash: String,
}

/// Carve a remainder subset of `catalog_path` into `out_path`, keeping only the
/// untranslated leftovers.
///
/// When `only_ids` is `Some` those exact ids are kept (the reuse→split flow
/// passes the reuse report's `remaining_count` ids here). When `None` the
/// writable-untranslated set is computed from the catalog (standalone split).
///
/// Reads the catalog from disk via the Qt adapter; does not touch the in-memory
/// store. Available regardless of the `ollama` feature.
#[tauri::command]
pub(crate) fn split_remainder(
    catalog_path: String,
    out_path: String,
    only_ids: Option<Vec<String>>,
    _state: tauri::State<'_, AppState>,
) -> Result<SplitReportDto, String> {
    let abs = PathBuf::from(&catalog_path);
    let out_abs = PathBuf::from(&out_path);

    let keep_ids: HashSet<UnitId> = match only_ids {
        Some(ids) => ids.into_iter().map(UnitId::from).collect(),
        None => svc::standalone_remainder_ids(&abs)?,
    };

    svc::split_remainder(&abs, &keep_ids, &out_abs)
}

/// Merge a translated remainder back into its base, writing the result to
/// `out_path`.
///
/// Overlap / stray-id guard failures from the merge engine come back as
/// `Err(String)` carrying the offending ids verbatim, so the frontend can show
/// which units broke the merge rather than a bare "merge failed".
///
/// Reads both catalogs from disk via the Qt adapter; does not touch the
/// in-memory store. Available regardless of the `ollama` feature.
#[tauri::command]
pub(crate) fn merge_catalogs(
    base_path: String,
    with_path: String,
    out_path: String,
    _state: tauri::State<'_, AppState>,
) -> Result<MergeReportDto, String> {
    let base_abs = PathBuf::from(&base_path);
    let with_abs = PathBuf::from(&with_path);
    let out_abs = PathBuf::from(&out_path);

    svc::merge_catalogs(&base_abs, &with_abs, &out_abs)
}

/// Declare a reference catalog in the project manifest and persist it.
///
/// Mirrors `add_catalog_to_project`: appends a `[[references]]` entry and
/// re-saves the manifest, returning a fresh `ProjectOpenResponse` so the UI can
/// re-render. Errors if the path already exists in the references list or the
/// file is not found on disk.
#[tauri::command]
pub(crate) fn add_project_reference(
    entry: ReferenceEntry,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectOpenResponse, String> {
    let mut guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = guard.as_mut().ok_or_else(error::no_project)?;
    project.add_reference(entry).map_err(|e| e.to_string())?;
    project.save_manifest().map_err(|e| e.to_string())?;
    let summary = project.summary();
    Ok(ProjectOpenResponse {
        summary,
        warnings: vec![],
    })
}

/// Remove the reference at `path` (manifest-relative) from the project manifest
/// and persist it.
///
/// Idempotent — succeeds whether or not an entry matched. Returns a fresh
/// `ProjectOpenResponse`.
#[tauri::command]
pub(crate) fn remove_project_reference(
    path: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectOpenResponse, String> {
    let manifest_path = PathBuf::from(&path);
    let mut guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = guard.as_mut().ok_or_else(error::no_project)?;
    project
        .remove_reference(&manifest_path)
        .map_err(|e| e.to_string())?;
    project.save_manifest().map_err(|e| e.to_string())?;
    let summary = project.summary();
    Ok(ProjectOpenResponse {
        summary,
        warnings: vec![],
    })
}
