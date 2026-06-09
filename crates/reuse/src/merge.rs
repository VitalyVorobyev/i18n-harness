//! Merge: fold a translated remainder back into its base.
//!
//! The split step carves the untranslated leftovers of an original catalog
//! into a standalone `.ts` remainder (via `adapter_qt::write_subset`). A
//! translator fills that remainder; `merge_back` folds the filled units back
//! into the base half so `base ∪ remainder` reconstructs a fully-translated
//! original.
//!
//! # Contract
//!
//! - The remainder's unit ids MUST be a **subset** of the base's ids. Any
//!   stray id aborts with [`ReuseError::MergeStrayIds`] — the remainder was
//!   split from a different base, or the base was edited after the split.
//! - **Disjointness guard.** No id may be `Finished` (with a complete target)
//!   in *both* base and remainder. The two files are supposed to be disjoint
//!   halves; an overlap means they are not, and the merge aborts with
//!   [`ReuseError::MergeOverlap`] rather than silently picking a winner.
//! - On success the returned `units` are the remainder's translated units,
//!   ready for `adapter_qt::apply(&base, &units, out)`. Only remainder units
//!   that actually carry translation content are returned; fully-empty
//!   remainder units (a translator skipped them) are omitted so apply does not
//!   churn no-op edits.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use i18n_harness_adapter_qt::{Catalog, extract};
use i18n_harness_core::{Unit, UnitId, UnitState};

use crate::error::ReuseError;
use crate::report::MergeReport;

/// Result of [`merge_back`].
///
/// `base` is the extracted base catalog; `units` are the remainder's
/// translated units to apply into it. The caller runs
/// `adapter_qt::apply(&base, &units, out)` — the validation gate having
/// already run on the remainder when it was translated, and apply's own
/// per-unit checks guarding the write.
#[derive(Debug)]
pub struct MergeOutcome {
    /// The extracted base catalog. Hand it to `adapter_qt::apply` with
    /// [`Self::units`].
    pub base: Catalog,
    /// The remainder's translated units, keyed by ids present in `base`.
    pub units: Vec<Unit>,
    /// Counts describing the merge.
    pub report: MergeReport,
}

/// Fold the translated remainder at `translated_remainder_path` back into the
/// base catalog at `base_path`.
///
/// See the module docs for the subset / disjointness contract.
///
/// # Errors
///
/// - [`ReuseError::Extract`] if either catalog cannot be extracted.
/// - [`ReuseError::MergeStrayIds`] if the remainder contains ids absent from
///   the base.
/// - [`ReuseError::MergeOverlap`] if any id is finished-and-complete in both
///   files.
pub fn merge_back(
    base_path: &Path,
    translated_remainder_path: &Path,
) -> Result<MergeOutcome, ReuseError> {
    let base = extract(base_path).map_err(|source| ReuseError::Extract {
        path: base_path.to_path_buf(),
        source,
    })?;
    let remainder = extract(translated_remainder_path).map_err(|source| ReuseError::Extract {
        path: translated_remainder_path.to_path_buf(),
        source,
    })?;

    // Index the base by id once for O(1) subset / overlap checks.
    let base_by_id: HashMap<&UnitId, &Unit> = base.units().iter().map(|u| (&u.id, u)).collect();

    // Subset guard: every remainder id must exist in the base.
    let mut stray: BTreeSet<&str> = BTreeSet::new();
    for unit in remainder.units() {
        if !base_by_id.contains_key(&unit.id) {
            stray.insert(unit.id.as_str());
        }
    }
    if !stray.is_empty() {
        return Err(ReuseError::MergeStrayIds {
            base: base_path.to_path_buf(),
            ids: stray.into_iter().map(str::to_owned).collect(),
        });
    }

    // Disjointness guard: no id finished-and-complete in both halves.
    let mut overlap: BTreeSet<&str> = BTreeSet::new();
    for unit in remainder.units() {
        if is_finished_complete(unit)
            && let Some(base_unit) = base_by_id.get(&unit.id)
            && is_finished_complete(base_unit)
        {
            overlap.insert(unit.id.as_str());
        }
    }
    if !overlap.is_empty() {
        return Err(ReuseError::MergeOverlap {
            base: base_path.to_path_buf(),
            ids: overlap.into_iter().map(str::to_owned).collect(),
        });
    }

    // Collect the remainder's translated units. A unit with an empty target
    // carries no work to merge; skip it so apply emits no no-op edit for it.
    let mut units: Vec<Unit> = Vec::new();
    let mut report = MergeReport::default();
    for unit in remainder.units() {
        if unit.target.is_empty() {
            continue;
        }
        report.merged += 1;
        if unit.target.is_complete() {
            report.merged_complete += 1;
        }
        units.push(unit.clone());
    }

    Ok(MergeOutcome {
        base,
        units,
        report,
    })
}

/// True if a unit is `Finished` with a complete target — the shape that means
/// "this half owns the finished translation of this id".
fn is_finished_complete(unit: &Unit) -> bool {
    unit.state == UnitState::Finished && unit.target.is_complete()
}
