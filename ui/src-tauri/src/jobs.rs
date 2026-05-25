//! In-process registry of cancellable background jobs.
//!
//! The registry pairs a [`JobId`] (UUID v4 hex string, chosen by the
//! registry, opaque to the caller) with the [`CancellationToken`] the
//! worker polls. The frontend holds the id between the
//! `translate_batch_in_project` response and a later `cancel_translation`
//! call; the worker thread uses the token to know when to break.
//!
//! # Invariants
//!
//! 1. **Ids are unique within a process lifetime.** Backed by UUID v4
//!    so the probability of a collision is effectively zero. Ids are
//!    not persisted across restarts; restarting the app drops every
//!    pending job (no resumability — see roadmap).
//! 2. **`cancel(id)` is idempotent.** Calling it twice for the same id
//!    is a no-op the second time (the token is sticky), and
//!    cancelling an unknown id returns `false` rather than erroring —
//!    races between the worker exiting and the user clicking Cancel
//!    are expected.
//! 3. **The worker is responsible for `deregister`.** Successful
//!    completion, internal failure, and observed-cancellation all
//!    deregister; the registry never garbage-collects on its own.
//!    Leaks would only happen if the worker panics without unwind
//!    cleanup; for the M4.2c.2 surface (Ollama-backed translates) this
//!    is treated as best-effort.
//! 4. **No reuse of cancelled ids.** A cancelled job that the worker
//!    later removes can never be re-registered with the same id (UUIDs
//!    are not recycled). The frontend re-issues a fresh job on retry.
//!
//! # What this is NOT
//!
//! - Not a queue. The registry tracks running jobs but does not
//!   schedule them; the Tauri command spawns the worker thread itself.
//! - Not a state machine. Cancelled / completed / failed are
//!   distinctions surfaced via Tauri events; the registry only knows
//!   "alive" vs "removed".
//! - Not persistent. In-flight jobs do not survive an app restart.

use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::cancellation::CancellationToken;

/// Stable, opaque, process-unique identifier for one background job.
///
/// Format: lowercase hex UUID v4 with hyphens stripped (32 chars). The
/// frontend stores it as a string and passes it back unmodified to
/// [`JobRegistry::cancel`].
pub(crate) type JobId = String;

/// In-process map from [`JobId`] to its [`CancellationToken`].
///
/// Use [`Self::register`] to start tracking a new job (returns both the
/// fresh id and a fresh token), [`Self::cancel`] to signal it,
/// [`Self::deregister`] from the worker once it exits, and
/// [`Self::active_count`] for diagnostics. See module docs for the
/// contract.
#[derive(Debug, Default)]
pub(crate) struct JobRegistry {
    // Mutex<HashMap<...>> is fine here: the only contention points are
    // register/cancel/deregister, each of which runs in well under a
    // microsecond. No need for the RwLock complexity.
    jobs: Mutex<HashMap<JobId, CancellationToken>>,
}

impl JobRegistry {
    /// Construct an empty registry.
    ///
    /// `#[allow(dead_code)]`: kept for parity with [`CancellationToken::new`]
    /// and as the obvious entry point for tests; the production `AppState`
    /// builds the registry via [`Default::default`].
    #[allow(dead_code)]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Register a new job. Returns the freshly minted id and the
    /// matching cancellation token. The caller is responsible for
    /// passing the id to the frontend and the token (cloned) into the
    /// worker thread.
    ///
    /// Both the registry's copy of the token and the returned copy
    /// share the same atomic flag — cancelling on either is visible
    /// from every clone, including the one the worker holds.
    pub(crate) fn register(&self) -> (JobId, CancellationToken) {
        let id: JobId = Uuid::new_v4().simple().to_string();
        let token = CancellationToken::new();
        // Poisoning would mean an earlier worker thread panicked while
        // holding the lock. Treat that as a fatal-but-recoverable
        // condition: log via tracing and reset the inner map. The
        // alternative — propagating PoisonError up — would force every
        // caller to deal with a condition that, in practice, means
        // "the app is in a bad state, surface it but keep going".
        let mut guard = self.jobs.lock().unwrap_or_else(|poisoned| {
            tracing::warn!("JobRegistry lock poisoned; recovering");
            poisoned.into_inner()
        });
        guard.insert(id.clone(), token.clone());
        (id, token)
    }

    /// Signal the named job for cancellation.
    ///
    /// Returns `true` if a job with that id was found (and signalled),
    /// `false` if no such job exists. Unknown ids are not errors: the
    /// frontend may issue a Cancel that races with the worker's
    /// terminal event, in which case the worker has already
    /// deregistered the id. Idempotent; double-cancel is a no-op.
    pub(crate) fn cancel(&self, id: &str) -> bool {
        let guard = self.jobs.lock().unwrap_or_else(|poisoned| {
            tracing::warn!("JobRegistry lock poisoned; recovering");
            poisoned.into_inner()
        });
        if let Some(token) = guard.get(id) {
            token.cancel();
            true
        } else {
            false
        }
    }

    /// Remove the named job from the registry. Called by the worker
    /// thread when it exits (regardless of outcome — success, failure,
    /// observed cancellation). No-op for unknown ids.
    pub(crate) fn deregister(&self, id: &str) {
        let mut guard = self.jobs.lock().unwrap_or_else(|poisoned| {
            tracing::warn!("JobRegistry lock poisoned; recovering");
            poisoned.into_inner()
        });
        guard.remove(id);
    }

    /// Number of currently-tracked jobs. For diagnostics only.
    ///
    /// `#[allow(dead_code)]`: not currently surfaced via IPC, but kept as the
    /// natural sanity-check primitive for tests and future "is the app idle?"
    /// checks.
    #[allow(dead_code)]
    pub(crate) fn active_count(&self) -> usize {
        self.jobs
            .lock()
            .map(|g| g.len())
            .unwrap_or_else(|poisoned| poisoned.into_inner().len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_registry_is_empty() {
        let r = JobRegistry::new();
        assert_eq!(r.active_count(), 0);
    }

    #[test]
    fn register_returns_distinct_ids() {
        let r = JobRegistry::new();
        let (a, _) = r.register();
        let (b, _) = r.register();
        assert_ne!(a, b);
        assert_eq!(r.active_count(), 2);
    }

    #[test]
    fn register_returns_uncancelled_token() {
        let r = JobRegistry::new();
        let (_, t) = r.register();
        assert!(!t.is_cancelled());
    }

    #[test]
    fn cancel_signals_the_registered_token() {
        let r = JobRegistry::new();
        let (id, token) = r.register();
        assert!(r.cancel(&id));
        assert!(token.is_cancelled());
    }

    #[test]
    fn cancel_unknown_id_returns_false() {
        let r = JobRegistry::new();
        assert!(!r.cancel("not-a-real-id"));
        let (id, _) = r.register();
        r.deregister(&id);
        // After deregister the same id is unknown.
        assert!(!r.cancel(&id));
    }

    #[test]
    fn double_cancel_is_idempotent() {
        let r = JobRegistry::new();
        let (id, token) = r.register();
        assert!(r.cancel(&id));
        assert!(r.cancel(&id));
        assert!(token.is_cancelled());
    }

    #[test]
    fn deregister_drops_the_entry() {
        let r = JobRegistry::new();
        let (id, _) = r.register();
        assert_eq!(r.active_count(), 1);
        r.deregister(&id);
        assert_eq!(r.active_count(), 0);
    }

    #[test]
    fn deregister_unknown_is_noop() {
        let r = JobRegistry::new();
        r.deregister("ghost"); // does not panic
        assert_eq!(r.active_count(), 0);
    }

    #[test]
    fn cancel_after_deregister_leaves_held_token_uncancelled() {
        // A worker that deregisters before observing cancellation will
        // exit clean; a late Cancel from the UI hits the unknown-id path
        // and is a no-op. This is the expected race outcome.
        let r = JobRegistry::new();
        let (id, token) = r.register();
        r.deregister(&id);
        assert!(!r.cancel(&id));
        assert!(!token.is_cancelled());
    }

    #[test]
    fn id_format_is_simple_uuid_hex() {
        let r = JobRegistry::new();
        let (id, _) = r.register();
        assert_eq!(id.len(), 32, "uuid simple form is 32 hex chars");
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
