//! [`TranslationOutcome`] and [`TranslatedText`] — what a backend produces
//! for each input unit.
//!
//! See [`crate::TranslationBackend`] for the trait that returns these and
//! the rationale for not handing out [`i18n_harness_core::Unit`]s.

use std::collections::BTreeMap;

use i18n_harness_core::Flag;
use serde::{Deserialize, Serialize};

/// One backend's verdict on one input unit, in batch order.
///
/// # Invariants on the parent vector
///
/// When returned from [`crate::TranslationBackend::translate_batch`], the
/// outer `Vec<TranslationOutcome>` MUST have
/// `len() == batch.units.len()` and the outcome at index `i` MUST refer
/// to the unit at `batch.units[i]`. Trait implementations that cannot
/// guarantee this (e.g., a JSON response from an LLM with shuffled keys)
/// must re-align internally before returning.
///
/// # Why this enum, not `Option<String>`
///
/// `Option<String>` collapses *skip* and *fail* into one bucket. The
/// metrics view in §8 wants to distinguish "the model gave up" (Failed,
/// counts against quality) from "the user requested skip" (Skipped, does
/// not count). The CLI's retry logic also wants to know which Failed
/// outcomes are retryable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TranslationOutcome {
    /// Backend produced a translation. The caller merges
    /// [`TranslatedText`] into the unit and runs the gate.
    Translated {
        /// The produced text (singular or plural-form vector).
        text: TranslatedText,
        /// Model-supplied semantic flags. The caller merges these into
        /// `unit.flags`; the gate passes them through.
        ///
        /// Allowed flags: any [`Flag`] with
        /// `severity() == FlagSeverity::Semantic`. Hard/soft flags from
        /// the backend are ignored — those are the gate's territory.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        flags: Vec<Flag>,
        /// Model-supplied self-reported confidence in this translation,
        /// in `[0.0, 1.0]`. `None` for backends that do not report
        /// confidence (e.g. the manual backend or the legacy v1 plain-text
        /// Ollama prompt). The caller copies this onto
        /// [`i18n_harness_core::Unit::confidence`] and never normalises.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        confidence: Option<f32>,
        /// Per-flag explanatory notes from the model. Keys are flags from
        /// `flags`; flags absent from this map have no note. The caller
        /// merges this into [`i18n_harness_core::Unit::flag_notes`].
        ///
        /// Only the semantic group is allowed here; if a backend ever
        /// emits a gate-produced flag the caller's merge can drop it
        /// silently — the gate is the canonical producer of those flags.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        flag_notes: BTreeMap<Flag, String>,
    },

    /// Backend deliberately declined the unit (e.g., manual backend
    /// invoked by a user who pressed "skip"). The caller leaves the unit
    /// untranslated; this is **not** a quality signal.
    Skipped {
        /// Human-readable explanation for the metrics log and UI.
        reason: String,
    },

    /// Backend tried the unit and could not produce usable output.
    /// Counts as a quality signal in the metrics view.
    Failed {
        /// Human-readable explanation. Surfaced verbatim to the metrics
        /// log; should be short and machine-greppable (e.g.
        /// `"no-plural-forms"`, `"empty-response"`).
        reason: String,

        /// `true` if the caller can retry this unit and reasonably expect
        /// a different result (transient — e.g., model timed out on this
        /// slot). `false` if the failure is deterministic given the
        /// current prompt (e.g., model returned malformed JSON twice in
        /// a row).
        ///
        /// The caller's retry policy reads this; the trait does not
        /// retry on its own.
        retryable: bool,

        /// Category of failure, used by the caller to decide whether to
        /// surface as a backend infrastructure error (return `Err` from
        /// the IPC command) or as a per-unit gate finding (return `Ok`
        /// with a `GateReport` carrying a `BackendMalformedResponse`).
        ///
        /// The field is named `failure_kind` (not `kind`) because the
        /// outer enum is serialised with `#[serde(tag = "kind")]`, and
        /// serde refuses to let a variant field shadow that tag.
        ///
        /// Defaults to [`FailureKind::Unspecified`] so older constructions
        /// of `Failed { reason, retryable }` keep compiling and so on-disk
        /// JSONL written before this field existed deserializes cleanly.
        #[serde(default)]
        failure_kind: FailureKind,
    },
}

/// Categorisation of a [`TranslationOutcome::Failed`].
///
/// The Tauri layer uses this to route failures: infrastructure issues
/// (network/auth) bubble up as `Err(String)` from the command (the user
/// cannot fix them from the editor); per-unit content failures
/// (malformed model JSON) surface as a hard gate finding that renders
/// inline alongside the unit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FailureKind {
    /// Category not declared by the producing backend; treat as a
    /// generic backend error. The default for legacy `Failed { reason,
    /// retryable }` literals.
    #[default]
    Unspecified,
    /// Transport failure (timeout, DNS, refused connection, non-2xx HTTP
    /// without a usable body, etc.). The user cannot fix this from the
    /// editor; the caller should bubble it up.
    Network,
    /// Backend is unreachable in a structural sense (model not loaded,
    /// service not running, auth proxy rejecting). Distinct from
    /// `Network` so the UI can suggest the right remediation.
    BackendUnavailable,
    /// Backend produced a response we could not parse against the strict
    /// schema (unknown flag kind, confidence out of `[0.0, 1.0]`, JSON
    /// shape wrong). The caller surfaces this as a hard gate finding
    /// (`BackendMalformedResponse`) so the prompt can be tuned.
    MalformedResponse,
}

impl TranslationOutcome {
    /// Construct a successful singular outcome with no model-supplied
    /// flags, no confidence, and no flag notes. Convenience for tests
    /// and trivial backends.
    pub fn translated_singular(text: impl Into<String>) -> Self {
        Self::Translated {
            text: TranslatedText::Singular(text.into()),
            flags: Vec::new(),
            confidence: None,
            flag_notes: BTreeMap::new(),
        }
    }

    /// Construct a successful plural outcome with no model-supplied
    /// flags, no confidence, and no flag notes.
    pub fn translated_plural<I, S>(forms: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::Translated {
            text: TranslatedText::Plural(forms.into_iter().map(Into::into).collect()),
            flags: Vec::new(),
            confidence: None,
            flag_notes: BTreeMap::new(),
        }
    }

    /// True if the outcome carries usable text.
    pub fn is_translated(&self) -> bool {
        matches!(self, Self::Translated { .. })
    }
}

/// Text payload of a successful [`TranslationOutcome::Translated`].
///
/// A backend produces either a single string (for singular units) or one
/// string per CLDR plural category in canonical order (zero, one, two,
/// few, many, other — only the slots applicable to the target locale).
/// The caller checks shape match against the unit's `plural_arity`.
///
/// On the wire (JSONL intermediate, metrics) we use an externally-tagged
/// representation: `{"singular": "..."}` or `{"plural": ["...", ...]}`.
/// Compact and round-trips cleanly through `serde_json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TranslatedText {
    /// Single target string. Matches `Unit { plural_arity: None, .. }`.
    Singular(String),
    /// One string per CLDR plural form, in canonical order. Length must
    /// equal the target locale's plural arity.
    Plural(Vec<String>),
}

impl TranslatedText {
    /// Number of slots. `1` for singular, `forms.len()` for plural.
    pub fn slot_count(&self) -> usize {
        match self {
            Self::Singular(_) => 1,
            Self::Plural(forms) => forms.len(),
        }
    }

    /// True if every slot has non-empty text.
    pub fn all_non_empty(&self) -> bool {
        match self {
            Self::Singular(s) => !s.is_empty(),
            Self::Plural(forms) => forms.iter().all(|s| !s.is_empty()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translated_singular_helper_has_no_flags_no_confidence_no_notes() {
        let out = TranslationOutcome::translated_singular("Hallo");
        match out {
            TranslationOutcome::Translated {
                text,
                flags,
                confidence,
                flag_notes,
            } => {
                assert_eq!(text, TranslatedText::Singular("Hallo".into()));
                assert!(flags.is_empty());
                assert!(confidence.is_none());
                assert!(flag_notes.is_empty());
            }
            other => panic!("expected Translated, got {other:?}"),
        }
    }

    #[test]
    fn outcomes_serialize_with_kind_tag() {
        let out = TranslationOutcome::translated_singular("x");
        let s = serde_json::to_string(&out).unwrap();
        assert!(s.contains("\"kind\":\"translated\""), "{s}");
    }

    #[test]
    fn failed_outcome_carries_retryable_flag_and_kind() {
        let out = TranslationOutcome::Failed {
            reason: "model-empty".into(),
            retryable: true,
            failure_kind: FailureKind::Network,
        };
        let s = serde_json::to_string(&out).unwrap();
        assert!(s.contains("\"retryable\":true"), "{s}");
        assert!(s.contains("\"failure_kind\":\"network\""), "{s}");
    }

    #[test]
    fn failed_outcome_kind_defaults_to_unspecified_on_deserialize() {
        // On-disk JSONL written before M4.6.1 added FailureKind has no
        // `failure_kind` field on Failed; the deserializer must accept
        // that (the outer enum tag `"kind":"failed"` is the variant
        // selector and never collides with this inner field).
        let json = r#"{"kind":"failed","reason":"model-empty","retryable":true}"#;
        let out: TranslationOutcome = serde_json::from_str(json).expect("deserialize");
        match out {
            TranslationOutcome::Failed { failure_kind, .. } => {
                assert_eq!(failure_kind, FailureKind::Unspecified);
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn translated_outcome_confidence_and_flag_notes_round_trip() {
        let mut notes = BTreeMap::new();
        notes.insert(Flag::AmbiguousSource, "ambiguous".into());
        notes.insert(Flag::BrandTerm, "brand".into());
        let out = TranslationOutcome::Translated {
            text: TranslatedText::Singular("Hallo".into()),
            flags: vec![Flag::AmbiguousSource, Flag::BrandTerm],
            confidence: Some(0.42),
            flag_notes: notes,
        };
        let s = serde_json::to_string(&out).unwrap();
        let back: TranslationOutcome = serde_json::from_str(&s).unwrap();
        assert_eq!(out, back);
    }

    #[test]
    fn plural_slot_count_matches_form_count() {
        let p = TranslatedText::Plural(vec!["a".into(), "b".into(), "c".into()]);
        assert_eq!(p.slot_count(), 3);
        let s = TranslatedText::Singular("x".into());
        assert_eq!(s.slot_count(), 1);
    }
}
