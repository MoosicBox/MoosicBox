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
