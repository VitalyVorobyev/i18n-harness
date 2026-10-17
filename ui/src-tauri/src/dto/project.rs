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
/// A unit qualifies if its review status is `NeedsReview`/`Conflict`, it carries
/// a **hard** (un-shippable) flag, or it is still `Proposed`. A `Finished` unit
/// whose only flags are soft/semantic is *done* and is excluded — see
/// [`CatalogStateCounts`] for the rationale.
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
///
/// `needs_review` is the deduped headline ("items to review"): a unit counts
/// once if it has an explicit `NeedsReview`/`Conflict` review status, carries a
/// **hard** (un-shippable) flag, or is still `Proposed` (machine output not yet
/// confirmed). A `Finished` unit that only carries soft/semantic flags is
/// *done* and is excluded — that overlap was the source of the count that did
/// not reconcile with finished/proposed/left.
///
/// The `conflicts`, `hard_flagged`, and `proposed` fields are a display
/// breakdown of *why* units land in the headline. They may overlap (a single
/// unit can be both a conflict and hard-flagged), so they are not expected to
/// sum to `needs_review`; they exist so the UI can render
/// `N to review · C conflicts · F flagged · P proposed` and let the user see
/// the composition of the headline.
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
    /// Deduped headline: units that genuinely need human attention (see the
    /// type-level doc for the exact predicate).
    pub needs_review: usize,
    /// Units whose review status is `Conflict` (reference-reuse disagreement).
    /// Breakdown field — may overlap `needs_review` contributors.
    pub conflicts: usize,
    /// Units carrying at least one hard (un-shippable) flag. Breakdown field —
    /// may overlap `needs_review` contributors.
    pub hard_flagged: usize,
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
