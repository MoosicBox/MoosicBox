//! Future types for the simulator runtime.
//!
//! This module provides sleep, interval, and timeout futures that work with
//! the simulator's controlled time advancement.

use std::{
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::{Duration, SystemTime},
};

use switchy_time::{instant_now, now};

use futures::future::FusedFuture;
use pin_project_lite::pin_project;

pin_project! {
    /// A future that completes after a specified duration.
    ///
    /// This is the simulator's implementation of a sleep future. Time advancement
    /// is controlled by the simulator runtime.
    #[derive(Debug, Copy, Clone)]
    pub struct Sleep {
        #[pin]
        now: SystemTime,
        #[pin]
        duration: Duration,
        #[pin]
        polled: bool,
        #[pin]
        completed: bool,
    }
}

impl Sleep {
    /// Creates a new `Sleep` future that completes after the specified duration.
    #[must_use]
    pub fn new(duration: Duration) -> Self {
        Self {
            now: switchy_time::now(),
            duration,
            polled: false,
            completed: false,
        }
    }
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut this = self.project();
        log::trace!(
            "Polling Sleep: now={:?} duration={:?} polled={} completed={}",
            this.now,
            this.duration,
            this.polled,
            this.completed,
        );

        let polled = *this.polled;

        if polled {
            let duration = switchy_time::now().duration_since(*this.now).unwrap();
            log::trace!(
                "Sleep polled: {}ms/{}ms",
                duration.as_millis(),
                this.duration.as_millis(),
            );
            if duration >= *this.duration {
                *this.completed.as_mut() = true;
                return Poll::Ready(());
            }
        }

        if !polled {
            *this.polled.as_mut() = true;
        }

        cx.waker().wake_by_ref();

        Poll::Pending
    }
}

impl FusedFuture for Sleep {
    fn is_terminated(&self) -> bool {
        self.completed
    }
}

pin_project! {
    /// A future that completes at a specific instant in time.
    ///
    /// This future resolves when the simulator time reaches or exceeds the target instant.
    #[derive(Debug, Copy, Clone)]
    pub struct Instant {
        #[pin]
        instant: std::time::Instant,
        #[pin]
        polled: bool,
        #[pin]
        completed: bool,
    }
}

impl Instant {
    /// Creates a new `Instant` future that completes at the specified instant.
    #[must_use]
    pub const fn new(instant: std::time::Instant) -> Self {
        Self {
            instant,
            polled: false,
            completed: false,
        }
    }
}

/// Converts a `SystemTime` to an `Instant` for the simulator.
///
/// This function calculates the equivalent `Instant` for a given `SystemTime` by computing
/// the delta between the target time and the current system time, then applying that delta
/// to the current instant.
///
/// # Errors
///
/// * Returns `SystemTimeError` if the time calculations overflow or underflow
///
/// # Panics
///
/// * If the instant calculation results in a value that cannot be represented
fn system_time_to_instant(
    target: SystemTime,
) -> Result<std::time::Instant, std::time::SystemTimeError> {
    let now_sys = now();
    let now_inst = instant_now();

    if target >= now_sys {
        // target is in the future (or now)
        let delta: Duration = target.duration_since(now_sys)?;
        Ok(now_inst + delta)
    } else {
        // target is in the past
        let delta: Duration = now_sys.duration_since(target)?;
        Ok(now_inst.checked_sub(delta).unwrap())
    }
}

impl Future for Instant {
    type Output = std::time::Instant;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut this = self.project();
        log::trace!(
            "Polling Instant: instant={:?} polled={} completed={}",
            this.instant,
            this.polled,
            this.completed,
        );

        let polled = *this.polled;

        if polled {
            let now = system_time_to_instant(switchy_time::now()).unwrap();
            log::trace!("Instant polled: now={:?} instant={:?}", now, this.instant,);
            if now > *this.instant {
                *this.completed.as_mut() = true;
                return Poll::Ready(now);
            }
        }

        if !polled {
            *this.polled.as_mut() = true;
        }

        cx.waker().wake_by_ref();

        Poll::Pending
    }
}

impl FusedFuture for Instant {
    fn is_terminated(&self) -> bool {
        self.completed
    }
}

pin_project! {
    /// An interval that yields values at a fixed rate.
    ///
    /// This is the simulator's implementation of an interval timer. It yields values
    /// at regular intervals controlled by the simulator's time advancement.
    #[derive(Debug, Copy, Clone)]
    pub struct Interval {
        #[pin]
        now: SystemTime,
        #[pin]
        interval: Duration,
        #[pin]
        polled: bool,
        #[pin]
        completed: bool,
    }
}

impl Interval {
    /// Creates a new `Interval` that yields values at the specified interval.
    #[must_use]
    pub fn new(interval: Duration) -> Self {
        Self {
            now: switchy_time::now(),
            interval,
            polled: false,
            completed: false,
        }
    }

    /// Returns a future that completes at the next tick.
    ///
    /// # Panics
    ///
    /// * If the `Instant` fails to create
    pub fn tick(&mut self) -> Instant {
        Instant::new(system_time_to_instant(switchy_time::now() + self.interval).unwrap())
    }

    /// Resets the interval to the current time.
    ///
    /// This resets the internal state so the next tick will occur one interval from now.
    pub fn reset(&mut self) {
        self.now = switchy_time::now();
        self.polled = false;
        self.completed = false;
    }

    /// Polls for the next tick of the interval.
    ///
    /// # Panics
    ///
    /// * If time goes backwards
    pub fn poll_tick(&mut self, cx: &mut Context) -> Poll<std::time::Instant> {
        if self.completed {
            // Reset for next tick
            self.now = switchy_time::now();
            self.polled = false;
            self.completed = false;
        }

        if self.polled {
            let duration = switchy_time::now().duration_since(self.now).unwrap();
            if duration >= self.interval {
                self.completed = true;
                let instant = system_time_to_instant(switchy_time::now()).unwrap();
                return Poll::Ready(instant);
            }
        }

        if !self.polled {
            self.polled = true;
        }

        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

/// Error returned when a timeout operation exceeds its deadline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Elapsed;

impl fmt::Display for Elapsed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "deadline has elapsed")
    }
}

impl std::error::Error for Elapsed {}

pin_project! {
    /// A future that wraps another future with a timeout.
    ///
    /// If the inner future doesn't complete within the specified duration,
    /// the timeout future returns an `Elapsed` error.
    #[derive(Debug)]
    pub struct Timeout<F> {
        #[pin]
        future: F,
        #[pin]
        sleep: Sleep,
    }
}

impl<F> Timeout<F> {
    /// Creates a new `Timeout` that wraps the given future.
    ///
    /// The timeout will expire after the specified duration.
    #[must_use]
    pub fn new(duration: Duration, future: F) -> Self {
        Self {
            future,
            sleep: Sleep::new(duration),
        }
    }

    /// Consumes the `Timeout` and returns the inner future.
    #[must_use]
    pub fn into_inner(self) -> F {
        self.future
    }
}

impl<F> Future for Timeout<F>
where
    F: Future,
{
    type Output = Result<F::Output, Elapsed>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = self.project();

        // First check if the inner future is ready
        if let Poll::Ready(output) = this.future.poll(cx) {
            return Poll::Ready(Ok(output));
        }

        // Then check if the timeout has elapsed
        if this.sleep.poll(cx) == Poll::Ready(()) {
            return Poll::Ready(Err(Elapsed));
        }

        Poll::Pending
    }
}

impl<F> FusedFuture for Timeout<F>
where
    F: FusedFuture,
{
    fn is_terminated(&self) -> bool {
        self.future.is_terminated() || self.sleep.is_terminated()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::ready;

    #[test]
    fn sleep_future_implements_fused_future() {
        let sleep = Sleep::new(Duration::from_millis(10));
        assert!(!sleep.is_terminated());
    }

    #[test]
    fn interval_reset_restarts_timing() {
        let mut interval = Interval::new(Duration::from_millis(100));

        // First tick should be created with current time
        let _tick1 = interval.tick();

        // Reset the interval
        interval.reset();

        // After reset, state should be back to initial
        assert!(!interval.polled);
        assert!(!interval.completed);
    }

    #[test]
    fn interval_poll_tick_returns_ready_after_duration() {
        use std::task::{Context, Poll};

        let mut interval = Interval::new(Duration::from_millis(1));
        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);

        // First poll should return Pending
        assert!(matches!(interval.poll_tick(&mut cx), Poll::Pending));

        // Mark as polled to test the ready path
        interval.polled = true;
        // Advance time in test by updating now
        interval.now = switchy_time::now() - Duration::from_millis(2);

        let result = interval.poll_tick(&mut cx);
        assert!(matches!(result, Poll::Ready(_)));
    }

    #[test]
    fn instant_future_implements_fused_future() {
        let instant = Instant::new(instant_now() + Duration::from_millis(10));
        assert!(!instant.is_terminated());
    }

    #[test]
    fn timeout_into_inner_returns_original_future() {
        let original_future = ready(42);
        let timeout = Timeout::new(Duration::from_millis(100), original_future);

        let inner = timeout.into_inner();
        // The future should still be the same
        let result = futures::executor::block_on(inner);
        assert_eq!(result, 42);
    }

    #[test]
    fn elapsed_error_displays_correctly() {
        let err = Elapsed;
        assert_eq!(err.to_string(), "deadline has elapsed");
    }

    #[test]
    fn elapsed_error_is_clonable() {
        let err1 = Elapsed;
        let err2 = err1.clone();
        assert_eq!(err1, err2);
    }

    #[test]
    fn sleep_creates_with_current_time() {
        let sleep = Sleep::new(Duration::from_millis(100));
        let now = switchy_time::now();

        // Sleep's now should be very close to current time (within a small window)
        let diff = sleep
            .now
            .duration_since(now)
            .unwrap_or_else(|_| now.duration_since(sleep.now).unwrap());
        assert!(diff < Duration::from_millis(10));
    }

    #[test]
    fn interval_creates_with_current_time() {
        let interval = Interval::new(Duration::from_millis(100));
        let now = switchy_time::now();

        // Interval's now should be very close to current time
        let diff = interval
            .now
            .duration_since(now)
            .unwrap_or_else(|_| now.duration_since(interval.now).unwrap());
        assert!(diff < Duration::from_millis(10));
    }

    #[test]
    fn system_time_to_instant_handles_future_time() {
        let future_time = now() + Duration::from_secs(10);
        let result = system_time_to_instant(future_time);
        assert!(result.is_ok());
    }

    #[test]
    fn system_time_to_instant_handles_past_time() {
        let past_time = now() - Duration::from_secs(10);
        let result = system_time_to_instant(past_time);
        assert!(result.is_ok());
    }

    #[test]
    fn system_time_to_instant_handles_current_time() {
        let current_time = now();
        let result = system_time_to_instant(current_time);
        assert!(result.is_ok());
    }
}
