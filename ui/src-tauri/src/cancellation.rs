//! Cancellation primitive for long-running background jobs.
//!
//! A [`CancellationToken`] is a one-shot, monotonic flag: once set, it
//! stays set. A producer (the Tauri command thread accepting a cancel
//! request) signals via [`CancellationToken::cancel`]; a consumer (a
//! worker thread running a batch translate) polls via
//! [`CancellationToken::is_cancelled`] between units. This is by design
//! cooperative — the worker decides at which boundaries it is safe to
//! stop. Inflight HTTP calls run to completion.
//!
//! # Invariants
//!
//! 1. **Cancellation is sticky.** Once `cancel()` returns, every
//!    subsequent `is_cancelled()` on any clone of the same token
//!    returns `true`. There is no `reset()`: restarting a cancelled job
//!    requires a fresh token.
//! 2. **Shared state across clones.** Cloning a token produces another
//!    handle to the same atomic flag. Signalling on any clone is
//!    visible on every clone.
//! 3. **Send + Sync.** The token is safe to clone across thread
//!    boundaries; this is the whole point of the type.
//!
//! # What this is NOT
//!
//! - Not a `Future` / async cancellation. The worker is a plain OS
//!   thread and we explicitly stay off `tokio` here (the rest of the
//!   workspace is sync).
//! - Not a way to interrupt arbitrary blocking syscalls. A backend
//!   network call that has already started will not abort midway; the
//!   worker checks the flag *between* network calls.
//! - Not a way to know why a job was cancelled. The token carries one
//!   bit. The reason, if any, belongs in the terminal event payload
//!   emitted by the worker.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// One-shot cancellation flag shared across thread boundaries.
///
/// Construct with [`CancellationToken::new`] (or [`Default::default`]),
/// clone for each thread that needs to observe or trigger cancellation,
/// poll via [`Self::is_cancelled`], signal via [`Self::cancel`]. See
/// the module-level docs for the invariants.
#[derive(Debug, Clone, Default)]
pub(crate) struct CancellationToken {
    // SeqCst is overkill for a single bool flipped from false→true and
    // observed elsewhere — Relaxed would suffice for correctness — but
    // the cost is negligible next to an LLM round-trip, and SeqCst
    // removes one whole category of "did I pick the right ordering?"
    // questions from the surface.
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    /// Construct a fresh, un-cancelled token.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Signal cancellation. Idempotent: calling twice is a no-op.
    ///
    /// All clones of `self` observe `is_cancelled() == true` after this
    /// call returns.
    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Returns `true` if [`Self::cancel`] has been called on any clone
    /// of this token.
    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn default_is_not_cancelled() {
        let t = CancellationToken::new();
        assert!(!t.is_cancelled());
        let d: CancellationToken = Default::default();
        assert!(!d.is_cancelled());
    }

    #[test]
    fn cancel_is_observable_through_clone() {
        let producer = CancellationToken::new();
        let consumer = producer.clone();
        assert!(!consumer.is_cancelled());
        producer.cancel();
        assert!(consumer.is_cancelled());
    }

    #[test]
    fn cancel_is_sticky_and_idempotent() {
        let t = CancellationToken::new();
        t.cancel();
        t.cancel(); // idempotent
        t.cancel();
        assert!(t.is_cancelled());
    }

    #[test]
    fn independent_tokens_do_not_share_state() {
        let a = CancellationToken::new();
        let b = CancellationToken::new();
        a.cancel();
        assert!(a.is_cancelled());
        assert!(!b.is_cancelled());
    }

    #[test]
    fn cancel_visible_across_threads() {
        let producer = CancellationToken::new();
        let consumer = producer.clone();
        let handle = thread::spawn(move || {
            // Spin briefly. In production the worker calls is_cancelled
            // between network round-trips, not in a tight loop; for the
            // test we just confirm the flag eventually flips.
            for _ in 0..10_000 {
                if consumer.is_cancelled() {
                    return true;
                }
                std::hint::spin_loop();
            }
            false
        });
        producer.cancel();
        let observed = handle.join().expect("worker thread panicked");
        assert!(observed);
    }
}
