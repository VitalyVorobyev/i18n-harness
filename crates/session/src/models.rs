//! Model servers: what models are available, and the backend for one.

use i18n_harness_backend::TranslationBackend;
use i18n_harness_backend::discovery::{ModelInfo, PromptStyle, ServerKind};
use i18n_harness_project::{BackendConfig, BackendKind};
use serde::{Deserialize, Serialize};

use crate::error::{Result, SessionError};

/// Default address of a local Ollama server.
pub const DEFAULT_OLLAMA_HOST: &str = "http://localhost:11434";

/// Where models come from and which one to use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSettings {
    /// The kind of server.
    pub server: ServerKind,
    /// Server address (`http://localhost:11434`, `http://localhost:1234/v1`).
    pub host: String,
    /// The model to translate with.
    pub model: Option<String>,
    /// Context window in tokens, when the user overrode the default.
    pub num_ctx: Option<u32>,
}

impl Default for ModelSettings {
    fn default() -> Self {
        Self {
            server: ServerKind::Ollama,
            host: std::env::var("OLLAMA_HOST").unwrap_or_else(|_| DEFAULT_OLLAMA_HOST.to_owned()),
            model: None,
            num_ctx: None,
        }
    }
}

impl ModelSettings {
    /// Settings from a project's `[backend.default]` table.
    pub fn from_config(config: Option<&BackendConfig>) -> Self {
        let mut settings = Self::default();
        if let Some(config) = config {
            if config.kind == BackendKind::OpenAiCompatible {
                settings.server = ServerKind::OpenAiCompatible;
            }
            if let Some(host) = &config.host {
                settings.host.clone_from(host);
            }
            settings.model.clone_from(&config.model);
            settings.num_ctx = config.num_ctx;
        }
        settings
    }

    /// The `[backend.default]` table for these settings.
    pub fn to_config(&self) -> BackendConfig {
        BackendConfig {
            kind: match self.server {
                ServerKind::Ollama => BackendKind::Ollama,
                ServerKind::OpenAiCompatible => BackendKind::OpenAiCompatible,
            },
            model: self.model.clone(),
            host: Some(self.host.clone()),
            num_ctx: self.num_ctx,
        }
    }
}

/// A model a user can pick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelChoice {
    /// Identifier to store in the settings.
    pub id: String,
    /// Size on disk in bytes, when known.
    pub size_bytes: Option<u64>,
    /// Parameter count as the server prints it (`"9.2B"`).
    pub parameter_size: Option<String>,
    /// Quantization (`"Q8_0"`).
    pub quantization: Option<String>,
    /// Context window, when known.
    pub context_length: Option<u32>,
    /// How the model is prompted.
    pub prompt_style: PromptStyle,
}

impl From<ModelInfo> for ModelChoice {
    fn from(info: ModelInfo) -> Self {
        Self {
            id: info.summary.id,
            size_bytes: info.summary.size_bytes,
            parameter_size: info.summary.parameter_size,
            quantization: info.summary.quantization,
            context_length: info.context_length,
            prompt_style: info.prompt_style,
        }
    }
}

/// The models on the configured server that can translate (embedding-only
/// models are left out).
///
/// # Errors
///
/// [`SessionError::Backend`] when the server cannot be reached; for a local
/// Ollama server that usually means it is not running.
pub fn list_models(settings: &ModelSettings) -> Result<Vec<ModelChoice>> {
    use i18n_harness_backend::discovery::ModelProvider;
    let provider: Box<dyn ModelProvider> = match settings.server {
        #[cfg(feature = "ollama")]
        ServerKind::Ollama => Box::new(
            i18n_harness_backend::discovery::OllamaServer::new(&settings.host)
                .with_api_key(std::env::var("OLLAMA_API_KEY").ok()),
        ),
        #[cfg(feature = "openai-compatible")]
        ServerKind::OpenAiCompatible => Box::new(
            i18n_harness_backend::discovery::OpenAiServer::new(&settings.host)
                .with_api_key(std::env::var("OPENAI_API_KEY").ok()),
        ),
        #[allow(unreachable_patterns)]
        _ => return Err(unsupported(settings.server)),
    };
    let mut choices = Vec::new();
    for summary in provider.list_models()? {
        let info = provider.describe(&summary.id)?;
        if info.can_generate() {
            choices.push(ModelChoice::from(info));
        }
    }
    Ok(choices)
}

/// The translation backend for `settings`.
///
/// # Errors
///
/// [`SessionError::NeedsModel`] when no model is selected;
/// [`SessionError::Backend`] for a server kind this build cannot translate
/// with.
pub fn backend(settings: &ModelSettings) -> Result<Box<dyn TranslationBackend>> {
    let model = settings.model.clone().ok_or(SessionError::NeedsModel)?;
    match settings.server {
        #[cfg(feature = "ollama")]
        ServerKind::Ollama => {
            let mut backend = i18n_harness_backend::OllamaBackend::new()?
                .with_host(settings.host.clone())
                .with_model(model);
            if let Some(num_ctx) = settings.num_ctx {
                backend = backend.with_num_ctx(num_ctx);
            }
            Ok(Box::new(backend))
        }
        other => Err(unsupported(other)),
    }
}

fn unsupported(server: ServerKind) -> SessionError {
    SessionError::Backend(i18n_harness_backend::BackendError::Configuration {
        backend: format!("{server:?}"),
        message: "translating through this kind of server is not available in this build".into(),
    })
}
