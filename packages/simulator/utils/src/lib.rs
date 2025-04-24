#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::{LazyLock, atomic::AtomicU32};

use tokio_util::sync::CancellationToken;

pub static SEED: LazyLock<u64> = LazyLock::new(|| {
    let seed = getrandom::u64().unwrap();
    std::env::var("SIMULATOR_SEED")
        .ok()
        .map_or(seed, |x| x.parse::<u64>().unwrap())
});

#[must_use]
pub fn seed() -> u64 {
    *SEED
}

pub static DURATION: LazyLock<u64> = LazyLock::new(|| {
    std::env::var("SIMULATOR_DURATION")
        .ok()
        .map_or(u64::MAX, |x| x.parse::<u64>().unwrap())
});

#[must_use]
pub fn duration() -> u64 {
    *DURATION
}

pub static STEP: LazyLock<AtomicU32> = LazyLock::new(|| AtomicU32::new(0));

pub fn step() -> u32 {
    STEP.load(std::sync::atomic::Ordering::SeqCst)
}

pub static SIMULATOR_CANCELLATION_TOKEN: LazyLock<CancellationToken> =
    LazyLock::new(CancellationToken::new);

pub fn simulator_cancellation_token() -> CancellationToken {
    SIMULATOR_CANCELLATION_TOKEN.clone()
}

pub fn cancel_simulation() {
    SIMULATOR_CANCELLATION_TOKEN.cancel();
}
