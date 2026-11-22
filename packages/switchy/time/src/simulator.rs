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
//!
//! The wall clock ([`now`]) may also jump at a clock advance, forward or backward, as NTP and
//! daylight saving adjustments make it: see [`set_wall_clock_jumps`]. The monotonic clock
//! ([`instant_now`], [`crate::Instant`]) never does.

use std::{
    cell::Cell,
    sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Marker type for temporarily using real system time instead of simulated time.
///
/// This type is used internally by [`with_real_time`] to track when code should
/// use actual system time rather than simulated time. It has no public methods
/// and is only used as a scoped thread-local marker.
pub struct RealTime;

thread_local! {
    /// How many [`with_real_time`] scopes the current thread is inside. A property of the
    /// call stack, not of the simulation run, so it stays per OS thread.
    static REAL_TIME_DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// The simulated clock of one simulation run, kept in its [`switchy_simulation::Context`] so
/// every simulated thread of the run reads the same time.
#[derive(Default)]
struct Clock(Mutex<ClockState>);

#[derive(Default)]
struct ClockState {
    epoch_offset: Option<u64>,
    step_multiplier: Option<u64>,
    clock_generation: u64,
    step: u64,
    /// How far the wall clock has jumped in all, in milliseconds.
    wall_adjustment: i64,
    /// The probability of a wall clock jump at each advance (`None`: not read yet).
    wall_jumps: Option<f64>,
}

/// The farthest a wall clock jump goes forward.
pub const WALL_CLOCK_JUMP_FORWARD: Duration = Duration::from_hours(1);
/// The farthest a wall clock jump goes backward.
pub const WALL_CLOCK_JUMP_BACKWARD: Duration = Duration::from_mins(1);

/// Make the current run's wall clock jump with probability `probability` (clamped to 0..=1)
/// at each clock advance, instead of `SIMULATOR_WALL_CLOCK_JUMPS` (default 0).
///
/// A jump moves [`now`] (not [`instant_now`] or [`crate::Instant`], which stay monotonic)
/// forward by up to [`WALL_CLOCK_JUMP_FORWARD`] or, as often, backward by up to
/// [`WALL_CLOCK_JUMP_BACKWARD`]. Its draws come from the run's `switchy_time::wall_clock`
/// stream, and only while the probability is above 0, so other draw sequences are the same
/// either way.
pub fn set_wall_clock_jumps(probability: f64) {
    with_clock(|clock| clock.wall_jumps = Some(probability.clamp(0.0, 1.0)));
}

/// The current run's probability of a wall clock jump at each advance: the one set with
/// [`set_wall_clock_jumps`], or else `SIMULATOR_WALL_CLOCK_JUMPS` (default 0).
///
/// # Panics
///
/// * If `SIMULATOR_WALL_CLOCK_JUMPS` is not a number
#[must_use]
pub fn wall_clock_jumps() -> f64 {
    with_clock(wall_jumps)
}

fn wall_jumps(clock: &mut ClockState) -> f64 {
    *clock.wall_jumps.get_or_insert_with(|| {
        std::env::var("SIMULATOR_WALL_CLOCK_JUMPS").map_or(0.0, |value| {
            value
                .parse::<f64>()
                .unwrap_or_else(|_| {
                    panic!("SIMULATOR_WALL_CLOCK_JUMPS must be a probability, got '{value}'")
                })
                .clamp(0.0, 1.0)
        })
    })
}

/// How far the current run's wall clock has jumped in all (see [`set_wall_clock_jumps`]), in
/// milliseconds: positive when ahead.
#[must_use]
pub fn wall_clock_adjustment() -> i64 {
    with_clock(|clock| clock.wall_adjustment)
}

/// At a clock advance, maybe jump the wall clock.
fn maybe_jump_wall_clock(clock: &mut ClockState) {
    let probability = wall_jumps(clock);
    if probability <= 0.0 {
        return;
    }
    let jump = {
        let stream = switchy_simulation::stream("switchy_time::wall_clock");
        let mut rng = stream.lock().unwrap_or_else(PoisonError::into_inner);
        if !rng.chance(probability) {
            return;
        }
        let forward = rng.chance(0.5);
        let limit = if forward {
            WALL_CLOCK_JUMP_FORWARD
        } else {
            WALL_CLOCK_JUMP_BACKWARD
        };
        let limit = u64::try_from(limit.as_millis()).unwrap_or(u64::MAX);
        let millis = i64::try_from(rng.range_inclusive(1..=limit)).unwrap_or(i64::MAX);
        if forward { millis } else { -millis }
    };
    crate::trace!("wall clock jumps by {jump}ms");
    clock.wall_adjustment = clock.wall_adjustment.saturating_add(jump);
}

fn clock() -> Arc<Clock> {
    switchy_simulation::state::<Clock>()
}

fn clock_state(clock: &Clock) -> MutexGuard<'_, ClockState> {
    clock.0.lock().unwrap_or_else(PoisonError::into_inner)
}

fn with_clock<R>(f: impl FnOnce(&mut ClockState) -> R) -> R {
    let clock = clock();
    let mut state = clock_state(&clock);
    f(&mut state)
}

/// A uniform draw in `min..=max` for the clock's random parameters.
///
/// With the `random` feature the run's `switchy_random` generator is used, which keeps the
/// sequences of earlier releases; without it a dedicated `switchy_simulation` stream is used,
/// so the simulator needs no `rand`.
fn draw_inclusive(min: u64, max: u64) -> u64 {
    #[cfg(feature = "random")]
    {
        switchy_random::rng().gen_range(min..=max)
    }
    #[cfg(not(feature = "random"))]
    {
        switchy_simulation::stream("switchy_time")
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .range_inclusive(min..=max)
    }
}

/// A step multiplier in `1..1_000_000_000`, skewed strongly towards small values.
fn draw_step_multiplier() -> u64 {
    #[cfg(feature = "random")]
    {
        switchy_random::rng().gen_range_disti(1..1_000_000_000, 20)
    }
    #[cfg(not(feature = "random"))]
    {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        {
            let (value, unit) = {
                let stream = switchy_simulation::stream("switchy_time");
                let mut rng = stream.lock().unwrap_or_else(PoisonError::into_inner);
                (rng.range(1..1_000_000_000) as f64, rng.unit_f64())
            };
            let factor = unit.mul_add(1.0 - 0.0001, 0.0001);
            (value * factor.powi(20)).round() as u64
        }
    }
}

/// Executes a function using real system time instead of simulated time.
///
/// This temporarily disables time simulation for the duration of the function call,
/// allowing code to access actual system time even when running in simulator mode.
///
/// # Examples
///
/// ```rust
/// use switchy_time::simulator::{with_real_time, now};
///
/// // Inside this closure, now() returns actual system time
/// let real_time = with_real_time(|| {
///     now()
/// });
/// ```
pub fn with_real_time<T>(f: impl FnOnce() -> T) -> T {
    struct Leave;
    impl Drop for Leave {
        fn drop(&mut self) {
            REAL_TIME_DEPTH.set(REAL_TIME_DEPTH.get() - 1);
        }
    }
    REAL_TIME_DEPTH.set(REAL_TIME_DEPTH.get() + 1);
    let _leave = Leave;
    f()
}

pub(crate) fn is_real_time() -> bool {
    REAL_TIME_DEPTH.get() > 0
}

const EPOCH_PROFILE_LOW_MIN: u64 = 946_684_800_000;
const EPOCH_PROFILE_LOW_MAX: u64 = 2_524_608_000_000;
const EPOCH_PROFILE_WIDE_MIN: u64 = 315_532_800_000;
const EPOCH_PROFILE_WIDE_MAX: u64 = 4_102_444_800_000;
const EPOCH_PROFILE_FULL_MIN: u64 = 1;
const EPOCH_PROFILE_FULL_MAX: u64 = 99_999_999_999_999;

fn parse_u64_env(var_name: &str, value: &str) -> u64 {
    value.parse::<u64>().unwrap_or_else(|e| {
        panic!("{var_name} must be a valid u64 unix millis value: '{value}' ({e})")
    })
}

fn profile_bounds(profile: &str) -> (u64, u64) {
    match profile.trim().to_ascii_lowercase().as_str() {
        "low" => (EPOCH_PROFILE_LOW_MIN, EPOCH_PROFILE_LOW_MAX),
        "wide" => (EPOCH_PROFILE_WIDE_MIN, EPOCH_PROFILE_WIDE_MAX),
        "full" => (EPOCH_PROFILE_FULL_MIN, EPOCH_PROFILE_FULL_MAX),
        other => {
            panic!("SIMULATOR_EPOCH_RANGE_PROFILE must be one of low|wide|full, got '{other}'")
        }
    }
}

fn epoch_bounds() -> (u64, u64) {
    let min = std::env::var("SIMULATOR_EPOCH_MIN").ok();
    let max = std::env::var("SIMULATOR_EPOCH_MAX").ok();

    if min.is_some() || max.is_some() {
        let min_str = min.unwrap_or_else(|| {
            panic!("SIMULATOR_EPOCH_MIN and SIMULATOR_EPOCH_MAX must both be set")
        });
        let max_str = max.unwrap_or_else(|| {
            panic!("SIMULATOR_EPOCH_MIN and SIMULATOR_EPOCH_MAX must both be set")
        });

        let min_value = parse_u64_env("SIMULATOR_EPOCH_MIN", &min_str);
        let max_value = parse_u64_env("SIMULATOR_EPOCH_MAX", &max_str);

        assert!(
            min_value <= max_value,
            "SIMULATOR_EPOCH_MIN ({min_value}) must be <= SIMULATOR_EPOCH_MAX ({max_value})"
        );

        return (min_value, max_value);
    }

    std::env::var("SIMULATOR_EPOCH_RANGE_PROFILE").ok().map_or(
        (EPOCH_PROFILE_FULL_MIN, EPOCH_PROFILE_FULL_MAX),
        |profile| profile_bounds(&profile),
    )
}

fn gen_epoch_offset() -> u64 {
    // Draw even when the offset is given, so a run replayed with its recorded
    // `SIMULATOR_EPOCH_OFFSET` takes as many draws from the run's generator as the original
    // did, and every later draw matches.
    let (min, max) = epoch_bounds();
    let drawn = draw_inclusive(min, max);
    std::env::var("SIMULATOR_EPOCH_OFFSET").map_or(drawn, |value| {
        parse_u64_env("SIMULATOR_EPOCH_OFFSET", &value)
    })
}

/// Resets the epoch offset to a new random value.
///
/// The epoch offset determines the base Unix timestamp for simulated time.
///
/// # Panics
///
/// * If the `EPOCH_OFFSET` `RwLock` fails to write to
/// * If `SIMULATOR_EPOCH_OFFSET` is set but cannot be parsed as a `u64`
/// * If `SIMULATOR_EPOCH_MIN` and `SIMULATOR_EPOCH_MAX` are not both set when either is provided
/// * If `SIMULATOR_EPOCH_MIN` or `SIMULATOR_EPOCH_MAX` cannot be parsed as `u64`
/// * If `SIMULATOR_EPOCH_MIN` is greater than `SIMULATOR_EPOCH_MAX`
/// * If `SIMULATOR_EPOCH_RANGE_PROFILE` is set to an unsupported value
pub fn reset_epoch_offset() {
    let value = gen_epoch_offset();
    crate::trace!("reset_epoch_offset to seed={value}");
    with_clock(|clock| clock.epoch_offset = Some(value));
}

/// Sets the epoch offset (in milliseconds) of the current run's clock.
///
/// Unlike setting `SIMULATOR_EPOCH_OFFSET`, this draws nothing from the run's generator, so a
/// harness can pin a recorded run's offset after [`reset_epoch_offset`] without shifting any
/// later draw.
pub fn set_epoch_offset(millis: u64) {
    crate::trace!("set_epoch_offset to {millis}");
    with_clock(|clock| clock.epoch_offset = Some(millis));
}

/// Returns the current epoch offset in milliseconds.
///
/// The epoch offset is the base Unix timestamp used for time simulation.
/// If not previously set, generates and caches a new random value.
///
/// # Panics
///
/// * If the `EPOCH_OFFSET` `RwLock` fails to read from or write to
/// * If `SIMULATOR_EPOCH_OFFSET` is set but cannot be parsed as a `u64`
/// * If `SIMULATOR_EPOCH_MIN` and `SIMULATOR_EPOCH_MAX` are not both set when either is provided
/// * If `SIMULATOR_EPOCH_MIN` or `SIMULATOR_EPOCH_MAX` cannot be parsed as `u64`
/// * If `SIMULATOR_EPOCH_MIN` is greater than `SIMULATOR_EPOCH_MAX`
/// * If `SIMULATOR_EPOCH_RANGE_PROFILE` is set to an unsupported value
#[must_use]
pub fn epoch_offset() -> u64 {
    if let Some(value) = with_clock(|clock| clock.epoch_offset) {
        return value;
    }
    let value = gen_epoch_offset();
    with_clock(|clock| *clock.epoch_offset.get_or_insert(value))
}

fn gen_step_multiplier() -> u64 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let value = {
        let value = draw_step_multiplier();
        if value == 0 { 1 } else { value }
    };
    std::env::var("SIMULATOR_STEP_MULTIPLIER")
        .ok()
        .map_or(value, |x| x.parse::<u64>().unwrap())
}

/// Resets the step multiplier to a new random value.
///
/// The step multiplier controls how much simulated time advances per step.
/// Resetting it invalidates captured [`crate::Instant`] values.
///
/// # Panics
///
/// * If the clock generation counter overflows.
/// * If the `STEP_MULTIPLIER` `RwLock` fails to write to
/// * If the `SIMULATOR_STEP_MULTIPLIER` environment variable is set but cannot be parsed as a `u64`
pub fn reset_step_multiplier() {
    let value = gen_step_multiplier();
    crate::trace!("reset_step_multiplier to seed={value}");
    with_clock(|clock| {
        advance_generation(clock);
        clock.step_multiplier = Some(value);
    });
}

/// Sets the step multiplier: how many milliseconds of simulated time pass per step (at
/// least one). Like [`reset_step_multiplier`], it invalidates captured [`crate::Instant`]
/// values.
///
/// # Panics
///
/// * If the clock generation counter overflows.
pub fn set_step_multiplier(millis: u64) {
    crate::trace!("set_step_multiplier to {millis}");
    with_clock(|clock| {
        advance_generation(clock);
        clock.step_multiplier = Some(millis.max(1));
    });
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
    if let Some(value) = with_clock(|clock| clock.step_multiplier) {
        return value;
    }
    let value = gen_step_multiplier();
    with_clock(|clock| *clock.step_multiplier.get_or_insert(value))
}

pub(crate) fn clock_generation() -> u64 {
    with_clock(|clock| clock.clock_generation)
}

const fn advance_generation(clock: &mut ClockState) {
    clock.clock_generation = clock
        .clock_generation
        .checked_add(1)
        .expect("simulation clock generation overflow");
}

/// Sets the current simulation step to the specified value.
///
/// The step counter controls the progression of simulated time. Moving backwards
/// invalidates captured [`crate::Instant`] values.
///
/// # Panics
///
/// * If the clock generation counter overflows during a rewind.
/// * If the `STEP` `RwLock` fails to write to
#[must_use]
pub fn set_step(step: u64) -> u64 {
    crate::trace!("set_step to step={step}");
    with_clock(|clock| {
        if step < clock.step {
            advance_generation(clock);
        } else if step > clock.step {
            maybe_jump_wall_clock(clock);
        }
        clock.step = step;
    });
    step
}

/// Advances the simulation to the next step.
///
/// Increments the step counter by one, advancing simulated time.
///
/// # Panics
///
/// * If the `STEP` `RwLock` fails to read from or write to
/// * If the step counter is already `u64::MAX`; the counter is left unchanged.
#[must_use]
pub fn next_step() -> u64 {
    set_step(
        current_step()
            .checked_add(1)
            .expect("simulation step overflow"),
    )
}

/// Resets the simulation step counter to zero and invalidates captured
/// [`crate::Instant`] values, even when the counter is already zero.
///
/// # Panics
///
/// * If the clock generation counter overflows.
/// * If the `STEP` `RwLock` fails to write to
pub fn reset_step() {
    with_clock(|clock| {
        advance_generation(clock);
        clock.step = 0;
    });
}

/// Advances the simulation to the first step at which [`instant_now`] is at or after
/// `target`, returning the step. Does nothing when `target` has already passed.
///
/// Simulated time only moves in whole steps, so the clock may land past `target` by up to one
/// step multiplier.
///
/// # Panics
///
/// * If the step counter would overflow
#[must_use]
pub fn advance_to(target: Instant) -> u64 {
    let now = monotonic_instant_now();
    let step = current_step();
    if target <= now {
        return step;
    }
    let step_nanos = u128::from(step_multiplier()) * 1_000_000;
    let steps = (target - now).as_nanos().div_ceil(step_nanos);
    let steps = u64::try_from(steps).expect("simulation step overflow");
    set_step(step.checked_add(steps).expect("simulation step overflow"))
}

/// Returns the current simulation step.
///
/// # Panics
///
/// * If the `STEP` `RwLock` fails to read from
#[must_use]
pub fn current_step() -> u64 {
    with_clock(|clock| clock.step)
}

/// Returns the current simulated system time, or real time if in a `with_real_time` context.
///
/// Simulated time is calculated based on the epoch offset, step counter, and step multiplier.
///
/// # Examples
///
/// ```rust
/// use std::time::UNIX_EPOCH;
/// use switchy_time::simulator::{now, reset_step, set_step};
///
/// reset_step();
/// let _ = set_step(0);
/// let t0 = now();
/// let _ = set_step(1);
/// let t1 = now();
///
/// assert!(t1.duration_since(t0).is_ok());
/// assert!(t0.duration_since(UNIX_EPOCH).is_ok());
/// ```
///
/// # Panics
///
/// * If multiplication or addition overflows while calculating simulated milliseconds
/// * If adding the simulated duration to `UNIX_EPOCH` overflows
#[must_use]
pub fn now() -> SystemTime {
    if is_real_time() {
        return SystemTime::now();
    }

    let epoch_offset = epoch_offset();
    let step_multiplier = step_multiplier();
    let step = current_step();
    let mult_step = step.checked_mul(step_multiplier).unwrap();
    let millis = epoch_offset.checked_add(mult_step).unwrap();
    let millis = millis.saturating_add_signed(wall_clock_adjustment());
    crate::trace!(
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
/// # Examples
///
/// ```rust
/// use switchy_time::simulator::{instant_now, reset_step, set_step};
///
/// reset_step();
/// let _ = set_step(0);
/// let i0 = instant_now();
/// let _ = set_step(1);
/// let i1 = instant_now();
///
/// assert!(i1 >= i0);
/// ```
///
/// # Panics
///
/// * If multiplication overflows while calculating simulated milliseconds
#[must_use]
pub fn instant_now() -> Instant {
    if is_real_time() {
        return Instant::now();
    }

    monotonic_instant_now()
}

/// Reads the simulator clock even inside a real-time override.
pub(crate) fn monotonic_instant_now() -> Instant {
    let step_multiplier = step_multiplier();
    let step = current_step();
    let mult_step = step.checked_mul(step_multiplier).unwrap();
    let duration = Duration::from_millis(mult_step);

    crate::trace!(
        "instant_now: step={step} step_multiplier={step_multiplier} duration_millis={mult_step}"
    );

    *BASE_INSTANT + duration
}

/// Returns the current simulated local date and time, or real time if in a `with_real_time` context.
///
/// # Panics
///
/// * If [`now`] panics while calculating simulated time
#[cfg(feature = "chrono")]
#[must_use]
pub fn datetime_local_now() -> chrono::DateTime<chrono::Local> {
    if is_real_time() {
        return chrono::Local::now();
    }

    // Convert simulated SystemTime to Local DateTime
    let system_time = now();
    chrono::DateTime::from(system_time)
}

/// Returns the current simulated UTC date and time, or real time if in a `with_real_time` context.
///
/// # Panics
///
/// * If [`now`] panics while calculating simulated time
#[cfg(feature = "chrono")]
#[must_use]
pub fn datetime_utc_now() -> chrono::DateTime<chrono::Utc> {
    if is_real_time() {
        return chrono::Utc::now();
    }

    // Convert simulated SystemTime to UTC DateTime
    let system_time = now();
    chrono::DateTime::from(system_time)
}

#[cfg(test)]
mod tests {
    #[test]
    fn real_time_scope_restores_simulation_after_unwind() {
        std::thread::spawn(|| {
            super::with_clock(|clock| clock.step_multiplier = Some(1));
            super::with_clock(|clock| clock.epoch_offset = Some(1));
            let wall = super::now();
            let monotonic = super::instant_now();
            let result = std::panic::catch_unwind(|| {
                super::with_real_time(|| {
                    assert!(super::is_real_time());
                    let nested = std::panic::catch_unwind(|| {
                        super::with_real_time(|| panic!("nested scope failure"));
                    });
                    assert!(nested.is_err());
                    assert!(super::is_real_time());
                    panic!("outer scope failure");
                });
            });
            assert!(result.is_err());
            assert!(!super::is_real_time());
            assert_eq!(super::now(), wall);
            assert_eq!(super::instant_now(), monotonic);
            let _ = super::next_step();
            assert_eq!(super::now(), wall + Duration::from_millis(1));
            assert_eq!(super::instant_now(), monotonic + Duration::from_millis(1));
        })
        .join()
        .unwrap();
    }

    #[test]
    fn overflowing_clock_reads_preserve_state() {
        std::thread::spawn(|| {
            // Set private thread-local fixtures instead of mutating process environment.
            super::with_clock(|clock| clock.step_multiplier = Some(2));
            super::with_clock(|clock| clock.epoch_offset = Some(1));
            let wall = super::now();
            let monotonic = super::instant_now();
            let _ = super::set_step(u64::MAX);
            assert!(std::panic::catch_unwind(super::now).is_err());
            assert!(std::panic::catch_unwind(super::instant_now).is_err());
            assert_eq!(super::current_step(), u64::MAX);
            assert_eq!(super::step_multiplier(), 2);
            assert_eq!(super::epoch_offset(), 1);
            super::reset_step();
            assert_eq!(super::now(), wall);
            assert_eq!(super::instant_now(), monotonic);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn epoch_addition_overflow_does_not_break_monotonic_time() {
        std::thread::spawn(|| {
            super::with_clock(|clock| clock.step_multiplier = Some(1));
            super::with_clock(|clock| clock.epoch_offset = Some(u64::MAX));
            let baseline = super::instant_now();
            let _ = super::next_step();
            assert!(std::panic::catch_unwind(super::now).is_err());
            assert_eq!(super::current_step(), 1);
            assert_eq!(super::epoch_offset(), u64::MAX);
            assert_eq!(super::instant_now(), baseline + Duration::from_millis(1));
            // Repair only the wall-clock fixture; do not rewind monotonic time.
            super::with_clock(|clock| clock.epoch_offset = Some(1));
            assert_eq!(super::now(), UNIX_EPOCH + Duration::from_millis(2));
            assert_eq!(super::instant_now(), baseline + Duration::from_millis(1));
        })
        .join()
        .unwrap();
    }

    /// Wall clock (milliseconds since the epoch) and monotonic readings over 300 advances of
    /// a second each, with jumps at `probability`.
    fn wall_clock_readings(seed: u64, probability: Option<f64>) -> Vec<(u64, Instant)> {
        switchy_simulation::with_context(switchy_simulation::Context::new(seed), || {
            super::set_step_multiplier(1000);
            super::set_epoch_offset(1_700_000_000_000);
            if let Some(probability) = probability {
                super::set_wall_clock_jumps(probability);
            }
            (0..300)
                .map(|_| {
                    let _ = super::next_step();
                    let wall = super::now().duration_since(UNIX_EPOCH).unwrap();
                    (
                        u64::try_from(wall.as_millis()).unwrap(),
                        super::instant_now(),
                    )
                })
                .collect()
        })
    }

    #[test]
    fn the_wall_clock_jumps_while_instants_stay_monotonic() {
        let readings = wall_clock_readings(3, Some(0.2));
        let (mut forward, mut backward) = (0, 0);
        for pair in readings.windows(2) {
            let ((wall, instant), (next_wall, next_instant)) = (pair[0], pair[1]);
            assert_eq!(next_instant - instant, Duration::from_secs(1));
            let step = i128::from(next_wall) - i128::from(wall);
            assert!((1000 - 60_000..=1000 + 3_600_000).contains(&step), "{step}");
            if step > 1000 {
                forward += 1;
            } else if step < 1000 {
                backward += 1;
            }
        }
        assert!(
            forward > 0 && backward > 0,
            "{forward} forward, {backward} backward"
        );
        assert!(
            readings.windows(2).any(|pair| pair[1].0 < pair[0].0),
            "went backward"
        );
    }

    #[test]
    fn wall_clock_jumps_are_deterministic_per_seed() {
        let first = wall_clock_readings(11, Some(0.1));
        let walls = |readings: &[(u64, Instant)]| readings.iter().map(|r| r.0).collect::<Vec<_>>();
        assert_eq!(walls(&first), walls(&wall_clock_readings(11, Some(0.1))));
        assert_ne!(walls(&first), walls(&wall_clock_readings(12, Some(0.1))));
    }

    #[test]
    fn without_jumps_the_wall_clock_follows_the_steps_and_draws_nothing() {
        for probability in [None, Some(0.0)] {
            let readings = wall_clock_readings(5, probability);
            for (i, (wall, _)) in readings.iter().enumerate() {
                assert_eq!(*wall, 1_700_000_000_000 + (i as u64 + 1) * 1000);
            }
        }
        // The jump stream is untouched: its first draw is a fresh run's first draw.
        let first_draw = |advance: bool| {
            switchy_simulation::with_context(switchy_simulation::Context::new(5), || {
                if advance {
                    super::set_wall_clock_jumps(0.0);
                    let _ = super::next_step();
                }
                switchy_simulation::stream("switchy_time::wall_clock")
                    .lock()
                    .unwrap()
                    .next_u64()
            })
        };
        assert_eq!(first_draw(true), first_draw(false));
    }

    #[test]
    fn advance_to_lands_on_the_first_step_reaching_the_target() {
        std::thread::spawn(|| {
            super::with_clock(|clock| clock.step_multiplier = Some(10));
            let start = super::instant_now();
            assert_eq!(super::advance_to(start), 0);
            assert_eq!(super::advance_to(start + Duration::from_millis(1)), 1);
            assert_eq!(super::advance_to(start + Duration::from_millis(25)), 3);
            assert_eq!(super::instant_now(), start + Duration::from_millis(30));
            assert_eq!(super::advance_to(start + Duration::from_millis(30)), 3);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn set_step_multiplier_sets_the_time_per_step() {
        std::thread::spawn(|| {
            super::set_step_multiplier(0);
            assert_eq!(super::step_multiplier(), 1);
            super::set_step_multiplier(7);
            let start = super::instant_now();
            let _ = super::next_step();
            assert_eq!(super::instant_now() - start, Duration::from_millis(7));
        })
        .join()
        .unwrap();
    }

    #[test]
    fn next_step_overflow_preserves_counter() {
        std::thread::spawn(|| {
            let _ = super::set_step(u64::MAX - 1);
            assert_eq!(super::next_step(), u64::MAX);
            assert!(std::panic::catch_unwind(super::next_step).is_err());
            assert_eq!(super::current_step(), u64::MAX);
        })
        .join()
        .unwrap();
    }
    use super::*;
    use serial_test::serial;
    use std::{collections::BTreeMap, time::Duration};
    #[cfg(feature = "random")]
    use switchy_random::rand::rand::Rng as _;

    // Note: All tests in this module use #[serial] because they interact with:
    // 1. Thread-local state (EPOCH_OFFSET, STEP_MULTIPLIER, STEP) that can be reused
    //    between tests when the test runner's thread pool reuses threads
    // 2. Global environment variables (SIMULATOR_EPOCH_OFFSET, SIMULATOR_STEP_MULTIPLIER)
    // 3. The test_reset_step_multiplier test modifies environment variables using unsafe blocks
    //
    // Running these tests in parallel causes race conditions where one test's state changes
    // affect another test's expectations. The serial_test crate ensures tests run one at a time.

    struct EnvGuard {
        originals: BTreeMap<String, Option<String>>,
    }

    impl EnvGuard {
        fn new(names: &[&str]) -> Self {
            let originals = names
                .iter()
                .map(|name| (name.to_string(), std::env::var(name).ok()))
                .collect::<BTreeMap<_, _>>();
            Self { originals }
        }

        fn set(name: &str, value: &str) {
            unsafe {
                std::env::set_var(name, value);
            }
        }

        fn remove(name: &str) {
            unsafe {
                std::env::remove_var(name);
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (name, value) in &self.originals {
                match value {
                    Some(value) => unsafe {
                        std::env::set_var(name, value);
                    },
                    None => unsafe {
                        std::env::remove_var(name);
                    },
                }
            }
        }
    }

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
    fn test_set_epoch_offset_pins_the_offset() {
        reset_epoch_offset();
        set_epoch_offset(1_234_567);
        assert_eq!(epoch_offset(), 1_234_567);
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
    fn test_reset_epoch_offset_with_env_var() {
        let _guard = EnvGuard::new(&[
            "SIMULATOR_EPOCH_OFFSET",
            "SIMULATOR_EPOCH_MIN",
            "SIMULATOR_EPOCH_MAX",
            "SIMULATOR_EPOCH_RANGE_PROFILE",
        ]);

        // Set a known value and reset to it
        EnvGuard::set("SIMULATOR_EPOCH_OFFSET", "5000000000");
        reset_epoch_offset();
        let first = epoch_offset();
        assert_eq!(first, 5_000_000_000, "Should use env var value");

        // Change to a different value and reset again
        EnvGuard::set("SIMULATOR_EPOCH_OFFSET", "9000000000");
        reset_epoch_offset();
        let second = epoch_offset();
        assert_eq!(second, 9_000_000_000, "Should use new env var value");

        // Verify they're different
        assert_ne!(first, second);
    }

    #[test_log::test]
    #[serial]
    fn test_epoch_offset_range_precedence_uses_fixed_offset() {
        let _guard = EnvGuard::new(&[
            "SIMULATOR_EPOCH_OFFSET",
            "SIMULATOR_EPOCH_MIN",
            "SIMULATOR_EPOCH_MAX",
            "SIMULATOR_EPOCH_RANGE_PROFILE",
        ]);

        EnvGuard::set("SIMULATOR_EPOCH_OFFSET", "42");
        EnvGuard::set("SIMULATOR_EPOCH_MIN", "100");
        EnvGuard::set("SIMULATOR_EPOCH_MAX", "200");
        EnvGuard::set("SIMULATOR_EPOCH_RANGE_PROFILE", "low");

        reset_epoch_offset();

        assert_eq!(epoch_offset(), 42);
    }

    #[test_log::test]
    #[serial]
    fn test_epoch_offset_uses_min_max_bounds() {
        let _guard = EnvGuard::new(&[
            "SIMULATOR_EPOCH_OFFSET",
            "SIMULATOR_EPOCH_MIN",
            "SIMULATOR_EPOCH_MAX",
            "SIMULATOR_EPOCH_RANGE_PROFILE",
        ]);
        EnvGuard::remove("SIMULATOR_EPOCH_OFFSET");
        EnvGuard::set("SIMULATOR_EPOCH_MIN", "100");
        EnvGuard::set("SIMULATOR_EPOCH_MAX", "200");
        EnvGuard::set("SIMULATOR_EPOCH_RANGE_PROFILE", "full");

        for _ in 0..50 {
            reset_epoch_offset();
            let offset = epoch_offset();
            assert!((100..=200).contains(&offset));
        }
    }

    #[test_log::test]
    #[serial]
    fn test_epoch_offset_uses_profile_low() {
        let _guard = EnvGuard::new(&[
            "SIMULATOR_EPOCH_OFFSET",
            "SIMULATOR_EPOCH_MIN",
            "SIMULATOR_EPOCH_MAX",
            "SIMULATOR_EPOCH_RANGE_PROFILE",
        ]);
        EnvGuard::remove("SIMULATOR_EPOCH_OFFSET");
        EnvGuard::remove("SIMULATOR_EPOCH_MIN");
        EnvGuard::remove("SIMULATOR_EPOCH_MAX");
        EnvGuard::set("SIMULATOR_EPOCH_RANGE_PROFILE", "low");

        for _ in 0..20 {
            reset_epoch_offset();
            let offset = epoch_offset();
            assert!((EPOCH_PROFILE_LOW_MIN..=EPOCH_PROFILE_LOW_MAX).contains(&offset));
        }
    }

    #[test_log::test]
    #[serial]
    fn test_epoch_offset_uses_profile_full() {
        let _guard = EnvGuard::new(&[
            "SIMULATOR_EPOCH_OFFSET",
            "SIMULATOR_EPOCH_MIN",
            "SIMULATOR_EPOCH_MAX",
            "SIMULATOR_EPOCH_RANGE_PROFILE",
        ]);
        EnvGuard::remove("SIMULATOR_EPOCH_OFFSET");
        EnvGuard::remove("SIMULATOR_EPOCH_MIN");
        EnvGuard::remove("SIMULATOR_EPOCH_MAX");
        EnvGuard::set("SIMULATOR_EPOCH_RANGE_PROFILE", "full");

        reset_epoch_offset();
        let offset = epoch_offset();

        assert!((EPOCH_PROFILE_FULL_MIN..=EPOCH_PROFILE_FULL_MAX).contains(&offset));
    }

    #[cfg(feature = "random")]
    #[test_log::test]
    #[serial]
    fn test_seeded_epoch_offsets_follow_deterministic_sequence() {
        let _guard = EnvGuard::new(&[
            "SIMULATOR_SEED",
            "SIMULATOR_EPOCH_OFFSET",
            "SIMULATOR_EPOCH_MIN",
            "SIMULATOR_EPOCH_MAX",
            "SIMULATOR_EPOCH_RANGE_PROFILE",
        ]);
        EnvGuard::set("SIMULATOR_SEED", "424242");
        EnvGuard::remove("SIMULATOR_EPOCH_OFFSET");
        EnvGuard::remove("SIMULATOR_EPOCH_MIN");
        EnvGuard::remove("SIMULATOR_EPOCH_MAX");
        EnvGuard::set("SIMULATOR_EPOCH_RANGE_PROFILE", "wide");

        switchy_random::simulator::reset_rng();

        let mut expected_rng =
            switchy_random::simulator::SimulatorRng::new(switchy_random::simulator::seed());
        let expected_a = expected_rng.gen_range(EPOCH_PROFILE_WIDE_MIN..=EPOCH_PROFILE_WIDE_MAX);
        let expected_b = expected_rng.gen_range(EPOCH_PROFILE_WIDE_MIN..=EPOCH_PROFILE_WIDE_MAX);

        reset_epoch_offset();
        let actual_a = epoch_offset();
        reset_epoch_offset();
        let actual_b = epoch_offset();

        assert_eq!(actual_a, expected_a);
        assert_eq!(actual_b, expected_b);
    }

    #[test_log::test]
    #[serial]
    #[should_panic(expected = "SIMULATOR_EPOCH_MIN and SIMULATOR_EPOCH_MAX must both be set")]
    fn test_epoch_offset_min_requires_max() {
        let _guard = EnvGuard::new(&[
            "SIMULATOR_EPOCH_OFFSET",
            "SIMULATOR_EPOCH_MIN",
            "SIMULATOR_EPOCH_MAX",
            "SIMULATOR_EPOCH_RANGE_PROFILE",
        ]);
        EnvGuard::remove("SIMULATOR_EPOCH_OFFSET");
        EnvGuard::set("SIMULATOR_EPOCH_MIN", "100");
        EnvGuard::remove("SIMULATOR_EPOCH_MAX");
        EnvGuard::remove("SIMULATOR_EPOCH_RANGE_PROFILE");

        reset_epoch_offset();
    }

    #[test_log::test]
    #[serial]
    #[should_panic(expected = "SIMULATOR_EPOCH_MIN and SIMULATOR_EPOCH_MAX must both be set")]
    fn test_epoch_offset_max_requires_min() {
        let _guard = EnvGuard::new(&[
            "SIMULATOR_EPOCH_OFFSET",
            "SIMULATOR_EPOCH_MIN",
            "SIMULATOR_EPOCH_MAX",
            "SIMULATOR_EPOCH_RANGE_PROFILE",
        ]);
        EnvGuard::remove("SIMULATOR_EPOCH_OFFSET");
        EnvGuard::remove("SIMULATOR_EPOCH_MIN");
        EnvGuard::set("SIMULATOR_EPOCH_MAX", "100");
        EnvGuard::remove("SIMULATOR_EPOCH_RANGE_PROFILE");

        reset_epoch_offset();
    }

    #[test_log::test]
    #[serial]
    #[should_panic(expected = "SIMULATOR_EPOCH_MIN (200) must be <= SIMULATOR_EPOCH_MAX (100)")]
    fn test_epoch_offset_rejects_invalid_bounds() {
        let _guard = EnvGuard::new(&[
            "SIMULATOR_EPOCH_OFFSET",
            "SIMULATOR_EPOCH_MIN",
            "SIMULATOR_EPOCH_MAX",
            "SIMULATOR_EPOCH_RANGE_PROFILE",
        ]);
        EnvGuard::remove("SIMULATOR_EPOCH_OFFSET");
        EnvGuard::set("SIMULATOR_EPOCH_MIN", "200");
        EnvGuard::set("SIMULATOR_EPOCH_MAX", "100");
        EnvGuard::remove("SIMULATOR_EPOCH_RANGE_PROFILE");

        reset_epoch_offset();
    }

    #[test_log::test]
    #[serial]
    #[should_panic(expected = "SIMULATOR_EPOCH_RANGE_PROFILE must be one of low|wide|full")]
    fn test_epoch_offset_rejects_invalid_profile() {
        let _guard = EnvGuard::new(&[
            "SIMULATOR_EPOCH_OFFSET",
            "SIMULATOR_EPOCH_MIN",
            "SIMULATOR_EPOCH_MAX",
            "SIMULATOR_EPOCH_RANGE_PROFILE",
        ]);
        EnvGuard::remove("SIMULATOR_EPOCH_OFFSET");
        EnvGuard::remove("SIMULATOR_EPOCH_MIN");
        EnvGuard::remove("SIMULATOR_EPOCH_MAX");
        EnvGuard::set("SIMULATOR_EPOCH_RANGE_PROFILE", "unknown");

        reset_epoch_offset();
    }

    #[test_log::test]
    #[serial]
    #[should_panic(expected = "SIMULATOR_EPOCH_MIN must be a valid u64 unix millis value")]
    fn test_epoch_offset_rejects_invalid_min_parse() {
        let _guard = EnvGuard::new(&[
            "SIMULATOR_EPOCH_OFFSET",
            "SIMULATOR_EPOCH_MIN",
            "SIMULATOR_EPOCH_MAX",
            "SIMULATOR_EPOCH_RANGE_PROFILE",
        ]);
        EnvGuard::remove("SIMULATOR_EPOCH_OFFSET");
        EnvGuard::set("SIMULATOR_EPOCH_MIN", "abc");
        EnvGuard::set("SIMULATOR_EPOCH_MAX", "100");
        EnvGuard::remove("SIMULATOR_EPOCH_RANGE_PROFILE");

        reset_epoch_offset();
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

        let _ = set_step(100);
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
        let _ = set_step(100);
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
        let _ = next_step();
        let time2 = now();
        let _ = next_step();
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
        let _ = set_step(5);
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
        let _ = next_step();
        let instant2 = instant_now();
        let _ = next_step();
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
        let _ = set_step(10);
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
        assert!(sim_diff > Duration::from_hours(24));
    }

    #[test_log::test]
    #[serial]
    fn test_with_real_time_instant() {
        reset_step();
        reset_step_multiplier();

        let base_instant = *BASE_INSTANT;

        // Get simulated instant
        let _ = set_step(1000);
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
        let _ = set_step(100);
        assert_eq!(current_step(), 100);

        // Reset should take us back to 0
        reset_step();
        assert_eq!(current_step(), 0);

        // Should be able to increment from 0 again
        let _ = next_step();
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
        let _ = set_step(42);
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
        let _ = set_step(large_step);

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
