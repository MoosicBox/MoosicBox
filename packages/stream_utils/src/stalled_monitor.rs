use std::io::{ErrorKind, Result};
use std::task::Poll;
use std::time::Duration;

use pin_project::pin_project;
use thiserror::Error;

#[pin_project]
pub struct StalledReadMonitor<T, R: futures::Stream<Item = Result<T>>> {
    #[pin]
    inner: R,
    sleeper: Option<tokio::time::Interval>,
}

impl<T, R: futures::Stream<Item = Result<T>>> StalledReadMonitor<T, R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            sleeper: None,
        }
    }

    pub fn with_timeout(self, timeout_duration: Duration) -> Self {
        let mut sleeper = tokio::time::interval(timeout_duration);
        sleeper.reset();

        Self {
            inner: self.inner,
            sleeper: Some(sleeper),
        }
    }
}

#[derive(Error, Debug)]
pub enum StalledReadMonitorError {
    #[error("Stalled")]
    Stalled,
}

impl<T, R: futures::Stream<Item = Result<T>>> futures::Stream for StalledReadMonitor<T, R> {
    type Item = Result<T>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.project();

        let response = this.inner.poll_next(cx);

        match response {
            Poll::Ready(Some(ref resp)) => {
                log::trace!("Received stream poll response ok={}", resp.is_ok());

                if let Some(sleeper) = this.sleeper {
                    sleeper.reset();
                }
            }
            Poll::Ready(None) | Poll::Pending => {
                if let Some(sleeper) = this.sleeper {
                    if let Poll::Ready(instant) = sleeper.poll_tick(cx) {
                        log::debug!("StalledReadMonitor timed out at {instant:?}");
                        return Poll::Ready(Some(Err(std::io::Error::new(
                            ErrorKind::TimedOut,
                            StalledReadMonitorError::Stalled,
                        ))));
                    }
                }
            }
        }

        response
    }
}
