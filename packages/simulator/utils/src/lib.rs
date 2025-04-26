#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::{LazyLock, RwLock, atomic::AtomicU32};

use tokio_util::sync::CancellationToken;

static DURATION: LazyLock<u64> = LazyLock::new(|| {
    std::env::var("SIMULATOR_DURATION")
        .ok()
        .map_or(u64::MAX, |x| x.parse::<u64>().unwrap())
});

#[must_use]
pub fn duration() -> u64 {
    *DURATION
}

static STEP: LazyLock<AtomicU32> = LazyLock::new(|| AtomicU32::new(1));

pub fn reset_step() {
    STEP.store(1, std::sync::atomic::Ordering::SeqCst);
}

pub fn current_step() -> u32 {
    STEP.load(std::sync::atomic::Ordering::SeqCst)
}

pub fn step_next() -> u32 {
    STEP.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}

static SIMULATOR_CANCELLATION_TOKEN: LazyLock<RwLock<CancellationToken>> =
    LazyLock::new(|| RwLock::new(CancellationToken::new()));

/// # Panics
///
/// * If the `SIMULATOR_CANCELLATION_TOKEN` `RwLock` fails to write to
pub fn reset_simulator_cancellation_token() {
    *SIMULATOR_CANCELLATION_TOKEN.write().unwrap() = CancellationToken::new();
}

/// # Panics
///
/// * If the `SIMULATOR_CANCELLATION_TOKEN` `RwLock` fails to read from
pub fn simulator_cancellation_token() -> CancellationToken {
    SIMULATOR_CANCELLATION_TOKEN.read().unwrap().clone()
}

/// # Panics
///
/// * If the `SIMULATOR_CANCELLATION_TOKEN` `RwLock` fails to read from
pub fn cancel_simulation() {
    SIMULATOR_CANCELLATION_TOKEN.read().unwrap().cancel();
}
