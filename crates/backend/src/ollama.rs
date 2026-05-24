//! Ollama HTTP backend — translates via `POST /api/generate` on a locally
//! running Ollama server.
//!
//! # Feature gate
//!
//! This module is compiled only with `--features ollama`. It brings in
//! `ureq` (sync HTTP, no async runtime) as its only extra dependency.
//!
//! # Prompt template (v2 is the default)
//!
//! The default template lives at `crates/backend/prompts/ollama-translate-v2.txt`
//! and instructs the model to emit a strict-JSON object on a single line
//! (translation + flags + confidence). It is embedded at compile time via
//! `include_str!` and parsed once in [`OllamaBackend::new`]. Template
//! tokens follow the v1 vocabulary defined in [`crate::prompt`].
//!
//! v1 (plain-text response) remains shipped for the CLI's `--prompt`
//! override path; callers select it via [`OllamaBackend::with_template`]
//! and the response shape switches automatically (see
//! [`PromptResponseShape`]).
//!
//! # `num_ctx`
//!
//! Ollama's built-in default is 4 096 tokens, which silently truncates
//! batches that include long source strings plus a non-trivial glossary
//! block. The backend sets `num_ctx` to **8 192** on every request. This
//! covers the typical prompt size comfortably. Callers that need a larger
//! window can override via [`OllamaBackend::with_num_ctx`].
//!
//! # Plural units
//!
//! Plural units (`unit.plural_arity.is_some()`) are handled by issuing one
//! HTTP call per CLDR plural slot for the target locale, in CLDR canonical
//! order (`zero, one, two, few, many, other` — only the slots applicable
//! to the locale). Each request reuses the singular prompt template and
//! appends a single-line directive naming the CLDR category — e.g.,
//! `Plural form: produce the "other" form (CLDR category for zh_Hans).`
//!
//! Trade-off: N HTTP calls per plural unit (where N is the locale's
//! plural arity — 1 for Mandarin, 2 for German/Spanish, up to 6 for
//! Arabic) versus a single call returning a JSON object with one slot per
//! category. The per-call approach was picked because:
//!
//! - One failed slot does not poison the rest of the unit; the per-slot
//!   error path is identical to the singular one.
//! - No JSON re-alignment risk if the model returns a partial object.
//! - The prompt template stays untouched.
//!
//! If a future quality metric shows this is too slow for large plural-
//! heavy catalogs, the optimisation is to add a JSON-structured plural
//! prompt as a separate `OllamaBackend::with_plural_strategy(...)` knob.
//!
//! # Environment variables (read at construction only)
//!
//! - `OLLAMA_HOST` — base URL for the Ollama server, e.g.
//!   `http://192.168.1.10:11434`. Defaults to `http://localhost:11434`.
//! - `OLLAMA_MODEL` — model tag (e.g. `gemma4:e4b`, `gemma4:e2b`).
//!   Defaults to the built-in [`DEFAULT_MODEL`]; per the implementation
//!   plan, Gemma 4 E4B is the workspace standard (better quality) with
//!   E2B as the fast alternative — choose with this env var or
//!   [`OllamaBackend::with_model`].
//! - `OLLAMA_API_KEY` — Bearer token for remote Ollama deployments that sit
//!   behind an auth proxy. Not required for the default local setup.
//!
//! None of these variables is read at translation time; the values are
//! captured once in [`OllamaBackend::new`] and held on `self`.
//!
//! # Performance
//!
//! v1 issues **one HTTP request per unit**. This is the simplest correct
//! implementation. Batched prompting (send N units in one request, parse a
//! JSON array response, re-align to batch order) is a future optimisation;
//! it introduces JSON-alignment complexity that is not worth the risk until
//! the per-unit quality metrics are established.

use std::collections::BTreeMap;
use std::time::Duration;

use i18n_harness_core::{Batch, Flag, FlagSeverity};
use i18n_harness_glossary::{Glossary, Register};
use i18n_harness_locales::Locale;
use serde::Deserialize;

use crate::context::PromptContext;
use crate::error::BackendError;
use crate::outcome::{FailureKind, TranslatedText, TranslationOutcome};
use crate::prompt::PromptTemplate;
use crate::trait_def::TranslationBackend;

/// The v2 template text. v2 is the default prompt and instructs the model
/// to emit a single-line strict-JSON object: see
/// `prompts/ollama-translate-v2.txt` for the schema.
const TEMPLATE_BODY_V2: &str = include_str!("../prompts/ollama-translate-v2.txt");

/// The v1 template text. Kept around for the CLI's `--prompt` override
/// path: callers that select v1 get the plain-text response shape
/// preserved verbatim.
const TEMPLATE_BODY_V1: &str = include_str!("../prompts/ollama-translate-v1.txt");

const DEFAULT_HOST: &str = "http://localhost:11434";
/// Default Ollama model tag. The plan calls for Gemma 4 E4B (better
/// quality) as the workspace default with E2B as the fast alternative;
/// the maintainer has E2B installed locally so the binary default is
/// E2B. Override via `OLLAMA_MODEL` or [`OllamaBackend::with_model`] to
/// pick a different tag without rebuilding.
pub const DEFAULT_MODEL: &str = "gemma4:e2b";
const DEFAULT_NUM_CTX: u32 = 8192;
const DEFAULT_TIMEOUT_SECS: u64 = 120;

/// How a prompt template expects the model to format its response.
///
/// Selected once at construction time so the per-request parse path
/// stays branch-free in the hot loop.
///
/// - [`Self::StrictJson`] — v2 contract. Response must be a single
///   JSON object with `translation`, optional `flags`, and a `confidence`
///   field. Anything else becomes
///   [`TranslationOutcome::Failed`] with
///   [`FailureKind::MalformedResponse`]; the gate then surfaces the
///   failure inline so the prompt can be tuned.
/// - [`Self::PlainText`] — v1 contract. The model emits the translation
///   verbatim. No structured flags, no confidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptResponseShape {
    /// v2: strict-JSON single-line response.
    StrictJson,
    /// v1: plain-text response, no structured metadata.
    PlainText,
}

/// Ollama HTTP backend.
///
/// Construct with [`OllamaBackend::new`] (reads `OLLAMA_HOST`,
/// `OLLAMA_MODEL`, and `OLLAMA_API_KEY` from the environment) or via the
/// builder methods.
pub struct OllamaBackend {
    host: String,
    model: String,
    num_ctx: u32,
    request_timeout: Duration,
    template: PromptTemplate,
    /// Tells [`Self::call_generate`] how to parse the response body.
    /// Locked to the template at construction; flipping it without
    /// changing the template would produce nonsense.
    response_shape: PromptResponseShape,
    api_key: Option<String>,
}

impl OllamaBackend {
    /// Construct from environment variables and built-in defaults.
    ///
    /// Reads `OLLAMA_HOST`, `OLLAMA_MODEL`, and `OLLAMA_API_KEY` once at
    /// construction. Never reads environment variables at translation
    /// time. The default template is v2 (strict-JSON contract).
    pub fn new() -> Result<Self, BackendError> {
        let host = std::env::var("OLLAMA_HOST").unwrap_or_else(|_| DEFAULT_HOST.to_owned());
        let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_owned());
        let api_key = std::env::var("OLLAMA_API_KEY").ok();
        Ok(Self {
            host,
            model,
            num_ctx: DEFAULT_NUM_CTX,
            request_timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            template: PromptTemplate::new(TEMPLATE_BODY_V2, "v2"),
            response_shape: PromptResponseShape::StrictJson,
            api_key,
        })
    }

    /// Override the Ollama server base URL (default: `http://localhost:11434`).
    #[must_use]
    pub fn with_host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    /// Override the model tag (default: [`DEFAULT_MODEL`], overridable via
    /// the `OLLAMA_MODEL` env var at construction time).
    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Override the `num_ctx` option sent on every request (default: 8192).
    #[must_use]
    pub fn with_num_ctx(mut self, num_ctx: u32) -> Self {
        self.num_ctx = num_ctx;
        self
    }

    /// Override the per-request timeout (default: 120 s).
    #[must_use]
    pub fn with_timeout(mut self, dur: Duration) -> Self {
        self.request_timeout = dur;
        self
    }

    /// Set a Bearer API key for remote Ollama deployments behind an auth
    /// proxy. Not required for the default local setup.
    #[must_use]
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Replace the prompt template and declare its response shape.
    ///
    /// The two MUST agree: a `StrictJson` template instructs the model to
    /// emit JSON and the parser expects JSON; a `PlainText` template
    /// instructs the model to emit a bare string and the parser wraps it
    /// verbatim. Pairing them wrong produces useless failures.
    ///
    /// Used by the CLI's `--prompt` override path to load v1 or a
    /// user-supplied template without rebuilding the backend.
    #[must_use]
    pub fn with_template(
        mut self,
        template: PromptTemplate,
        response_shape: PromptResponseShape,
    ) -> Self {
        self.template = template;
        self.response_shape = response_shape;
        self
    }

    /// Convenience: load the v1 plain-text template. Returns `self` with
    /// the template and response shape both set to v1.
    #[must_use]
    pub fn with_template_v1(self) -> Self {
        self.with_template(
            PromptTemplate::new(TEMPLATE_BODY_V1, "v1"),
            PromptResponseShape::PlainText,
        )
    }
}

impl Default for OllamaBackend {
    /// Delegates to [`OllamaBackend::new`]. Panics if construction fails,
    /// which only happens if the compile-time template is malformed.
    fn default() -> Self {
        Self::new().expect("OllamaBackend::default failed")
    }
}

impl TranslationBackend for OllamaBackend {
    fn name(&self) -> &str {
        "ollama"
    }

    fn is_deterministic(&self) -> bool {
        false
    }

    fn translate_batch(
        &self,
        batch: &Batch,
        locale: &Locale,
        glossary: Option<&Glossary>,
    ) -> Result<Vec<TranslationOutcome>, BackendError> {
        let mut outcomes = Vec::with_capacity(batch.units.len());

        let agent = build_agent(self.request_timeout)?;

        for unit in &batch.units {
            let register = glossary
                .and_then(|g| g.register_for(locale.id))
                .map(Register::to_locales_register)
                .map(register_from_locales)
                .unwrap_or_else(|| register_from_locales(locale.register));

            let ctx = PromptContext::new(unit, locale, register, glossary, &unit.flags);

            let outcome = if unit.plural_arity.is_some() {
                self.translate_plural(&agent, locale, &ctx)?
            } else {
                let prompt = self.template.render(&ctx);
                self.call_generate(&agent, &prompt)?
            };
            outcomes.push(outcome);
        }

        debug_assert_eq!(
            outcomes.len(),
            batch.units.len(),
            "translate_batch must return one outcome per unit"
        );
        Ok(outcomes)
    }
}

impl OllamaBackend {
    /// Translate a plural unit by issuing one HTTP call per CLDR plural
    /// slot in the target locale, in canonical order.
    ///
    /// On any per-form failure the whole unit becomes
    /// `Failed { reason: "ollama-plural-form-<form>: <inner>", kind:
    /// <inner kind> }`. Whole-batch errors (network, auth, protocol)
    /// bubble up via `?` and abort the batch — matches the singular
    /// path's behaviour.
    ///
    /// Per-form flags and notes from the model are merged on the way out:
    /// the union of all forms' semantic flags lands on the outcome, with
    /// the **first** non-empty note per flag winning. Confidence is
    /// reported as the **minimum** across forms (most pessimistic) since
    /// the plural unit as a whole is only as confident as its weakest
    /// form.
    fn translate_plural(
        &self,
        agent: &ureq::Agent,
        locale: &Locale,
        ctx: &PromptContext<'_>,
    ) -> Result<TranslationOutcome, BackendError> {
        let mut forms: Vec<String> = Vec::with_capacity(locale.cldr_plural.len());
        let mut merged_flags: Vec<Flag> = Vec::new();
        let mut merged_notes: BTreeMap<Flag, String> = BTreeMap::new();
        let mut min_confidence: Option<f32> = None;
        for category in locale.cldr_plural {
            // Re-render the prompt for each form so the template's
            // `{plural_category_line_or_empty}` token resolves to the
            // current CLDR category. The rest of the context is constant
            // across forms.
            let per_form_ctx = ctx.with_plural_category(*category);
            let prompt = self.template.render(&per_form_ctx);
            match self.call_generate(agent, &prompt)? {
                TranslationOutcome::Translated {
                    text: TranslatedText::Singular(s),
                    flags,
                    confidence,
                    flag_notes,
                } => {
                    forms.push(s);
                    for f in flags {
                        if !merged_flags.contains(&f) {
                            merged_flags.push(f);
                        }
                    }
                    for (f, note) in flag_notes {
                        merged_notes.entry(f).or_insert(note);
                    }
                    if let Some(c) = confidence {
                        min_confidence = Some(min_confidence.map_or(c, |prev| prev.min(c)));
                    }
                }
                TranslationOutcome::Translated {
                    text: TranslatedText::Plural(_),
                    ..
                } => {
                    // call_generate always returns Singular — defensive
                    // arm so adding new variants is a visible TODO, not a
                    // silent fall-through.
                    return Ok(TranslationOutcome::Failed {
                        reason: "ollama-plural-unexpected-shape".into(),
                        retryable: false,
                        failure_kind: FailureKind::MalformedResponse,
                    });
                }
                TranslationOutcome::Failed {
                    reason,
                    retryable,
                    failure_kind,
                } => {
                    return Ok(TranslationOutcome::Failed {
                        reason: format!("ollama-plural-form-{category}: {reason}"),
                        retryable,
                        failure_kind,
                    });
                }
                TranslationOutcome::Skipped { reason } => {
                    return Ok(TranslationOutcome::Skipped { reason });
                }
            }
        }
        Ok(TranslationOutcome::Translated {
            text: TranslatedText::Plural(forms),
            flags: merged_flags,
            confidence: min_confidence,
            flag_notes: merged_notes,
        })
    }

    /// Send one `POST /api/generate` request and map the response to a
    /// [`TranslationOutcome`].
    ///
    /// Returns `Err(BackendError)` only for whole-batch failures (network,
    /// auth, protocol). Empty or malformed per-unit responses map to
    /// `Ok(TranslationOutcome::Failed { .. })`. Malformed strict-JSON
    /// responses are tagged [`FailureKind::MalformedResponse`] so the
    /// Tauri layer can surface them as a gate finding instead of a
    /// backend infrastructure error.
    fn call_generate(
        &self,
        agent: &ureq::Agent,
        prompt: &str,
    ) -> Result<TranslationOutcome, BackendError> {
        let url = format!("{}/api/generate", self.host.trim_end_matches('/'));

        let body = serde_json::json!({
            "model": self.model,
            "prompt": prompt,
            "stream": false,
            "options": {
                "num_ctx": self.num_ctx,
            }
        });

        let mut req = agent.post(&url);
        if let Some(key) = &self.api_key {
            req = req.header("Authorization", format!("Bearer {key}"));
        }

        // ureq v3 maps 4xx/5xx to Err(ureq::Error::StatusCode(N)) by default,
        // so map_ureq_error handles auth/config/network error codes. Any
        // successful Ok(_) here will be a 2xx response.
        let response = req
            .send_json(&body)
            .map_err(|e| map_ureq_error(e, self.name()))?;

        let text = response
            .into_body()
            .read_to_string()
            .map_err(|e| BackendError::Protocol {
                backend: self.name().to_owned(),
                message: format!("failed to read response body: {e}"),
            })?;

        let parsed: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| BackendError::Protocol {
                backend: self.name().to_owned(),
                message: format!("ollama returned non-JSON response: {e}"),
            })?;

        let response_text = parsed
            .get("response")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_owned();

        if response_text.is_empty() {
            return Ok(TranslationOutcome::Failed {
                reason: "ollama-empty-response".into(),
                retryable: true,
                failure_kind: FailureKind::Unspecified,
            });
        }

        match self.response_shape {
            PromptResponseShape::PlainText => Ok(TranslationOutcome::Translated {
                text: TranslatedText::Singular(response_text),
                flags: Vec::new(),
                confidence: None,
                flag_notes: BTreeMap::new(),
            }),
            // `call_generate` issues one prompt per CLDR plural form (or
            // one prompt for a singular unit), so the model is always
            // being asked for a SINGLE form. An array payload would
            // mean the model ignored the per-form directive — surface
            // that as MalformedResponse so the prompt can be tuned, not
            // by writing a structurally invalid Plural target into a
            // singular slot.
            PromptResponseShape::StrictJson => {
                Ok(parse_v2_response(&response_text, ExpectedShape::Singular))
            }
        }
    }
}

/// Which response shape `parse_v2_response` should accept.
///
/// The v2 prompt schema permits either a bare string or an array (in case
/// a future "one call per plural unit" strategy lands). At today's call
/// sites the model is always asked for a single form, so the parser
/// rejects arrays as malformed; when the all-at-once strategy lands it
/// will pass [`Self::Plural`] with the locale's CLDR arity instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExpectedShape {
    /// The caller wants a singular string. Array payloads become
    /// `Failed { failure_kind: MalformedResponse }`.
    Singular,
    /// The caller wants a plural array of the given length. Singular
    /// payloads, or arrays of the wrong length, become
    /// `Failed { failure_kind: MalformedResponse }`. Not used by the
    /// current backend (kept so the parser surface does not need a
    /// breaking change when the all-at-once strategy lands).
    #[allow(dead_code)]
    Plural { arity: usize },
}

/// Parse a v2 strict-JSON response body and turn it into a
/// [`TranslationOutcome`].
///
/// `expected` pins what the caller asked the model to produce; a payload
/// of the wrong shape becomes `Failed { failure_kind: MalformedResponse,
/// retryable: false }`. This prevents an array reply from being silently
/// written into a singular [`i18n_harness_core::Unit`] slot — that would
/// violate the unit invariant (`plural_arity == None` implies
/// `Target::Singular`).
///
/// All parse / validation failures collapse to `Failed { failure_kind:
/// MalformedResponse, retryable: false }`. The `reason` is short and
/// machine-greppable so the Inspector can surface it verbatim; we
/// deliberately do **not** include the full response body in the reason
/// because models routinely emit thousands of tokens of preamble before
/// the JSON object and that would dominate the UI.
fn parse_v2_response(body: &str, expected: ExpectedShape) -> TranslationOutcome {
    let raw: V2Response = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => {
            return TranslationOutcome::Failed {
                reason: format!("v2-parse-error: {e}"),
                retryable: false,
                failure_kind: FailureKind::MalformedResponse,
            };
        }
    };

    if !raw.confidence.is_finite() || !(0.0..=1.0).contains(&raw.confidence) {
        return TranslationOutcome::Failed {
            reason: format!("v2-confidence-out-of-bounds: {}", raw.confidence),
            retryable: false,
            failure_kind: FailureKind::MalformedResponse,
        };
    }

    let text = match (raw.translation, expected) {
        (TranslationField::Singular(s), ExpectedShape::Singular) => TranslatedText::Singular(s),
        (TranslationField::Plural(forms), ExpectedShape::Plural { arity })
            if forms.len() == arity =>
        {
            TranslatedText::Plural(forms)
        }
        (TranslationField::Singular(_), ExpectedShape::Plural { arity }) => {
            return TranslationOutcome::Failed {
                reason: format!(
                    "v2-shape-mismatch: expected plural array of arity {arity}, got singular string"
                ),
                retryable: false,
                failure_kind: FailureKind::MalformedResponse,
            };
        }
        (TranslationField::Plural(forms), ExpectedShape::Singular) => {
            return TranslationOutcome::Failed {
                reason: format!(
                    "v2-shape-mismatch: expected singular string, got plural array of length {}",
                    forms.len()
                ),
                retryable: false,
                failure_kind: FailureKind::MalformedResponse,
            };
        }
        (TranslationField::Plural(forms), ExpectedShape::Plural { arity }) => {
            return TranslationOutcome::Failed {
                reason: format!(
                    "v2-shape-mismatch: expected plural array of arity {arity}, got length {}",
                    forms.len()
                ),
                retryable: false,
                failure_kind: FailureKind::MalformedResponse,
            };
        }
    };

    let mut flags: Vec<Flag> = Vec::new();
    let mut flag_notes: BTreeMap<Flag, String> = BTreeMap::new();
    for entry in raw.flags.unwrap_or_default() {
        if entry.kind.severity() != FlagSeverity::Semantic {
            // The prompt forbids gate-produced kinds; if the model emits
            // one anyway we treat the entire response as malformed
            // rather than silently dropping the flag — silent drop would
            // hide a prompt drift we want to see.
            return TranslationOutcome::Failed {
                reason: format!(
                    "v2-non-semantic-flag: {:?} is not in the allowed set",
                    entry.kind
                ),
                retryable: false,
                failure_kind: FailureKind::MalformedResponse,
            };
        }
        if !flags.contains(&entry.kind) {
            flags.push(entry.kind);
        }
        if let Some(note) = entry.note {
            if !note.is_empty() {
                flag_notes.insert(entry.kind, note);
            }
        }
    }

    TranslationOutcome::Translated {
        text,
        flags,
        confidence: Some(raw.confidence),
        flag_notes,
    }
}

/// Strict-JSON shape the v2 prompt instructs the model to emit. Unknown
/// fields are rejected — the failure becomes a `MalformedResponse`
/// outcome — so prompt drift is loud rather than silent.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct V2Response {
    translation: TranslationField,
    #[serde(default)]
    flags: Option<Vec<V2Flag>>,
    confidence: f32,
}

/// Singular vs plural shape of the `translation` field. Untagged so the
/// model emits a bare string or a bare array; the prompt example shows
/// both.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TranslationField {
    Singular(String),
    Plural(Vec<String>),
}

/// One flag entry inside the v2 `flags` array. `note` is optional.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct V2Flag {
    kind: Flag,
    #[serde(default)]
    note: Option<String>,
}

/// Build a `ureq::Agent` with the given timeout applied globally.
fn build_agent(timeout: Duration) -> Result<ureq::Agent, BackendError> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build();
    Ok(ureq::Agent::new_with_config(config))
}

/// Map a `ureq::Error` (transport-level) to a [`BackendError`].
///
/// `send_json` raises `ureq::Error::StatusCode` for non-2xx responses that
/// come back with a body; all other variants are transport failures.
fn map_ureq_error(e: ureq::Error, backend: &str) -> BackendError {
    match &e {
        ureq::Error::StatusCode(code) => {
            let code = *code;
            match code {
                401 | 403 => BackendError::Auth {
                    backend: backend.to_owned(),
                    message: format!("HTTP {code}"),
                },
                404 => BackendError::Configuration {
                    backend: backend.to_owned(),
                    message: format!("HTTP 404 — model not found or endpoint wrong: {e}"),
                },
                _ => BackendError::Network {
                    backend: backend.to_owned(),
                    message: format!("HTTP {code}: {e}"),
                },
            }
        }
        _ => BackendError::Network {
            backend: backend.to_owned(),
            message: e.to_string(),
        },
    }
}

/// 1:1 passthrough from the locales-crate `Register` to the glossary-crate
/// `Register`. Same rationale as `ManualBackend`'s version.
fn register_from_locales(r: i18n_harness_locales::Register) -> Register {
    match r {
        i18n_harness_locales::Register::Formal => Register::Formal,
        i18n_harness_locales::Register::Informal => Register::Informal,
        i18n_harness_locales::Register::Neutral => Register::Neutral,
    }
}

#[cfg(test)]
mod tests {
    //! Pure-function tests for the v2 strict-JSON parser. The mocked-HTTP
    //! tests for end-to-end behaviour live in `tests/ollama.rs`.

    use super::*;

    #[test]
    fn parse_v2_happy_path_singular() {
        let body = r#"{"translation":"Hallo","confidence":0.92}"#;
        match parse_v2_response(body, ExpectedShape::Singular) {
            TranslationOutcome::Translated {
                text,
                flags,
                confidence,
                flag_notes,
            } => {
                assert_eq!(text, TranslatedText::Singular("Hallo".into()));
                assert!(flags.is_empty());
                assert_eq!(confidence, Some(0.92));
                assert!(flag_notes.is_empty());
            }
            other => panic!("expected Translated, got {other:?}"),
        }
    }

    #[test]
    fn parse_v2_happy_path_plural() {
        // The Plural shape is for the future "one call returns all forms"
        // strategy; the parser surface already accepts it so adding that
        // strategy is non-breaking.
        let body = r#"{"translation":["1 Element","%n Elemente"],"confidence":0.8}"#;
        match parse_v2_response(body, ExpectedShape::Plural { arity: 2 }) {
            TranslationOutcome::Translated {
                text: TranslatedText::Plural(forms),
                confidence,
                ..
            } => {
                assert_eq!(forms, vec!["1 Element", "%n Elemente"]);
                assert_eq!(confidence, Some(0.8));
            }
            other => panic!("expected Translated plural, got {other:?}"),
        }
    }

    #[test]
    fn parse_v2_plural_payload_when_singular_expected_is_malformed() {
        // P1 codex finding: a singular call receiving an array reply must
        // surface as MalformedResponse so the singular Unit slot does not
        // get a Plural target written into it (which would violate the
        // Unit invariant `plural_arity == None ⇒ Target::Singular`).
        let body = r#"{"translation":["form-one","form-two"],"confidence":0.8}"#;
        match parse_v2_response(body, ExpectedShape::Singular) {
            TranslationOutcome::Failed {
                failure_kind,
                reason,
                ..
            } => {
                assert_eq!(failure_kind, FailureKind::MalformedResponse);
                assert!(reason.contains("v2-shape-mismatch"), "reason: {reason}");
            }
            other => panic!("expected Failed MalformedResponse, got {other:?}"),
        }
    }

    #[test]
    fn parse_v2_singular_payload_when_plural_expected_is_malformed() {
        let body = r#"{"translation":"only-one-form","confidence":0.8}"#;
        match parse_v2_response(body, ExpectedShape::Plural { arity: 2 }) {
            TranslationOutcome::Failed {
                failure_kind,
                reason,
                ..
            } => {
                assert_eq!(failure_kind, FailureKind::MalformedResponse);
                assert!(reason.contains("v2-shape-mismatch"), "reason: {reason}");
            }
            other => panic!("expected Failed MalformedResponse, got {other:?}"),
        }
    }

    #[test]
    fn parse_v2_plural_wrong_arity_is_malformed() {
        let body = r#"{"translation":["one","two","three"],"confidence":0.8}"#;
        match parse_v2_response(body, ExpectedShape::Plural { arity: 2 }) {
            TranslationOutcome::Failed {
                failure_kind,
                reason,
                ..
            } => {
                assert_eq!(failure_kind, FailureKind::MalformedResponse);
                assert!(reason.contains("v2-shape-mismatch"), "reason: {reason}");
            }
            other => panic!("expected Failed MalformedResponse, got {other:?}"),
        }
    }

    #[test]
    fn parse_v2_with_flags_and_notes() {
        let body = r#"{"translation":"Aufnahme","flags":[{"kind":"ambiguous-source","note":"could be noun or verb"},{"kind":"brand-term"}],"confidence":0.55}"#;
        match parse_v2_response(body, ExpectedShape::Singular) {
            TranslationOutcome::Translated {
                flags,
                flag_notes,
                confidence,
                ..
            } => {
                assert_eq!(flags, vec![Flag::AmbiguousSource, Flag::BrandTerm]);
                assert_eq!(
                    flag_notes.get(&Flag::AmbiguousSource),
                    Some(&"could be noun or verb".to_string())
                );
                assert!(
                    !flag_notes.contains_key(&Flag::BrandTerm),
                    "absent note must not insert an empty entry"
                );
                assert_eq!(confidence, Some(0.55));
            }
            other => panic!("expected Translated, got {other:?}"),
        }
    }

    #[test]
    fn parse_v2_unknown_flag_kind_is_malformed() {
        let body = r#"{"translation":"Hallo","flags":[{"kind":"made-up-flag"}],"confidence":0.9}"#;
        match parse_v2_response(body, ExpectedShape::Singular) {
            TranslationOutcome::Failed {
                failure_kind,
                reason,
                ..
            } => {
                assert_eq!(failure_kind, FailureKind::MalformedResponse);
                assert!(reason.contains("v2-parse-error"), "reason: {reason}");
            }
            other => panic!("expected Failed MalformedResponse, got {other:?}"),
        }
    }

    #[test]
    fn parse_v2_non_semantic_flag_is_malformed() {
        // PlaceholderMismatch is gate-produced; the prompt forbids it.
        let body =
            r#"{"translation":"Hallo","flags":[{"kind":"placeholder-mismatch"}],"confidence":0.9}"#;
        match parse_v2_response(body, ExpectedShape::Singular) {
            TranslationOutcome::Failed {
                failure_kind,
                reason,
                ..
            } => {
                assert_eq!(failure_kind, FailureKind::MalformedResponse);
                assert!(reason.contains("v2-non-semantic-flag"), "reason: {reason}");
            }
            other => panic!("expected Failed MalformedResponse, got {other:?}"),
        }
    }

    #[test]
    fn parse_v2_confidence_out_of_bounds_is_malformed() {
        let body = r#"{"translation":"Hallo","confidence":1.5}"#;
        match parse_v2_response(body, ExpectedShape::Singular) {
            TranslationOutcome::Failed {
                failure_kind,
                reason,
                ..
            } => {
                assert_eq!(failure_kind, FailureKind::MalformedResponse);
                assert!(
                    reason.contains("confidence-out-of-bounds"),
                    "reason: {reason}"
                );
            }
            other => panic!("expected Failed MalformedResponse, got {other:?}"),
        }

        let body = r#"{"translation":"Hallo","confidence":-0.1}"#;
        match parse_v2_response(body, ExpectedShape::Singular) {
            TranslationOutcome::Failed { failure_kind, .. } => {
                assert_eq!(failure_kind, FailureKind::MalformedResponse);
            }
            other => panic!("expected Failed MalformedResponse, got {other:?}"),
        }
    }

    #[test]
    fn parse_v2_missing_confidence_is_malformed() {
        // Confidence is required by the v2 contract; the prompt says so.
        let body = r#"{"translation":"Hallo"}"#;
        match parse_v2_response(body, ExpectedShape::Singular) {
            TranslationOutcome::Failed { failure_kind, .. } => {
                assert_eq!(failure_kind, FailureKind::MalformedResponse);
            }
            other => panic!("expected Failed MalformedResponse, got {other:?}"),
        }
    }

    #[test]
    fn parse_v2_unknown_top_level_field_is_malformed() {
        // deny_unknown_fields trips on any extension the model invents.
        let body = r#"{"translation":"Hallo","confidence":0.9,"extra":"oops"}"#;
        match parse_v2_response(body, ExpectedShape::Singular) {
            TranslationOutcome::Failed { failure_kind, .. } => {
                assert_eq!(failure_kind, FailureKind::MalformedResponse);
            }
            other => panic!("expected Failed MalformedResponse, got {other:?}"),
        }
    }

    #[test]
    fn parse_v2_non_json_body_is_malformed() {
        let body = "Hello, here is my translation: Hallo";
        match parse_v2_response(body, ExpectedShape::Singular) {
            TranslationOutcome::Failed { failure_kind, .. } => {
                assert_eq!(failure_kind, FailureKind::MalformedResponse);
            }
            other => panic!("expected Failed MalformedResponse, got {other:?}"),
        }
    }
}
