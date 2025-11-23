//! Time simulation for deterministic testing.
//!
//! This module provides a simulated time system where time progression is controlled
//! programmatically via step counters and multipliers. This enables deterministic testing
//! of time-dependent code.
//!
//! Time simulation is based on three components:
//!
//! * Epoch offset - The base Unix timestamp in milliseconds
//! * Step counter - The current simulation step
//! * Step multiplier - How many milliseconds of simulated time pass per step
//!
//! Simulated time is calculated as: `epoch_offset + (step * step_multiplier)`

use std::{
    cell::RefCell,
    sync::{LazyLock, RwLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use scoped_tls::scoped_thread_local;

/// Marker type for temporarily using real system time instead of simulated time.
pub struct RealTime;

scoped_thread_local! {
    static REAL_TIME: RealTime
}

/// Executes a function using real system time instead of simulated time.
///
/// This temporarily disables time simulation for the duration of the function call,
/// allowing code to access actual system time even when running in simulator mode.
pub fn with_real_time<T>(f: impl FnOnce() -> T) -> T {
    REAL_TIME.set(&RealTime, f)
}

thread_local! {
    static EPOCH_OFFSET: RefCell<RwLock<Option<u64>>> = const { RefCell::new(RwLock::new(None)) };
}

fn gen_epoch_offset() -> u64 {
    let value = switchy_random::rng().gen_range(1..100_000_000_000_000u64);

    std::env::var("SIMULATOR_EPOCH_OFFSET")
        .ok()
        .map_or(value, |x| x.parse::<u64>().unwrap())
}

/// Resets the epoch offset to a new random value.
///
/// The epoch offset determines the base Unix timestamp for simulated time.
///
/// # Panics
///
/// * If the `EPOCH_OFFSET` `RwLock` fails to write to
/// * If the `SIMULATOR_EPOCH_OFFSET` environment variable is set but cannot be parsed as a `u64`
pub fn reset_epoch_offset() {
    let value = gen_epoch_offset();
    log::trace!("reset_epoch_offset to seed={value}");
    EPOCH_OFFSET.with_borrow_mut(|x| *x.write().unwrap() = Some(value));
}

/// Returns the current epoch offset in milliseconds.
///
/// The epoch offset is the base Unix timestamp used for time simulation.
/// If not previously set, generates and caches a new random value.
///
/// # Panics
///
/// * If the `EPOCH_OFFSET` `RwLock` fails to read from or write to
/// * If the `SIMULATOR_EPOCH_OFFSET` environment variable is set but cannot be parsed as a `u64`
#[must_use]
pub fn epoch_offset() -> u64 {
    let value = EPOCH_OFFSET.with_borrow(|x| *x.read().unwrap());
    value.unwrap_or_else(|| {
        let value = gen_epoch_offset();
        EPOCH_OFFSET.with_borrow_mut(|x| *x.write().unwrap() = Some(value));
        value
    })
}

thread_local! {
    static STEP_MULTIPLIER: RefCell<RwLock<Option<u64>>> = const { RefCell::new(RwLock::new(None)) };
}

fn gen_step_multiplier() -> u64 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let value = {
        let value = switchy_random::rng().gen_range_disti(1..1_000_000_000, 20);
        if value == 0 { 1 } else { value }
    };
    std::env::var("SIMULATOR_STEP_MULTIPLIER")
        .ok()
        .map_or(value, |x| x.parse::<u64>().unwrap())
}

/// Resets the step multiplier to a new random value.
///
/// The step multiplier controls how much simulated time advances per step.
///
/// # Panics
///
/// * If the `STEP_MULTIPLIER` `RwLock` fails to write to
/// * If the `SIMULATOR_STEP_MULTIPLIER` environment variable is set but cannot be parsed as a `u64`
pub fn reset_step_multiplier() {
    let value = gen_step_multiplier();
    log::trace!("reset_step_multiplier to seed={value}");
    STEP_MULTIPLIER.with_borrow_mut(|x| *x.write().unwrap() = Some(value));
}

/// Returns the current step multiplier in milliseconds per step.
///
/// The step multiplier determines how much simulated time advances with each step.
/// If not previously set, generates and caches a new random value.
///
/// # Panics
///
/// * If the `STEP_MULTIPLIER` `RwLock` fails to read from or write to
/// * If the `SIMULATOR_STEP_MULTIPLIER` environment variable is set but cannot be parsed as a `u64`
#[must_use]
pub fn step_multiplier() -> u64 {
    let value = STEP_MULTIPLIER.with_borrow(|x| *x.read().unwrap());
    value.unwrap_or_else(|| {
        let value = gen_step_multiplier();
        STEP_MULTIPLIER.with_borrow_mut(|x| *x.write().unwrap() = Some(value));
        value
    })
}

thread_local! {
    static STEP: RefCell<RwLock<u64>> = const { RefCell::new(RwLock::new(0)) };
}

/// Sets the current simulation step to the specified value.
///
/// The step counter controls the progression of simulated time.
///
/// # Panics
///
/// * If the `STEP` `RwLock` fails to write to
#[allow(clippy::must_use_candidate)]
pub fn set_step(step: u64) -> u64 {
    log::trace!("set_step to step={step}");
    STEP.with_borrow_mut(|x| *x.write().unwrap() = step);
    step
}

/// Advances the simulation to the next step.
///
/// Increments the step counter by one, advancing simulated time.
///
/// # Panics
///
/// * If the `STEP` `RwLock` fails to read from or write to
#[allow(clippy::must_use_candidate)]
pub fn next_step() -> u64 {
    set_step(current_step() + 1)
}

/// Resets the simulation step counter to zero.
///
/// # Panics
///
/// * If the `STEP` `RwLock` fails to write to
pub fn reset_step() {
    set_step(0);
}

/// Returns the current simulation step.
///
/// # Panics
///
/// * If the `STEP` `RwLock` fails to read from
#[must_use]
pub fn current_step() -> u64 {
    STEP.with_borrow(|x| *x.read().unwrap())
}

/// Returns the current simulated system time, or real time if in a `with_real_time` context.
///
/// Simulated time is calculated based on the epoch offset, step counter, and step multiplier.
///
/// # Panics
///
/// * If the simulated `UNIX_EPOCH` offset is larger than a `u64` can store
#[must_use]
pub fn now() -> SystemTime {
    if REAL_TIME.is_set() {
        return SystemTime::now();
    }

    let epoch_offset = epoch_offset();
    let step_multiplier = step_multiplier();
    let step = current_step();
    let mult_step = step.checked_mul(step_multiplier).unwrap();
    let millis = epoch_offset.checked_add(mult_step).unwrap();
    log::trace!(
        "now: epoch_offset={epoch_offset} step={step} step_multiplier={step_multiplier} millis={millis}"
    );
    UNIX_EPOCH
        .checked_add(Duration::from_millis(millis))
        .unwrap()
}

// Base instant for simulated monotonic time
static BASE_INSTANT: LazyLock<Instant> = LazyLock::new(Instant::now);

/// Returns a simulated monotonic instant, or real instant if in a `with_real_time` context.
///
/// Simulated instant is calculated based on the current step and step multiplier.
///
/// # Panics
///
/// * If the simulated duration causes an overflow
#[must_use]
pub fn instant_now() -> Instant {
    if REAL_TIME.is_set() {
        return Instant::now();
    }

    let step_multiplier = step_multiplier();
    let step = current_step();
    let mult_step = step.checked_mul(step_multiplier).unwrap();
    let duration = Duration::from_millis(mult_step);

    log::trace!(
        "instant_now: step={step} step_multiplier={step_multiplier} duration_millis={mult_step}"
    );

    *BASE_INSTANT + duration
}

/// Returns the current simulated local date and time, or real time if in a `with_real_time` context.
#[cfg(feature = "chrono")]
#[must_use]
pub fn datetime_local_now() -> chrono::DateTime<chrono::Local> {
    if REAL_TIME.is_set() {
        return chrono::Local::now();
    }

    // Convert simulated SystemTime to Local DateTime
    let system_time = now();
    chrono::DateTime::from(system_time)
}

/// Returns the current simulated UTC date and time, or real time if in a `with_real_time` context.
#[cfg(feature = "chrono")]
#[must_use]
pub fn datetime_utc_now() -> chrono::DateTime<chrono::Utc> {
    if REAL_TIME.is_set() {
        return chrono::Utc::now();
    }

    // Convert simulated SystemTime to UTC DateTime
    let system_time = now();
    chrono::DateTime::from(system_time)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::time::Duration;

    // Note: All tests in this module use #[serial] because they interact with:
    // 1. Thread-local state (EPOCH_OFFSET, STEP_MULTIPLIER, STEP) that can be reused
    //    between tests when the test runner's thread pool reuses threads
    // 2. Global environment variables (SIMULATOR_EPOCH_OFFSET, SIMULATOR_STEP_MULTIPLIER)
    // 3. The test_reset_step_multiplier test modifies environment variables using unsafe blocks
    //
    // Running these tests in parallel causes race conditions where one test's state changes
    // affect another test's expectations. The serial_test crate ensures tests run one at a time.

    #[test_log::test]
    #[serial]
    fn test_epoch_offset_initialization() {
        // Reset to get a fresh epoch offset
        reset_epoch_offset();
        let offset = epoch_offset();

        // Epoch offset should be in a reasonable range
        assert!(offset >= 1);
        assert!(offset < 100_000_000_000_000u64);

        // Multiple calls should return the same value
        assert_eq!(offset, epoch_offset());
    }

    #[test_log::test]
    #[serial]
    fn test_reset_epoch_offset() {
        reset_epoch_offset();
        let first = epoch_offset();

        reset_epoch_offset();
        let second = epoch_offset();

        // After reset, we should get a different value (with extremely high probability)
        // Note: There's a tiny chance this could fail if random generates the same value
        assert_ne!(first, second);
    }

    #[test_log::test]
    #[serial]
    fn test_step_multiplier_initialization() {
        reset_step_multiplier();
        let multiplier = step_multiplier();

        // Step multiplier should be at least 1
        assert!(multiplier >= 1);

        // Multiple calls should return the same value
        assert_eq!(multiplier, step_multiplier());
    }

    #[test_log::test]
    #[serial]
    fn test_reset_step_multiplier() {
        // Save original value
        let original = std::env::var("SIMULATOR_STEP_MULTIPLIER").ok();

        // Set a known value and reset to it
        unsafe {
            std::env::set_var("SIMULATOR_STEP_MULTIPLIER", "100");
        }
        reset_step_multiplier();
        let first = step_multiplier();
        assert_eq!(first, 100, "Should use env var value");

        // Change to a different value and reset again
        unsafe {
            std::env::set_var("SIMULATOR_STEP_MULTIPLIER", "200");
        }
        reset_step_multiplier();
        let second = step_multiplier();
        assert_eq!(second, 200, "Should use new env var value");

        // Verify they're different
        assert_ne!(first, second);

        // Restore original value
        match original {
            Some(val) => unsafe {
                std::env::set_var("SIMULATOR_STEP_MULTIPLIER", val);
            },
            None => unsafe {
                std::env::remove_var("SIMULATOR_STEP_MULTIPLIER");
            },
        }
    }

    #[test_log::test]
    #[serial]
    fn test_current_step_starts_at_zero() {
        reset_step();
        assert_eq!(current_step(), 0);
    }

    #[test_log::test]
    #[serial]
    fn test_set_step() {
        reset_step();

        let result = set_step(42);
        assert_eq!(result, 42);
        assert_eq!(current_step(), 42);

        set_step(100);
        assert_eq!(current_step(), 100);
    }

    #[test_log::test]
    #[serial]
    fn test_next_step() {
        reset_step();
        assert_eq!(current_step(), 0);

        let first = next_step();
        assert_eq!(first, 1);
        assert_eq!(current_step(), 1);

        let second = next_step();
        assert_eq!(second, 2);
        assert_eq!(current_step(), 2);
    }

    #[test_log::test]
    #[serial]
    fn test_reset_step() {
        set_step(100);
        assert_eq!(current_step(), 100);

        reset_step();
        assert_eq!(current_step(), 0);
    }

    #[test_log::test]
    #[serial]
    fn test_now_advances_with_steps() {
        reset_epoch_offset();
        reset_step_multiplier();
        reset_step();

        let time1 = now();
        next_step();
        let time2 = now();
        next_step();
        let time3 = now();

        // Time should advance monotonically
        assert!(time2 > time1);
        assert!(time3 > time2);

        // Calculate expected differences
        let diff1 = time2.duration_since(time1).unwrap();
        let diff2 = time3.duration_since(time2).unwrap();

        // The differences should be equal to the step multiplier
        let multiplier = step_multiplier();
        assert_eq!(diff1, Duration::from_millis(multiplier));
        assert_eq!(diff2, Duration::from_millis(multiplier));
    }

    #[test_log::test]
    #[serial]
    fn test_now_calculation() {
        reset_step();
        reset_epoch_offset();
        reset_step_multiplier();

        let epoch = epoch_offset();
        let multiplier = step_multiplier();

        // At step 0
        let time = now();
        let duration_since_epoch = time.duration_since(UNIX_EPOCH).unwrap();
        assert_eq!(duration_since_epoch.as_millis(), u128::from(epoch));

        // At step 5
        set_step(5);
        let time = now();
        let duration_since_epoch = time.duration_since(UNIX_EPOCH).unwrap();
        let expected_millis = epoch + (5 * multiplier);
        assert_eq!(
            duration_since_epoch.as_millis(),
            u128::from(expected_millis)
        );
    }

    #[test_log::test]
    #[serial]
    fn test_instant_now_advances_with_steps() {
        reset_step_multiplier();
        reset_step();

        let instant1 = instant_now();
        next_step();
        let instant2 = instant_now();
        next_step();
        let instant3 = instant_now();

        // Instant should advance monotonically
        assert!(instant2 > instant1);
        assert!(instant3 > instant2);

        // Calculate expected differences
        let diff1 = instant2 - instant1;
        let diff2 = instant3 - instant2;

        // The differences should be equal to the step multiplier
        let multiplier = step_multiplier();
        assert_eq!(diff1, Duration::from_millis(multiplier));
        assert_eq!(diff2, Duration::from_millis(multiplier));
    }

    #[test_log::test]
    #[serial]
    fn test_instant_now_calculation() {
        reset_step();
        reset_step_multiplier();

        let multiplier = step_multiplier();
        let base = instant_now(); // At step 0

        // At step 10
        set_step(10);
        let instant = instant_now();
        let elapsed = instant - base;
        assert_eq!(elapsed, Duration::from_millis(10 * multiplier));
    }

    #[test_log::test]
    #[serial]
    fn test_with_real_time_now() {
        reset_step();
        reset_epoch_offset();

        // Get simulated time (should be based on epoch offset, likely in the past)
        let sim_time = now();

        // Get real time within with_real_time context
        let real_time = with_real_time(now);

        // Real time should be different from simulated time
        // Real time should be close to actual system time
        let actual_system_time = SystemTime::now();
        let diff = actual_system_time
            .duration_since(real_time)
            .or_else(|_| real_time.duration_since(actual_system_time))
            .unwrap();

        // Real time should be within 1 second of actual system time
        assert!(diff < Duration::from_secs(1));

        // Simulated time should be very different from real time
        let sim_diff = actual_system_time
            .duration_since(sim_time)
            .or_else(|_| sim_time.duration_since(actual_system_time))
            .unwrap();

        // Simulated time should be at least 1 day different from real time
        // (given the random epoch offset range)
        assert!(sim_diff > Duration::from_secs(86400));
    }

    #[test_log::test]
    #[serial]
    fn test_with_real_time_instant() {
        reset_step();
        reset_step_multiplier();

        let base_instant = *BASE_INSTANT;

        // Get simulated instant
        set_step(1000);
        let sim_instant = instant_now();

        // Simulated instant should be far ahead of base
        assert!(sim_instant > base_instant);

        // Get real instant within with_real_time context
        let real_instant = with_real_time(instant_now);

        // Real instant should be close to actual Instant::now()
        // The simulated instant should be different
        assert_ne!(sim_instant, real_instant);
    }

    #[test_log::test]
    #[serial]
    fn test_with_real_time_nested() {
        reset_step();

        // Nested with_real_time should work
        let result = with_real_time(|| {
            let time1 = now();

            with_real_time(|| {
                let time2 = now();

                // Both should be real time, so very close to each other
                let diff = time2
                    .duration_since(time1)
                    .or_else(|_| time1.duration_since(time2))
                    .unwrap();

                // Should be within a few milliseconds
                assert!(diff < Duration::from_millis(100));

                time2
            })
        });

        // Should have gotten a real time value
        let actual = SystemTime::now();
        let diff = actual
            .duration_since(result)
            .or_else(|_| result.duration_since(actual))
            .unwrap();
        assert!(diff < Duration::from_secs(1));
    }

    #[test_log::test]
    #[serial]
    fn test_step_counter_independence_across_resets() {
        // Set up initial state
        set_step(100);
        assert_eq!(current_step(), 100);

        // Reset should take us back to 0
        reset_step();
        assert_eq!(current_step(), 0);

        // Should be able to increment from 0 again
        next_step();
        assert_eq!(current_step(), 1);
    }

    #[test_log::test]
    #[serial]
    fn test_time_simulation_consistency() {
        // Test that time calculation is consistent
        reset_step();
        reset_epoch_offset();
        reset_step_multiplier();

        let epoch = epoch_offset();
        let multiplier = step_multiplier();

        // Calculate expected time for step 42
        set_step(42);
        let expected_millis = epoch + (42 * multiplier);
        let expected_time = UNIX_EPOCH + Duration::from_millis(expected_millis);

        let actual_time = now();

        assert_eq!(actual_time, expected_time);
    }

    #[cfg(feature = "chrono")]
    #[test_log::test]
    #[serial]
    fn test_datetime_utc_now() {
        reset_step();
        reset_epoch_offset();
        reset_step_multiplier();

        let system_time = now();
        let datetime = datetime_utc_now();

        // Convert SystemTime to chrono DateTime for comparison
        let expected: chrono::DateTime<chrono::Utc> = system_time.into();

        // Should be the same time
        assert_eq!(datetime, expected);
    }

    #[cfg(feature = "chrono")]
    #[test_log::test]
    #[serial]
    fn test_datetime_local_now() {
        reset_step();
        reset_epoch_offset();
        reset_step_multiplier();

        let system_time = now();
        let datetime = datetime_local_now();

        // Convert SystemTime to chrono DateTime for comparison
        let expected: chrono::DateTime<chrono::Local> = system_time.into();

        // Should be the same time
        assert_eq!(datetime, expected);
    }

    #[cfg(feature = "chrono")]
    #[test_log::test]
    #[serial]
    fn test_datetime_with_real_time() {
        reset_step();

        // Get simulated datetime
        let sim_datetime = datetime_utc_now();

        // Get real datetime
        let real_datetime = with_real_time(datetime_utc_now);

        // They should be different
        assert_ne!(sim_datetime, real_datetime);

        // Real datetime should be close to actual UTC now
        let actual = chrono::Utc::now();
        let diff = if real_datetime > actual {
            real_datetime - actual
        } else {
            actual - real_datetime
        };

        // Should be within 1 second
        assert!(diff < chrono::Duration::seconds(1));
    }

    #[test_log::test]
    #[serial]
    fn test_step_multiplier_never_zero() {
        // Even if random generation produces 0, it should be corrected to 1
        // We can't easily test this directly, but we can verify the invariant
        reset_step_multiplier();
        let multiplier = step_multiplier();
        assert!(multiplier >= 1);
    }

    #[test_log::test]
    #[serial]
    fn test_large_step_values() {
        reset_step_multiplier();
        let multiplier = step_multiplier();

        // Test with a large step value
        let large_step = 1_000_000u64;
        set_step(large_step);

        // Should not panic and should calculate correctly
        let time = now();
        let instant = instant_now();

        // Verify time advanced appropriately
        assert!(time > UNIX_EPOCH);

        // The instant should have advanced by large_step * multiplier milliseconds
        let expected_duration = Duration::from_millis(large_step * multiplier);
        let actual_duration = instant - *BASE_INSTANT;
        assert_eq!(actual_duration, expected_duration);
    }

    #[test_log::test]
    #[serial]
    fn test_epoch_offset_caching() {
        reset_epoch_offset();

        // First call initializes
        let first = epoch_offset();

        // Subsequent calls should return cached value without reinitializing
        let second = epoch_offset();
        let third = epoch_offset();

        assert_eq!(first, second);
        assert_eq!(second, third);
    }

    #[test_log::test]
    #[serial]
    fn test_step_multiplier_caching() {
        reset_step_multiplier();

        // First call initializes
        let first = step_multiplier();

        // Subsequent calls should return cached value without reinitializing
        let second = step_multiplier();
        let third = step_multiplier();

        assert_eq!(first, second);
        assert_eq!(second, third);
    }
}
