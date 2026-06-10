//! [`Flag`] enum and [`FlagSet`] container carried by every [`crate::Unit`].
//!
//! Flags fall into three groups:
//!
//! 1. **Hard / gate-produced.** These block write-back when the
//!    validation gate runs: placeholder multiset mismatch, plural arity
//!    mismatch, ICU parse failure. They are *facts* about the unit, not
//!    opinions.
//! 2. **Soft / gate-produced.** These warn but do not block: accelerator
//!    mismatch, length expansion past `length_warn_ratio`, CJK punctuation
//!    tolerance, placeholder agreement risk.
//! 3. **Model-supplied semantic.** The translation backend can attach
//!    these to express uncertainty: ambiguous source, idiom, insufficient
//!    context, low confidence. The gate does not produce them; the UI
//!    surfaces them.
//!
//! Flags carry no payload here — they name the *kind* of issue. A separate
//! `GateReport` carries the structured details (which placeholder,
//! observed vs expected, etc.). This keeps `Unit` cheap to serialize and
//! keeps the gate's diagnostic output out of the on-disk intermediate.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// A single validation or semantic flag on a [`crate::Unit`].
///
/// The variant naming uses kebab-case in the serialized form for stability;
/// adding a new variant is backward-compatible if the deserializer rejects
/// unknown variants (which is the default `serde` behavior — older binaries
/// reading newer files will error rather than silently lose flags, and the
/// caller can then surface the version mismatch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Flag {
    // ── Hard (gate) ────────────────────────────────────────────────────────
    /// The target's placeholder multiset does not match the source's.
    PlaceholderMismatch,
    /// The unit's plural arity does not match the target locale's CLDR
    /// plural-category count.
    PluralArityMismatch,
    /// The target text is not parseable as ICU MessageFormat.
    IcuParseError,
    /// The unit is marked [`crate::UnitState::Finished`] but at least one
    /// required target slot is `None`. The harness must not ship a
    /// "finished" unit with missing text.
    EmptyTargetWhenFinished,

    // ── Soft (gate) ────────────────────────────────────────────────────────
    /// The accelerator marker (`&`) is present in the source but not in the
    /// target (or vice versa, or in a different position).
    AccelMismatch,
    /// The target is materially longer than the source's
    /// `length_warn_ratio` — likely to overflow a UI control.
    LengthWarn,
    /// CJK full-width punctuation is present in a place where the source
    /// used ASCII punctuation; tolerated by the gate but flagged for review.
    CjkPunctuationTolerated,
    /// A placeholder substitutes for a noun whose case/gender cannot be
    /// resolved at translation time (German `den`/`dem`/`der`, Spanish
    /// `el`/`la`). Cannot be checked, but worth a human glance.
    PlaceholderAgreementRisk,
    /// HTML/markup tags present in the source were dropped, reordered,
    /// added, or differently-named in the target. The gate compares
    /// `<tag>` / `</tag>` multisets, treating tag NAMES as opaque and
    /// ignoring attribute differences.
    MarkupTagMismatch,
    /// The translation backend returned a response we could not parse
    /// against the strict v2 contract (unknown flag kind, confidence out
    /// of `[0.0, 1.0]`, JSON shape wrong). Not produced by the gate
    /// validator itself; the Tauri command handlers construct a
    /// `GateReport` with this flag when the backend signals
    /// `FailureKind::MalformedResponse` so the failure surfaces inline
    /// alongside the unit instead of being lost behind a generic
    /// backend error. Hard severity: there is no translation to ship.
    BackendMalformedResponse,

    // ── Model-supplied semantic ────────────────────────────────────────────
    /// The source is ambiguous; the model picked one reading but is not
    /// confident (e.g. "Record" — noun or verb?).
    AmbiguousSource,
    /// The source is idiomatic and the model produced a literal translation.
    Idiom,
    /// The model lacked context to translate confidently.
    InsufficientContext,
    /// The model's overall confidence in this translation is low.
    LowConfidence,
    /// The source contains what looks like a product/brand name that is not
    /// covered by the glossary's do-not-translate list. The model translated
    /// it tentatively and asks for human confirmation.
    BrandTerm,
    /// The source register (formal/informal/marketing/etc.) is hard to carry
    /// into the target locale; the model produced one reading but the
    /// register match is uncertain.
    ToneMismatch,
}

/// Severity of a [`Flag`], used by the CLI and UI to group findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FlagSeverity {
    /// Blocks write-back. The unit cannot be applied as-is.
    Hard,
    /// Warns but does not block.
    Soft,
    /// Informational — the model self-reported uncertainty.
    Semantic,
}

impl Flag {
    /// Classify this flag's severity.
    ///
    /// This is the canonical mapping used by the CLI and UI; do not duplicate
    /// it in other crates.
    pub fn severity(self) -> FlagSeverity {
        match self {
            Self::PlaceholderMismatch
            | Self::PluralArityMismatch
            | Self::IcuParseError
            | Self::EmptyTargetWhenFinished
            | Self::BackendMalformedResponse => FlagSeverity::Hard,
            Self::AccelMismatch
            | Self::LengthWarn
            | Self::CjkPunctuationTolerated
            | Self::PlaceholderAgreementRisk
            | Self::MarkupTagMismatch => FlagSeverity::Soft,
            Self::AmbiguousSource
            | Self::Idiom
            | Self::InsufficientContext
            | Self::LowConfidence
            | Self::BrandTerm
            | Self::ToneMismatch => FlagSeverity::Semantic,
        }
    }
}

/// A stable, deduplicated, deterministically-ordered set of [`Flag`]s.
///
/// We use a `BTreeSet` rather than a `Vec` because flag *order* on a unit is
/// not meaningful but flag *presence/absence* is, and we want the JSONL line
/// for a unit to be byte-stable so diffs stay readable.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FlagSet(BTreeSet<Flag>);

impl FlagSet {
    /// An empty [`FlagSet`].
    pub fn new() -> Self {
        Self(BTreeSet::new())
    }

    /// Returns true if there are no flags set.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Number of distinct flags currently set.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Insert a flag. Returns `true` if it was not already present.
    pub fn insert(&mut self, flag: Flag) -> bool {
        self.0.insert(flag)
    }

    /// Returns true if `flag` is set.
    pub fn contains(&self, flag: Flag) -> bool {
        self.0.contains(&flag)
    }

    /// Iterate flags in their stable sorted order.
    pub fn iter(&self) -> impl Iterator<Item = Flag> + '_ {
        self.0.iter().copied()
    }

    /// Returns true if any flag with [`FlagSeverity::Hard`] is set. Callers
    /// (gate, CLI) treat this as the "must not write back" condition.
    pub fn has_hard(&self) -> bool {
        self.iter().any(|f| f.severity() == FlagSeverity::Hard)
    }
}

impl FromIterator<Flag> for FlagSet {
    fn from_iter<I: IntoIterator<Item = Flag>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl<'a> IntoIterator for &'a FlagSet {
    type Item = Flag;
    type IntoIter = std::iter::Copied<std::collections::btree_set::Iter<'a, Flag>>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_partition_is_total() {
        // Every variant must have a defined severity. If a new variant lands
        // without updating `severity()`, the match becomes non-exhaustive
        // and this test won't compile — that's the intended trip-wire.
        let all = [
            Flag::PlaceholderMismatch,
            Flag::PluralArityMismatch,
            Flag::IcuParseError,
            Flag::EmptyTargetWhenFinished,
            Flag::AccelMismatch,
            Flag::LengthWarn,
            Flag::CjkPunctuationTolerated,
            Flag::PlaceholderAgreementRisk,
            Flag::MarkupTagMismatch,
            Flag::BackendMalformedResponse,
            Flag::AmbiguousSource,
            Flag::Idiom,
            Flag::InsufficientContext,
            Flag::LowConfidence,
            Flag::BrandTerm,
            Flag::ToneMismatch,
        ];
        for f in all {
            let _s = f.severity();
        }
    }

    #[test]
    fn new_semantic_variants_are_semantic_severity() {
        // Pin the severity of the semantic-flag additions so a future refactor
        // cannot silently demote them to Hard/Soft, which would change the
        // gate's write-back blocking semantics.
        assert_eq!(Flag::BrandTerm.severity(), FlagSeverity::Semantic);
        assert_eq!(Flag::ToneMismatch.severity(), FlagSeverity::Semantic);
    }

    #[test]
    fn new_semantic_variants_serialize_as_kebab_case() {
        // The on-the-wire kind names are part of the prompt-v2 contract.
        assert_eq!(
            serde_json::to_string(&Flag::BrandTerm).unwrap(),
            r#""brand-term""#
        );
        assert_eq!(
            serde_json::to_string(&Flag::ToneMismatch).unwrap(),
            r#""tone-mismatch""#
        );
    }

    #[test]
    fn has_hard_distinguishes_severities() {
        let mut s = FlagSet::new();
        assert!(!s.has_hard());

        s.insert(Flag::LengthWarn);
        assert!(!s.has_hard(), "soft flag must not be hard");

        s.insert(Flag::PlaceholderMismatch);
        assert!(s.has_hard(), "hard flag must trigger has_hard");
    }
}
