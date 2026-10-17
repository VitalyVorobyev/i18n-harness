//! Process-wide state: the open session and the running translation job.

use std::sync::{Mutex, MutexGuard};

use i18n_harness_session::{CancelToken, Session};

use crate::error::{CommandError, CommandResult};

/// Shared by every command (`tauri::Builder::manage`).
#[derive(Default)]
pub struct AppState {
    session: Mutex<Option<Session>>,
    job: Mutex<Option<CancelToken>>,
}

impl AppState {
    /// Lock the session slot (poisoning is recovered: the session holds no
    /// invariants a panic between two statements could break).
    pub(crate) fn slot(&self) -> MutexGuard<'_, Option<Session>> {
        self.session.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Run `f` with the open session.
    pub(crate) fn with<T>(
        &self,
        f: impl FnOnce(&mut Session) -> CommandResult<T>,
    ) -> CommandResult<T> {
        let mut slot = self.slot();
        let session = slot.as_mut().ok_or_else(CommandError::no_folder)?;
        f(session)
    }

    /// Register a new job; fails when one is already running.
    pub(crate) fn begin_job(&self) -> CommandResult<CancelToken> {
        let mut job = self.job.lock().unwrap_or_else(|e| e.into_inner());
        if job.is_some() {
            return Err(CommandError::busy());
        }
        let token = CancelToken::new();
        *job = Some(token.clone());
        Ok(token)
    }

    /// Forget the running job.
    pub(crate) fn end_job(&self) {
        *self.job.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// Cancel the running job, if any.
    pub(crate) fn cancel_job(&self) {
        if let Some(token) = self.job.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            token.cancel();
        }
    }
}
