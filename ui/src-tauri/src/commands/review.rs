//! Commands for review-status management (set, accept, scan).

use std::path::PathBuf;

use i18n_harness_core::{Unit, UnitId, UnitState};

use crate::dto::{ReviewQueueResponse, ReviewStatusInput};
use crate::error;
use crate::state::AppState;

/// Record a review-status change for one unit.
///
/// Appends an event to `review.jsonl` via the project's store. As a side
/// effect, the in-memory unit (if the catalog is currently open in the project
/// store) has its `review_status` field set immediately so the UI reflects the
/// change without re-opening the catalog.
///
/// Review state lives in `review.jsonl`, not in the catalog file, so the
/// catalog's `dirty` flag is not set.
#[tauri::command]
pub(crate) fn set_review_status_in_project(
    catalog_path: String,
    unit_id: String,
    input: ReviewStatusInput,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let abs = PathBuf::from(&catalog_path);

    {
        let project_guard = state
            .project
            .lock()
            .map_err(error::lock_poisoned("project"))?;
        let project = project_guard.as_ref().ok_or_else(error::no_project)?;
        let uid = UnitId::from(unit_id.clone());
        project
            .set_review_status(
                &abs,
                &uid,
                input.status,
                input.source_hash_at_review,
                input.reviewer_note,
            )
            .map_err(|e| e.to_string())?;
    }

    // Update the in-memory unit if the catalog is open; silently skip if not.
    if let Ok(mut store) = state.project_catalogs.lock() {
        if let Some(entry) = store.get_mut(&abs) {
            let uid = UnitId::from(unit_id);
            if let Some(unit) = entry.catalog.find_unit_mut(&uid) {
                unit.review_status = input.status;
            }
        }
    }

    Ok(())
}

/// Accept a unit as reviewed: clear its flags and flag notes, then append a
/// `Reviewed` event to `review.jsonl`.
///
/// The unit's `state` is left unchanged (per M4.3a.1: the human controls the
/// `Proposed → Finished` transition via Save/accept flows). The catalog is
/// marked dirty because clearing flags is a meaningful edit that the next Save
/// will persist.
///
/// Lock ordering: acquire `project_catalogs` first for the mutation, drop it,
/// then acquire `project` for the durable review write. This mirrors the
/// pattern established in `translate_unit_in_project`.
#[tauri::command]
pub(crate) fn accept_unit_in_project(
    catalog_path: String,
    unit_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Unit, String> {
    accept_unit_in_project_impl(&catalog_path, &unit_id, &state)
}

/// Integration-test surface — same logic as [`accept_unit_in_project`] but
/// takes `&AppState` directly so tests can call it without a Tauri runtime.
#[doc(hidden)]
pub fn accept_unit_in_project_impl(
    catalog_path: &str,
    unit_id: &str,
    state: &AppState,
) -> Result<Unit, String> {
    use i18n_harness_core::{FlagSet, ReviewStatus};
    use std::collections::BTreeMap;

    let abs = PathBuf::from(catalog_path);
    let uid = UnitId::from(unit_id);

    // Read the unit's source_hash without mutating, so we can do the durable
    // write first. Codex P2: clearing flags before the fallible review.jsonl
    // append would leave the in-memory state mutated on append failure, and
    // a later Save would persist a cleared-flags state with no matching
    // review event.
    let source_hash = {
        let store = state
            .project_catalogs
            .lock()
            .map_err(error::lock_poisoned("project_catalogs"))?;
        let entry = store
            .get(&abs)
            .ok_or_else(|| "catalog not open in project".to_string())?;
        let unit = entry
            .catalog
            .units()
            .iter()
            .find(|u| u.id == uid)
            .ok_or_else(|| format!("unit not found: {unit_id}"))?;
        unit.source_hash.clone().unwrap_or_default()
    };
    // project_catalogs lock is now dropped.

    // Durable write first. If this fails, the in-memory state is untouched
    // and the caller can retry safely.
    {
        let project_guard = state
            .project
            .lock()
            .map_err(error::lock_poisoned("project"))?;
        let project = project_guard.as_ref().ok_or_else(error::no_project)?;
        project
            .set_review_status(&abs, &uid, Some(ReviewStatus::Reviewed), source_hash, None)
            .map_err(|e| format!("set_review_status failed: {e}"))?;
    }

    // Durable write succeeded — now mutate the in-memory unit and return it.
    let mut store = state
        .project_catalogs
        .lock()
        .map_err(error::lock_poisoned("project_catalogs"))?;
    let entry = store
        .get_mut(&abs)
        .ok_or_else(|| "catalog not open in project".to_string())?;
    let unit = entry
        .catalog
        .find_unit_mut(&uid)
        .ok_or_else(|| format!("unit not found: {unit_id}"))?;

    unit.flags = FlagSet::new();
    unit.flag_notes = BTreeMap::new();
    unit.review_status = Some(ReviewStatus::Reviewed);
    // Accept also transitions Proposed → Finished. The HANDOFF redesign
    // treats Accept as the manual finish action; the gate's hard-flag guard
    // (caller-side) already prevents accepting an unsound unit. Other states
    // are left alone (Finished stays Finished; Untranslated stays
    // Untranslated, though the caller should not invoke this in that case).
    if matches!(unit.state, UnitState::Proposed) {
        unit.state = UnitState::Finished;
    }
    let merged = unit.clone();
    entry.dirty = true;

    Ok(merged)
}

/// Scan every catalog in the open project for units that need human review.
///
/// A unit qualifies if:
/// - `unit.review_status == NeedsReview`, OR
/// - `unit.flags` is non-empty (any flag — gate-produced or model-supplied).
///
/// Catalogs that have already been opened in `project_catalogs` are scanned
/// from the in-memory store. Catalogs that are NOT yet open are extracted via
/// the Qt adapter and folded with `Project::apply_review_state`, then cached
/// into the store with `dirty: false` — the same side-effect as
/// `open_catalog_in_project`.
///
/// Only `qt-ts` catalogs are supported today. Non-`qt-ts` catalogs are
/// skipped with a `tracing::warn!` and will be wired in M4.4/M4.5.
#[tauri::command]
pub(crate) fn scan_project_review_state(
    state: tauri::State<'_, AppState>,
) -> Result<ReviewQueueResponse, String> {
    crate::services::review_scan::collect(&state)
}
