//! Backend-aware monotonic instants.

use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Clock {
    Real,
    /// A simulation run's clock: its context id and the clock generation.
    #[cfg(feature = "simulator")]
    Simulated(u64, u64),
}

/// A monotonic timestamp whose elapsed time uses its originating clock.
///
/// Unlike the legacy [`crate::instant_now`] result, this type never implicitly
/// reads real time when measuring a simulated interval. Simulation clocks are
/// currently thread-local: observing a simulated instant on another thread is
/// rejected rather than silently using that thread's unrelated clock. Resets,
/// rewinds and explicit multiplier resets invalidate captured simulated instants;
/// reading elapsed/remaining time then panics instead of mixing clock epochs.
// Ordering compares the clocks' instants; instants of different clocks are not comparable in
// any meaningful way, as with `std::time::Instant` across processes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instant {
    value: std::time::Instant,
    clock: Clock,
}

impl Instant {
    /// Captures the selected backend's current monotonic time.
    ///
    /// A simulator real-time override is retained by the resulting instant.
    ///
    /// # Panics
    ///
    /// * If the simulator clock configuration is invalid or overflows.
    #[must_use]
    pub fn now() -> Self {
        #[cfg(feature = "simulator")]
        if !crate::simulator::is_real_time() {
            return Self {
                value: crate::simulator::monotonic_instant_now(),
                clock: Clock::Simulated(
                    switchy_simulation::current().id(),
                    crate::simulator::clock_generation(),
                ),
            };
        }
        Self {
            value: std::time::Instant::now(),
            clock: Clock::Real,
        }
    }

    /// Measures elapsed time using the clock that created this instant.
    ///
    /// # Panics
    ///
    /// * If a simulated instant is observed in another simulation run or after a clock reset.
    /// * If the simulator clock configuration is invalid or overflows.
    #[must_use]
    pub fn elapsed(self) -> Duration {
        self.read_clock().saturating_duration_since(self.value)
    }

    /// Returns the remaining time until this instant, saturating at zero.
    ///
    /// # Panics
    ///
    /// * If a simulated instant is observed in another simulation run or after a clock reset.
    /// * If the simulator clock configuration is invalid or overflows.
    #[must_use]
    pub fn remaining(self) -> Duration {
        self.value.saturating_duration_since(self.read_clock())
    }

    /// Adds a duration without changing the clock, returning `None` on overflow.
    #[expect(
        clippy::must_use_candidate,
        reason = "Option is already must_use; repository API convention"
    )]
    pub fn checked_add(self, duration: Duration) -> Option<Self> {
        self.value
            .checked_add(duration)
            .map(|value| Self { value, ..self })
    }

    /// Subtracts a duration without changing the clock, returning `None` on overflow.
    #[expect(
        clippy::must_use_candidate,
        reason = "Option is already must_use; repository API convention"
    )]
    pub fn checked_sub(self, duration: Duration) -> Option<Self> {
        self.value
            .checked_sub(duration)
            .map(|value| Self { value, ..self })
    }

    /// Returns the interval, or `None` for different clocks or reversed instants.
    #[expect(
        clippy::must_use_candidate,
        reason = "Option is already must_use; repository API convention"
    )]
    pub fn checked_duration_since(self, earlier: Self) -> Option<Duration> {
        if self.clock != earlier.clock {
            return None;
        }
        self.value.checked_duration_since(earlier.value)
    }

    /// Returns the interval, saturating reversed instants at zero.
    ///
    /// # Panics
    ///
    /// * If the instants belong to different clocks or simulation generations.
    #[must_use]
    pub fn saturating_duration_since(self, earlier: Self) -> Duration {
        assert_eq!(
            self.clock, earlier.clock,
            "instants belong to different clocks"
        );
        self.value.saturating_duration_since(earlier.value)
    }

    /// Returns the interval since `earlier`, saturating reversed instants at zero (as
    /// `std::time::Instant::duration_since` does).
    ///
    /// # Panics
    ///
    /// * If the instants belong to different clocks or simulation generations.
    #[must_use]
    pub fn duration_since(self, earlier: Self) -> Duration {
        self.saturating_duration_since(earlier)
    }

    fn read_clock(self) -> std::time::Instant {
        match self.clock {
            Clock::Real => std::time::Instant::now(),
            #[cfg(feature = "simulator")]
            Clock::Simulated(run, generation) => {
                assert_eq!(
                    run,
                    switchy_simulation::current().id(),
                    "simulation clock belongs to another simulation run"
                );
                assert_eq!(
                    generation,
                    crate::simulator::clock_generation(),
                    "simulation clock was reset"
                );
                crate::simulator::monotonic_instant_now()
            }
        }
    }
}

impl std::ops::Add<Duration> for Instant {
    type Output = Self;

    /// # Panics
    ///
    /// * On overflow, as `std::time::Instant` does.
    fn add(self, duration: Duration) -> Self {
        self.checked_add(duration)
            .expect("overflow when adding duration to instant")
    }
}

impl std::ops::AddAssign<Duration> for Instant {
    fn add_assign(&mut self, duration: Duration) {
        *self = *self + duration;
    }
}

impl std::ops::Sub<Duration> for Instant {
    type Output = Self;

    /// # Panics
    ///
    /// * On overflow, as `std::time::Instant` does.
    fn sub(self, duration: Duration) -> Self {
        self.checked_sub(duration)
            .expect("overflow when subtracting duration from instant")
    }
}

impl std::ops::SubAssign<Duration> for Instant {
    fn sub_assign(&mut self, duration: Duration) {
        *self = *self - duration;
    }
}

impl std::ops::Sub<Self> for Instant {
    type Output = Duration;

    /// The interval since `earlier`, saturating at zero; see [`Instant::duration_since`].
    fn sub(self, earlier: Self) -> Duration {
        self.duration_since(earlier)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instants_order_add_and_subtract_like_std() {
        let start = Instant::now();
        let later = start + Duration::from_secs(1);
        assert!(later > start);
        assert_eq!(later - start, Duration::from_secs(1));
        assert_eq!(later.duration_since(start), Duration::from_secs(1));
        assert_eq!(start.duration_since(later), Duration::ZERO);
        assert_eq!(later - Duration::from_secs(1), start);
        let mut moved = start;
        moved += Duration::from_secs(1);
        assert_eq!(moved, later);
        moved -= Duration::from_secs(1);
        assert_eq!(moved, start);
    }

    #[cfg(feature = "simulator")]
    #[test]
    #[serial_test::serial]
    fn elapsed_and_deadlines_follow_only_simulated_steps() {
        let previous = crate::simulator::current_step();
        let _ = crate::simulator::set_step(0);
        let started = Instant::now();
        let step = Duration::from_millis(crate::simulator::step_multiplier());
        let deadline = started.checked_add(step * 2).unwrap();
        assert_eq!(started.elapsed(), Duration::ZERO);
        assert_eq!(deadline.remaining(), step * 2);
        let _ = crate::simulator::set_step(1);
        assert_eq!(started.elapsed(), step);
        assert_eq!(deadline.remaining(), step);
        // A real-time scope must not change an existing simulated clock.
        crate::simulator::with_real_time(|| {
            assert_eq!(started.elapsed(), step);
            assert_eq!(Instant::now().checked_duration_since(started), None);
        });
        let _ = crate::simulator::set_step(3);
        assert_eq!(deadline.remaining(), Duration::ZERO);
        assert_eq!(started.checked_duration_since(Instant::now()), None);
        let _ = crate::simulator::set_step(previous);
    }

    #[cfg(feature = "simulator")]
    #[test]
    #[serial_test::serial]
    fn simulated_instant_is_shared_within_a_run_and_rejected_outside_it() {
        let started = Instant::now();
        // A thread without the run's context belongs to another run.
        assert!(
            std::thread::spawn(move || started.elapsed())
                .join()
                .is_err()
        );
        // A thread given the run's context (as switchy_thread does) shares its clock.
        let run = switchy_simulation::current();
        let elapsed = std::thread::spawn(move || {
            switchy_simulation::set_current(Some(run));
            started.elapsed()
        })
        .join()
        .unwrap();
        assert_eq!(elapsed, started.elapsed());
    }

    #[cfg(feature = "simulator")]
    #[test]
    #[serial_test::serial]
    fn resets_and_rewinds_reject_stale_clock_observations() {
        use crate::simulator;
        let previous = simulator::current_step();
        for reset in [
            simulator::reset_step as fn(),
            simulator::reset_step_multiplier,
            || {
                let _ = simulator::set_step(0);
            },
        ] {
            let _ = simulator::set_step(2);
            let stale = Instant::now();
            reset();
            let current = Instant::now();
            assert_eq!(current.checked_duration_since(stale), None);
            assert!(std::panic::catch_unwind(|| current.saturating_duration_since(stale)).is_err());
            assert!(std::panic::catch_unwind(|| stale.elapsed()).is_err());
            assert!(std::panic::catch_unwind(|| stale.remaining()).is_err());
            assert_eq!(current.elapsed(), Duration::ZERO);
        }
        // Even a reset while already at zero starts a distinct epoch.
        simulator::reset_step();
        let stale = Instant::now();
        simulator::reset_step();
        assert_eq!(Instant::now().checked_duration_since(stale), None);
        let _ = simulator::set_step(previous);
    }

    #[test]
    fn real_elapsed_is_bounded_by_real_observations() {
        let check = || {
            let before = std::time::Instant::now();
            let started = Instant::now();
            let elapsed = started.elapsed();
            assert!(elapsed <= before.elapsed());
            assert_eq!(
                started.checked_duration_since(started),
                Some(Duration::ZERO)
            );
            let later = started.checked_add(Duration::from_secs(1)).unwrap();
            assert_eq!(
                later.checked_duration_since(started),
                Some(Duration::from_secs(1))
            );
            assert_eq!(started.saturating_duration_since(later), Duration::ZERO);
            assert_eq!(
                later.saturating_duration_since(started),
                Duration::from_secs(1)
            );
            assert_eq!(later.checked_sub(Duration::from_secs(1)), Some(started));
        };
        #[cfg(feature = "simulator")]
        crate::simulator::with_real_time(check);
        #[cfg(not(feature = "simulator"))]
        check();
    }
}
