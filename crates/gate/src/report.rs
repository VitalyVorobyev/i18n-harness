//! Structured gate output: [`GateReport`], [`Finding`], [`FindingDetail`].
//!
//! Each call to [`crate::validate`] returns one [`GateReport`] per unit
//! containing every check that fired. Two things callers want from this:
//!
//! 1. A cheap [`FlagSet`] summary that can be merged into the unit's
//!    `flags` field without recomputing anything.
//! 2. Human-readable details — which placeholder, observed vs expected
//!    count, which arm — so the CLI and UI can render meaningful messages.
//!
//! We represent the detail as a closed enum [`FindingDetail`] with one
//! variant per check. Callers `match` on it to format; a free-form
//! `message: String` would force every consumer to re-parse the message to
//! get the same information.

use i18n_harness_core::{Flag, FlagSet, UnitId};
use serde::{Deserialize, Serialize};

use crate::icu::ParseError as IcuParseError;

/// Full gate report for a single [`crate::Unit`].
///
/// # Layout choice
///
/// We store [`Finding`]s in a flat `Vec` rather than per-rule fields.
/// Rationale:
///
/// - Callers that want a [`FlagSet`] summary do one pass.
/// - Callers that want to format diagnoses do one pass.
/// - There is no rule that produces more than one finding per call right
///   now, but we do not want to design that in — a future "every empty
///   plural slot is its own finding" extension fits the flat shape.
///
/// The companion [`Self::flags`] field is the deduplicated mirror, computed
/// at construction time so the caller doesn't recompute it.
///
/// `Eq` is intentionally not derived because [`Finding`]s may carry `f32`
/// fields (length ratios). Use [`PartialEq`] for comparison in tests.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GateReport {
    /// The unit this report describes.
    pub unit_id: UnitId,

    /// Every check that fired, in evaluation order. Empty iff the unit
    /// passes every check.
    pub findings: Vec<Finding>,

    /// Deduplicated summary of [`Self::findings`] (one entry per `Flag`
    /// kind). Safe to merge into [`crate::Unit::flags`].
    pub flags: FlagSet,
}

impl GateReport {
    /// Build a report from the unit id and a vector of findings.
    ///
    /// The `flags` summary is computed automatically.
    pub(crate) fn from_findings(unit_id: UnitId, findings: Vec<Finding>) -> Self {
        let flags: FlagSet = findings.iter().map(|f| f.flag).collect();
        Self {
            unit_id,
            findings,
            flags,
        }
    }

    /// True if no checks fired.
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }

    /// True if at least one hard finding is present. Callers (CLI, backend
    /// driver) use this to block write-back.
    pub fn has_hard(&self) -> bool {
        self.flags.has_hard()
    }
}

/// One finding: the [`Flag`] kind plus the structured payload describing
/// what triggered it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    /// Flag kind — the same enum that lives in `unit.flags`.
    pub flag: Flag,
    /// Per-rule structured detail.
    pub detail: FindingDetail,
}

/// Structured detail for every check the gate can produce.
///
/// One variant per rule. New rules add a variant; existing variants are
/// stable (renaming or restructuring requires a SemVer bump).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "rule", rename_all = "kebab-case")]
pub enum FindingDetail {
    /// Source and target placeholder multisets disagree.
    PlaceholderMismatch(PlaceholderMismatchDetail),

    /// Plural unit's target does not have the locale's CLDR arity.
    PluralArityMismatch(PluralArityMismatchDetail),

    /// Target text is not valid ICU MessageFormat (per the focused gate
    /// parser).
    IcuParseError(IcuParseDetail),

    /// Unit is in [`Finished`](i18n_harness_core::UnitState::Finished)
    /// state but at least one required target slot is `None`.
    EmptyTargetWhenFinished(EmptyTargetDetail),

    /// Accelerator (`&`) count mismatch between source and target.
    AccelMismatch(AccelDetail),

    /// Target is materially longer than `source.len() * length_warn_ratio`.
    LengthWarn(LengthWarnDetail),

    /// CJK-script target uses full-width punctuation where the source uses
    /// ASCII. Informational — the convention is expected.
    CjkPunctuationTolerated(CjkPunctuationDetail),

    /// A placeholder is immediately preceded by a determiner-like word in
    /// a Latin-script target; gender/case agreement cannot be resolved at
    /// translation time.
    PlaceholderAgreementRisk(PlaceholderAgreementDetail),
}

/// Detail for [`Flag::PlaceholderMismatch`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaceholderMismatchDetail {
    /// Which target slot the mismatch was found in. For singular units this
    /// is `0`; for plural units it is the CLDR-ordered form index.
    pub slot: u32,
    /// Tokens present in the source but missing (or under-counted) in the
    /// target.
    pub missing: Vec<String>,
    /// Tokens present in the target but absent (or over-counted) in the
    /// source.
    pub extra: Vec<String>,
}

/// Detail for [`Flag::PluralArityMismatch`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluralArityMismatchDetail {
    /// Number of target forms expected for this locale.
    pub expected: u32,
    /// Number of target forms found.
    pub found: u32,
    /// `true` if the unit's `target` was the singular variant but the unit
    /// is plural — the structural variant is wrong, not just the count.
    pub wrong_variant: bool,
}

/// Detail for [`Flag::IcuParseError`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IcuParseDetail {
    /// Which target slot the bad ICU was found in.
    pub slot: u32,
    /// Byte offset within the slot's text where the parser gave up.
    pub byte_offset: usize,
    /// Human-readable cause.
    pub message: String,
}

impl IcuParseDetail {
    pub(crate) fn from_parse(slot: u32, err: IcuParseError) -> Self {
        Self {
            slot,
            byte_offset: err.byte_offset,
            message: err.message,
        }
    }
}

/// Detail for the "finished but empty" hard check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmptyTargetDetail {
    /// Which slot was found to be `None`. For singular units this is `0`;
    /// for plural units it is the CLDR-ordered form index.
    pub slot: u32,
}

/// Detail for [`Flag::AccelMismatch`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccelDetail {
    /// Count of literal `&` (excluding `&&`) in the source.
    pub source_count: u32,
    /// Count of literal `&` in the target (summed across plural forms).
    pub target_count: u32,
}

/// Detail for [`Flag::LengthWarn`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LengthWarnDetail {
    /// Source character count.
    pub source_chars: u32,
    /// Target character count (summed across plural forms).
    pub target_chars: u32,
    /// Threshold from the locale (`length_warn_ratio`).
    pub threshold: f32,
    /// Observed ratio `target_chars / source_chars`.
    pub ratio: f32,
}

/// Detail for [`Flag::CjkPunctuationTolerated`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CjkPunctuationDetail {
    /// Which full-width punctuation characters were observed in the target.
    /// Deduplicated and sorted for stable output.
    pub characters: Vec<char>,
}

/// Detail for [`Flag::PlaceholderAgreementRisk`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaceholderAgreementDetail {
    /// The placeholder token immediately preceded by a determiner.
    pub placeholder: String,
    /// The determiner word that preceded it.
    pub determiner: String,
}
