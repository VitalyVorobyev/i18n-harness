//! The one error type of the session layer.

use std::path::PathBuf;

use serde::Serialize;

/// Why a session operation failed.
///
/// Serializes as `{ "kind": "...", "message": "..." }` so a UI can branch on
/// the kind (for instance to show "start Ollama" help on [`Self::Backend`])
/// and show the message as is.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// The folder or file could not be opened as a project.
    #[error("{0}")]
    Project(#[from] i18n_harness_project::ProjectError),

    /// A catalog file could not be read, parsed or written.
    #[error("{0}")]
    Catalog(#[from] i18n_harness_formats::CatalogError),

    /// The path is not one of the session's open catalogs.
    #[error("{} is not open in this session", path.display())]
    UnknownCatalog {
        /// The path that was asked for.
        path: PathBuf,
    },

    /// The unit id does not exist in the catalog.
    #[error("no string with id `{id}` in {}", path.display())]
    UnknownUnit {
        /// Catalog path.
        path: PathBuf,
        /// The unit id that was asked for.
        id: String,
    },

    /// The unit cannot be edited (it was removed from the source code).
    #[error("`{id}` is no longer used by the application and cannot be edited")]
    NotEditable {
        /// Unit id.
        id: String,
    },

    /// The catalog's language is not one the harness knows.
    #[error("unknown language `{0}`")]
    UnknownLocale(String),

    /// An edited text uses placeholder syntax the format cannot store.
    #[error("{0}")]
    Placeholder(#[from] i18n_harness_formats::PlaceholderError),

    /// No model is selected for translation.
    #[error("no translation model is selected")]
    NeedsModel,

    /// The model server failed (not running, unreachable, rejected the
    /// request, or returned something unusable).
    #[error("{0}")]
    Backend(#[from] i18n_harness_backend::BackendError),

    /// The operation was cancelled.
    #[error("cancelled")]
    Cancelled,

    /// Any other file-system failure.
    #[error("{context}: {source}")]
    Io {
        /// What the session was doing.
        context: String,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
}

impl SessionError {
    /// Stable machine-readable kind.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Project(_) => "project",
            Self::Catalog(_) => "catalog",
            Self::UnknownCatalog { .. } => "unknown-catalog",
            Self::UnknownUnit { .. } => "unknown-unit",
            Self::NotEditable { .. } => "not-editable",
            Self::UnknownLocale(_) => "unknown-locale",
            Self::Placeholder(_) => "placeholder",
            Self::NeedsModel => "needs-model",
            Self::Backend(_) => "backend",
            Self::Cancelled => "cancelled",
            Self::Io { .. } => "io",
        }
    }
}

impl Serialize for SessionError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("SessionError", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

/// Result alias for the session layer.
pub type Result<T, E = SessionError> = std::result::Result<T, E>;
