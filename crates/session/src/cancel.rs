//! Cooperative cancellation for long translation runs.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A shared flag a running job checks between units.
///
/// Cloning shares the flag: cancel through any clone and every clone sees it.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// A token that is not cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask the job to stop at the next unit boundary.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Whether [`Self::cancel`] was called on any clone.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelling_a_clone_cancels_the_original() {
        let token = CancelToken::new();
        let clone = token.clone();
        assert!(!token.is_cancelled());
        clone.cancel();
        assert!(token.is_cancelled());
    }
}
