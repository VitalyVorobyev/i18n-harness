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
}

/// Aggregated result of a project-wide review-queue scan.
#[derive(Debug, Serialize)]
pub struct ReviewQueueResponse {
    /// Total units that need review across all catalogs.
    pub total_count: usize,
    /// Per-catalog unit count, keyed by absolute catalog path.
    pub by_catalog: BTreeMap<String, usize>,
    /// All items, sorted by catalog path then unit id.
    pub items: Vec<ReviewQueueItem>,
}
