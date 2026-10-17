//! Wire types for the on-demand catalog gate (inspector reasons + per-file
//! statistics).

use i18n_harness_gate::GateReport;
use serde::Serialize;

/// Result of gating every unit in an open catalog.
///
/// `reports` carries only the **non-clean** units (those with at least one
/// finding) so the UI can render the reason a unit was flagged in the
/// inspector; clean units are omitted to keep the payload small. `stats` is
/// the per-catalog breakdown the per-file statistics view renders.
#[derive(Debug, Clone, Serialize)]
pub struct CatalogGateResponse {
    /// Absolute path the catalog was opened from — the handle the UI keys on.
    pub path: String,
    /// Gate reports for the units that produced at least one finding, in
    /// document order. Each report carries its own `unit_id`.
    pub reports: Vec<GateReport>,
    /// Aggregate counts for the whole catalog.
    pub stats: CatalogGateStats,
}

/// Per-catalog counts: unit states plus how many units the gate flagged.
///
/// `hard` and `soft` are disjoint and counted per unit: a unit with any hard
/// finding lands in `hard`; a unit with only soft (or semantic) findings lands
/// in `soft`. `hard + soft` is therefore the number of flagged units, never
/// double-counted.
#[derive(Debug, Clone, Default, Serialize)]
pub struct CatalogGateStats {
    /// Every unit, including non-writable ones.
    pub total: usize,
    /// `UnitState::Finished`.
    pub finished: usize,
    /// `UnitState::Proposed` (has text, not signed off).
    pub proposed: usize,
    /// `UnitState::Untranslated` (writable, empty target).
    pub untranslated: usize,
    /// `UnitState::Vanished` + `UnitState::Obsolete` (never touched).
    pub vanished_obsolete: usize,
    /// Units with at least one hard finding.
    pub hard: usize,
    /// Units with findings but no hard finding (soft/semantic only).
    pub soft: usize,
}
