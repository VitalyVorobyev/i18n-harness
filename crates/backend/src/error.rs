//! [`BackendError`] — whole-batch failure modes.
//!
//! Per-unit failures are not represented here; they live inside
//! [`crate::TranslationOutcome::Failed`]. `BackendError` is what a backend
//! returns when it could not even attempt the batch — network down, model
//! unloaded, auth failed, request malformed.

use thiserror::Error;

/// Reasons a [`crate::TranslationBackend::translate_batch`] call can fail
/// at the **whole-batch** level.
///
/// The variant set is closed; new variants are SemVer breaking on the
/// trait surface. Backends that need a more granular reason embed it in
/// the `message` field of [`Self::Backend`].
#[derive(Debug, Error)]
pub enum BackendError {
    /// Network/IO transport failure (HTTP socket closed, DNS failed,
    /// connection refused).
    ///
    /// Caller policy: typically retry the whole batch with backoff; if it
    /// keeps failing, surface to the user.
    #[error("network error talking to backend `{backend}`: {message}")]
    Network {
        /// Backend name (the value of [`crate::TranslationBackend::name`]).
        backend: String,
        /// Underlying message; verbatim from the transport library.
        message: String,
    },

    /// The backend responded but the response could not be parsed or did
    /// not match the expected shape.
    ///
    /// Caller policy: do **not** retry (the prompt itself produced
    /// garbage); abort the batch and surface to the user.
    #[error("backend `{backend}` returned malformed response: {message}")]
    Protocol {
        /// Backend name.
        backend: String,
        /// What went wrong — JSON parse error, missing field, etc.
        message: String,
    },

    /// The backend rejected the request for credential reasons (HTTP 401,
    /// 403, expired key).
    ///
    /// Caller policy: do not retry; abort and surface the user's
    /// credential is the problem.
    #[error("backend `{backend}` rejected authentication: {message}")]
    Auth {
        /// Backend name.
        backend: String,
        /// What the backend said.
        message: String,
    },

    /// The backend requires a configuration value the caller did not
    /// provide (model name, endpoint URL, glossary path).
    ///
    /// Caller policy: do not retry; surface the missing setting.
    #[error("backend `{backend}` misconfigured: {message}")]
    Configuration {
        /// Backend name.
        backend: String,
        /// Which setting is missing/invalid.
        message: String,
    },

    /// The backend was given a batch it cannot serve (empty, too large,
    /// units the backend's prompt template cannot render).
    ///
    /// Caller policy: do not retry as-is; either re-batch with a smaller
    /// size or skip the offending units.
    #[error("backend `{backend}` cannot serve batch: {message}")]
    UnsupportedBatch {
        /// Backend name.
        backend: String,
        /// What about the batch is unsupported.
        message: String,
    },

    /// Catch-all for backend-specific failures that do not fit the
    /// variants above. The string is shown to the user; backends should
    /// use this sparingly.
    #[error("backend `{backend}` error: {message}")]
    Backend {
        /// Backend name.
        backend: String,
        /// Free-form message.
        message: String,
    },
}

impl BackendError {
    /// Construct a [`Self::Network`] error from a backend name and message.
    pub fn network(backend: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Network {
            backend: backend.into(),
            message: message.into(),
        }
    }

    /// Construct a [`Self::Protocol`] error.
    pub fn protocol(backend: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Protocol {
            backend: backend.into(),
            message: message.into(),
        }
    }

    /// Construct a [`Self::Configuration`] error.
    pub fn configuration(backend: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Configuration {
            backend: backend.into(),
            message: message.into(),
        }
    }

    /// Construct a [`Self::UnsupportedBatch`] error.
    pub fn unsupported_batch(backend: impl Into<String>, message: impl Into<String>) -> Self {
        Self::UnsupportedBatch {
            backend: backend.into(),
            message: message.into(),
        }
    }

    /// Return the backend name attached to the error, for metrics logging.
    pub fn backend_name(&self) -> &str {
        match self {
            Self::Network { backend, .. }
            | Self::Protocol { backend, .. }
            | Self::Auth { backend, .. }
            | Self::Configuration { backend, .. }
            | Self::UnsupportedBatch { backend, .. }
            | Self::Backend { backend, .. } => backend,
        }
    }
}
