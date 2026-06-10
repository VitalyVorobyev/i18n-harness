//! Wire-format types for the reference-reuse / remainder-split / merge commands.
//!
//! These mirror the report structs from `i18n-harness-reuse`, flattened for the
//! IPC bridge. Conflict candidates are carried explicitly so the React review
//! surface can render "3 references say X, 1 says Y" without re-reading any
//! reference catalog. The same candidate list is also persisted as the review
//! note for each conflicting unit so it survives a project reopen.

use serde::Serialize;

/// One reference's proposed translation for a conflicting unit, flattened for
/// the wire.
///
/// `text` is the candidate translation rendered for display. For singular units
/// it is the single string; for plural units the CLDR forms are joined with the
/// unit separator `\u{1F}` (matching `ConflictText::Plural`), so the React layer
/// can split it back into per-form rows.
#[derive(Debug, Clone, Serialize)]
pub struct ReferenceConflictCandidateDto {
    /// Absolute path of the reference catalog this candidate came from. When
    /// several references share the same text this names the first; the others
    /// are folded into `also_from`.
    pub reference: String,
    /// Other references that supplied this exact same translation. Empty when
    /// only one reference proposed it.
    pub also_from: Vec<String>,
    /// Whether the candidate is a singular or plural target — lets the UI decide
    /// whether to split `text` on the unit separator.
    pub is_plural: bool,
    /// The candidate translation. Plural forms are joined with `\u{1F}`.
    pub text: String,
}

/// A unit where two or more references supplied finished but differing
/// translations. The base unit is left untranslated; the candidates are
/// surfaced so a human can pick.
#[derive(Debug, Clone, Serialize)]
pub struct ReferenceConflictDto {
    /// The base unit id with disagreeing references.
    pub unit_id: String,
    /// The distinct candidate translations, in first-seen (declaration) order.
    pub candidates: Vec<ReferenceConflictCandidateDto>,
}

/// Wire response from `reuse_references_in_project`.
///
/// Counts mirror [`ReuseReport`](i18n_harness_reuse::ReuseReport); the conflict
/// candidate list is the full structured detail. `remaining_count` is the number
/// of writable units with no candidate — the set a subsequent split would carve
/// out. The catalog's in-memory entry has already been refreshed server-side, so
/// the caller should re-open / re-list the catalog to pull the post-reuse units.
#[derive(Debug, Serialize)]
pub struct ReuseReportDto {
    /// Absolute path of the base catalog the reuse was applied to.
    pub catalog_path: String,
    /// Absolute paths of the reference catalogs consulted, in priority order.
    pub references: Vec<String>,
    /// Units promoted to `Finished` (agreed candidate, gate-clean, complete).
    pub copied_finished: usize,
    /// Units that received an agreed candidate but were kept at `Proposed` for
    /// review (gate-flagged or incomplete target).
    pub copied_needs_review: usize,
    /// Units where references disagreed; nothing was copied and a human must
    /// pick. The detail is in `conflicts`.
    pub conflict_count: usize,
    /// Writable units with no candidate — feed a subsequent split.
    pub remaining_count: usize,
    /// The exact ids in the remaining set, in catalog order. Conflicted units
    /// are **excluded** (they are left for human resolution, not handed to
    /// translators). The caller passes these straight to `split_remainder` as
    /// `only_ids` so an Export Remainder right after a reuse pass carves out
    /// precisely the reuse leftovers — never the conflicts.
    pub remaining_ids: Vec<String>,
    /// Full per-unit conflict detail.
    pub conflicts: Vec<ReferenceConflictDto>,
}

/// Wire response from `split_remainder`.
#[derive(Debug, Serialize)]
pub struct SplitReportDto {
    /// Absolute path of the base catalog the subset was carved from.
    pub base_path: String,
    /// Absolute path the remainder subset was written to.
    pub out_path: String,
    /// Number of units written into the remainder.
    pub kept_count: usize,
}

/// Wire response from `merge_catalogs`.
///
/// `overlap_ids` / `stray_ids` are empty on success. On a merge guard failure
/// the command returns `Err(String)` carrying the human message *and* one of
/// these lists is surfaced via the error string — see the command docs. This
/// struct is the success shape only.
#[derive(Debug, Serialize)]
pub struct MergeReportDto {
    /// Absolute path of the base catalog the remainder was folded into.
    pub base_path: String,
    /// Absolute path of the translated remainder that was merged.
    pub with_path: String,
    /// Absolute path the merged result was written to.
    pub out_path: String,
    /// Remainder units folded into the base as override translations.
    pub merged: usize,
    /// Of `merged`, how many carried a complete (finished-ready) target.
    pub merged_complete: usize,
}
