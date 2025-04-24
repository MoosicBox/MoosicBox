use std::{
    sync::LazyLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use moosicbox_random::non_uniform_distribute_i32;

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub static EPOCH_OFFSET: LazyLock<u64> = LazyLock::new(|| {
    non_uniform_distribute_i32!(moosicbox_random::RNG.gen_range(1..100_000_000_000u64), 10) as u64
});

pub static STEP_MULTIPLIER: LazyLock<u64> = LazyLock::new(|| {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    std::env::var("SIMULATOR_STEP_MULTIPLIER").ok().map_or_else(
        || {
            let rng = &moosicbox_random::RNG;
            let value = rng.gen_range(1..1_000_000);
            let value = non_uniform_distribute_i32!(value, 10) as u64;
            if value == 0 { 1 } else { value }
        },
        |x| x.parse::<u64>().unwrap(),
    )
});

/// # Panics
///
/// * If the simulated `UNIX_EPOCH` offset is larger than a `u64` can store
#[must_use]
pub fn now() -> SystemTime {
    let epoch_offset = *EPOCH_OFFSET;
    let step_multiplier = *STEP_MULTIPLIER;
    let step = u64::from(moosicbox_simulator_utils::step());
    let mult_step = step.checked_mul(step_multiplier).unwrap();
    let millis = epoch_offset.checked_add(mult_step).unwrap();
    log::debug!(
        "now: epoch_offset={epoch_offset} step={step} step_multiplier={step_multiplier} millis={millis}"
    );
    UNIX_EPOCH
        .checked_add(Duration::from_millis(millis))
        .unwrap()
}
