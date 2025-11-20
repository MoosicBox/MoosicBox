//! Stream timeout and throttling monitoring.
//!
//! This module provides [`StalledReadMonitor`], a wrapper for streams that can detect
//! when data flow stalls and enforce timeout or rate-limiting policies.
//!
//! Available when the `stalled-monitor` feature is enabled.

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

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream::{self, StreamExt};
    use std::time::Duration;

    #[test_log::test(switchy_async::test)]
    async fn test_stalled_monitor_no_timeout_or_throttle() {
        // Test that monitor passes through data without timeout or throttle
        let data = vec![1, 2, 3, 4, 5];
        let stream = stream::iter(data.clone());
        let mut monitor = StalledReadMonitor::new(stream);

        let mut results = vec![];
        while let Some(item) = monitor.next().await {
            results.push(item.unwrap());
        }

        assert_eq!(results, data);
    }

    #[test_log::test(switchy_async::test(real_time))]
    async fn test_stalled_monitor_with_timeout() {
        // Test that monitor times out when stream stalls
        let stream = stream::pending::<i32>(); // Stream that never produces data
        let mut monitor = StalledReadMonitor::new(stream).with_timeout(Duration::from_millis(50));

        // Should timeout since stream never produces data
        let result = monitor.next().await;
        assert!(result.is_some(), "Should get timeout error");
        let error = result.unwrap().unwrap_err();
        assert_eq!(error.kind(), ErrorKind::TimedOut);
    }

    #[test_log::test(switchy_async::test(real_time))]
    async fn test_stalled_monitor_timeout_reset_on_data() {
        // Test that timeout is reset when data is received
        // We use a simple stream that should complete quickly to verify timeout doesn't fire
        let items = vec![1, 2, 3];
        let stream = stream::iter(items.clone());

        let mut monitor = StalledReadMonitor::new(stream).with_timeout(Duration::from_secs(1));

        let mut results = vec![];
        while let Some(item) = monitor.next().await {
            results.push(item.unwrap());
        }

        // Should receive all items without timing out
        assert_eq!(results, vec![1, 2, 3]);
    }

    #[test_log::test(switchy_async::test(real_time))]
    async fn test_stalled_monitor_with_throttle() {
        // Test that throttle limits data consumption rate
        use std::time::Instant;

        let data = vec![1, 2, 3];
        let stream = stream::iter(data.clone());
        let mut monitor = StalledReadMonitor::new(stream).with_throttle(Duration::from_millis(50));

        let start = Instant::now();
        let mut results = vec![];
        while let Some(item) = monitor.next().await {
            results.push(item.unwrap());
        }
        let elapsed = start.elapsed();

        assert_eq!(results, data);
        // Should take at least 100ms (2 * 50ms throttle for 3 items)
        // Note: First item might not be throttled, so we check for 2 intervals
        assert!(
            elapsed >= Duration::from_millis(90),
            "Throttling should slow down consumption (elapsed: {elapsed:?})"
        );
    }

    #[test_log::test(switchy_async::test(real_time))]
    async fn test_stalled_monitor_with_timeout_and_throttle() {
        // Test that both timeout and throttle work together
        let data = vec![1, 2, 3];
        let stream = stream::iter(data.clone());
        let mut monitor = StalledReadMonitor::new(stream)
            .with_timeout(Duration::from_millis(200))
            .with_throttle(Duration::from_millis(30));

        let mut results = vec![];
        while let Some(item) = monitor.next().await {
            results.push(item.unwrap());
        }

        assert_eq!(results, data);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_stalled_monitor_empty_stream() {
        // Test monitor with empty stream
        let stream = stream::iter(Vec::<i32>::new());
        let mut monitor = StalledReadMonitor::new(stream);

        let result = monitor.next().await;
        assert!(result.is_none(), "Empty stream should return None");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_stalled_monitor_single_item() {
        // Test monitor with single item
        let stream = stream::iter(vec![42]);
        let mut monitor = StalledReadMonitor::new(stream);

        let result = monitor.next().await.unwrap().unwrap();
        assert_eq!(result, 42);

        let end = monitor.next().await;
        assert!(end.is_none());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_stalled_monitor_error_display() {
        // Test that StalledReadMonitorError displays correctly
        let error = StalledReadMonitorError::Stalled;
        assert_eq!(format!("{error}"), "Stalled");
    }
}
