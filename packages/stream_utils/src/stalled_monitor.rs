use std::io::{ErrorKind, Result};
use std::task::Poll;
use std::time::Duration;

use pin_project::pin_project;
use thiserror::Error;

#[pin_project]
pub struct StalledReadMonitor<T, R: futures::Stream<Item = Result<T>>> {
    #[pin]
    inner: R,
    sleeper: tokio::time::Interval,
}

impl<T, R: futures::Stream<Item = Result<T>>> StalledReadMonitor<T, R> {
    pub fn new(inner: R) -> Self {
        let mut sleeper = tokio::time::interval(Duration::from_secs(5));
        sleeper.reset();

        Self { inner, sleeper }
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
            Poll::Ready(Some(_)) => {
                this.sleeper.reset();
            }
            Poll::Ready(None) | Poll::Pending => {
                if let Poll::Ready(_) = this.sleeper.poll_tick(cx) {
                    return Poll::Ready(Some(Err(std::io::Error::new(
                        ErrorKind::TimedOut,
                        StalledReadMonitorError::Stalled,
                    ))));
                }
            }
        }

        response
    }
}
