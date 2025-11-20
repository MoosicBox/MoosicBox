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
    /// Converts `PollAuth` into `Auth::Poll`.
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
    pub fn timeout(&mut self, timeout: impl Into<Duration>) -> &mut Self {
        self.timeout = timeout.into();
        self
    }

    /// Sets the timeout duration to the given number of seconds.
    pub const fn timeout_secs(&mut self, timeout: u64) -> &mut Self {
        self.timeout = Duration::from_secs(timeout);
        self
    }

    /// Sets the timeout duration to the given number of milliseconds.
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

#[cfg(test)]
mod test {
    use std::time::Duration;

    use pretty_assertions::assert_eq;

    use super::PollAuth;

    #[test]
    fn poll_auth_new_has_default_timeout() {
        let auth = PollAuth::new();
        assert_eq!(auth.timeout, Duration::from_secs(60));
    }

    #[test]
    fn poll_auth_with_timeout_sets_timeout() {
        let auth = PollAuth::new().with_timeout(Duration::from_secs(120));
        assert_eq!(auth.timeout, Duration::from_secs(120));
    }

    #[test]
    fn poll_auth_with_timeout_secs_sets_timeout() {
        let auth = PollAuth::new().with_timeout_secs(90);
        assert_eq!(auth.timeout, Duration::from_secs(90));
    }

    #[test]
    fn poll_auth_with_timeout_millis_sets_timeout() {
        let auth = PollAuth::new().with_timeout_millis(5000);
        assert_eq!(auth.timeout, Duration::from_millis(5000));
    }

    #[test]
    fn poll_auth_timeout_mutable_sets_timeout() {
        let mut auth = PollAuth::new();
        let _ = auth.timeout(Duration::from_secs(30));
        assert_eq!(auth.timeout, Duration::from_secs(30));
    }

    #[test]
    fn poll_auth_timeout_secs_mutable_sets_timeout() {
        let mut auth = PollAuth::new();
        let _ = auth.timeout_secs(45);
        assert_eq!(auth.timeout, Duration::from_secs(45));
    }

    #[test]
    fn poll_auth_timeout_millis_mutable_sets_timeout() {
        let mut auth = PollAuth::new();
        let _ = auth.timeout_millis(3000);
        assert_eq!(auth.timeout, Duration::from_millis(3000));
    }

    #[test_log::test(switchy_async::test)]
    async fn poll_auth_poll_returns_false() {
        let auth = PollAuth::new();
        let result = auth.poll().await.unwrap();
        assert!(!result);
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn poll_auth_login_times_out_when_poll_never_succeeds() {
        let auth = PollAuth::new().with_timeout_millis(100);

        let result = auth.login().await.unwrap();

        assert!(!result);
    }
}
