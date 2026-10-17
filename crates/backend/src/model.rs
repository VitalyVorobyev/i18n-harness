//! [`ModelBackend`]: any model on an Ollama or OpenAI-compatible server,
//! prompted through a [`Profile`].

use std::collections::BTreeMap;
use std::time::Duration;

use i18n_harness_core::Batch;
use i18n_harness_glossary::{Glossary, Register};
use i18n_harness_locales::Locale;
use serde_json::{Value, json};

use crate::discovery::ServerKind;
use crate::error::BackendError;
use crate::http::{build_agent, request_json};
use crate::outcome::{FailureKind, TranslatedText, TranslationOutcome};
use crate::profile::{Profile, ProfileMode, Request, clean_output};
use crate::trait_def::TranslationBackend;

/// Default request timeout: a slow local model on a long string.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);

/// A model on a server, prompted through a profile.
///
/// Translates one single-form text per request and returns the cleaned
/// answer. Plural units are expanded into single forms by the caller (the
/// session substitutes a sample count for the count placeholder), so a
/// plural unit reaching this backend fails instead of being guessed at.
#[derive(Debug, Clone)]
pub struct ModelBackend {
    server: ServerKind,
    host: String,
    model: String,
    profile: Profile,
    api_key: Option<String>,
    num_ctx: Option<u32>,
    timeout: Duration,
}

impl ModelBackend {
    /// `model` on the `server` at `host`, prompted with `profile`.
    pub fn new(
        server: ServerKind,
        host: impl Into<String>,
        model: impl Into<String>,
        profile: Profile,
    ) -> Self {
        Self {
            server,
            host: host.into(),
            model: model.into(),
            profile,
            api_key: None,
            num_ctx: None,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Send `Authorization: Bearer <key>`.
    #[must_use]
    pub fn with_api_key(mut self, key: Option<String>) -> Self {
        self.api_key = key;
        self
    }

    /// Context window to request (Ollama only).
    #[must_use]
    pub fn with_num_ctx(mut self, num_ctx: Option<u32>) -> Self {
        self.num_ctx = num_ctx;
        self
    }

    /// Request timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// The profile in use.
    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    /// The model id.
    pub fn model(&self) -> &str {
        &self.model
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.host.trim_end_matches('/'))
    }

    /// Ask the model; returns its raw answer.
    fn ask(&self, agent: &ureq::Agent, request: &Request<'_>) -> Result<String, BackendError> {
        let text = request.unit.source.as_str();
        let prompt = self.profile.render(&self.profile.prompt, request);
        let system = self
            .profile
            .system
            .as_deref()
            .map(|s| self.profile.render(s, request));
        let stop = self.profile.stop_for(text);
        let max_tokens = output_token_limit(text);
        let o = &self.profile.options;
        let key = self.api_key.as_deref();
        let backend = self.name();

        match self.server {
            ServerKind::Ollama => {
                let mut options = json!({ "temperature": o.temperature });
                let opts = options.as_object_mut().expect("json object");
                if let Some(seed) = o.seed {
                    opts.insert("seed".into(), seed.into());
                }
                if let Some(k) = o.top_k {
                    opts.insert("top_k".into(), k.into());
                }
                if let Some(p) = o.top_p {
                    opts.insert("top_p".into(), json!(p));
                }
                if let Some(r) = o.repeat_penalty {
                    opts.insert("repeat_penalty".into(), json!(r));
                }
                if let Some(n) = self.num_ctx {
                    opts.insert("num_ctx".into(), n.into());
                }
                opts.insert("num_predict".into(), max_tokens.into());
                if !stop.is_empty() {
                    opts.insert("stop".into(), json!(stop));
                }
                let mut body = match self.profile.mode {
                    ProfileMode::Chat => {
                        let mut messages = Vec::new();
                        if let Some(system) = &system {
                            messages.push(json!({ "role": "system", "content": system }));
                        }
                        messages.push(json!({ "role": "user", "content": prompt }));
                        json!({ "model": self.model, "messages": messages, "stream": false, "options": options })
                    }
                    ProfileMode::Completion => json!({
                        "model": self.model, "prompt": prompt, "raw": true, "stream": false, "options": options
                    }),
                };
                if let Some(keep_alive) = &self.profile.keep_alive {
                    body["keep_alive"] = json!(keep_alive);
                }
                let path = match self.profile.mode {
                    ProfileMode::Chat => "/api/chat",
                    ProfileMode::Completion => "/api/generate",
                };
                let response = request_json(agent, &self.url(path), Some(&body), key, backend)?;
                let answer = match self.profile.mode {
                    ProfileMode::Chat => response.pointer("/message/content"),
                    ProfileMode::Completion => response.get("response"),
                };
                answer
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .ok_or_else(|| protocol(backend, "the answer has no text"))
            }
            ServerKind::OpenAiCompatible => {
                // The OpenAI API accepts at most four stop sequences.
                let stop: Vec<&String> = stop.iter().take(4).collect();
                let mut body = match self.profile.mode {
                    ProfileMode::Chat => {
                        let mut messages = Vec::new();
                        if let Some(system) = &system {
                            messages.push(json!({ "role": "system", "content": system }));
                        }
                        messages.push(json!({ "role": "user", "content": prompt }));
                        json!({ "model": self.model, "messages": messages, "stream": false })
                    }
                    ProfileMode::Completion => {
                        json!({ "model": self.model, "prompt": prompt, "stream": false })
                    }
                };
                body["temperature"] = json!(o.temperature);
                body["max_tokens"] = json!(max_tokens);
                if let Some(seed) = o.seed {
                    body["seed"] = json!(seed);
                }
                if let Some(p) = o.top_p {
                    body["top_p"] = json!(p);
                }
                if !stop.is_empty() {
                    body["stop"] = json!(stop);
                }
                let path = match self.profile.mode {
                    ProfileMode::Chat => "/chat/completions",
                    ProfileMode::Completion => "/completions",
                };
                let response = request_json(agent, &self.url(path), Some(&body), key, backend)?;
                let answer = match self.profile.mode {
                    ProfileMode::Chat => response.pointer("/choices/0/message/content"),
                    ProfileMode::Completion => response.pointer("/choices/0/text"),
                };
                answer
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .ok_or_else(|| protocol(backend, "the answer has no text"))
            }
        }
    }
}

/// Most tokens a translation of `text` may take. A model that loops would
/// otherwise generate until the request times out; a UI translation is never
/// many times longer than its original.
fn output_token_limit(text: &str) -> u32 {
    let chars = u32::try_from(text.chars().count()).unwrap_or(u32::MAX);
    chars.saturating_mul(2).saturating_add(64)
}

fn protocol(backend: &str, message: &str) -> BackendError {
    BackendError::Protocol {
        backend: backend.to_owned(),
        message: message.to_owned(),
    }
}

impl TranslationBackend for ModelBackend {
    fn name(&self) -> &str {
        match self.server {
            ServerKind::Ollama => "ollama",
            ServerKind::OpenAiCompatible => "openai-compatible",
        }
    }

    fn is_deterministic(&self) -> bool {
        self.profile.options.temperature == 0.0 && self.profile.options.seed.is_some()
    }

    fn translate_batch(
        &self,
        batch: &Batch,
        locale: &Locale,
        glossary: Option<&Glossary>,
    ) -> Result<Vec<TranslationOutcome>, BackendError> {
        let english = Locale::by_id("en").expect("CLDR knows English");
        self.translate_batch_from(batch, english, locale, glossary)
    }

    fn translate_batch_from(
        &self,
        batch: &Batch,
        source: &Locale,
        locale: &Locale,
        glossary: Option<&Glossary>,
    ) -> Result<Vec<TranslationOutcome>, BackendError> {
        let agent = build_agent(self.timeout);
        let register =
            glossary
                .and_then(|g| g.register_for(locale.id))
                .unwrap_or(match locale.register {
                    i18n_harness_locales::Register::Formal => Register::Formal,
                    i18n_harness_locales::Register::Informal => Register::Informal,
                    i18n_harness_locales::Register::Neutral => Register::Neutral,
                });
        let mut outcomes = Vec::with_capacity(batch.units.len());
        for unit in &batch.units {
            if unit.plural_arity.is_some() {
                outcomes.push(TranslationOutcome::Failed {
                    reason: "plural units must be expanded into single forms before translation"
                        .into(),
                    retryable: false,
                    failure_kind: FailureKind::Unspecified,
                });
                continue;
            }
            let request = Request {
                unit,
                source,
                target: locale,
                register,
                glossary,
            };
            let raw = self.ask(&agent, &request)?;
            outcomes.push(match clean_output(&raw, &unit.source, locale) {
                Some(text) => TranslationOutcome::Translated {
                    text: TranslatedText::Singular(text),
                    flags: Vec::new(),
                    confidence: None,
                    flag_notes: BTreeMap::new(),
                },
                None => TranslationOutcome::Failed {
                    reason: "the model returned an empty answer".into(),
                    retryable: true,
                    failure_kind: FailureKind::Unspecified,
                },
            });
        }
        Ok(outcomes)
    }
}
