use std::{
    cell::RefCell,
    sync::RwLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use scoped_tls::scoped_thread_local;

pub struct RealTime;

scoped_thread_local! {
    static REAL_TIME: RealTime
}

pub fn with_real_time<T>(f: impl FnOnce() -> T) -> T {
    REAL_TIME.set(&RealTime, f)
}

thread_local! {
    static EPOCH_OFFSET: RefCell<RwLock<Option<u64>>> = const { RefCell::new(RwLock::new(None)) };
}

fn gen_epoch_offset() -> u64 {
    let value = moosicbox_random::rng().gen_range(1..100_000_000_000_000u64);

    std::env::var("SIMULATOR_EPOCH_OFFSET")
        .ok()
        .map_or(value, |x| x.parse::<u64>().unwrap())
}

/// # Panics
///
/// * If the `EPOCH_OFFSET` `RwLock` fails to write to
pub fn reset_epoch_offset() {
    let value = gen_epoch_offset();
    log::trace!("reset_epoch_offset to seed={value}");
    EPOCH_OFFSET.with_borrow_mut(|x| *x.write().unwrap() = Some(value));
}

/// # Panics
///
/// * If the `EPOCH_OFFSET` `RwLock` fails to read from
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
        let value = moosicbox_random::rng().gen_range_disti(1..1_000_000_000, 20);
        if value == 0 { 1 } else { value }
    };
    std::env::var("SIMULATOR_STEP_MULTIPLIER")
        .ok()
        .map_or(value, |x| x.parse::<u64>().unwrap())
}

/// # Panics
///
/// * If the `STEP_MULTIPLIER` `RwLock` fails to write to
pub fn reset_step_multiplier() {
    let value = gen_step_multiplier();
    log::trace!("reset_step_multiplier to seed={value}");
    STEP_MULTIPLIER.with_borrow_mut(|x| *x.write().unwrap() = Some(value));
}

/// # Panics
///
/// * If the `STEP_MULTIPLIER` `RwLock` fails to read from
#[must_use]
pub fn step_multiplier() -> u64 {
    let value = STEP_MULTIPLIER.with_borrow(|x| *x.read().unwrap());
    value.unwrap_or_else(|| {
        let value = gen_epoch_offset();
        STEP_MULTIPLIER.with_borrow_mut(|x| *x.write().unwrap() = Some(value));
        value
    })
}

thread_local! {
    static STEP: RefCell<RwLock<u64>> = const { RefCell::new(RwLock::new(0)) };
}

/// # Panics
///
/// * If the `STEP` `RwLock` fails to write to
#[allow(clippy::must_use_candidate)]
pub fn set_step(step: u64) -> u64 {
    log::trace!("set_step to step={step}");
    STEP.with_borrow_mut(|x| *x.write().unwrap() = step);
    step
}

/// # Panics
///
/// * If the `STEP` `RwLock` fails to write to
#[allow(clippy::must_use_candidate)]
pub fn next_step() -> u64 {
    set_step(current_step() + 1)
}

/// # Panics
///
/// * If the `STEP` `RwLock` fails to write to
pub fn reset_step() {
    set_step(0);
}

/// # Panics
///
/// * If the `STEP` `RwLock` fails to read from
#[must_use]
pub fn current_step() -> u64 {
    STEP.with_borrow(|x| *x.read().unwrap())
}

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
