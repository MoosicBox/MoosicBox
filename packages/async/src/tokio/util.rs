use futures::future::{FusedFuture, FutureExt};

/// A cancellation token that provides FusedFuture-compatible cancellation
#[derive(Debug, Clone)]
pub struct CancellationToken {
    inner: tokio_util::sync::CancellationToken,
}

impl CancellationToken {
    /// Create a new cancellation token
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: tokio_util::sync::CancellationToken::new(),
        }
    }

    /// Create a child token that will be cancelled when this token is cancelled
    #[must_use]
    pub fn child_token(&self) -> Self {
        Self {
            inner: self.inner.child_token(),
        }
    }

    /// Cancel this token
    pub fn cancel(&self) {
        self.inner.cancel();
    }

    /// Check if this token has been cancelled
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }

    /// Wait for this token to be cancelled, returning a `FusedFuture`
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
