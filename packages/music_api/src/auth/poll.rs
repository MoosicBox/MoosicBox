use std::time::Duration;

use switchy_async::futures::FutureExt as _;

use super::Auth;

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

    /// # Errors
    ///
    /// * If the poll fails
    #[allow(clippy::unused_async)]
    pub async fn poll(&self) -> Result<bool, Box<dyn std::error::Error>> {
        Ok(false)
    }
}

impl PollAuth {
    /// # Errors
    ///
    /// * If the poll auth fails
    pub async fn login(&self) -> Result<bool, Box<dyn std::error::Error>> {
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
