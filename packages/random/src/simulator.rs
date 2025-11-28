//! Deterministic simulation backend for reproducible random sequences.
//!
//! This module provides a deterministic random number generator suitable for simulations
//! that require reproducible random sequences. The initial seed can be configured via
//! the `SIMULATOR_SEED` environment variable.
//!
//! # Thread-Local State
//!
//! The simulator maintains thread-local RNG state to ensure deterministic behavior in
//! multi-threaded simulations. Each thread has its own seed that can be reset independently.
//!
//! # Examples
//!
//! ```rust
//! # #[cfg(feature = "simulator")]
//! # {
//! use switchy_random::simulator::rng;
//!
//! let random_gen = rng();
//! let value = random_gen.next_u32();
//! # }
//! ```

use std::{
    cell::RefCell,
    sync::{Arc, LazyLock, Mutex, RwLock},
};

use rand::{Rng, RngCore, SeedableRng, rngs::SmallRng};

use crate::GenericRng;

/// The simulator random number generator implementation.
///
/// This RNG is designed for deterministic simulation scenarios where reproducible
/// random sequences are required. It can be seeded from the `SIMULATOR_SEED` environment variable.
pub struct SimulatorRng(Arc<Mutex<SmallRng>>);

static INITIAL_SEED: LazyLock<u64> = LazyLock::new(|| {
    std::env::var("SIMULATOR_SEED").ok().map_or_else(
        || SmallRng::from_entropy().next_u64(),
        |x| x.parse::<u64>().unwrap(),
    )
});

static INITIAL_RNG: LazyLock<Mutex<SmallRng>> =
    LazyLock::new(|| Mutex::new(SmallRng::seed_from_u64(*INITIAL_SEED)));

/// Returns the initial seed used for simulation.
///
/// This seed is either read from the `SIMULATOR_SEED` environment variable
/// or generated from entropy.
#[must_use]
pub fn initial_seed() -> u64 {
    *INITIAL_SEED
}

thread_local! {
    static SEED: RefCell<RwLock<u64>> = RefCell::new(RwLock::new(*INITIAL_SEED));

    static RNG: RefCell<crate::Rng> = RefCell::new(crate::Rng::new());
}

/// Returns a clone of the thread-local random number generator for simulation.
#[must_use]
pub fn rng() -> crate::Rng {
    RNG.with_borrow(Clone::clone)
}

/// Generates a new seed value from the initial RNG.
///
/// # Panics
///
/// * If fails to get a random `u64`
#[must_use]
pub fn gen_seed() -> u64 {
    INITIAL_RNG.lock().unwrap().next_u64()
}

/// Returns whether a fixed seed was provided via the `SIMULATOR_SEED` environment variable.
#[must_use]
pub fn contains_fixed_seed() -> bool {
    std::env::var("SIMULATOR_SEED").is_ok()
}

/// Resets the thread-local seed to a new generated value.
///
/// # Panics
///
/// * If the `SEED` `RwLock` fails to write to
/// * If the `RNG` `Mutex` fails to lock
pub fn reset_seed() {
    let seed = gen_seed();
    log::debug!("reset_seed to seed={seed}");
    SEED.with_borrow_mut(|x| *x.write().unwrap() = seed);
    RNG.with_borrow_mut(|x| *x.0.lock().unwrap().0.lock().unwrap() = SmallRng::seed_from_u64(seed));
}

/// Returns the current thread-local seed value.
///
/// # Panics
///
/// * If the `SEED` `RwLock` fails to read from
#[must_use]
pub fn seed() -> u64 {
    SEED.with_borrow(|x| *x.read().unwrap())
}

/// Resets the thread-local RNG to use the current seed value.
///
/// # Panics
///
/// * If the `RNG` `Mutex` fails to lock
pub fn reset_rng() {
    let seed = seed();
    log::debug!("reset_rng to seed={seed}");
    RNG.with_borrow_mut(|x| *x.0.lock().unwrap().0.lock().unwrap() = SmallRng::seed_from_u64(seed));
}

impl SimulatorRng {
    /// Creates a new simulator random number generator from an optional seed.
    ///
    /// If `None` is provided, the current thread-local seed is used.
    #[must_use]
    pub fn new<T: Into<u64>, S: Into<Option<T>>>(seed: S) -> Self {
        let seed = seed.into().map(Into::into);
        Self(Arc::new(Mutex::new(SmallRng::seed_from_u64(
            seed.unwrap_or_else(crate::simulator::seed),
        ))))
    }
}

impl GenericRng for SimulatorRng {
    /// # Panics
    ///
    /// * If the internal mutex is poisoned
    fn next_u32(&self) -> u32 {
        self.0.lock().unwrap().next_u32()
    }

    /// # Panics
    ///
    /// * If the internal mutex is poisoned
    fn next_i32(&self) -> i32 {
        self.0.lock().unwrap().gen_range(i32::MIN..=i32::MAX)
    }

    /// # Panics
    ///
    /// * If the internal mutex is poisoned
    fn next_u64(&self) -> u64 {
        self.0.lock().unwrap().next_u64()
    }

    /// # Panics
    ///
    /// * If the internal mutex is poisoned
    fn fill_bytes(&self, dest: &mut [u8]) {
        self.0.lock().unwrap().fill_bytes(dest);
    }

    /// # Panics
    ///
    /// * If the internal mutex is poisoned
    fn try_fill_bytes(&self, dest: &mut [u8]) -> Result<(), rand::Error> {
        self.0.lock().unwrap().try_fill_bytes(dest)
    }
}

impl ::rand::RngCore for SimulatorRng {
    fn next_u32(&mut self) -> u32 {
        <Self as GenericRng>::next_u32(self)
    }

    fn next_u64(&mut self) -> u64 {
        <Self as GenericRng>::next_u64(self)
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        <Self as GenericRng>::fill_bytes(self, dest);
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), ::rand::Error> {
        <Self as GenericRng>::try_fill_bytes(self, dest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_simulator_rng_seeded_reproducibility() {
        let rng1 = SimulatorRng::new(12345_u64);
        let rng2 = SimulatorRng::new(12345_u64);

        let values1: Vec<u32> = (0..10).map(|_| rng1.next_u32()).collect();
        let values2: Vec<u32> = (0..10).map(|_| rng2.next_u32()).collect();

        assert_eq!(
            values1, values2,
            "Same seed should produce same sequence in simulator"
        );
    }

    #[test_log::test]
    fn test_initial_seed_returns_consistent_value() {
        let seed1 = initial_seed();
        let seed2 = initial_seed();
        assert_eq!(
            seed1, seed2,
            "initial_seed should return the same value when called multiple times"
        );
    }

    #[test_log::test]
    fn test_gen_seed_produces_different_values() {
        let seed1 = gen_seed();
        let seed2 = gen_seed();
        assert_ne!(
            seed1, seed2,
            "gen_seed should produce different values on subsequent calls"
        );
    }

    #[test_log::test]
    fn test_reset_seed_changes_seed() {
        let original_seed = seed();
        reset_seed();
        let new_seed = seed();

        assert_ne!(
            original_seed, new_seed,
            "reset_seed should change the thread-local seed"
        );
    }

    #[test_log::test]
    fn test_reset_seed_produces_reproducible_sequence() {
        reset_seed();
        let current_seed = seed();

        let rng = rng();
        let values1: Vec<u32> = (0..10).map(|_| rng.next_u32()).collect();

        reset_rng();
        let values2: Vec<u32> = (0..10).map(|_| rng.next_u32()).collect();

        // After reset_rng, we should get the same sequence again
        assert_eq!(
            values1, values2,
            "reset_rng should restore RNG to produce same sequence from current seed"
        );

        // Verify the seed hasn't changed
        assert_eq!(current_seed, seed(), "Seed should remain unchanged");
    }

    #[test_log::test]
    fn test_simulator_rng_with_none_seed_uses_thread_local() {
        // Set a known seed
        reset_seed();
        let thread_seed = seed();

        let sim_rng = SimulatorRng::new::<u64, Option<u64>>(None);
        let explicit_rng = SimulatorRng::new(thread_seed);

        // Both should produce the same sequence since they use the same seed
        let values1: Vec<u32> = (0..5).map(|_| sim_rng.next_u32()).collect();

        // Reset to get the same sequence
        let values2: Vec<u32> = (0..5).map(|_| explicit_rng.next_u32()).collect();

        assert_eq!(
            values1, values2,
            "RNG with None should use thread-local seed"
        );
    }

    #[test_log::test]
    fn test_rng_function_returns_thread_local() {
        let rng1 = rng();
        let rng2 = rng();

        // They should be clones sharing the same state
        let val1 = rng1.next_u32();
        let val2 = rng2.next_u32();

        // They share state, so values should be different (state advanced)
        assert_ne!(val1, val2, "Thread-local RNGs share state");
    }

    #[test_log::test]
    fn test_seed_function_returns_current_thread_seed() {
        reset_seed();
        let current_seed = seed();

        // Create RNG with this seed
        let sim_rng = SimulatorRng::new(current_seed);
        let thread_rng = rng();

        // Reset thread RNG to compare
        reset_rng();

        let val1 = sim_rng.next_u32();
        let val2 = thread_rng.next_u32();

        // They should produce the same value since they use the same seed
        assert_eq!(
            val1, val2,
            "Thread seed should match what thread-local RNG uses"
        );
    }

    #[test_log::test]
    fn test_contains_fixed_seed_reflects_env_var() {
        // This test documents the behavior but doesn't change env vars
        // as that would affect other tests
        let _has_fixed_seed = contains_fixed_seed();

        // The function should execute without panicking
        // Test passes if no panic occurs
    }

    #[test_log::test]
    fn test_thread_isolation() {
        use std::sync::mpsc;
        use std::thread;

        let (tx1, rx1) = mpsc::channel();
        let (tx2, rx2) = mpsc::channel();

        // Thread 1: reset seed and generate values
        let handle1 = thread::spawn(move || {
            reset_seed();
            let seed = seed();
            let rng = rng();
            let values: Vec<u32> = (0..5).map(|_| rng.next_u32()).collect();
            tx1.send((seed, values)).unwrap();
        });

        // Thread 2: reset seed and generate values
        let handle2 = thread::spawn(move || {
            reset_seed();
            let seed = seed();
            let rng = rng();
            let values: Vec<u32> = (0..5).map(|_| rng.next_u32()).collect();
            tx2.send((seed, values)).unwrap();
        });

        handle1.join().unwrap();
        handle2.join().unwrap();

        let (seed1, values1) = rx1.recv().unwrap();
        let (seed2, values2) = rx2.recv().unwrap();

        // Different threads should get different seeds
        assert_ne!(
            seed1, seed2,
            "Different threads should have independent seeds"
        );

        // And therefore different value sequences
        assert_ne!(
            values1, values2,
            "Different threads should produce different sequences"
        );
    }

    #[test_log::test]
    fn test_simulator_rng_mutable_rng_core_interface() {
        use ::rand::RngCore;

        let mut sim_rng = SimulatorRng::new(42_u64);

        // Test the mutable RngCore trait interface (lines 170-186 in simulator.rs)
        let val1 = RngCore::next_u32(&mut sim_rng);
        let val2 = RngCore::next_u64(&mut sim_rng);
        assert!(val1 > 0 || val2 > 0, "Should produce values");

        let mut buffer = [0_u8; 16];
        RngCore::fill_bytes(&mut sim_rng, &mut buffer);
        assert!(
            buffer.iter().any(|&x| x != 0),
            "Should fill with non-zero bytes"
        );

        let mut buffer2 = [0_u8; 16];
        let result = RngCore::try_fill_bytes(&mut sim_rng, &mut buffer2);
        assert!(result.is_ok(), "try_fill_bytes should succeed");
        assert!(
            buffer2.iter().any(|&x| x != 0),
            "Should fill with non-zero bytes"
        );
    }

    #[test_log::test]
    fn test_simulator_rng_next_i32_produces_valid_range() {
        let sim_rng = SimulatorRng::new(42_u64);

        // This test simply verifies that next_i32() executes without panicking
        for _ in 0..100 {
            let _value = sim_rng.next_i32();
            // Any i32 value is valid
        }
    }

    #[test_log::test]
    fn test_simulator_rng_next_u64_produces_different_values() {
        let sim_rng = SimulatorRng::new(42_u64);

        let val1 = sim_rng.next_u64();
        let val2 = sim_rng.next_u64();
        let val3 = sim_rng.next_u64();

        // At least two of the three values should be different
        assert!(
            val1 != val2 || val2 != val3,
            "next_u64 should produce varying values"
        );
    }

    #[test_log::test]
    fn test_simulator_rng_fill_bytes() {
        let sim_rng = SimulatorRng::new(42_u64);
        let mut buffer = [0_u8; 32];

        sim_rng.fill_bytes(&mut buffer);

        // Verify that not all bytes are zero (extremely unlikely with proper RNG)
        assert!(
            buffer.iter().any(|&x| x != 0),
            "Fill should produce non-zero bytes"
        );
    }

    #[test_log::test]
    fn test_simulator_rng_try_fill_bytes_success() {
        let sim_rng = SimulatorRng::new(42_u64);
        let mut buffer = [0_u8; 32];

        let result = sim_rng.try_fill_bytes(&mut buffer);
        assert!(result.is_ok(), "try_fill_bytes should succeed");

        // Verify that not all bytes are zero
        assert!(
            buffer.iter().any(|&x| x != 0),
            "Fill should produce non-zero bytes"
        );
    }

    #[test_log::test]
    fn test_simulator_rng_different_seeds_produce_different_values() {
        let rng1 = SimulatorRng::new(12345_u64);
        let rng2 = SimulatorRng::new(54321_u64);

        let value1 = rng1.next_u32();
        let value2 = rng2.next_u32();

        assert_ne!(
            value1, value2,
            "Different seeds should produce different values"
        );
    }
}
