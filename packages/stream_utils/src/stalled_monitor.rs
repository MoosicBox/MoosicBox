use std::io::{ErrorKind, Result};
use std::task::Poll;
use std::time::Duration;

use pin_project::pin_project;
use thiserror::Error;

#[pin_project]
pub struct StalledReadMonitor<T, R: futures::Stream<Item = T>> {
    #[pin]
    inner: R,
    sleeper: Option<tokio::time::Interval>,
    throttler: Option<tokio::time::Interval>,
}

impl<T, R: futures::Stream<Item = T>> StalledReadMonitor<T, R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            sleeper: None,
            throttler: None,
        }
    }

    pub fn with_timeout(self, timeout_duration: Duration) -> Self {
        let mut sleeper = tokio::time::interval(timeout_duration);
        sleeper.reset();

        Self {
            inner: self.inner,
            sleeper: Some(sleeper),
            throttler: self.throttler,
        }
    }

    pub fn with_throttle(self, throttle_duration: Duration) -> Self {
        let mut throttler = tokio::time::interval(throttle_duration);
        throttler.reset();

        Self {
            inner: self.inner,
            sleeper: self.sleeper,
            throttler: Some(throttler),
        }
    }
}

#[derive(Error, Debug)]
pub enum StalledReadMonitorError {
    #[error("Stalled")]
    Stalled,
}

impl<T, R: futures::Stream<Item = T>> futures::Stream for StalledReadMonitor<T, R> {
    type Item = Result<T>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.project();

        if let Some(throttler) = this.throttler {
            match throttler.poll_tick(cx) {
                Poll::Ready(instant) => {
                    log::debug!("StalledReadMonitor throttled for {instant:?}");
                }
                Poll::Pending => {
                    log::trace!("Received throttle pending response");
                    return Poll::Pending;
                }
            }
        }

        let response = this.inner.poll_next(cx);

        match response {
            Poll::Ready(None) => {
                log::trace!("Received stream poll finished response");
            }
            Poll::Ready(Some(_)) => {
                log::trace!("Received stream poll response");

                if let Some(sleeper) = this.sleeper {
                    sleeper.reset();
                }
            }
            Poll::Pending => {
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

        response.map(|x| x.map(|y| Ok(y)))
    }
}
