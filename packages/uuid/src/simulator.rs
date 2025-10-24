//! Deterministic UUID generation for testing and simulation.
//!
//! This module provides UUID v4 generation using a seeded random number generator,
//! allowing for reproducible UUIDs in test and simulation environments.
//!
//! The seed can be configured via the `SIMULATOR_UUID_SEED` environment variable.
//! If not set, defaults to 12345.

use switchy_env::var_parse_or;
use switchy_random::{GenericRng, Rng};
use uuid::Uuid;

static RNG: std::sync::LazyLock<Rng> = std::sync::LazyLock::new(|| {
    let seed = var_parse_or("SIMULATOR_UUID_SEED", 12345u64);

    log::debug!("Using UUID seed: {seed}");
    Rng::from_seed(seed)
});

/// Generate a deterministic UUID v4 for simulation
#[must_use]
pub fn new_v4() -> Uuid {
    let mut bytes = [0u8; 16];
    RNG.fill_bytes(&mut bytes);

    // Set version (4) and variant bits according to RFC 4122
    bytes[6] = (bytes[6] & 0x0f) | 0x40; // Version 4
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // Variant 10

    Uuid::from_bytes(bytes)
}

/// Generate a deterministic UUID v4 as a string for simulation
#[must_use]
pub fn new_v4_string() -> String {
    new_v4().to_string()
}
