//! Time utilities for the simulator runtime.
//!
//! This module provides sleep, interval, and timeout functions that work with
//! the simulator's controlled time advancement.

use std::future::IntoFuture;

use crate::simulator::futures::{Sleep, Timeout};

// Re-export types for compatibility
pub use crate::simulator::futures::{Elapsed, Interval};
pub use std::time::Duration;

// Re-export simulator functionality
/// Simulator-specific time utilities.
///
/// This module provides functions for controlling simulator time behavior.
pub mod simulator {
    pub use switchy_time::simulator::with_real_time;
}

/// Creates a future that completes after the specified duration.
///
/// This returns a `Sleep` future that will complete after the given duration has elapsed.
/// Time advancement is controlled by the simulator runtime.
#[must_use]
pub fn sleep(duration: Duration) -> Sleep {
    Sleep::new(duration)
}

/// Creates an interval that yields at a fixed rate.
///
/// This returns an `Interval` that yields values at regular intervals specified by the duration.
/// Time advancement is controlled by the simulator runtime.
#[must_use]
pub fn interval(duration: Duration) -> crate::simulator::futures::Interval {
    crate::simulator::futures::Interval::new(duration)
}

/// Requires a future to complete before the specified duration.
///
/// This wraps the given future with a timeout. If the future doesn't complete within
/// the specified duration, the timeout future will return an `Elapsed` error.
/// Time advancement is controlled by the simulator runtime.
#[must_use]
pub fn timeout<F>(duration: Duration, future: F) -> Timeout<F::IntoFuture>
where
    F: IntoFuture,
{
    Timeout::new(duration, future.into_future())
}
