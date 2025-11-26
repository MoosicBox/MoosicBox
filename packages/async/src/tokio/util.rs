//! Utility types for the Tokio runtime.
//!
//! This module provides additional utilities like cancellation tokens for managing async operations.

use futures::future::{FusedFuture, FutureExt};

/// A cancellation token that provides FusedFuture-compatible cancellation.
///
/// This wraps `tokio_util::sync::CancellationToken` to provide futures that implement
/// `FusedFuture`, making them compatible with operations that require fused futures.
#[derive(Debug, Clone)]
pub struct CancellationToken {
    inner: tokio_util::sync::CancellationToken,
}

impl CancellationToken {
    /// Creates a new cancellation token.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: tokio_util::sync::CancellationToken::new(),
        }
    }

    /// Creates a child token that will be cancelled when this token is cancelled.
    ///
    /// Child tokens can be used to create hierarchical cancellation scopes.
    #[must_use]
    pub fn child_token(&self) -> Self {
        Self {
            inner: self.inner.child_token(),
        }
    }

    /// Cancels this token and all its child tokens.
    pub fn cancel(&self) {
        self.inner.cancel();
    }

    /// Checks if this token has been cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }

    /// Returns a future that completes when this token is cancelled.
    ///
    /// The returned future implements `FusedFuture`, making it safe to use in select loops.
    #[must_use]
    pub fn cancelled(&self) -> impl FusedFuture<Output = ()> + '_ {
        self.inner.cancelled().fuse()
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{Builder, GenericRuntime as _};

    /// Tests that our wrapper's `cancelled()` method returns a `FusedFuture` that
    /// completes when the token is already cancelled
    #[test_log::test]
    fn test_cancelled_future_completes_immediately_when_already_cancelled() {
        let runtime = crate::tokio::runtime::build_runtime(&Builder::new()).unwrap();

        let token = CancellationToken::new();
        token.cancel();

        runtime.block_on(async {
            // This should complete immediately since token is already cancelled
            token.cancelled().await;
        });

        runtime.wait().unwrap();
    }

    /// Tests that the `cancelled()` future works correctly in a `tokio::select!` loop,
    /// which requires `FusedFuture` - the main value-add of our wrapper
    #[cfg(feature = "time")]
    #[test_log::test]
    fn test_cancelled_future_works_in_select() {
        let runtime = crate::tokio::runtime::build_runtime(&Builder::new()).unwrap();

        let token = CancellationToken::new();
        let token_clone = token.clone();

        runtime.block_on(async {
            let result = tokio::select! {
                () = token.cancelled() => "cancelled",
                () = async {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    token_clone.cancel();
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                } => "timeout",
            };

            assert_eq!(result, "cancelled");
        });

        runtime.wait().unwrap();
    }

    /// Tests that child tokens created via our wrapper propagate cancellation correctly
    #[test_log::test]
    fn test_child_token_cancellation_propagation() {
        let parent = CancellationToken::new();
        let child = parent.child_token();

        assert!(!parent.is_cancelled());
        assert!(!child.is_cancelled());

        // Cancel parent - child should be cancelled
        parent.cancel();

        assert!(parent.is_cancelled());
        assert!(child.is_cancelled());
    }
}
