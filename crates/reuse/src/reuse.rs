//! Reuse: copy expert translations from reference catalogs into a base.
//!
//! # The per-unit contract (the load-bearing rule)
//!
//! For each **writable** base unit `U` (state `Untranslated` or `Proposed`):
//!
//! 1. **Candidates** = reference units sharing `U.id`, with `state ==
//!    Finished` and a complete target. Because the id is the natural key of
//!    the source for an adapter (Qt: `<context>::<source>`), an exact-id match
//!    means the source — and therefore the placeholder multiset — is identical
//!    by construction.
//! 2. **0 candidates** → `U` is left unchanged and its id goes to
//!    `remaining_ids` (it feeds the split step).
//! 3. **≥1 candidate, all targets EQUAL** → copy the translation into `U`,
//!    then run the gate on `U`. If the gate is clean **and** the target is
//!    complete, promote `U` to `Finished` (`copied_finished`); otherwise keep
//!    the copied text but leave `U` at `Proposed` (`copied_needs_review`).
//!    Provenance records the first reference (declaration order) that carried
//!    the winning text.
//! 4. **≥2 candidates that DIFFER** → conflict. Do **not** copy. Leave `U`
//!    untranslated. Record a [`ReferenceConflict`] listing each distinct
//!    candidate text and which references voted for it. Conflicted ids are
//!    **excluded** from `remaining_ids`.
//!
//! Agreement, not order, decides whether a copy happens. Declaration order
//! only makes provenance deterministic when references agree. Non-writable
//! base units (`Finished` / `Vanished` / `Obsolete`) are never touched.

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use i18n_harness_adapter_qt::{Catalog, extract};
use i18n_harness_core::{Target, Unit, UnitId, UnitState};
use i18n_harness_gate::validate;
use i18n_harness_glossary::Glossary;
use i18n_harness_locales::Locale;

use crate::error::ReuseError;
use crate::report::{
    ConflictCandidate, ConflictText, CopiedDisposition, CopiedUnit, ReferenceConflict, ReuseReport,
};

/// Result of [`reuse_from_references`].
///
/// `base` is the extracted base catalog; `units` are the merged override units
/// the caller passes straight to `adapter_qt::apply(&base, &units, out)`. The
/// `units` vector contains **every** base unit (writable and non-writable) so
/// the apply call is a complete picture; only writable units that received an
/// agreed translation differ from the on-disk truth. `report` describes what
/// happened.
#[derive(Debug)]
pub struct ReuseOutcome {
    /// The extracted base catalog. Hand it to `adapter_qt::apply` together with
    /// [`Self::units`].
    pub base: Catalog,
    /// The merged unit set: copied/promoted writable units plus every other
    /// base unit unchanged. Apply-ready.
    pub units: Vec<Unit>,
    /// Structured description of the reuse pass.
    pub report: ReuseReport,
}

/// The set of writable base unit ids that are not complete — i.e. genuinely
/// untranslated leftovers that a split step would carve out.
///
/// "Writable" means state `Untranslated` or `Proposed` (see
/// [`UnitState::is_writable`]); "untranslated" here means the target is **not**
/// complete. A `Proposed` unit whose target is already complete (a draft a
/// human has typed but not finalized) is *not* returned — it has content, it
/// just is not signed off, and the split step is about routing *empty* work to
/// a translator.
///
/// This is the standalone entry point for splitting a catalog with no reuse
/// pass in front of it. The reuse→split flow uses
/// [`ReuseReport::remaining_ids`] directly instead, because that set already
/// excludes units a reference could fill.
pub fn writable_untranslated_ids(catalog: &Catalog) -> HashSet<UnitId> {
    catalog
        .units()
        .iter()
        .filter(|u| u.state.is_writable() && !u.target.is_complete())
        .map(|u| u.id.clone())
        .collect()
}

/// Copy expert translations from `reference_paths` into the catalog at
/// `base_path`.
///
/// `reference_paths` are in declaration / priority order; that order only
/// affects provenance and conflict-candidate ordering, never *whether* a copy
/// happens (agreement does that). `glossary` is threaded into the gate so the
/// same glossary-aware checks the rest of the pipeline runs apply here too.
///
/// # Errors
///
/// Returns [`ReuseError::Extract`] if the base or any reference cannot be
/// extracted. A reference that fails to extract aborts the whole pass — a
/// half-applied reuse is never persisted, and the caller can fix the bad
/// reference and retry deterministically.
pub fn reuse_from_references(
    base_path: &Path,
    reference_paths: &[PathBuf],
    locale: &Locale,
    glossary: Option<&Glossary>,
) -> Result<ReuseOutcome, ReuseError> {
    let base = extract(base_path).map_err(|source| ReuseError::Extract {
        path: base_path.to_path_buf(),
        source,
    })?;

    // Extract every reference up front; build a per-id index of finished,
    // complete candidate targets paired with the reference they came from.
    // Indexing once is O(refs · units); the per-base-unit lookup is then O(1).
    let mut candidates: HashMap<UnitId, Vec<Candidate>> = HashMap::new();
    for ref_path in reference_paths {
        let ref_catalog = extract(ref_path).map_err(|source| ReuseError::Extract {
            path: ref_path.to_path_buf(),
            source,
        })?;
        for unit in ref_catalog.units() {
            if unit.state == UnitState::Finished && unit.target.is_complete() {
                candidates
                    .entry(unit.id.clone())
                    .or_default()
                    .push(Candidate {
                        reference: ref_path.clone(),
                        target: unit.target.clone(),
                    });
            }
        }
    }

    let mut report = ReuseReport::default();
    let mut units: Vec<Unit> = base.units().to_vec();

    for unit in &mut units {
        if !unit.state.is_writable() {
            continue;
        }
        let Some(cands) = candidates.get(&unit.id) else {
            // Rule 2: no candidate — leftover for the split.
            report.remaining_ids.push(unit.id.clone());
            continue;
        };
        debug_assert!(
            !cands.is_empty(),
            "candidate index never stores an empty vec",
        );

        match classify(cands) {
            Agreement::Unanimous { target, reference } => {
                apply_copy(unit, target, reference, locale, glossary, &mut report);
            }
            Agreement::Conflict(conflict) => {
                // Rule 4: disagreeing references — do not copy, do not add to
                // remaining_ids. The unit stays untranslated; a human decides.
                if let Some(status) = needs_review_status() {
                    unit.review_status = Some(status);
                }
                report.conflicts.push(ReferenceConflict {
                    id: unit.id.clone(),
                    candidates: conflict,
                });
            }
        }
    }

    Ok(ReuseOutcome {
        base,
        units,
        report,
    })
}

/// One finished reference target for a given id.
struct Candidate {
    reference: PathBuf,
    target: Target,
}

/// Whether the candidates for one unit agree.
enum Agreement<'a> {
    /// All candidates carry the same target; `reference` is the first one in
    /// declaration order.
    Unanimous {
        target: &'a Target,
        reference: &'a Path,
    },
    /// At least two distinct targets; the vector lists each distinct option in
    /// first-seen order with its voters.
    Conflict(Vec<ConflictCandidate>),
}

/// Group candidates by their target, preserving first-seen order, and decide
/// whether they are unanimous.
fn classify(cands: &[Candidate]) -> Agreement<'_> {
    // Distinct targets in first-seen order, each with the references that voted
    // for it. A linear scan keeps the order stable and the cost is trivial for
    // the handful of references a project declares.
    let mut groups: Vec<(&Target, Vec<&Path>)> = Vec::new();
    for cand in cands {
        if let Some((_, voters)) = groups.iter_mut().find(|(t, _)| **t == cand.target) {
            voters.push(&cand.reference);
        } else {
            groups.push((&cand.target, vec![&cand.reference]));
        }
    }

    if groups.len() == 1 {
        let (target, voters) = &groups[0];
        return Agreement::Unanimous {
            target,
            reference: voters[0],
        };
    }

    let candidates = groups
        .into_iter()
        .map(|(target, voters)| {
            let mut iter = voters.into_iter();
            let first = iter.next().expect("a group always has ≥1 voter");
            ConflictCandidate {
                reference: first.to_path_buf(),
                also_from: iter.map(Path::to_path_buf).collect(),
                text: conflict_text(target),
            }
        })
        .collect();
    Agreement::Conflict(candidates)
}

/// Copy an agreed reference target into `unit`, run the gate, and bucket the
/// result. Implements rule 3.
fn apply_copy(
    unit: &mut Unit,
    target: &Target,
    reference: &Path,
    locale: &Locale,
    glossary: Option<&Glossary>,
    report: &mut ReuseReport,
) {
    unit.target = target.clone();
    // Promote to Proposed before gating so EmptyTargetWhenFinished does not
    // fire while we are still deciding; mirrors the CLI translate path.
    unit.state = UnitState::Proposed;

    let gate_report = validate(unit, locale, glossary);
    // Merge gate flags onto the unit so the same flag summary the rest of the
    // pipeline carries is present here too (the structured detail lives in the
    // GateReport, which the caller can re-run; we only fold the FlagSet).
    for flag in gate_report.flags.iter() {
        unit.flags.insert(flag);
    }

    let disposition = if gate_report.is_clean() && unit.target.is_complete() {
        unit.state = UnitState::Finished;
        report.copied_finished.push(unit.id.clone());
        CopiedDisposition::Finished
    } else {
        // Keep the copied text, leave at Proposed for a human to review.
        if let Some(status) = needs_review_status() {
            unit.review_status = Some(status);
        }
        report.copied_needs_review.push(unit.id.clone());
        CopiedDisposition::NeedsReview
    };

    report.copied.push(CopiedUnit {
        id: unit.id.clone(),
        winning_reference: reference.to_path_buf(),
        disposition,
    });
}

/// Render a target as display text for a conflict candidate.
fn conflict_text(target: &Target) -> ConflictText {
    match target {
        Target::Singular { text } => ConflictText::Singular(text.clone().unwrap_or_default()),
        Target::Plural { forms } => ConflictText::Plural(
            forms
                .iter()
                .map(|f| f.clone().unwrap_or_default())
                .collect(),
        ),
    }
}

/// The `review_status` to stamp on copied-needs-review and conflict units.
///
/// The report is the source of truth; this is an ergonomic hint for the Tauri
/// layer. Centralized so the choice is made once.
fn needs_review_status() -> Option<i18n_harness_core::ReviewStatus> {
    Some(i18n_harness_core::ReviewStatus::NeedsReview)
}
