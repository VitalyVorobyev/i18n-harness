//! Validation gate: the trust boundary between LLM output and catalog
//! write-back.
//!
//! See `docs/initial_design.md` §8 and `CLAUDE.md` invariant #2. The gate is
//! the deterministic Rust component that guarantees a weak model cannot
//! corrupt catalog structure: every translated unit must pass through the
//! same rule set before any adapter is allowed to write it back.
//!
//! # Contract
//!
//! The entry points are [`validate`] (one unit) and [`validate_batch`] (a
//! slice). Both are **pure functions** — no I/O, no globals, no allocation
//! outside the returned [`GateReport`]s. They are also **format-agnostic**:
//! the gate sees only [`Unit`] values whose `source` and `target` are
//! already in ICU MessageFormat. Adapters do the format-specific
//! normalization on extract.
//!
//! # Rules
//!
//! Hard checks (set [`FlagSeverity::Hard`] flags; block write-back):
//!
//! 1. **Placeholder multiset.** Source multiset of placeholder tokens equals
//!    the target's. For plural units, each form is checked individually.
//! 2. **Plural arity.** If the unit is plural, the target must be
//!    [`Target::Plural`] with one slot per CLDR category for the target
//!    locale.
//! 3. **ICU parse.** Target text parses as ICU MessageFormat per the focused
//!    parser in the internal `icu` module (placeholder set + plural/select
//!    arity only, not full ICU4X — see `docs/implementation_plan.md` §13
//!    decision #2).
//! 4. **Non-empty when finished.** If `unit.state == Finished`, every
//!    required target slot must be `Some`.
//!
//! Soft checks (set [`FlagSeverity::Soft`] flags; warn but do not block):
//!
//! - **Accelerator.** Count of literal `&` (not `&&`) in source vs target.
//! - **Length warn.** Total target chars > source chars ×
//!   `locale.length_warn_ratio`.
//! - **CJK punctuation.** Source uses ASCII `,.:;!?` where target uses
//!   `，。：；！？`. Only fires when `locale.script == Script::Han`.
//! - **Placeholder agreement risk.** A placeholder is immediately preceded
//!   by a Latin determiner-like word. Heuristic; only fires for `Latin`
//!   script with `Formal`/`Informal` register.
//!
//! # What the gate does NOT do
//!
//! - It does **not** read or write the catalog. The catalog is the source
//!   of truth; the adapter does I/O.
//! - It does **not** know about Qt, PO, or ICU-JSON syntax.
//! - It does **not** check semantic adequacy. Model-supplied semantic flags
//!   (`AmbiguousSource`, `Idiom`, …) are produced by the backend and
//!   surfaced by the UI; the gate passes them through untouched.
//! - It does **not** check the source — the source is the reference. If the
//!   source itself has malformed ICU, that is a bug in the adapter, not the
//!   gate's concern.
//!
//! [`FlagSeverity::Hard`]: i18n_harness_core::FlagSeverity::Hard
//! [`FlagSeverity::Soft`]: i18n_harness_core::FlagSeverity::Soft
//! [`Target::Plural`]: i18n_harness_core::Target::Plural

#![forbid(unsafe_code)]

mod check;
mod icu;
mod report;

pub mod metrics;

pub use report::{
    AccelDetail, CjkPunctuationDetail, EmptyTargetDetail, Finding, FindingDetail, GateReport,
    IcuParseDetail, LengthWarnDetail, PlaceholderAgreementDetail, PlaceholderMismatchDetail,
    PluralArityMismatchDetail,
};

use i18n_harness_core::Unit;
use i18n_harness_glossary::Glossary;
use i18n_harness_locales::Locale;

/// Validate one [`Unit`] against a target locale.
///
/// `glossary` is accepted as `Option<&Glossary>` so that the M2 glossary
/// rules can land without changing this signature; in M1 the value is
/// ignored.
///
/// The returned [`GateReport`] carries:
///
/// - A list of [`Finding`]s with per-rule structured detail (which
///   placeholder, observed vs expected count, etc.).
/// - A [`FlagSet`](i18n_harness_core::FlagSet) summary mirroring those
///   findings; it can be merged into [`Unit::flags`] by the caller.
///
/// The function never panics. ICU parse failures are reported as findings;
/// they do not abort validation of remaining rules. Independent rules are
/// evaluated regardless of whether earlier rules fired, so a single call
/// reports every diagnosable issue at once.
pub fn validate(unit: &Unit, locale: &Locale, glossary: Option<&Glossary>) -> GateReport {
    check::run(unit, locale, glossary)
}

/// Validate a slice of [`Unit`]s in document order.
///
/// Equivalent to mapping [`validate`] over `units`; provided for symmetry
/// with batch-level call sites and to make the common case look like one
/// call. The returned vector is in the same order as the input.
pub fn validate_batch(
    units: &[Unit],
    locale: &Locale,
    glossary: Option<&Glossary>,
) -> Vec<GateReport> {
    units
        .iter()
        .map(|u| validate(u, locale, glossary))
        .collect()
}
