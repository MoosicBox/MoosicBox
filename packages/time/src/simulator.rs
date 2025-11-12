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
