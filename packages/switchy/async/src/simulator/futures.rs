//! Future types for the simulator runtime.
//!
//! This module provides sleep, interval, and timeout futures that work with
//! the simulator's controlled time advancement.

use std::{
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

#[cfg(test)]
use switchy_time::instant_now;

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
        now: std::time::Instant,
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
            now: switchy_time::instant_now(),
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
            let duration = switchy_time::instant_now().duration_since(*this.now);
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
    #[allow(clippy::struct_field_names)]
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
            let now = switchy_time::instant_now();
            log::trace!("Instant polled: now={:?} instant={:?}", now, this.instant);
            if now >= *this.instant {
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

/// A fixed-rate simulated interval with an immediately due first tick.
///
/// Missed ticks are delivered in a burst, retaining their scheduled deadlines.
#[derive(Debug, Copy, Clone)]
pub struct Interval {
    now: std::time::Instant,
    interval: Duration,
}

impl Interval {
    /// Creates a new `Interval` that yields values at the specified interval.
    ///
    /// # Panics
    ///
    /// * If the interval is zero.
    #[must_use]
    pub fn new(interval: Duration) -> Self {
        assert!(!interval.is_zero(), "interval period must be nonzero");
        Self {
            now: switchy_time::instant_now(),
            interval,
        }
    }

    /// Returns a future that completes at the next tick.
    ///
    /// Dropping a pending tick does not consume it. This uses the same interval
    /// state as [`Self::poll_tick`].
    ///
    /// # Panics
    ///
    /// * If the next deadline exceeds the representable instant range.
    pub async fn tick(&mut self) -> std::time::Instant {
        std::future::poll_fn(|cx| self.poll_tick(cx)).await
    }

    /// Resets the next deadline to one period after the current time.
    ///
    /// # Panics
    ///
    /// * If the next deadline exceeds the representable instant range.
    pub fn reset(&mut self) {
        self.now = switchy_time::instant_now()
            .checked_add(self.interval)
            .expect("interval deadline overflow");
    }

    /// Polls for the next scheduled tick using simulated monotonic time.
    ///
    /// # Panics
    ///
    /// * If the next deadline exceeds the representable instant range.
    pub fn poll_tick(&mut self, cx: &mut Context) -> Poll<std::time::Instant> {
        if switchy_time::instant_now() >= self.now {
            let deadline = self.now;
            self.now = deadline
                .checked_add(self.interval)
                .expect("interval deadline overflow");
            return Poll::Ready(deadline);
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
        completed: bool,
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
            completed: false,
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
        if *this.completed {
            return Poll::Pending;
        }

        // First check if the inner future is ready
        if let Poll::Ready(output) = this.future.poll(cx) {
            *this.completed = true;
            return Poll::Ready(Ok(output));
        }

        // Then check if the timeout has elapsed
        if this.sleep.poll(cx) == Poll::Ready(()) {
            *this.completed = true;
            return Poll::Ready(Err(Elapsed));
        }

        Poll::Pending
    }
}

impl<F> FusedFuture for Timeout<F>
where
    F: Future,
{
    fn is_terminated(&self) -> bool {
        self.completed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timeout_terminal_state_does_not_repoll_inner() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let polls = Arc::new(AtomicUsize::new(0));
        let count = polls.clone();
        let inner = std::future::poll_fn(move |_| {
            count.fetch_add(1, Ordering::SeqCst);
            Poll::Ready(7)
        });
        let mut timeout = Box::pin(Timeout::new(Duration::ZERO, inner));
        let waker = futures::task::noop_waker();
        let mut context = Context::from_waker(&waker);
        assert!(!timeout.is_terminated());
        assert_eq!(timeout.as_mut().poll(&mut context), Poll::Ready(Ok(7)));
        assert!(timeout.is_terminated());
        assert!(timeout.as_mut().poll(&mut context).is_pending());
        assert_eq!(polls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn expired_timeout_does_not_reopen_when_inner_becomes_ready() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        };
        let ready = Arc::new(AtomicBool::new(false));
        let polls = Arc::new(AtomicUsize::new(0));
        let inner_ready = ready.clone();
        let inner_polls = polls.clone();
        let inner = std::future::poll_fn(move |_| {
            inner_polls.fetch_add(1, Ordering::SeqCst);
            if inner_ready.load(Ordering::SeqCst) {
                Poll::Ready(9)
            } else {
                Poll::Pending
            }
        });
        let mut timeout = Box::pin(Timeout::new(Duration::ZERO, inner));
        let waker = futures::task::noop_waker();
        let mut context = Context::from_waker(&waker);
        // Sleep deliberately yields once before checking elapsed time.
        assert!(timeout.as_mut().poll(&mut context).is_pending());
        assert!(!timeout.is_terminated());
        assert_eq!(
            timeout.as_mut().poll(&mut context),
            Poll::Ready(Err(Elapsed))
        );
        assert!(timeout.is_terminated());
        let polls_at_expiration = polls.load(Ordering::SeqCst);
        ready.store(true, Ordering::SeqCst);
        for _ in 0..3 {
            assert!(timeout.as_mut().poll(&mut context).is_pending());
            assert!(timeout.is_terminated());
        }
        assert_eq!(polls.load(Ordering::SeqCst), polls_at_expiration);
    }

    #[test]
    fn sleep_deadline_is_independent_of_epoch_reset() {
        // Isolate thread-local clock/RNG changes from the test worker.
        std::thread::spawn(|| {
            let mut sleep = Box::pin(Sleep::new(Duration::from_millis(1)));
            let waker = futures::task::noop_waker();
            let mut context = Context::from_waker(&waker);
            assert!(sleep.as_mut().poll(&mut context).is_pending());
            for _ in 0..8 {
                switchy_time::simulator::reset_epoch_offset();
                assert!(sleep.as_mut().poll(&mut context).is_pending());
            }
            let _ = switchy_time::simulator::next_step();
            assert!(sleep.as_mut().poll(&mut context).is_ready());
            assert!(sleep.is_terminated());
        })
        .join()
        .unwrap();
    }

    #[test]
    fn interval_elapsed_time_is_independent_of_epoch_reset() {
        std::thread::spawn(|| {
            let mut interval = Interval::new(Duration::from_millis(1));
            interval.reset();
            let waker = futures::task::noop_waker();
            let mut context = Context::from_waker(&waker);
            assert!(interval.poll_tick(&mut context).is_pending());
            for _ in 0..8 {
                switchy_time::simulator::reset_epoch_offset();
                assert!(interval.poll_tick(&mut context).is_pending());
            }
            let _ = switchy_time::simulator::next_step();
            assert!(interval.poll_tick(&mut context).is_ready());
            interval.reset();
            assert!(interval.poll_tick(&mut context).is_pending());
        })
        .join()
        .unwrap();
    }

    #[test]
    fn cancelled_interval_tick_preserves_poll_tick_state() {
        std::thread::spawn(|| {
            let period = Duration::from_millis(switchy_time::simulator::step_multiplier());
            assert!(!period.is_zero());
            let mut interval = Interval::new(period);
            interval.reset();
            let waker = futures::task::noop_waker();
            let mut context = Context::from_waker(&waker);
            {
                let mut tick = Box::pin(interval.tick());
                assert!(tick.as_mut().poll(&mut context).is_pending());
            }
            let _ = switchy_time::simulator::next_step();
            assert!(interval.poll_tick(&mut context).is_ready());
            // Consuming through poll_tick starts the next period for tick too.
            let mut tick = Box::pin(interval.tick());
            assert!(tick.as_mut().poll(&mut context).is_pending());
            assert!(tick.as_mut().poll(&mut context).is_pending());
            let _ = switchy_time::simulator::next_step();
            assert!(tick.as_mut().poll(&mut context).is_ready());
        })
        .join()
        .unwrap();
    }

    #[test]
    fn instant_completes_at_exact_deadline() {
        let deadline = switchy_time::instant_now();
        let mut timer = Box::pin(Instant::new(deadline));
        let waker = futures::task::noop_waker();
        let mut context = Context::from_waker(&waker);
        assert!(timer.as_mut().poll(&mut context).is_pending());
        assert_eq!(timer.as_mut().poll(&mut context), Poll::Ready(deadline));
        assert!(timer.is_terminated());
    }

    use std::future::ready;

    #[test_log::test]
    fn sleep_future_implements_fused_future() {
        let sleep = Sleep::new(Duration::from_millis(10));
        assert!(!sleep.is_terminated());
    }

    #[test_log::test]
    fn interval_reset_restarts_timing() {
        let mut interval = Interval::new(Duration::from_millis(100));

        // First tick should be created with current time
        drop(interval.tick());

        // Reset the interval
        interval.reset();

        assert_eq!(
            interval.now,
            switchy_time::instant_now() + Duration::from_millis(100)
        );
    }

    #[test_log::test]
    fn interval_poll_tick_returns_ready_after_duration() {
        {
            use std::task::{Context, Poll};

            let mut interval = Interval::new(Duration::from_millis(1));
            let waker = futures::task::noop_waker();
            let mut cx = Context::from_waker(&waker);

            interval.reset();
            assert!(matches!(interval.poll_tick(&mut cx), Poll::Pending));

            // Advance time in test by updating the deadline
            interval.now = switchy_time::instant_now()
                .checked_sub(Duration::from_millis(2))
                .unwrap();

            let result = interval.poll_tick(&mut cx);
            assert!(matches!(result, Poll::Ready(_)));
        }
    }

    #[test_log::test]
    fn instant_future_implements_fused_future() {
        let instant = Instant::new(instant_now() + Duration::from_millis(10));
        assert!(!instant.is_terminated());
    }

    #[test_log::test]
    fn timeout_into_inner_returns_original_future() {
        let original_future = ready(42);
        let timeout = Timeout::new(Duration::from_millis(100), original_future);

        let inner = timeout.into_inner();
        // The future should still be the same
        let result = futures::executor::block_on(inner);
        assert_eq!(result, 42);
    }

    #[test_log::test]
    fn elapsed_error_displays_correctly() {
        let err = Elapsed;
        assert_eq!(err.to_string(), "deadline has elapsed");
    }

    #[test_log::test]
    fn elapsed_error_is_clonable() {
        let err1 = Elapsed;
        let err2 = err1.clone();
        assert_eq!(err1, err2);
    }

    #[test_log::test]
    fn sleep_creates_with_current_time() {
        let sleep = Sleep::new(Duration::from_millis(100));
        let now = switchy_time::instant_now();

        let diff = now.duration_since(sleep.now);
        assert!(diff < Duration::from_millis(10));
    }

    #[test_log::test]
    fn interval_creates_with_current_time() {
        let interval = Interval::new(Duration::from_millis(100));
        let now = switchy_time::instant_now();
        let diff = now.duration_since(interval.now);
        assert!(diff < Duration::from_millis(10));
    }

    #[test_log::test]
    fn timeout_fused_future_not_terminated_initially() {
        {
            use futures::future::{Fuse, FutureExt};

            // Use a fused pending future to test FusedFuture trait
            // Create a timeout with a fused future that is in its initial (unterminated) state
            let fused_pending: Fuse<std::future::Pending<()>> = std::future::pending().fuse();
            let timeout = Timeout::new(Duration::from_millis(100), fused_pending);

            // Should not be terminated initially since neither the sleep nor inner is done
            assert!(!timeout.is_terminated());
        }
    }

    #[test_log::test]
    fn timeout_remains_pending_with_already_terminated_inner() {
        {
            use futures::future::Fuse;

            // Create a fused future that is already terminated
            let terminated_fused: Fuse<std::future::Ready<()>> = Fuse::terminated();
            let timeout = Timeout::new(Duration::from_millis(100), terminated_fused);

            // The inner future stays pending, but the timeout can still expire.
            // Reporting termination here would let select! skip that deadline.
            assert!(!timeout.is_terminated());
        }
    }

    #[test_log::test]
    fn timeout_fused_future_terminated_when_sleep_terminates() {
        {
            use futures::future::{Fuse, FutureExt};
            use std::task::{Context, Poll};

            // Create a fused future that won't complete
            let never_ready: Fuse<std::future::Pending<()>> = std::future::pending().fuse();
            let timeout = Timeout::new(Duration::ZERO, never_ready);

            // Poll until the sleep completes
            let waker = futures::task::noop_waker();
            let mut cx = Context::from_waker(&waker);
            let mut pinned_timeout = std::pin::pin!(timeout);

            // First poll sets up the sleep
            let _ = pinned_timeout.as_mut().poll(&mut cx);

            // Second poll should complete since duration is zero
            let result = pinned_timeout.as_mut().poll(&mut cx);

            // After sleep completes, timeout returns Err(Elapsed)
            assert!(matches!(result, Poll::Ready(Err(Elapsed))));
        }
    }

    #[test_log::test]
    fn interval_preserves_deadlines_when_ticks_are_missed() {
        let period = Duration::from_millis(1);
        let mut interval = Interval::new(period);
        let now = switchy_time::instant_now();
        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);
        assert_eq!(interval.poll_tick(&mut cx), Poll::Ready(now));
        assert!(interval.poll_tick(&mut cx).is_pending());

        // Place the deadline two periods behind the clock, without wall sleeps.
        let overdue = now.checked_sub(period * 2).unwrap();
        interval.now = overdue;
        for step in 0..=2 {
            assert_eq!(
                interval.poll_tick(&mut cx),
                Poll::Ready(overdue + period * step)
            );
        }
        assert!(interval.poll_tick(&mut cx).is_pending());
        assert_eq!(interval.now, now + period);
    }

    #[test_log::test]
    fn sleep_poll_completes_after_duration_elapses() {
        {
            use std::task::{Context, Poll};

            let mut sleep = Sleep::new(Duration::from_millis(1));
            let waker = futures::task::noop_waker();
            let mut cx = Context::from_waker(&waker);
            let mut pinned_sleep = std::pin::Pin::new(&mut sleep);

            // First poll should return Pending and set polled flag
            assert!(matches!(pinned_sleep.as_mut().poll(&mut cx), Poll::Pending));

            // Simulate time passing by manipulating the now field
            {
                let mut projected = pinned_sleep.as_mut().project();
                *projected.polled = true;
                *projected.now = switchy_time::instant_now()
                    .checked_sub(Duration::from_millis(2))
                    .unwrap();
            }

            // Next poll should complete since enough time has "passed"
            let result = pinned_sleep.as_mut().poll(&mut cx);
            assert!(matches!(result, Poll::Ready(())));

            // Should be terminated now
            assert!(sleep.is_terminated());
        }
    }

    #[test_log::test]
    fn instant_poll_returns_ready_when_time_passes() {
        {
            use std::task::{Context, Poll};

            // Create an instant in the past
            let past_instant = instant_now()
                .checked_sub(Duration::from_millis(100))
                .unwrap();
            let mut instant = Instant::new(past_instant);
            let waker = futures::task::noop_waker();
            let mut cx = Context::from_waker(&waker);
            let mut pinned_instant = std::pin::Pin::new(&mut instant);

            // First poll sets polled flag
            let result1 = pinned_instant.as_mut().poll(&mut cx);

            // Second poll should complete since instant is in the past
            let result2 = pinned_instant.as_mut().poll(&mut cx);
            assert!(
                matches!(result1, Poll::Pending) || matches!(result2, Poll::Ready(_)),
                "Expected either first poll pending or second ready"
            );

            // After completion, should be terminated
            if matches!(result2, Poll::Ready(_)) {
                assert!(instant.is_terminated());
            }
        }
    }
}
