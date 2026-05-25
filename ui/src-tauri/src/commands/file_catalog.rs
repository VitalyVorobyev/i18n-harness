//! Commands for the stand-alone (non-project) file-catalog mode.

use std::path::PathBuf;

use i18n_harness_core::{Unit, UnitId};
use i18n_harness_locales::Locale;

use crate::commands::util::build_catalog_response;
use crate::dto::{CatalogResponse, SaveSummary, TargetEdit};
use crate::error;
use crate::state::{AppState, OpenCatalog};

/// Open a Qt `.ts` catalog at `path` and stash it in [`AppState`].
///
/// Returns the units for the frontend to render. The catalog itself
/// (including the byte buffer needed for byte-stable round-trip) is
/// kept server-side; the frontend identifies it by path on follow-up
/// commands. Opening a new catalog replaces the previously-open one.
#[tauri::command]
pub(crate) fn open_catalog(
    path: String,
    state: tauri::State<'_, AppState>,
) -> Result<CatalogResponse, String> {
    let abs = PathBuf::from(&path);
    let catalog =
        i18n_harness_adapter_qt::extract(&abs).map_err(|e| format!("extract failed: {e}"))?;
    let response = build_catalog_response(&abs, &catalog);
    let mut current = state
        .catalog
        .lock()
        .map_err(error::lock_poisoned("catalog"))?;
    *current = Some(OpenCatalog { path: abs, catalog });
    Ok(response)
}

/// Write a target into the currently-open catalog's in-memory unit.
///
/// Promotes a writable unit's state from `Untranslated` to `Proposed`
/// the first time any text is set. Refuses to touch `Vanished` /
/// `Obsolete` units (the harness must never modify those).
#[tauri::command]
pub(crate) fn update_unit_target(
    unit_id: String,
    edit: TargetEdit,
    state: tauri::State<'_, AppState>,
) -> Result<Unit, String> {
    let mut current = state
        .catalog
        .lock()
        .map_err(error::lock_poisoned("catalog"))?;
    let open = current.as_mut().ok_or_else(error::no_catalog)?;
    let id = UnitId::from(unit_id);
    let unit = open
        .catalog
        .find_unit_mut(&id)
        .ok_or_else(|| format!("unit not found: {id}"))?;
    crate::services::unit_edit::apply_target_edit(unit, edit)?;
    Ok(unit.clone())
}

/// Persist the in-memory catalog to disk via the byte-stable adapter.
///
/// If `out_path` is `None`, writes back to the path the catalog was
/// opened from. The original bytes (modulo edited unit bodies) come
/// through verbatim — the M0 contract.
#[tauri::command]
pub(crate) fn save_catalog(
    out_path: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<SaveSummary, String> {
    let current = state
        .catalog
        .lock()
        .map_err(error::lock_poisoned("catalog"))?;
    let open = current.as_ref().ok_or_else(error::no_catalog)?;
    let target = out_path
        .map(PathBuf::from)
        .unwrap_or_else(|| open.path.clone());
    let units = open.catalog.units().to_vec();
    i18n_harness_adapter_qt::apply(&open.catalog, &units, &target)
        .map_err(|e| format!("apply failed: {e}"))?;
    Ok(SaveSummary {
        path: target.to_string_lossy().into_owned(),
        unit_count: units.len(),
    })
}

/// Drop all in-memory edits and re-read the catalog from disk. The
/// UI's "Discard changes" / revert action.
#[tauri::command]
pub(crate) fn discard_changes(
    state: tauri::State<'_, AppState>,
) -> Result<CatalogResponse, String> {
    let mut current = state
        .catalog
        .lock()
        .map_err(error::lock_poisoned("catalog"))?;
    let open = current.as_mut().ok_or_else(error::no_catalog)?;
    let fresh =
        i18n_harness_adapter_qt::extract(&open.path).map_err(|e| format!("extract failed: {e}"))?;
    let response = build_catalog_response(&open.path, &fresh);
    open.catalog = fresh;
    Ok(response)
}

/// Translate one unit via the configured backend, then run the gate.
///
/// Available only when the crate is built with the `ollama` feature
/// (the default). Returns the merged unit + gate report; the UI uses
/// findings inline.
#[cfg(feature = "ollama")]
#[tauri::command]
pub(crate) fn translate_unit(
    unit_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<crate::dto::TranslateResult, String> {
    use i18n_harness_backend::{OllamaBackend, TranslationBackend};
    use i18n_harness_core::{Batch, BatchKey};

    let mut current = state
        .catalog
        .lock()
        .map_err(error::lock_poisoned("catalog"))?;
    let open = current.as_mut().ok_or_else(error::no_catalog)?;
    let language = open
        .catalog
        .language()
        .ok_or_else(|| "catalog has no <TS language=…>; cannot translate".to_string())?;
    let locale = Locale::by_id(language)
        .ok_or_else(|| format!("unknown locale `{language}`; add it to crates/locales"))?;
    let id = UnitId::from(unit_id);
    let original = open
        .catalog
        .find_unit_mut(&id)
        .ok_or_else(|| format!("unit not found: {id}"))?
        .clone();
    if !original.state.is_writable() {
        return Err(format!(
            "unit {id} is {state:?} — not translatable",
            state = original.state,
        ));
    }

    let batch = Batch::new(BatchKey::new("ui", 0), vec![original.clone()]);
    let backend =
        OllamaBackend::new().map_err(|e| format!("ollama backend construction failed: {e}"))?;
    let backend_name = backend.name().to_string();
    // Clone the glossary under the lock so we drop the guard before any
    // network call. Glossary owns small TOML-derived BTreeMaps; the
    // clone is cheap and avoids holding two locks at once.
    let glossary = state
        .glossary
        .lock()
        .map_err(error::lock_poisoned("glossary"))?
        .clone();
    let outcomes = backend
        .translate_batch(&batch, locale, glossary.as_ref())
        .map_err(|e| format!("backend `{backend_name}` failed: {e}"))?;
    let outcome = outcomes
        .into_iter()
        .next()
        .ok_or_else(|| format!("backend `{backend_name}` returned no outcomes"))?;

    let crate::services::translate::MergeOutcome { merged, report, .. } =
        crate::services::translate::merge_outcome(original, outcome, locale)?;

    // Persist the merged unit back into the catalog.
    if let Some(slot) = open.catalog.find_unit_mut(&id) {
        *slot = merged.clone();
    }

    Ok(crate::dto::TranslateResult {
        unit: merged,
        report,
    })
}
