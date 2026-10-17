//! The error every command returns.

use i18n_harness_session::SessionError;
use serde::Serialize;

/// A command failure as the frontend receives it: `{ kind, message }`.
#[derive(Debug, Serialize)]
pub(crate) struct CommandError {
    kind: &'static str,
    message: String,
}

impl CommandError {
    pub(crate) fn no_folder() -> Self {
        Self {
            kind: "no-folder",
            message: "no folder is open".into(),
        }
    }

    pub(crate) fn busy() -> Self {
        Self {
            kind: "busy",
            message: "a translation run is already in progress".into(),
        }
    }

    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self {
            kind: "internal",
            message: message.into(),
        }
    }
}

impl From<SessionError> for CommandError {
    fn from(e: SessionError) -> Self {
        Self {
            kind: e.kind(),
            message: e.to_string(),
        }
    }
}

pub(crate) type CommandResult<T> = Result<T, CommandError>;
