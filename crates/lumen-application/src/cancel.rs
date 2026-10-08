//! Cooperative cancellation shared by long-running use cases.
//!
//! Filesystem work runs on plain threads, so cancellation is a cheap flag that
//! workers check between units of work. Destructive steps (quarantine moves) are
//! never interrupted mid-step; they check the token only *between* items
//! (Rust ecosystem research, R3).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A cancellation token. Clones observe the same cancellation; children are
/// cancelled with their parent but can also be cancelled on their own.
#[derive(Debug, Clone)]
pub struct CancelToken {
    /// This token's flag followed by its ancestors' flags.
    chain: Arc<[Arc<AtomicBool>]>,
}

impl Default for CancelToken {
    fn default() -> Self {
        Self::new()
    }
}

impl CancelToken {
    /// A new, uncancelled root token.
    pub fn new() -> Self {
        Self {
            chain: Arc::from([Arc::new(AtomicBool::new(false))]),
        }
    }

    /// A child token: cancelled when it or any ancestor is cancelled. Cancelling
    /// the child does not cancel the parent.
    #[must_use]
    pub fn child(&self) -> Self {
        let mut chain = Vec::with_capacity(self.chain.len() + 1);
        chain.push(Arc::new(AtomicBool::new(false)));
        chain.extend(self.chain.iter().cloned());
        Self {
            chain: chain.into(),
        }
    }

    /// Requests cancellation of this token and its children.
    pub fn cancel(&self) {
        if let Some(own) = self.chain.first() {
            own.store(true, Ordering::Release);
        }
    }

    /// Whether this token or any ancestor was cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.chain.iter().any(|flag| flag.load(Ordering::Acquire))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_cancellation() {
        let token = CancelToken::new();
        let clone = token.clone();
        clone.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn parents_cancel_children_but_not_the_reverse() {
        let operation = CancelToken::new();
        let volume_a = operation.child();
        let volume_b = operation.child();
        volume_a.cancel();
        assert!(volume_a.is_cancelled());
        assert!(!operation.is_cancelled() && !volume_b.is_cancelled());
        operation.cancel();
        assert!(volume_b.is_cancelled());
        assert!(
            volume_b.child().is_cancelled(),
            "grandchildren see ancestor cancellation"
        );
    }

    #[test]
    fn default_token_starts_uncancelled_and_can_be_cancelled() {
        let token = CancelToken::default();
        assert!(!token.is_cancelled());
        token.cancel();
        assert!(token.is_cancelled());
    }
}
