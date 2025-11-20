//! Stream timeout and throttling monitoring.
//!
//! This module provides [`StalledReadMonitor`], a wrapper for streams that can detect
//! when data flow stalls and enforce timeout or rate-limiting policies.
//!
//! Available when the `stalled-monitor` feature is enabled.
//!
//! # Examples
//!
//! Adding timeout monitoring to a byte stream:
//!
//! ```rust
//! use moosicbox_stream_utils::ByteWriter;
//! use std::time::Duration;
//! use std::io::Write;
//!
//! # async fn example() {
//! let mut writer = ByteWriter::default();
//! let stream = writer.stream()
//!     .stalled_monitor()
//!     .with_timeout(Duration::from_secs(30));
//!
//! // Stream will timeout if no data is received within 30 seconds
//! # }
//! ```

use std::io::{ErrorKind, Result};
use std::task::Poll;
use std::time::Duration;

use pin_project::pin_project;
use thiserror::Error;

/// A wrapper that monitors a stream for stalls and enforces timeout/throttling policies.
///
/// Wraps any [`futures::Stream`] and can detect when the stream stops producing data
/// (stalls). Can be configured with a timeout duration to return an error if no data
/// is received within the timeout period, and a throttle duration to limit how fast
/// data is consumed.
#[pin_project]
pub struct StalledReadMonitor<T, R: futures::Stream<Item = T>> {
    #[pin]
    inner: R,
    sleeper: Option<switchy_async::time::Interval>,
    throttler: Option<switchy_async::time::Interval>,
}

impl<T, R: futures::Stream<Item = T>> StalledReadMonitor<T, R> {
    /// Creates a new stalled read monitor wrapping the given stream.
    ///
    /// By default, no timeout or throttling is configured. Use [`with_timeout`](Self::with_timeout)
    /// and [`with_throttle`](Self::with_throttle) to configure these policies.
    #[must_use]
    pub const fn new(inner: R) -> Self {
        Self {
            inner,
            sleeper: None,
            throttler: None,
        }
    }

    /// Configures a timeout duration for stall detection.
    ///
    /// If no data is received from the stream within the specified duration,
    /// the monitor will return a [`std::io::Error`] with kind [`ErrorKind::TimedOut`].
    /// The timeout is reset each time data is successfully received.
    #[must_use]
    pub fn with_timeout(self, timeout_duration: Duration) -> Self {
        let mut sleeper = switchy_async::time::interval(timeout_duration);
        sleeper.reset();

        Self {
            inner: self.inner,
            sleeper: Some(sleeper),
            throttler: self.throttler,
        }
    }

    /// Configures a throttle duration to limit data consumption rate.
    ///
    /// When throttling is enabled, the monitor will wait at least the specified
    /// duration between reading items from the stream, effectively rate-limiting
    /// the stream consumption.
    #[must_use]
    pub fn with_throttle(self, throttle_duration: Duration) -> Self {
        let mut throttler = switchy_async::time::interval(throttle_duration);
        throttler.reset();

        Self {
            inner: self.inner,
            sleeper: self.sleeper,
            throttler: Some(throttler),
        }
    }
}

/// Errors that can occur in a stalled read monitor.
#[derive(Error, Debug)]
pub enum StalledReadMonitorError {
    /// The stream has stalled (no data received within the timeout period).
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
                if let Some(sleeper) = this.sleeper
                    && let Poll::Ready(instant) = sleeper.poll_tick(cx)
                {
                    log::debug!("StalledReadMonitor timed out at {instant:?}");
                    return Poll::Ready(Some(Err(std::io::Error::new(
                        ErrorKind::TimedOut,
                        StalledReadMonitorError::Stalled,
                    ))));
                }
            }
        }

        response.map(|x| x.map(|y| Ok(y)))
    }
}
