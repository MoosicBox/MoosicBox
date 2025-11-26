//! Standard random number generation backend using `rand::rngs::SmallRng`.
//!
//! This module provides the standard random number generator implementation for the crate,
//! using the `rand` crate's `SmallRng` for good performance with general-purpose randomness.
//!
//! # Examples
//!
//! ```rust
//! # #[cfg(feature = "rand")]
//! # {
//! use switchy_random::rand::rng;
//!
//! let random_gen = rng();
//! let value = random_gen.next_u32();
//! # }
//! ```

use std::sync::{Arc, Mutex};

use rand::{Rng as _, RngCore, SeedableRng, rngs::SmallRng};

/// Re-export of the `rand` crate for access to distribution types and traits.
///
/// This allows users to access `rand`'s types like `Distribution`, `Uniform`, etc.
/// without needing to add `rand` as a separate dependency.
pub use rand;

use crate::{GenericRng, Rng};

/// The global random number generator instance.
///
/// This static provides a shared RNG that can be used across the application.
pub static RNG: std::sync::LazyLock<Rng> = std::sync::LazyLock::new(Rng::new);

/// Returns a clone of the global random number generator.
#[must_use]
pub fn rng() -> crate::Rng {
    RNG.clone()
}

/// The underlying random number generator implementation using `rand::rngs::SmallRng`.
pub struct RandRng(Arc<Mutex<SmallRng>>);

impl RandRng {
    /// Creates a new random number generator from an optional seed.
    ///
    /// If `None` is provided, the RNG is seeded from entropy.
    #[must_use]
    pub fn new<T: Into<u64>, S: Into<Option<T>>>(seed: S) -> Self {
        Self(Arc::new(Mutex::new(
            seed.into()
                .map(Into::into)
                .map_or_else(SmallRng::from_entropy, SmallRng::seed_from_u64),
        )))
    }
}

impl GenericRng for RandRng {
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

impl ::rand::RngCore for RandRng {
    fn next_u32(&mut self) -> u32 {
        self.0.lock().unwrap().next_u32()
    }

    fn next_u64(&mut self) -> u64 {
        self.0.lock().unwrap().next_u64()
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.0.lock().unwrap().fill_bytes(dest);
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), ::rand::Error> {
        self.0.lock().unwrap().try_fill_bytes(dest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_rand_rng_seeded_reproducibility() {
        let rng1 = RandRng::new(12345_u64);
        let rng2 = RandRng::new(12345_u64);

        let values1: Vec<u32> = (0..10).map(|_| rng1.next_u32()).collect();
        let values2: Vec<u32> = (0..10).map(|_| rng2.next_u32()).collect();

        assert_eq!(
            values1, values2,
            "Same seed should produce same sequence in rand backend"
        );
    }

    #[test_log::test]
    fn test_rand_rng_different_seeds_produce_different_values() {
        let rng1 = RandRng::new(12345_u64);
        let rng2 = RandRng::new(54321_u64);

        let value1 = rng1.next_u32();
        let value2 = rng2.next_u32();

        assert_ne!(
            value1, value2,
            "Different seeds should produce different values"
        );
    }

    #[test_log::test]
    fn test_rand_rng_with_none_seed_from_entropy() {
        let rng = RandRng::new::<u64, Option<u64>>(None);

        // Should be able to generate values without panicking
        let _value = rng.next_u32();
        let _value = rng.next_u64();
    }

    #[test_log::test]
    fn test_global_rng_function() {
        let rng1 = rng();
        let rng2 = rng();

        // Both should work and share state
        let val1 = rng1.next_u32();
        let val2 = rng2.next_u32();

        // They share state, so values should be different (state advanced)
        assert_ne!(val1, val2, "Global RNGs share state");
    }

    #[test_log::test]
    fn test_rand_rng_next_i32_produces_valid_range() {
        let rng = RandRng::new(42_u64);

        // This test simply verifies that next_i32() executes without panicking
        // The range check is redundant since i32 always contains all i32 values
        for _ in 0..100 {
            let _value = rng.next_i32();
            // Any i32 value is valid
        }
    }

    #[test_log::test]
    fn test_rand_rng_fill_bytes() {
        let rng = RandRng::new(42_u64);
        let mut buffer = [0_u8; 32];

        rng.fill_bytes(&mut buffer);

        // Verify that not all bytes are zero (extremely unlikely with proper RNG)
        assert!(
            buffer.iter().any(|&x| x != 0),
            "Fill should produce non-zero bytes"
        );
    }

    #[test_log::test]
    fn test_rand_rng_try_fill_bytes_success() {
        let rng = RandRng::new(42_u64);
        let mut buffer = [0_u8; 32];

        let result = rng.try_fill_bytes(&mut buffer);
        assert!(result.is_ok(), "try_fill_bytes should succeed");

        // Verify that not all bytes are zero
        assert!(
            buffer.iter().any(|&x| x != 0),
            "Fill should produce non-zero bytes"
        );
    }

    #[test_log::test]
    fn test_rand_rng_core_trait_implementation() {
        let rng = RandRng::new(42_u64);

        // Test RngCore trait methods through GenericRng
        let _u32_val = rng.next_u32();
        let _u64_val = rng.next_u64();

        let mut bytes = [0_u8; 16];
        rng.fill_bytes(&mut bytes);
        assert!(bytes.iter().any(|&x| x != 0));

        let mut bytes2 = [0_u8; 16];
        let result = rng.try_fill_bytes(&mut bytes2);
        assert!(result.is_ok());
        assert!(bytes2.iter().any(|&x| x != 0));
    }

    #[test_log::test]
    fn test_global_rng_static_initialization() {
        // Access the global RNG multiple times
        let _rng1 = &*RNG;
        let _rng2 = &*RNG;

        // Should not panic and should be accessible
        let _val = RNG.next_u32();
        // Test passes if no panic occurs
    }

    #[test_log::test]
    fn test_rand_rng_mutable_rng_core_interface() {
        use ::rand::RngCore;

        let mut rng = RandRng::new(42_u64);

        // Test the mutable RngCore trait interface (lines 95-111 in rand.rs)
        let val1 = RngCore::next_u32(&mut rng);
        let val2 = RngCore::next_u64(&mut rng);
        assert!(val1 > 0 || val2 > 0, "Should produce values");

        let mut buffer = [0_u8; 16];
        RngCore::fill_bytes(&mut rng, &mut buffer);
        assert!(
            buffer.iter().any(|&x| x != 0),
            "Should fill with non-zero bytes"
        );

        let mut buffer2 = [0_u8; 16];
        let result = RngCore::try_fill_bytes(&mut rng, &mut buffer2);
        assert!(result.is_ok(), "try_fill_bytes should succeed");
        assert!(
            buffer2.iter().any(|&x| x != 0),
            "Should fill with non-zero bytes"
        );
    }

    #[test_log::test]
    fn test_rand_rng_next_u64_produces_different_values() {
        let rng = RandRng::new(42_u64);

        let val1 = rng.next_u64();
        let val2 = rng.next_u64();
        let val3 = rng.next_u64();

        // At least two of the three values should be different
        assert!(
            val1 != val2 || val2 != val3,
            "next_u64 should produce varying values"
        );
    }
}
