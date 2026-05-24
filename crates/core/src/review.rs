//! Reviewer-process state for translation units.
//!
//! [`ReviewStatus`] is orthogonal to [`crate::UnitState`]: `UnitState` mirrors
//! the catalog's structural vocabulary (finished, unfinished, vanished, …)
//! while `ReviewStatus` records whether a human reviewer has accepted or
//! rejected a translation proposal. The two enums evolve independently and are
//! stored in separate places — `UnitState` in the catalog file, `ReviewStatus`
//! in the project's gitignored `review.jsonl`.

use serde::{Deserialize, Serialize};

/// Reviewer-process state of a translation unit.
///
/// Orthogonal to [`crate::UnitState`] — a unit can be `(UnitState::Proposed,
/// ReviewStatus::Approved)` (a confirmed translation that the catalog
/// still marks `type="unfinished"` because the team has not yet decided
/// to graduate it to `Finished`).
///
/// Variants are listed in the rough order a unit progresses through a
/// translator's pipeline, but transitions are *not* a strict linear
/// state machine: a reviewer can move a unit from `Reviewed` back to
/// `NeedsReview` by clicking "request another look", or jump from
/// `MachineTranslated` directly to `Approved` on accept-as-is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewStatus {
    /// No machine or human work has touched this unit yet.
    ///
    /// Set when a freshly-extracted unit is first persisted to the review
    /// store, e.g. on first manual save of a new catalog. Not the default
    /// for an un-touched unit — the *absence* of a record in `review.jsonl`
    /// is the default; `New` is set when something has affirmatively claimed
    /// the unit (a translator opened it, gave it a flag, etc.) but no MT or
    /// review has run.
    New,

    /// A machine translation backend produced a target.
    ///
    /// Set when `Project::set_review_status` is called with
    /// `MachineTranslated` at the end of a successful `translate_unit` /
    /// `translate_batch` when the gate is clean and no `Flag` triggered a
    /// review request. The bulk-translate flow in M4.8 lands units here.
    MachineTranslated,

    /// A machine translation produced flags, or a translator/reviewer
    /// explicitly requested a second look.
    ///
    /// Set when the gate or backend produced any [`crate::Flag`] of severity
    /// Hard or Warn on the unit's MT proposal (M4.6), or the UI invokes
    /// `Project::set_review_status(..., NeedsReview)` explicitly.
    NeedsReview,

    /// A human reviewer looked at the unit and accepted it without declaring
    /// it final.
    ///
    /// Set when the UI's "Accept" action runs on a flagged unit.
    Reviewed,

    /// A human declared this translation final for the current source.
    ///
    /// Set when the UI's "Save All" action persists a unit whose gate is clean
    /// and whose review state is `Reviewed` or `MachineTranslated`. Cleared
    /// (back to `NeedsReview`) by a source-hash mismatch — see the design
    /// doc §6 for the exact invalidation rule.
    Approved,

    /// `Approved` plus "do not re-translate by machine".
    ///
    /// Set when `Project::set_review_status(..., Locked)` is called explicitly
    /// from the UI. Bulk-translate (M4.8) skips locked units; `translate_unit`
    /// on a locked unit returns the existing target unchanged. Treated as
    /// `Approved` everywhere else.
    Locked,

    /// A human looked at the unit and concluded the MT proposal is wrong but
    /// did not supply a replacement.
    ///
    /// Set when the UI's "Reject" action runs.
    Rejected,

    /// Two or more reviewers gave the same unit contradictory edits since the
    /// last save.
    ///
    /// Reserved for M4.x multi-reviewer extensions; for v1 the variant exists
    /// in the enum but is never set automatically — leaving it in the API
    /// surface now prevents a breaking change later.
    Conflict,
}
