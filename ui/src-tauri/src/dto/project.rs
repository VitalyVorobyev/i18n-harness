//! Wire-format types for project open/scan commands.

use std::collections::BTreeMap;

use i18n_harness_project::ProjectSummary;
use serde::Serialize;

/// Wire response for `open_project` / `create_project`. Carries the summary
/// the UI binds against plus any non-fatal warnings (unknown locale ids,
/// glossary parse warnings). Hard failures come back as `Err(String)`.
#[derive(Debug, Serialize)]
pub struct ProjectOpenResponse {
    /// Compact project summary safe to send across the IPC bridge.
    pub summary: ProjectSummary,
    /// Human-readable warning strings (`UnknownLocale`, `Glossary(...)`).
    /// Empty when the project loads cleanly.
    pub warnings: Vec<String>,
}

/// One unit that requires human attention in the project-wide review queue.
///
/// A unit qualifies if `review_status == NeedsReview` OR `flags` is non-empty.
/// Both conditions are surfaced because flags are themselves a "human attention"
/// signal even before an explicit `NeedsReview` event has been recorded.
#[derive(Debug, Serialize)]
pub struct ReviewQueueItem {
    /// Absolute path to the catalog file on disk.
    pub catalog_path: String,
    /// Manifest-relative path for display in the table.
    pub catalog_manifest_path: String,
    /// Target locale id (e.g. `"de_DE"`).
    pub locale: String,
    /// The unit's id string.
    pub unit_id: String,
    /// Source text, truncated to 120 chars at a word boundary where possible.
    pub source_preview: String,
    /// Target text, truncated to 120 chars; empty string when untranslated.
    pub target_preview: String,
    /// Kebab-case flag names; empty when only `NeedsReview` triggered inclusion.
    pub flags: Vec<String>,
    /// Kebab-case `ReviewStatus` variant, or `None` when not set.
    pub review_status: Option<String>,
    /// Kebab-case unit state (`"untranslated"`, `"proposed"`, `"finished"`, …).
    pub state: String,
    /// Reviewer note from the unit's last review event, if any. Carries the
    /// JSON-encoded reference-conflict candidate list for `conflict` units so
    /// the Review conflict view survives a project reopen.
    pub reviewer_note: Option<String>,
}

/// Per-catalog unit-state tally, computed during the review-queue scan so the
/// UI can show progress numbers for every catalog (including unopened ones)
/// without a second extract pass.
#[derive(Debug, Default, Serialize)]
pub struct CatalogStateCounts {
    /// Every unit in the catalog, regardless of state.
    pub total: usize,
    /// `UnitState::Finished` units.
    pub finished: usize,
    /// `UnitState::Proposed` units.
    pub proposed: usize,
    /// `UnitState::Untranslated` units.
    pub untranslated: usize,
    /// `UnitState::Vanished` + `UnitState::Obsolete` units.
    pub vanished_obsolete: usize,
    /// Units flagged for human attention (`NeedsReview`/`Conflict` or any flag).
    pub needs_review: usize,
}

/// Aggregated result of a project-wide review-queue scan.
#[derive(Debug, Serialize)]
pub struct ReviewQueueResponse {
    /// Total units that need review across all catalogs.
    pub total_count: usize,
    /// Per-catalog needs-review unit count, keyed by absolute catalog path.
    pub by_catalog: BTreeMap<String, usize>,
    /// Per-catalog unit-state tally, keyed by absolute catalog path. Present
    /// for every scanned catalog, even fully-finished ones.
    pub stats_by_catalog: BTreeMap<String, CatalogStateCounts>,
    /// All items, sorted by catalog path then unit id.
    pub items: Vec<ReviewQueueItem>,
}
