//! Poll-based authentication implementation.
//!
//! This module provides `PollAuth`, which implements an authentication flow that polls
//! for authentication status until success or timeout. This is typically used for OAuth-style
//! flows where the user authenticates on a separate device or browser.

use std::time::Duration;

use switchy_async::futures::FutureExt as _;

use super::Auth;

/// Poll-based authentication configuration.
#[derive(Debug, Clone)]
pub struct PollAuth {
    timeout: Duration,
}

impl From<PollAuth> for Auth {
    fn from(value: PollAuth) -> Self {
        Self::Poll(value)
    }
}

impl Default for PollAuth {
    fn default() -> Self {
        Self::new()
    }
}

impl PollAuth {
    /// Creates a new poll authentication with default timeout (60 seconds).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            timeout: Duration::from_secs(60),
        }
    }

    /// Sets the timeout duration.
    #[must_use]
    pub fn with_timeout(mut self, timeout: impl Into<Duration>) -> Self {
        self.timeout = timeout.into();
        self
    }

    /// Sets the timeout duration to the given number of seconds.
    #[must_use]
    pub const fn with_timeout_secs(mut self, timeout: u64) -> Self {
        self.timeout = Duration::from_secs(timeout);
        self
    }

    /// Sets the timeout duration to the given number of milliseconds.
    #[must_use]
    pub const fn with_timeout_millis(mut self, timeout: u64) -> Self {
        self.timeout = Duration::from_millis(timeout);
        self
    }

    /// Sets the timeout duration.
    #[must_use]
    pub fn timeout(&mut self, timeout: impl Into<Duration>) -> &mut Self {
        self.timeout = timeout.into();
        self
    }

    /// Sets the timeout duration to the given number of seconds.
    #[must_use]
    pub const fn timeout_secs(&mut self, timeout: u64) -> &mut Self {
        self.timeout = Duration::from_secs(timeout);
        self
    }

    /// Sets the timeout duration to the given number of milliseconds.
    #[must_use]
    pub const fn timeout_millis(&mut self, timeout: u64) -> &mut Self {
        self.timeout = Duration::from_millis(timeout);
        self
    }

    /// Polls for authentication status.
    ///
    /// # Errors
    ///
    /// * If the poll operation fails
    #[allow(clippy::unused_async)]
    pub async fn poll(&self) -> Result<bool, Box<dyn std::error::Error + Send>> {
        Ok(false)
    }
}

impl PollAuth {
    /// Attempts to log in by polling until success or timeout.
    ///
    /// # Errors
    ///
    /// * If the poll authentication fails
    pub async fn login(&self) -> Result<bool, Box<dyn std::error::Error + Send>> {
        switchy_async::select! {
            success = self.poll().fuse() => {
                Ok(success?)
            },
            () = switchy_async::time::sleep(self.timeout) => {
                Ok(false)
            }
        }
    }
}
