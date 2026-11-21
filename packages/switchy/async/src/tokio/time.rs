//! Time utilities for the Tokio runtime.
//!
//! This module re-exports Tokio's time utilities including sleep, intervals, and timeouts.

pub use switchy_time::Instant;
pub use tokio::time::{
    Duration, Interval, MissedTickBehavior, Sleep, Timeout, error::Elapsed, interval, sleep,
    timeout,
};

/// Sleep until `deadline`.
pub fn sleep_until(deadline: Instant) -> Sleep {
    sleep(deadline.remaining())
}

/// Require `future` to complete before `deadline`.
pub fn timeout_at<F: std::future::IntoFuture>(
    deadline: Instant,
    future: F,
) -> Timeout<F::IntoFuture> {
    timeout(deadline.remaining(), future)
}
