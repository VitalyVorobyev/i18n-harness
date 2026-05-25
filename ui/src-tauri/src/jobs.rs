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

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
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

// ── Typed batch-slot tracking ─────────────────────────────────────────────────

/// Identifies which "batch slot" a worker is claiming.
///
/// `Catalog` is used by per-catalog/per-locale batch translates; two workers
/// cannot run concurrently against the same pair. `Eval` is the single global
/// evaluation slot — replacing the old `("__eval__", "__eval__")` magic key.
#[derive(Debug, Clone)]
pub(crate) enum BatchSlot {
    Catalog { abs: PathBuf, locale: String },
    Eval,
}

/// Typed wrapper over per-catalog and eval concurrency tracking.
///
/// Replaces the old `Mutex<BTreeSet<(PathBuf, String)>>` field on `AppState`
/// and the `("__eval__", "__eval__")` magic key. The eval slot is an
/// explicit boolean rather than a tuple inserted into the same set, so
/// misuses (accidentally claiming the catalog slot with eval coordinates) are
/// compile-time impossible.
#[derive(Default)]
pub(crate) struct ActiveBatches {
    per_catalog: Mutex<BTreeSet<(PathBuf, String)>>,
    eval_running: Mutex<bool>,
}

impl ActiveBatches {
    /// Try to claim a slot.
    ///
    /// Returns an `Err` string suitable for returning to the JS layer when
    /// the slot is already busy. Error strings are byte-stable with the old
    /// code paths so the UI error messages don't drift.
    pub(crate) fn try_claim(&self, slot: &BatchSlot) -> Result<(), String> {
        match slot {
            BatchSlot::Catalog { abs, locale } => {
                let mut set = self
                    .per_catalog
                    .lock()
                    .map_err(crate::error::lock_poisoned("active_batches"))?;
                let key = (abs.clone(), locale.clone());
                if !set.insert(key) {
                    return Err(
                        "a translation is already running for this catalog/locale".to_string()
                    );
                }
                Ok(())
            }
            BatchSlot::Eval => {
                let mut flag = self
                    .eval_running
                    .lock()
                    .map_err(crate::error::lock_poisoned("active_batches"))?;
                if *flag {
                    return Err("an evaluation is already running".to_string());
                }
                *flag = true;
                Ok(())
            }
        }
    }

    /// Release a previously-claimed slot. Idempotent — releasing an unheld
    /// slot is a no-op. A poisoned lock is logged and ignored so workers
    /// never panic in Drop.
    pub(crate) fn release(&self, slot: &BatchSlot) {
        match slot {
            BatchSlot::Catalog { abs, locale } => {
                if let Ok(mut set) = self.per_catalog.lock() {
                    set.remove(&(abs.clone(), locale.clone()));
                }
            }
            BatchSlot::Eval => {
                if let Ok(mut flag) = self.eval_running.lock() {
                    *flag = false;
                }
            }
        }
    }
}

// ── RAII scope guard ──────────────────────────────────────────────────────────

/// RAII guard that owns a job-id registration AND a batch-slot claim.
///
/// On Drop, deregisters the job from the [`JobRegistry`] and releases the
/// [`BatchSlot`] from [`ActiveBatches`] — in that order per the plan's
/// documented sequence (deregister first, then release).
///
/// # Lifetime constraint
///
/// `ScopedJobRegistration<'a>` borrows `JobRegistry` and `ActiveBatches` by
/// shared reference; the `'a` lifetime must outlive the guard. Workers create
/// the guard by calling [`adopt`][Self::adopt] at the top of the worker body,
/// borrowing from the `tauri::State<'_, AppState>` that lives for the full
/// worker function body.
pub(crate) struct ScopedJobRegistration<'a> {
    jobs: &'a JobRegistry,
    active: &'a ActiveBatches,
    pub(crate) job_id: JobId,
    // Held so the caller can retrieve the token via `token()` when using
    // `claim()`. In `adopt()` usage the worker already has the token, so
    // this copy is intentionally unused.
    #[allow(dead_code)]
    token: CancellationToken,
    slot: BatchSlot,
}

impl<'a> ScopedJobRegistration<'a> {
    /// Claim the slot and register a new job atomically from the caller's
    /// perspective (slot claim is visible before the job id is returned to JS).
    ///
    /// On error the slot is never claimed — the caller sees the busy-slot
    /// message without any side effects.
    #[allow(dead_code)] // primary entry point; used in tests and future callers
    pub(crate) fn claim(
        jobs: &'a JobRegistry,
        active: &'a ActiveBatches,
        slot: BatchSlot,
    ) -> Result<Self, String> {
        active.try_claim(&slot)?;
        let (job_id, token) = jobs.register();
        Ok(Self {
            jobs,
            active,
            job_id,
            token,
            slot,
        })
    }

    /// Adopt ownership of an already-registered job and an already-claimed
    /// slot. Used by workers that receive the `job_id`, `token`, and `slot`
    /// as move-captured values from the command thread and want Drop to own
    /// cleanup rather than writing it manually.
    ///
    /// # Safety contract
    ///
    /// The caller asserts that `job_id` is registered in `jobs` and `slot`
    /// is held in `active`. Calling this with stale values produces a no-op
    /// release (idempotent), not a panic.
    pub(crate) fn adopt(
        jobs: &'a JobRegistry,
        active: &'a ActiveBatches,
        job_id: JobId,
        token: CancellationToken,
        slot: BatchSlot,
    ) -> Self {
        Self {
            jobs,
            active,
            job_id,
            token,
            slot,
        }
    }

    // Used when `claim()` is the constructor — lets the caller retrieve the
    // freshly minted token. Not called from worker `adopt()` paths where the
    // token was already obtained by the command thread.
    #[allow(dead_code)]
    pub(crate) fn token(&self) -> &CancellationToken {
        &self.token
    }
}

impl Drop for ScopedJobRegistration<'_> {
    fn drop(&mut self) {
        // Deregister the job first, then release the slot — per plan sequence.
        self.jobs.deregister(&self.job_id);
        self.active.release(&self.slot);
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

    // ── ActiveBatches tests ───────────────────────────────────────────────────

    fn catalog_slot(path: &str, locale: &str) -> BatchSlot {
        BatchSlot::Catalog {
            abs: PathBuf::from(path),
            locale: locale.to_string(),
        }
    }

    #[test]
    fn active_batches_claim_release_per_catalog_is_independent_per_pair() {
        let ab = ActiveBatches::default();
        let slot_a = catalog_slot("/a/b.ts", "de");
        let slot_b = catalog_slot("/a/b.ts", "es"); // different locale
        let slot_c = catalog_slot("/x/y.ts", "de"); // different path

        // All three distinct pairs can be claimed simultaneously.
        assert!(ab.try_claim(&slot_a).is_ok());
        assert!(ab.try_claim(&slot_b).is_ok());
        assert!(ab.try_claim(&slot_c).is_ok());

        // Releasing one slot does not disturb the others.
        ab.release(&slot_a);
        assert!(ab.try_claim(&slot_a).is_ok()); // re-claimable after release
        assert!(ab.try_claim(&slot_b).is_err()); // still held
        assert!(ab.try_claim(&slot_c).is_err()); // still held
    }

    #[test]
    fn active_batches_second_claim_on_same_pair_returns_busy_error() {
        let ab = ActiveBatches::default();
        let slot = catalog_slot("/catalog.ts", "zh");

        assert!(ab.try_claim(&slot).is_ok());

        let err = ab.try_claim(&slot).unwrap_err();
        assert_eq!(
            err,
            "a translation is already running for this catalog/locale"
        );

        ab.release(&slot);
        // After release, a third claim succeeds.
        assert!(ab.try_claim(&slot).is_ok());
    }

    #[test]
    fn active_batches_eval_slot_excludes_itself_but_not_per_catalog() {
        let ab = ActiveBatches::default();
        let eval = BatchSlot::Eval;
        let catalog = catalog_slot("/c.ts", "fr");

        // Claiming eval does not block a catalog slot.
        assert!(ab.try_claim(&eval).is_ok());
        assert!(ab.try_claim(&catalog).is_ok());

        // Second eval claim returns the busy message.
        let err = ab.try_claim(&eval).unwrap_err();
        assert_eq!(err, "an evaluation is already running");

        // Releasing eval allows it to be claimed again.
        ab.release(&eval);
        assert!(ab.try_claim(&eval).is_ok());

        // Catalog slot is still held.
        assert!(ab.try_claim(&catalog).is_err());
    }
}
