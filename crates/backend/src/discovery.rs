//! Model discovery: which models a server offers, and how to prompt each.
//!
//! The harness ships no models and names none. Whatever a local server
//! (Ollama) or an OpenAI-compatible endpoint (LM Studio, vLLM, llama.cpp
//! server, a cloud API) reports is what the user can pick, at runtime.
//!
//! How a model wants to be prompted is read from what the server says about
//! it, never from its name: a GGUF model whose prompt template is a bare
//! pass-through (common for machine-translation fine-tunes trained on a raw
//! completion format) gets a completion-style prompt; anything with turn
//! markers, or any model that applies its tokenizer's own chat template,
//! gets a chat prompt.

use serde::{Deserialize, Serialize};

use crate::error::BackendError;

/// The kind of HTTP API a model server speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ServerKind {
    /// Ollama's native API (`/api/tags`, `/api/show`, `/api/chat`).
    Ollama,
    /// The OpenAI HTTP API (`/v1/models`, `/v1/chat/completions`).
    OpenAiCompatible,
}

/// How a model expects its prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PromptStyle {
    /// Instruction-following chat model: system and user messages.
    Chat,
    /// Raw text completion: the model continues a prompt written in its
    /// training format (no chat template applied).
    Completion,
    /// The server does not say; callers default to [`Self::Chat`].
    Unknown,
}

/// One model a server offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSummary {
    /// Identifier to send back to the server (`"name:tag"` for Ollama).
    pub id: String,
    /// On-disk size in bytes, when the server reports it.
    pub size_bytes: Option<u64>,
    /// Model family / architecture (`"llama"`, `"gemma3"`).
    pub family: Option<String>,
    /// Parameter count as the server prints it (`"9.2B"`).
    pub parameter_size: Option<String>,
    /// Quantization level (`"Q8_0"`).
    pub quantization: Option<String>,
    /// Content digest; changes when the model is re-pulled.
    pub digest: Option<String>,
}

/// Everything the harness needs to prompt one model well.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInfo {
    /// The model's summary entry.
    pub summary: ModelSummary,
    /// Maximum context length in tokens, when known.
    pub context_length: Option<u32>,
    /// Capabilities the server reports (`"completion"`, `"vision"`,
    /// `"embedding"`, `"tools"`). Empty when unknown.
    pub capabilities: Vec<String>,
    /// The server-side prompt template, when the server exposes it.
    pub template: Option<String>,
    /// How to prompt it, derived from `template`.
    pub prompt_style: PromptStyle,
}

impl ModelInfo {
    /// Whether the model can generate text at all (embedding-only models
    /// cannot translate). `true` when the server does not report
    /// capabilities.
    pub fn can_generate(&self) -> bool {
        self.capabilities.is_empty()
            || self
                .capabilities
                .iter()
                .any(|c| c == "completion" || c == "chat")
    }
}

/// A server that can report its models.
pub trait ModelProvider {
    /// Which API the server speaks.
    fn kind(&self) -> ServerKind;

    /// The models the server offers, in the server's order.
    ///
    /// # Errors
    ///
    /// [`BackendError::Network`] when the server is unreachable (for a local
    /// server: not running), [`BackendError::Auth`] on a rejected key,
    /// [`BackendError::Protocol`] on an unexpected response.
    fn list_models(&self) -> Result<Vec<ModelSummary>, BackendError>;

    /// Details for one model.
    ///
    /// # Errors
    ///
    /// As [`Self::list_models`]; [`BackendError::Configuration`] when the
    /// model does not exist.
    fn describe(&self, id: &str) -> Result<ModelInfo, BackendError>;
}

/// Classify how to prompt a model from what an Ollama server reports.
///
/// The Go `template` formats the prompt only for GGUF models (the llama.cpp
/// runner). Safetensors/MLX models are rendered by the tokenizer's own chat
/// template whatever the Go template says, and a model that reports
/// `thinking` or `tools` is a chat model. Only a GGUF model with a
/// pass-through template is a completion model.
pub fn classify_prompt_style(
    template: Option<&str>,
    format: Option<&str>,
    capabilities: &[String],
) -> PromptStyle {
    let chat_capability = capabilities.iter().any(|c| c == "thinking" || c == "tools");
    match format {
        Some(f) if !f.eq_ignore_ascii_case("gguf") => return PromptStyle::Chat,
        _ if chat_capability => return PromptStyle::Chat,
        _ => {}
    }
    prompt_style_of_template(template)
}

/// Classify a server-side prompt template by its shape.
///
/// A template that is nothing but `{{ … }}` actions (Ollama's Go template
/// syntax) adds no turn markers, so the model receives the prompt verbatim:
/// it must be prompted in its own completion format. A template with any
/// literal text around the actions (`<|im_start|>user`, `<start_of_turn>`,
/// `[INST]`) is a chat template.
pub fn prompt_style_of_template(template: Option<&str>) -> PromptStyle {
    let Some(template) = template else {
        return PromptStyle::Unknown;
    };
    if template.trim().is_empty() {
        // No template at all: the runtime decides; nothing to go on.
        return PromptStyle::Unknown;
    }
    let mut literal = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        literal.push_str(&rest[..start]);
        match rest[start..].find("}}") {
            Some(end) => rest = &rest[start + end + 2..],
            // Unterminated action: treat the remainder as literal text.
            None => {
                literal.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    literal.push_str(rest);
    if literal.trim().is_empty() {
        PromptStyle::Completion
    } else {
        PromptStyle::Chat
    }
}

#[cfg(feature = "ollama")]
pub use ollama_server::OllamaServer;

#[cfg(feature = "openai-compatible")]
pub use openai_server::OpenAiServer;

#[cfg(feature = "ollama")]
mod ollama_server {
    use std::time::Duration;

    use serde_json::{Value, json};

    use super::{ModelInfo, ModelProvider, ModelSummary, ServerKind, classify_prompt_style};
    use crate::error::BackendError;
    use crate::http::{build_agent, request_json};

    /// Discovery client for an Ollama server.
    #[derive(Debug, Clone)]
    pub struct OllamaServer {
        host: String,
        api_key: Option<String>,
        timeout: Duration,
    }

    impl OllamaServer {
        /// Client for the server at `host` (e.g. `http://localhost:11434`).
        pub fn new(host: impl Into<String>) -> Self {
            Self {
                host: host.into(),
                api_key: None,
                timeout: Duration::from_secs(10),
            }
        }

        /// Send `Authorization: Bearer <key>` (for a proxied server).
        #[must_use]
        pub fn with_api_key(mut self, key: Option<String>) -> Self {
            self.api_key = key;
            self
        }

        /// Request timeout (default 10 s; discovery calls are local and
        /// small).
        #[must_use]
        pub fn with_timeout(mut self, timeout: Duration) -> Self {
            self.timeout = timeout;
            self
        }

        fn url(&self, path: &str) -> String {
            format!("{}{path}", self.host.trim_end_matches('/'))
        }

        fn call(&self, path: &str, body: Option<&Value>) -> Result<Value, BackendError> {
            request_json(
                &build_agent(self.timeout),
                &self.url(path),
                body,
                self.api_key.as_deref(),
                "ollama",
            )
        }
    }

    fn str_field(v: &Value, key: &str) -> Option<String> {
        v.get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    }

    fn summary_from(entry: &Value, id: String) -> ModelSummary {
        let details = entry.get("details").cloned().unwrap_or(Value::Null);
        ModelSummary {
            id,
            size_bytes: entry.get("size").and_then(Value::as_u64),
            family: str_field(&details, "family"),
            parameter_size: str_field(&details, "parameter_size"),
            quantization: str_field(&details, "quantization_level"),
            digest: str_field(entry, "digest"),
        }
    }

    impl ModelProvider for OllamaServer {
        fn kind(&self) -> ServerKind {
            ServerKind::Ollama
        }

        fn list_models(&self) -> Result<Vec<ModelSummary>, BackendError> {
            let tags = self.call("/api/tags", None)?;
            let models = tags
                .get("models")
                .and_then(Value::as_array)
                .ok_or_else(|| BackendError::Protocol {
                    backend: "ollama".into(),
                    message: "/api/tags response has no `models` array".into(),
                })?;
            Ok(models
                .iter()
                .filter_map(|m| {
                    let id = str_field(m, "model").or_else(|| str_field(m, "name"))?;
                    Some(summary_from(m, id))
                })
                .collect())
        }

        fn describe(&self, id: &str) -> Result<ModelInfo, BackendError> {
            let show = self.call("/api/show", Some(&json!({ "model": id })))?;
            let mut summary = summary_from(&show, id.to_owned());
            if summary.digest.is_none() {
                summary.digest = self
                    .list_models()
                    .ok()
                    .and_then(|all| all.into_iter().find(|m| m.id == id))
                    .and_then(|m| m.digest);
            }
            let model_info = show.get("model_info").cloned().unwrap_or(Value::Null);
            let context_length = model_info.as_object().and_then(|info| {
                info.iter()
                    .find(|(k, _)| k.ends_with(".context_length"))
                    .and_then(|(_, v)| v.as_u64())
                    .and_then(|n| u32::try_from(n).ok())
            });
            let capabilities: Vec<String> = show
                .get("capabilities")
                .and_then(Value::as_array)
                .map(|caps| {
                    caps.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            let template = show
                .get("template")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let format = show
                .get("details")
                .and_then(|d| d.get("format"))
                .and_then(Value::as_str);
            let prompt_style = classify_prompt_style(template.as_deref(), format, &capabilities);
            Ok(ModelInfo {
                summary,
                context_length,
                capabilities,
                template,
                prompt_style,
            })
        }
    }
}

#[cfg(feature = "openai-compatible")]
mod openai_server {
    use std::time::Duration;

    use serde_json::Value;

    use super::{ModelInfo, ModelProvider, ModelSummary, PromptStyle, ServerKind};
    use crate::error::BackendError;
    use crate::http::{build_agent, request_json};

    /// Discovery client for an OpenAI-compatible server.
    #[derive(Debug, Clone)]
    pub struct OpenAiServer {
        base_url: String,
        api_key: Option<String>,
        timeout: Duration,
    }

    impl OpenAiServer {
        /// Client for the API rooted at `base_url` (the part before
        /// `/models`, e.g. `http://localhost:1234/v1`).
        pub fn new(base_url: impl Into<String>) -> Self {
            Self {
                base_url: base_url.into(),
                api_key: None,
                timeout: Duration::from_secs(10),
            }
        }

        /// Send `Authorization: Bearer <key>`.
        #[must_use]
        pub fn with_api_key(mut self, key: Option<String>) -> Self {
            self.api_key = key;
            self
        }

        /// Request timeout (default 10 s).
        #[must_use]
        pub fn with_timeout(mut self, timeout: Duration) -> Self {
            self.timeout = timeout;
            self
        }
    }

    impl ModelProvider for OpenAiServer {
        fn kind(&self) -> ServerKind {
            ServerKind::OpenAiCompatible
        }

        fn list_models(&self) -> Result<Vec<ModelSummary>, BackendError> {
            let url = format!("{}/models", self.base_url.trim_end_matches('/'));
            let body = request_json(
                &build_agent(self.timeout),
                &url,
                None,
                self.api_key.as_deref(),
                "openai-compatible",
            )?;
            let data = body.get("data").and_then(Value::as_array).ok_or_else(|| {
                BackendError::Protocol {
                    backend: "openai-compatible".into(),
                    message: "/models response has no `data` array".into(),
                }
            })?;
            Ok(data
                .iter()
                .filter_map(|m| m.get("id").and_then(Value::as_str))
                .map(|id| ModelSummary {
                    id: id.to_owned(),
                    size_bytes: None,
                    family: None,
                    parameter_size: None,
                    quantization: None,
                    digest: None,
                })
                .collect())
        }

        /// The OpenAI API exposes no template or context length; the model is
        /// assumed to be a chat model, which is what `/v1/chat/completions`
        /// requires anyway.
        fn describe(&self, id: &str) -> Result<ModelInfo, BackendError> {
            let summary = self
                .list_models()?
                .into_iter()
                .find(|m| m.id == id)
                .ok_or_else(|| BackendError::Configuration {
                    backend: "openai-compatible".into(),
                    message: format!("model `{id}` is not offered by {}", self.base_url),
                })?;
            Ok(ModelInfo {
                summary,
                context_length: None,
                capabilities: Vec::new(),
                template: None,
                prompt_style: PromptStyle::Chat,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pass_through_template_means_completion() {
        let t = "{{ if .System }}{{ .System }}{{ end }}{{ if .Prompt }}{{ .Prompt }}{{ end }}{{ .Response }}";
        assert_eq!(prompt_style_of_template(Some(t)), PromptStyle::Completion);
        assert_eq!(
            prompt_style_of_template(Some("{{ .Prompt }}")),
            PromptStyle::Completion
        );
        assert_eq!(prompt_style_of_template(Some("  ")), PromptStyle::Unknown);
    }

    #[test]
    fn templates_with_turn_markers_mean_chat() {
        let chatml = "{{ if .System }}<|im_start|>system\n{{ .System }}<|im_end|>\n{{ end }}\
                      {{ if .Prompt }}<|im_start|>user\n{{ .Prompt }}<|im_end|>\n{{ end }}\
                      <|im_start|>assistant\n{{ .Response }}<|im_end|>";
        assert_eq!(prompt_style_of_template(Some(chatml)), PromptStyle::Chat);
        let gemma = "<start_of_turn>user\n{{ .Prompt }}<end_of_turn>\n<start_of_turn>model\n";
        assert_eq!(prompt_style_of_template(Some(gemma)), PromptStyle::Chat);
    }

    #[test]
    fn only_gguf_models_without_chat_capabilities_can_be_completion() {
        let pass_through = Some("{{ .Prompt }}");
        let caps = |c: &[&str]| c.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            classify_prompt_style(pass_through, Some("gguf"), &caps(&["completion", "vision"])),
            PromptStyle::Completion
        );
        // MLX / safetensors: the tokenizer's chat template applies.
        assert_eq!(
            classify_prompt_style(pass_through, Some("safetensors"), &caps(&["completion"])),
            PromptStyle::Chat
        );
        assert_eq!(
            classify_prompt_style(
                pass_through,
                Some("gguf"),
                &caps(&["completion", "thinking"])
            ),
            PromptStyle::Chat
        );
    }

    #[test]
    fn missing_template_is_unknown() {
        assert_eq!(prompt_style_of_template(None), PromptStyle::Unknown);
    }

    #[test]
    fn embedding_only_models_cannot_generate() {
        let info = |caps: &[&str]| ModelInfo {
            summary: ModelSummary {
                id: "m".into(),
                size_bytes: None,
                family: None,
                parameter_size: None,
                quantization: None,
                digest: None,
            },
            context_length: None,
            capabilities: caps.iter().map(|s| (*s).to_owned()).collect(),
            template: None,
            prompt_style: PromptStyle::Unknown,
        };
        assert!(!info(&["embedding"]).can_generate());
        assert!(info(&["completion", "vision"]).can_generate());
        assert!(info(&[]).can_generate());
    }
}
