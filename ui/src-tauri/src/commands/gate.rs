//! On-demand validation gate over a whole open catalog.
//!
//! The gate is deterministic and cheap, so the UI re-runs it on catalog open
//! rather than persisting findings. This surfaces *why* a unit was flagged in
//! the inspector — including units flagged by reference-reuse or carried in
//! from `review.jsonl`, which never went through a translate run — and feeds
//! the per-file statistics view. Pure read: nothing here mutates state or
//! touches disk beyond the already-open catalog in memory.

use std::path::{Path, PathBuf};

use i18n_harness_core::{Unit, UnitState};
use i18n_harness_gate::{GateReport, validate};
use i18n_harness_glossary::Glossary;
use i18n_harness_locales::Locale;

use crate::dto::{CatalogGateResponse, CatalogGateStats};
use crate::error;
use crate::state::AppState;

/// Gate the currently-open stand-alone (non-project) catalog.
#[tauri::command]
pub(crate) fn gate_catalog(
    state: tauri::State<'_, AppState>,
) -> Result<CatalogGateResponse, String> {
    let glossary = state
        .glossary
        .lock()
        .map_err(error::lock_poisoned("glossary"))?
        .clone();

    let current = state
        .catalog
        .lock()
        .map_err(error::lock_poisoned("catalog"))?;
    let open = current.as_ref().ok_or_else(error::no_catalog)?;
    let language = open
        .catalog
        .language()
        .ok_or_else(|| "catalog has no <TS language=…>; cannot gate".to_string())?;
    let locale = Locale::by_id(language)
        .ok_or_else(|| format!("unknown locale `{language}`; add it to crates/locales"))?;

    let (reports, stats) = gate_units(open.catalog.units(), locale, glossary.as_ref());
    Ok(CatalogGateResponse {
        path: open.path.to_string_lossy().into_owned(),
        reports,
        stats,
    })
}

/// Gate a catalog open in the current project's per-catalog store.
#[tauri::command]
pub(crate) fn gate_catalog_in_project(
    catalog_path: String,
    state: tauri::State<'_, AppState>,
) -> Result<CatalogGateResponse, String> {
    let abs = PathBuf::from(&catalog_path);
    // Resolve locale + glossary under the project lock, then drop it before
    // touching the catalog store to avoid holding two locks at once.
    let (locale, glossary) = resolve_project_gate_context(&state, &abs)?;

    let store = state
        .project_catalogs
        .lock()
        .map_err(error::lock_poisoned("project_catalogs"))?;
    let entry = store.get(&abs).ok_or_else(error::no_catalog_in_project)?;

    let (reports, stats) = gate_units(entry.catalog.units(), locale, glossary.as_ref());
    Ok(CatalogGateResponse {
        path: abs.to_string_lossy().into_owned(),
        reports,
        stats,
    })
}

/// Resolve the locale + glossary for a project catalog, independent of any
/// translation backend (the gate never needs one). Mirrors the locale merge
/// in the translate path but does not require a configured Ollama backend, so
/// a project using a cloud or agent backend can still be gated.
fn resolve_project_gate_context(
    state: &tauri::State<'_, AppState>,
    abs: &Path,
) -> Result<(&'static Locale, Option<Glossary>), String> {
    let project_guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = project_guard.as_ref().ok_or_else(error::no_project)?;

    let catalog_ref = project
        .catalog(abs)
        .ok_or_else(|| "catalog not open in project".to_string())?;
    let locale_id = &catalog_ref.locale;
    let locale = project
        .locale(locale_id)
        .map(|r| r.workspace_locale())
        .or_else(|| Locale::by_id(locale_id))
        .ok_or_else(|| format!("unknown locale `{locale_id}`; add it to crates/locales"))?;
    let glossary = project.glossary().cloned();
    Ok((locale, glossary))
}

/// Run the gate over every unit, collecting non-clean reports and the
/// per-catalog state/severity breakdown in one pass.
fn gate_units(
    units: &[Unit],
    locale: &Locale,
    glossary: Option<&Glossary>,
) -> (Vec<GateReport>, CatalogGateStats) {
    let mut reports = Vec::new();
    let mut stats = CatalogGateStats {
        total: units.len(),
        ..Default::default()
    };
    for unit in units {
        match unit.state {
            UnitState::Finished => stats.finished += 1,
            UnitState::Proposed => stats.proposed += 1,
            UnitState::Untranslated => stats.untranslated += 1,
            UnitState::Vanished | UnitState::Obsolete => stats.vanished_obsolete += 1,
        }
        let report = validate(unit, locale, glossary);
        if report.is_clean() {
            continue;
        }
        if report.has_hard() {
            stats.hard += 1;
        } else {
            stats.soft += 1;
        }
        reports.push(report);
    }
    (reports, stats)
}
