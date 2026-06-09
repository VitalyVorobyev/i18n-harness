//! Report structs returned by the reuse and merge operations.
//!
//! These are the contract the CLI prints and the Tauri layer persists. They
//! are pure data — no catalog handles, no I/O. The conflict candidates in
//! particular live **only** here: they are deliberately *not* threaded onto
//! `Unit.flags` / `Unit.flag_notes`, because those are transient gate output
//! that does not round-trip into the `.ts` on a re-extract. The report is the
//! single source of truth for "which references disagreed, and on what text".

use std::path::PathBuf;

use i18n_harness_core::UnitId;

/// Outcome of reusing translations from one or more reference catalogs into a
/// base catalog.
///
/// # How to read it
///
/// Every **writable** base unit (state `Untranslated` or `Proposed`) lands in
/// exactly one of three buckets:
///
/// - It had ≥1 finished reference candidate and they all agreed → its id is in
///   [`Self::copied_finished`] or [`Self::copied_needs_review`], and a
///   [`CopiedUnit`] in [`Self::copied`] records the winning reference.
/// - It had ≥2 finished reference candidates that disagreed → a
///   [`ReferenceConflict`] in [`Self::conflicts`]. Its translation is **not**
///   copied; the unit is left untranslated for a human to decide.
/// - It had no finished reference candidate → its id is in
///   [`Self::remaining_ids`], the set that feeds the split step.
///
/// Non-writable base units (Finished/Vanished/Obsolete) appear in none of
/// these buckets — they are never touched.
///
/// # Disjointness invariant
///
/// `copied_finished`, `copied_needs_review`, the ids in `conflicts`, and
/// `remaining_ids` are pairwise disjoint, and their union is exactly the set
/// of writable base unit ids.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReuseReport {
    /// Units that received an agreed reference translation, passed the gate,
    /// and have a complete target — promoted to `Finished`.
    pub copied_finished: Vec<UnitId>,

    /// Units that received an agreed reference translation but were gate-flagged
    /// (or left with an incomplete target) — kept at `Proposed` so a human
    /// reviews them. The text is still copied in.
    pub copied_needs_review: Vec<UnitId>,

    /// Per-copied-unit provenance: which reference each copied translation came
    /// from. Parallel in spirit to `copied_finished ∪ copied_needs_review`;
    /// one entry per copied unit, in the order units were processed.
    pub copied: Vec<CopiedUnit>,

    /// Units where ≥2 references supplied *different* finished translations.
    /// Nothing is copied for these; the unit is left untranslated and a human
    /// (via the Tauri review store) picks the winner. Excluded from
    /// [`Self::remaining_ids`].
    pub conflicts: Vec<ReferenceConflict>,

    /// Writable base units with no finished reference candidate and no
    /// conflict — i.e. genuinely untranslated leftovers. This is the set the
    /// caller feeds to `adapter_qt::write_subset` to produce the split
    /// remainder.
    pub remaining_ids: Vec<UnitId>,
}

impl ReuseReport {
    /// Number of units promoted to `Finished` by reuse.
    pub fn copied_finished_count(&self) -> usize {
        self.copied_finished.len()
    }

    /// Number of units copied but kept at `Proposed` for review.
    pub fn copied_needs_review_count(&self) -> usize {
        self.copied_needs_review.len()
    }

    /// Number of conflicting units (left untranslated, awaiting a human pick).
    pub fn conflict_count(&self) -> usize {
        self.conflicts.len()
    }

    /// Number of leftover units with no candidate (feed the split).
    pub fn remaining_count(&self) -> usize {
        self.remaining_ids.len()
    }

    /// Total units that received a copied translation (finished + needs-review).
    pub fn copied_total(&self) -> usize {
        self.copied_finished.len() + self.copied_needs_review.len()
    }
}

/// Disposition of a single copied unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopiedDisposition {
    /// Copied, gate-clean, target complete → promoted to `Finished`.
    Finished,
    /// Copied but gate-flagged or target incomplete → kept at `Proposed`.
    NeedsReview,
}

/// Provenance for one copied unit: which reference won, and how the unit was
/// dispositioned after the gate ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopiedUnit {
    /// The base unit that received the translation.
    pub id: UnitId,
    /// The reference catalog the winning translation came from. When several
    /// references agree, this is the **first** one in declaration order, so
    /// provenance is deterministic.
    pub winning_reference: PathBuf,
    /// Whether the unit ended up `Finished` or `NeedsReview`.
    pub disposition: CopiedDisposition,
}

/// A unit where two or more references supplied finished but *different*
/// translations.
///
/// The base unit is left untranslated; the candidates are surfaced so a human
/// can pick. Stored in [`ReuseReport::conflicts`] only — never on the unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceConflict {
    /// The base unit id with disagreeing references.
    pub id: UnitId,
    /// The distinct candidate translations, in first-seen (declaration) order.
    /// At least two entries; equal candidates are collapsed so this lists only
    /// genuinely differing options.
    pub candidates: Vec<ConflictCandidate>,
}

/// One reference's proposed translation for a conflicting unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictCandidate {
    /// The reference catalog this candidate came from. When several references
    /// share the same text, this names the first one (the others are folded in
    /// via [`Self::also_from`]).
    pub reference: PathBuf,
    /// Other references that supplied this exact same translation. Empty when
    /// only one reference proposed it. Lets the UI show "3 references agree on
    /// X, 1 says Y" rather than flattening the vote.
    pub also_from: Vec<PathBuf>,
    /// The candidate translation, rendered for display. For singular units
    /// this is the single string; for plural units the CLDR forms are joined
    /// with the unit separator `\u{1F}` so equal/!equal comparison is exact
    /// and the UI can split it back.
    pub text: ConflictText,
}

/// The differing translation text of a conflict candidate.
///
/// Mirrors the singular/plural split of [`i18n_harness_core::Target`] so the
/// UI can render each form. Empty plural slots are represented as empty
/// strings (a finished reference target is always complete, so `None` slots
/// do not occur here).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictText {
    /// Single target string.
    Singular(String),
    /// One target per CLDR plural form, in canonical order.
    Plural(Vec<String>),
}

/// Outcome counts for a `merge_back`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeReport {
    /// Number of remainder units folded into the base as override translations.
    pub merged: usize,
    /// Of [`Self::merged`], how many carried a complete (finished-ready)
    /// target. The rest carried a partial or empty target and stay writable in
    /// the base after apply.
    pub merged_complete: usize,
}
