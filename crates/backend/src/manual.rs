//! [`ManualBackend`] — the "no model" backend.
//!
//! The manual backend is the one any test can drive without HTTP, files,
//! or a model installed. It is also the backend the **two-phase CLI flow**
//! uses to import translations produced out-of-band (by an agent,
//! by a human filling a JSONL file, etc.): the CLI passes a closure that
//! reads from disk; the backend turns each input unit into the
//! corresponding outcome.
//!
//! # Why it exists
//!
//! Two reasons:
//! 1. **Test substrate.** Every test of the trait surface uses this
//!    backend with an inline closure. We do not need an HTTP server.
//! 2. **Trait validation.** Implementing the trait against a non-HTTP
//!    backend forced the trait surface to NOT bake in I/O assumptions —
//!    the trait does not mention transport, and `ManualBackend` is the
//!    proof.
//!
//! # Determinism
//!
//! [`ManualBackend::is_deterministic`] reflects what the caller declared
//! at construction. A closure that reads from disk and produces the same
//! response for the same unit is deterministic; a closure that calls
//! out to a flaky service is not. The trait surface cannot inspect the
//! closure; the caller's word is the contract.

use std::collections::BTreeMap;

use i18n_harness_core::Batch;
use i18n_harness_glossary::{Glossary, Register};
use i18n_harness_locales::Locale;

use crate::context::PromptContext;
use crate::error::BackendError;
use crate::outcome::{FailureKind, TranslatedText, TranslationOutcome};
use crate::trait_def::TranslationBackend;

/// What a manual-backend closure returns for one unit.
///
/// Kept separate from [`TranslationOutcome`] for two reasons:
///
/// 1. The closure is a *plain function* — the caller does not have to
///    know about per-unit `Flag` semantic-severity handling. Manual mode
///    is the "what does the human say" surface; semantic flags live on
///    real backends.
/// 2. We pre-validate plural arity inside `translate_batch` before
///    forwarding to the outcome enum. If the caller's closure produces a
///    Singular response for a plural unit, that becomes
///    `TranslationOutcome::Failed` with `retryable: false`, not a panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManualResponse {
    /// Single target string.
    Singular(String),
    /// One target per CLDR plural form, in canonical order. Length must
    /// match the unit's target locale's `plural_arity`.
    Plural(Vec<String>),
    /// Skip this unit; leaves the unit untranslated, no quality
    /// penalty in metrics.
    Skip,
    /// Mark this unit as failed. The reason is surfaced in metrics; the
    /// retryable flag controls whether the CLI retries.
    Fail {
        /// Short, kebab-case reason ("model-empty", "user-refused", ...).
        reason: String,
        /// `true` if the caller should retry this unit.
        retryable: bool,
    },
}

/// Closure-driven [`TranslationBackend`].
///
/// The closure receives a [`PromptContext`] for one unit and produces a
/// [`ManualResponse`]. The backend wraps each response in the appropriate
/// [`TranslationOutcome`].
///
/// # Field visibility
///
/// `name`, `deterministic`, and the closure are private; construct with
/// [`Self::new`] and (optionally) [`Self::named`].
pub struct ManualBackend<F>
where
    F: Fn(&PromptContext<'_>) -> ManualResponse,
{
    name: String,
    deterministic: bool,
    f: F,
}

impl<F> ManualBackend<F>
where
    F: Fn(&PromptContext<'_>) -> ManualResponse,
{
    /// Construct a manual backend named `"manual"` and declared
    /// deterministic (the closure is assumed to be pure unless the
    /// caller specifies otherwise via [`Self::named`]).
    pub fn new(f: F) -> Self {
        Self {
            name: "manual".to_owned(),
            deterministic: true,
            f,
        }
    }

    /// Construct with explicit name and determinism flag. Use when the
    /// closure reads from an external source (in which case `name` might
    /// be `"manual-jsonl"`, `deterministic` is `false` if the source can
    /// change between calls).
    pub fn named(name: impl Into<String>, deterministic: bool, f: F) -> Self {
        Self {
            name: name.into(),
            deterministic,
            f,
        }
    }
}

impl<F> TranslationBackend for ManualBackend<F>
where
    F: Fn(&PromptContext<'_>) -> ManualResponse,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn is_deterministic(&self) -> bool {
        self.deterministic
    }

    fn translate_batch(
        &self,
        batch: &Batch,
        locale: &Locale,
        glossary: Option<&Glossary>,
    ) -> Result<Vec<TranslationOutcome>, BackendError> {
        let mut out = Vec::with_capacity(batch.units.len());
        for unit in &batch.units {
            let register = glossary
                .and_then(|g| g.register_for(locale.id))
                .map(Register::to_locales_register)
                .map(register_from_locales)
                .unwrap_or_else(|| register_from_locales(locale.register));
            let ctx = PromptContext::new(unit, locale, register, glossary, &unit.flags);
            let response = (self.f)(&ctx);
            out.push(translate_one(unit, locale, response));
        }
        Ok(out)
    }
}

fn translate_one(
    unit: &i18n_harness_core::Unit,
    locale: &Locale,
    response: ManualResponse,
) -> TranslationOutcome {
    match response {
        ManualResponse::Singular(text) => {
            if unit.plural_arity.is_some() {
                return TranslationOutcome::Failed {
                    reason: "expected-plural-got-singular".into(),
                    retryable: false,
                    failure_kind: FailureKind::Unspecified,
                };
            }
            TranslationOutcome::Translated {
                text: TranslatedText::Singular(text),
                flags: Vec::new(),
                confidence: None,
                flag_notes: BTreeMap::new(),
            }
        }
        ManualResponse::Plural(forms) => {
            if unit.plural_arity.is_none() {
                return TranslationOutcome::Failed {
                    reason: "expected-singular-got-plural".into(),
                    retryable: false,
                    failure_kind: FailureKind::Unspecified,
                };
            }
            let expected = locale.plural_arity() as usize;
            if forms.len() != expected {
                return TranslationOutcome::Failed {
                    reason: format!(
                        "plural-arity-mismatch:expected={expected} got={got}",
                        got = forms.len(),
                    ),
                    retryable: false,
                    failure_kind: FailureKind::Unspecified,
                };
            }
            TranslationOutcome::Translated {
                text: TranslatedText::Plural(forms),
                flags: Vec::new(),
                confidence: None,
                flag_notes: BTreeMap::new(),
            }
        }
        ManualResponse::Skip => TranslationOutcome::Skipped {
            reason: "manual-skip".into(),
        },
        ManualResponse::Fail { reason, retryable } => TranslationOutcome::Failed {
            reason,
            retryable,
            failure_kind: FailureKind::Unspecified,
        },
    }
}

/// Identity passthrough: the locales-crate Register variants line up 1:1
/// with the one we already have here. Kept as a function so the call
/// site stays readable and so future divergence has one place to fix.
fn register_from_locales(r: i18n_harness_locales::Register) -> Register {
    match r {
        i18n_harness_locales::Register::Formal => Register::Formal,
        i18n_harness_locales::Register::Informal => Register::Informal,
        i18n_harness_locales::Register::Neutral => Register::Neutral,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use i18n_harness_core::{Batch, BatchKey, Target, Unit};

    fn de_de() -> &'static Locale {
        Locale::by_id("de_DE").expect("de_DE")
    }

    fn singular_unit(id: &str, source: &str) -> Unit {
        Unit::untranslated_singular(id, source)
    }

    fn plural_unit(id: &str, source: &str) -> Unit {
        let mut u = Unit::untranslated_singular(id, source);
        u.plural_arity = Some(2);
        u.target = Target::Plural {
            forms: vec![None, None],
        };
        u
    }

    #[test]
    fn translates_singular_in_order() {
        let backend = ManualBackend::new(|ctx| ManualResponse::Singular(ctx.unit.source.clone()));
        let units = vec![
            singular_unit("a", "Hello"),
            singular_unit("b", "World"),
            singular_unit("c", "!"),
        ];
        let batch = Batch::new(BatchKey::new("h", 0), units);
        let out = backend.translate_batch(&batch, de_de(), None).expect("ok");
        assert_eq!(out.len(), batch.units.len());
        // Order preserved: index i in out corresponds to batch.units[i].
        for (outcome, unit) in out.iter().zip(&batch.units) {
            match outcome {
                TranslationOutcome::Translated {
                    text: TranslatedText::Singular(s),
                    ..
                } => assert_eq!(s, &unit.source),
                other => panic!("expected Translated singular, got {other:?}"),
            }
        }
    }

    #[test]
    fn skip_produces_skipped_outcome() {
        let backend = ManualBackend::new(|_| ManualResponse::Skip);
        let batch = Batch::new(BatchKey::new("h", 0), vec![singular_unit("a", "x")]);
        let out = backend.translate_batch(&batch, de_de(), None).unwrap();
        assert!(
            matches!(&out[0], TranslationOutcome::Skipped { .. }),
            "got {:?}",
            out[0]
        );
    }

    #[test]
    fn fail_produces_failed_outcome_with_retryable_flag() {
        let backend = ManualBackend::new(|_| ManualResponse::Fail {
            reason: "model-empty".into(),
            retryable: true,
        });
        let batch = Batch::new(BatchKey::new("h", 0), vec![singular_unit("a", "x")]);
        let out = backend.translate_batch(&batch, de_de(), None).unwrap();
        match &out[0] {
            TranslationOutcome::Failed {
                reason, retryable, ..
            } => {
                assert_eq!(reason, "model-empty");
                assert!(*retryable);
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn singular_response_to_plural_unit_is_failed_non_retryable() {
        let backend = ManualBackend::new(|_| ManualResponse::Singular("Hallo".into()));
        let batch = Batch::new(BatchKey::new("h", 0), vec![plural_unit("p", "%n items")]);
        let out = backend.translate_batch(&batch, de_de(), None).unwrap();
        match &out[0] {
            TranslationOutcome::Failed {
                reason, retryable, ..
            } => {
                assert_eq!(reason, "expected-plural-got-singular");
                assert!(!*retryable);
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn plural_response_with_wrong_arity_is_failed_non_retryable() {
        let backend = ManualBackend::new(|_| {
            ManualResponse::Plural(vec!["a".into(), "b".into(), "c".into()])
        });
        let batch = Batch::new(BatchKey::new("h", 0), vec![plural_unit("p", "%n items")]);
        let out = backend.translate_batch(&batch, de_de(), None).unwrap();
        match &out[0] {
            TranslationOutcome::Failed {
                reason, retryable, ..
            } => {
                assert!(reason.starts_with("plural-arity-mismatch"), "got {reason}");
                assert!(!*retryable);
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn plural_response_with_matching_arity_succeeds() {
        let backend =
            ManualBackend::new(|_| ManualResponse::Plural(vec!["eins".into(), "andere".into()]));
        let batch = Batch::new(BatchKey::new("h", 0), vec![plural_unit("p", "%n items")]);
        let out = backend.translate_batch(&batch, de_de(), None).unwrap();
        match &out[0] {
            TranslationOutcome::Translated {
                text: TranslatedText::Plural(forms),
                ..
            } => {
                assert_eq!(forms, &vec!["eins".to_string(), "andere".to_string()]);
            }
            other => panic!("expected Translated plural, got {other:?}"),
        }
    }

    #[test]
    fn name_and_determinism_are_reported() {
        let b1 = ManualBackend::new(|_| ManualResponse::Skip);
        assert_eq!(b1.name(), "manual");
        assert!(b1.is_deterministic());

        let b2 = ManualBackend::named("manual-jsonl", false, |_| ManualResponse::Skip);
        assert_eq!(b2.name(), "manual-jsonl");
        assert!(!b2.is_deterministic());
    }

    /// Sanity: a backend that tries to corrupt downstream fields cannot,
    /// because it never receives `&mut Unit`. We assert this *by
    /// compilation*: the closure's input is `&PromptContext`, which
    /// only holds shared references. The fact that this test compiles
    /// is the proof; the runtime check confirms the call site is
    /// unchanged by the closure.
    #[test]
    fn closure_cannot_mutate_structural_fields() {
        let initial_id = "preserve-me";
        let backend = ManualBackend::new(|ctx| {
            // We can READ the unit, but the type system prevents writing.
            // Anything we return is text-only and lands in the outcome.
            let _ = ctx.unit.id.clone();
            ManualResponse::Singular("done".into())
        });
        let batch = Batch::new(
            BatchKey::new("h", 0),
            vec![singular_unit(initial_id, "source text")],
        );
        let out = backend.translate_batch(&batch, de_de(), None).unwrap();
        // The unit in the batch is untouched after the backend call.
        assert_eq!(batch.units[0].id.as_str(), initial_id);
        assert_eq!(batch.units[0].source, "source text");
        assert!(matches!(&out[0], TranslationOutcome::Translated { .. }));
    }
}
