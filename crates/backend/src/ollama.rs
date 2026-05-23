//! Ollama HTTP backend — translates via `POST /api/generate` on a locally
//! running Ollama server.
//!
//! # Feature gate
//!
//! This module is compiled only with `--features ollama`. It brings in
//! `ureq` (sync HTTP, no async runtime) as its only extra dependency.
//!
//! # Prompt template
//!
//! The template lives at `crates/backend/prompts/ollama-translate-v1.txt`.
//! It is embedded at compile time via `include_str!` and parsed once in
//! [`OllamaBackend::new`]. Template tokens follow the v1 vocabulary defined
//! in [`crate::prompt`].
//!
//! # `num_ctx`
//!
//! Ollama's built-in default is 4 096 tokens, which silently truncates
//! batches that include long source strings plus a non-trivial glossary
//! block. The backend sets `num_ctx` to **8 192** on every request. This
//! covers the typical prompt size comfortably. Callers that need a larger
//! window can override via [`OllamaBackend::with_num_ctx`].
//!
//! # Plural units (v1 limitation)
//!
//! Plural units — i.e., units where `unit.plural_arity` is `Some(_)` —
//! return `TranslationOutcome::Failed { reason: "ollama-plural-not-supported-yet",
//! retryable: false }`. Wiring plural prompting cleanly (one prompt per CLDR
//! form, or a structured-output JSON prompt with aligned arity) is a
//! follow-up enhancement. Track in the project backlog.
//!
//! # Environment variables (read at construction only)
//!
//! - `OLLAMA_HOST` — base URL for the Ollama server, e.g.
//!   `http://192.168.1.10:11434`. Defaults to `http://localhost:11434`.
//! - `OLLAMA_API_KEY` — Bearer token for remote Ollama deployments that sit
//!   behind an auth proxy. Not required for the default local setup.
//!
//! Neither variable is read at translation time; the values are captured
//! once in [`OllamaBackend::new`] and held on `self`.
//!
//! # Performance
//!
//! v1 issues **one HTTP request per unit**. This is the simplest correct
//! implementation. Batched prompting (send N units in one request, parse a
//! JSON array response, re-align to batch order) is a future optimisation;
//! it introduces JSON-alignment complexity that is not worth the risk until
//! the per-unit quality metrics are established.

use std::time::Duration;

use i18n_harness_core::Batch;
use i18n_harness_glossary::{Glossary, Register};
use i18n_harness_locales::Locale;

use crate::context::PromptContext;
use crate::error::BackendError;
use crate::outcome::{TranslatedText, TranslationOutcome};
use crate::prompt::PromptTemplate;
use crate::trait_def::TranslationBackend;

/// The template text, embedded at compile time so there is no file-read at
/// translation time.
const TEMPLATE_BODY: &str = include_str!("../prompts/ollama-translate-v1.txt");

const DEFAULT_HOST: &str = "http://localhost:11434";
const DEFAULT_MODEL: &str = "gemma3:4b";
const DEFAULT_NUM_CTX: u32 = 8192;
const DEFAULT_TIMEOUT_SECS: u64 = 120;

/// Ollama HTTP backend.
///
/// Construct with [`OllamaBackend::new`] (reads `OLLAMA_HOST` /
/// `OLLAMA_API_KEY` from the environment) or via the builder methods.
pub struct OllamaBackend {
    host: String,
    model: String,
    num_ctx: u32,
    request_timeout: Duration,
    template: PromptTemplate,
    api_key: Option<String>,
}

impl OllamaBackend {
    /// Construct from environment variables and built-in defaults.
    ///
    /// Reads `OLLAMA_HOST` and `OLLAMA_API_KEY` once at construction.
    /// Never reads environment variables at translation time.
    pub fn new() -> Result<Self, BackendError> {
        let host = std::env::var("OLLAMA_HOST").unwrap_or_else(|_| DEFAULT_HOST.to_owned());
        let api_key = std::env::var("OLLAMA_API_KEY").ok();
        Ok(Self {
            host,
            model: DEFAULT_MODEL.to_owned(),
            num_ctx: DEFAULT_NUM_CTX,
            request_timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            template: PromptTemplate::new(TEMPLATE_BODY, "v1"),
            api_key,
        })
    }

    /// Override the Ollama server base URL (default: `http://localhost:11434`).
    #[must_use]
    pub fn with_host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    /// Override the model tag (default: `gemma3:4b`).
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
            // Plural support is a v1 follow-up; return a clean Failed outcome
            // so the caller can queue the unit for human review.
            if unit.plural_arity.is_some() {
                outcomes.push(TranslationOutcome::Failed {
                    reason: "ollama-plural-not-supported-yet".into(),
                    retryable: false,
                });
                continue;
            }

            let register = glossary
                .and_then(|g| g.register_for(locale.id))
                .map(Register::to_locales_register)
                .map(register_from_locales)
                .unwrap_or_else(|| register_from_locales(locale.register));

            let ctx = PromptContext::new(unit, locale, register, glossary, &unit.flags);
            let prompt = self.template.render(&ctx);
            let outcome = self.call_generate(&agent, &prompt)?;
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
    /// Send one `POST /api/generate` request and map the response to a
    /// [`TranslationOutcome`].
    ///
    /// Returns `Err(BackendError)` only for whole-batch failures (network,
    /// auth, protocol). Empty or malformed per-unit responses map to
    /// `Ok(TranslationOutcome::Failed { .. })`.
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

        let translated = parsed
            .get("response")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_owned();

        if translated.is_empty() {
            return Ok(TranslationOutcome::Failed {
                reason: "ollama-empty-response".into(),
                retryable: true,
            });
        }

        Ok(TranslationOutcome::Translated {
            text: TranslatedText::Singular(translated),
            flags: Vec::new(),
        })
    }
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
