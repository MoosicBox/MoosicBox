//! Thread-safe random number generation with pluggable backends.
//!
//! This crate provides a unified interface for random number generation with two backend
//! implementations:
//!
//! * **`rand`** - Standard random number generation using `rand::rngs::SmallRng`
//! * **`simulator`** - Deterministic random number generation for reproducible simulations
//!
//! # Features
//!
//! * Thread-safe RNG implementations that can be shared across threads
//! * Consistent API regardless of backend
//! * Support for seeded and entropy-based initialization
//! * Non-uniform distribution helpers for advanced use cases
//!
//! # Examples
//!
//! Basic random number generation:
//!
//! ```rust
//! # #[cfg(feature = "rand")]
//! # {
//! use switchy_random::Rng;
//!
//! let rng = Rng::new();
//! let random_value: u32 = rng.next_u32();
//! let random_range = rng.gen_range(1..=100);
//! # }
//! ```
//!
//! Using a seeded RNG for reproducible results:
//!
//! ```rust
//! # #[cfg(feature = "rand")]
//! # {
//! use switchy_random::Rng;
//!
//! let rng = Rng::from_seed(12345);
//! let value1 = rng.next_u32();
//!
//! let rng2 = Rng::from_seed(12345);
//! let value2 = rng2.next_u32();
//!
//! assert_eq!(value1, value2); // Same seed produces same values
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::{Arc, Mutex};

use ::rand::RngCore;

/// Standard random number generation backend.
///
/// This module provides random number generation using the `rand` crate's `SmallRng`.
/// It's suitable for general-purpose randomness with good performance.
#[cfg(feature = "rand")]
pub mod rand;

/// Deterministic simulation backend.
///
/// This module provides a deterministic random number generator suitable for simulations
/// that require reproducible random sequences. The initial seed can be configured via
/// the `SIMULATOR_SEED` environment variable.
#[cfg(feature = "simulator")]
pub mod simulator;

/// A thread-safe random number generator trait.
///
/// This trait extends `RngCore` with thread safety guarantees and additional methods
/// for generating random values.
pub trait GenericRng: Send + Sync + RngCore {
    /// Returns the next random `u32` value.
    fn next_u32(&self) -> u32;

    /// Returns the next random `i32` value.
    fn next_i32(&self) -> i32;

    /// Returns the next random `u64` value.
    fn next_u64(&self) -> u64;

    /// Fills the destination byte slice with random data.
    fn fill_bytes(&self, dest: &mut [u8]);

    /// Fills the destination byte slice with random data.
    ///
    /// # Errors
    ///
    /// * If the underlying random implementation fails to fill the bytes
    fn try_fill_bytes(&self, dest: &mut [u8]) -> Result<(), ::rand::Error>;
}

/// A thread-safe wrapper around a `GenericRng` implementation.
///
/// This wrapper provides interior mutability through `Arc<Mutex<R>>`, allowing
/// the RNG to be shared across threads and cloned.
pub struct RngWrapper<R: GenericRng>(Arc<Mutex<R>>);

impl<R: GenericRng> Clone for RngWrapper<R> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<R: GenericRng> RngCore for RngWrapper<R> {
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

impl<R: GenericRng> GenericRng for RngWrapper<R> {
    /// # Panics
    ///
    /// * If the internal mutex is poisoned
    #[inline]
    fn next_u32(&self) -> u32 {
        self.0.lock().unwrap().next_u32()
    }

    /// # Panics
    ///
    /// * If the internal mutex is poisoned
    #[inline]
    fn next_i32(&self) -> i32 {
        self.0.lock().unwrap().next_i32()
    }

    /// # Panics
    ///
    /// * If the internal mutex is poisoned
    #[inline]
    fn next_u64(&self) -> u64 {
        self.0.lock().unwrap().next_u64()
    }

    /// # Panics
    ///
    /// * If the internal mutex is poisoned
    #[inline]
    fn fill_bytes(&self, dest: &mut [u8]) {
        self.0.lock().unwrap().fill_bytes(dest);
    }

    /// # Panics
    ///
    /// * If the internal mutex is poisoned
    #[inline]
    fn try_fill_bytes(&self, dest: &mut [u8]) -> Result<(), ::rand::Error> {
        self.0.lock().unwrap().try_fill_bytes(dest)
    }
}

#[allow(unused)]
macro_rules! impl_rng {
    ($module:ident, $type:ty $(,)?) => {
        use ::rand::distributions::Distribution as _;

        pub use $module::rng;

        /// The primary random number generator type for this crate.
        ///
        /// This is a thread-safe wrapper around the underlying RNG implementation.
        pub type Rng = RngWrapper<$type>;

        impl Default for Rng {
            fn default() -> Self {
                Self::new()
            }
        }

        impl Rng {
            /// Creates a new random number generator with a random seed.
            #[must_use]
            pub fn new() -> Self {
                Self::from_seed(None)
            }

            /// Creates a new random number generator from an optional seed.
            ///
            /// If `None` is provided, a random seed will be used.
            #[must_use]
            pub fn from_seed<S: Into<Option<u64>>>(seed: S) -> Self {
                Self(Arc::new(Mutex::new(<$type>::new(seed))))
            }

            /// Returns the next random `u32` value.
            #[inline]
            #[must_use]
            pub fn next_u32(&self) -> u32 {
                <Self as GenericRng>::next_u32(self)
            }

            /// Returns the next random `i32` value.
            #[inline]
            #[must_use]
            pub fn next_i32(&self) -> i32 {
                <Self as GenericRng>::next_i32(self)
            }

            /// Returns the next random `u64` value.
            #[inline]
            #[must_use]
            pub fn next_u64(&self) -> u64 {
                <Self as GenericRng>::next_u64(self)
            }
        }

        impl Rng {
            /// Generates a random value of type `T`.
            ///
            /// The type must implement the `Standard` distribution.
            ///
            /// # Panics
            ///
            /// * If the internal mutex is poisoned
            #[must_use]
            pub fn random<T>(&self) -> T
            where
                ::rand::distributions::Standard: ::rand::prelude::Distribution<T>,
            {
                ::rand::distributions::Standard.sample(&mut *self.0.lock().unwrap())
            }

            /// Generates a random value within the specified range.
            ///
            /// # Panics
            ///
            /// * If the range is empty
            /// * If the internal mutex is poisoned
            #[must_use]
            pub fn gen_range<T, R>(&self, range: R) -> T
            where
                T: ::rand::distributions::uniform::SampleUniform,
                R: ::rand::distributions::uniform::SampleRange<T>,
            {
                assert!(!range.is_empty(), "cannot sample empty range");
                range.sample_single(&mut *self.0.lock().unwrap())
            }

            /// Generates a random value within the specified range with a non-uniform distribution.
            ///
            /// The distribution is controlled by the `dist` parameter using a floating-point power.
            ///
            /// # Panics
            ///
            /// * If the range is empty
            /// * If the internal mutex is poisoned
            #[must_use]
            pub fn gen_range_dist<T, R>(&self, range: R, dist: f64) -> T
            where
                T: ::rand::distributions::uniform::SampleUniform,
                R: ::rand::distributions::uniform::SampleRange<T>,
                T: F64Convertible,
            {
                assert!(!range.is_empty(), "cannot sample empty range");
                let value = range.sample_single(&mut *self.0.lock().unwrap());
                let value = non_uniform_distribute_f64(value.into_f64(), dist, self);
                T::from_f64(value)
            }

            /// Generates a random value within the specified range with a non-uniform distribution.
            ///
            /// The distribution is controlled by the `dist` parameter using an integer power.
            ///
            /// # Panics
            ///
            /// * If the range is empty
            /// * If the internal mutex is poisoned
            #[must_use]
            pub fn gen_range_disti<T, R>(&self, range: R, dist: i32) -> T
            where
                T: ::rand::distributions::uniform::SampleUniform,
                R: ::rand::distributions::uniform::SampleRange<T>,
                T: F64Convertible,
            {
                assert!(!range.is_empty(), "cannot sample empty range");
                let value = range.sample_single(&mut *self.0.lock().unwrap());
                let value = non_uniform_distribute_i32(value.into_f64(), dist, self);
                T::from_f64(value)
            }

            /// Samples a value from the given distribution.
            ///
            /// # Panics
            ///
            /// * If the internal mutex is poisoned
            #[must_use]
            pub fn sample<T, D: ::rand::prelude::Distribution<T>>(&self, distr: D) -> T {
                distr.sample(&mut *self.0.lock().unwrap())
            }

            /// Fills the destination with random values.
            ///
            /// # Panics
            ///
            /// * If the underlying `Rng` implementation fails to fill
            pub fn fill<T: ::rand::Fill + ?Sized>(&self, dest: &mut T) {
                dest.try_fill(&mut *self.0.lock().unwrap())
                    .unwrap_or_else(|_| core::panic!("Rng::fill failed"))
            }

            /// Fills the destination with random values.
            ///
            /// # Errors
            ///
            /// * If the underlying `Rng` implementation fails to fill
            pub fn try_fill<T: ::rand::Fill + ?Sized>(
                &self,
                dest: &mut T,
            ) -> Result<(), ::rand::Error> {
                dest.try_fill(&mut *self.0.lock().unwrap())
            }

            /// Generates a boolean with the given probability of being `true`.
            ///
            /// # Panics
            ///
            /// * If `p` is not in the range `[0.0, 1.0]`
            /// * If the internal mutex is poisoned
            #[must_use]
            pub fn gen_bool(&self, p: f64) -> bool {
                let d = ::rand::distributions::Bernoulli::new(p).unwrap();
                self.sample(d)
            }

            /// Generates a boolean with probability `numerator / denominator` of being `true`.
            ///
            /// # Panics
            ///
            /// * If `numerator > denominator` or `denominator == 0`
            /// * If the internal mutex is poisoned
            #[must_use]
            pub fn gen_ratio(&self, numerator: u32, denominator: u32) -> bool {
                let d =
                    ::rand::distributions::Bernoulli::from_ratio(numerator, denominator).unwrap();
                self.sample(d)
            }
        }
    };
}

/// A trait for types that can be converted to and from `f64`.
///
/// This is used internally for non-uniform distribution functions.
pub trait F64Convertible: Sized {
    /// Converts from `f64` to `Self`.
    fn from_f64(f: f64) -> Self;

    /// Converts from `Self` to `f64`.
    fn into_f64(self) -> f64;
}

macro_rules! impl_f64_convertible {
    ($type:ty $(,)?) => {
        impl F64Convertible for $type {
            #[allow(clippy::cast_possible_truncation)]
            fn from_f64(f: f64) -> Self {
                f as Self
            }

            #[allow(clippy::cast_lossless)]
            fn into_f64(self) -> f64 {
                self as f64
            }
        }
    };
}

macro_rules! impl_f64_round_convertible {
    ($type:ty $(,)?) => {
        impl F64Convertible for $type {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            fn from_f64(f: f64) -> Self {
                f.round() as Self
            }

            #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
            fn into_f64(self) -> f64 {
                self as f64
            }
        }
    };
}

impl_f64_convertible!(f32);
impl_f64_convertible!(f64);

impl_f64_round_convertible!(u8);
impl_f64_round_convertible!(u16);
impl_f64_round_convertible!(u32);
impl_f64_round_convertible!(u64);
impl_f64_round_convertible!(u128);

impl_f64_round_convertible!(i8);
impl_f64_round_convertible!(i16);
impl_f64_round_convertible!(i32);
impl_f64_round_convertible!(i64);
impl_f64_round_convertible!(i128);

/// Applies a non-uniform distribution to a value using a floating-point power.
///
/// This function scales the input value by a random factor raised to the given power,
/// creating a non-uniform distribution that can favor lower or higher values.
#[must_use]
#[cfg(any(feature = "simulator", feature = "rand"))]
pub fn non_uniform_distribute_f64(value: f64, pow: f64, rng: &Rng) -> f64 {
    value * rng.gen_range(0.0001..1.0f64).powf(pow)
}

/// Applies a non-uniform distribution to a value using an integer power.
///
/// This function scales the input value by a random factor raised to the given integer power,
/// creating a non-uniform distribution that can favor lower or higher values.
#[must_use]
#[cfg(any(feature = "simulator", feature = "rand"))]
pub fn non_uniform_distribute_i32(value: f64, pow: i32, rng: &Rng) -> f64 {
    value * rng.gen_range(0.0001..1.0f64).powi(pow)
}

#[cfg(feature = "simulator")]
impl_rng!(simulator, simulator::SimulatorRng);

#[cfg(all(not(feature = "simulator"), feature = "rand"))]
impl_rng!(rand, rand::RandRng);

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_seeded_rng_reproducibility() {
        let rng1 = Rng::from_seed(12345_u64);
        let rng2 = Rng::from_seed(12345_u64);

        let values1: Vec<u32> = (0..10).map(|_| rng1.next_u32()).collect();
        let values2: Vec<u32> = (0..10).map(|_| rng2.next_u32()).collect();

        assert_eq!(values1, values2, "Same seed should produce same sequence");
    }

    #[test_log::test]
    fn test_seeded_rng_different_seeds_produce_different_values() {
        let rng1 = Rng::from_seed(12345_u64);
        let rng2 = Rng::from_seed(54321_u64);

        let value1 = rng1.next_u32();
        let value2 = rng2.next_u32();

        assert_ne!(
            value1, value2,
            "Different seeds should produce different values"
        );
    }

    #[test_log::test]
    fn test_gen_range_produces_values_in_range() {
        let rng = Rng::from_seed(42_u64);

        for _ in 0..100 {
            let value = rng.gen_range(1..=100);
            assert!(
                (1..=100).contains(&value),
                "Generated value {value} should be in range [1, 100]"
            );
        }
    }

    #[test_log::test]
    #[should_panic(expected = "cannot sample empty range")]
    fn test_gen_range_panics_on_empty_range() {
        let rng = Rng::from_seed(42_u64);
        let _value = rng.gen_range(1..1);
    }

    #[test_log::test]
    fn test_gen_range_dist_produces_scaled_values() {
        let rng = Rng::from_seed(42_u64);

        // gen_range_dist applies non-uniform distribution that can scale values down
        // So values may be outside the original range
        for _ in 0..100 {
            let value = rng.gen_range_dist(1..=100, 2.0);
            // Values should be >= 0 (scaled down from range)
            assert!(value >= 0, "Generated value {value} should be non-negative");
        }
    }

    #[test_log::test]
    #[should_panic(expected = "cannot sample empty range")]
    fn test_gen_range_dist_panics_on_empty_range() {
        let rng = Rng::from_seed(42_u64);
        let _value = rng.gen_range_dist(1..1, 2.0);
    }

    #[test_log::test]
    fn test_gen_range_disti_produces_scaled_values() {
        let rng = Rng::from_seed(42_u64);

        // gen_range_disti applies non-uniform distribution that can scale values down
        // So values may be outside the original range
        for _ in 0..100 {
            let value = rng.gen_range_disti(1..=100, 2);
            // Values should be >= 0 (scaled down from range)
            assert!(value >= 0, "Generated value {value} should be non-negative");
        }
    }

    #[test_log::test]
    #[should_panic(expected = "cannot sample empty range")]
    fn test_gen_range_disti_panics_on_empty_range() {
        let rng = Rng::from_seed(42_u64);
        let _value = rng.gen_range_disti(1..1, 2);
    }

    #[test_log::test]
    fn test_non_uniform_distribute_f64_scales_value() {
        let rng = Rng::from_seed(42_u64);
        let original_value = 100.0;

        let distributed = non_uniform_distribute_f64(original_value, 1.0, &rng);
        assert!(
            distributed <= original_value,
            "Distributed value should be <= original with pow >= 1"
        );
        assert!(distributed > 0.0, "Distributed value should be positive");
    }

    #[test_log::test]
    fn test_non_uniform_distribute_f64_with_different_powers() {
        let rng = Rng::from_seed(42_u64);
        let value = 100.0;

        let dist_pow1 = non_uniform_distribute_f64(value, 1.0, &rng);
        let dist_pow2 = non_uniform_distribute_f64(value, 2.0, &rng);

        // Both should be valid and within bounds
        assert!(dist_pow1 > 0.0 && dist_pow1 <= value);
        assert!(dist_pow2 > 0.0 && dist_pow2 <= value);
    }

    #[test_log::test]
    fn test_non_uniform_distribute_i32_scales_value() {
        let rng = Rng::from_seed(42_u64);
        let original_value = 100.0;

        let distributed = non_uniform_distribute_i32(original_value, 1, &rng);
        assert!(
            distributed <= original_value,
            "Distributed value should be <= original with pow >= 1"
        );
        assert!(distributed > 0.0, "Distributed value should be positive");
    }

    #[test_log::test]
    fn test_gen_bool_with_zero_probability() {
        let rng = Rng::from_seed(42_u64);

        let results: Vec<bool> = (0..100).map(|_| rng.gen_bool(0.0)).collect();
        assert!(
            results.iter().all(|&x| !x),
            "gen_bool(0.0) should always return false"
        );
    }

    #[test_log::test]
    fn test_gen_bool_with_one_probability() {
        let rng = Rng::from_seed(42_u64);

        let results: Vec<bool> = (0..100).map(|_| rng.gen_bool(1.0)).collect();
        assert!(
            results.iter().all(|&x| x),
            "gen_bool(1.0) should always return true"
        );
    }

    #[test_log::test]
    fn test_gen_bool_with_half_probability() {
        let rng = Rng::from_seed(42_u64);

        let results: Vec<bool> = (0..1000).map(|_| rng.gen_bool(0.5)).collect();
        let true_count = results.iter().filter(|&&x| x).count();

        // With 1000 samples and 0.5 probability, we expect roughly 500 trues
        // Allow for statistical variance (between 400 and 600)
        assert!(
            (400..=600).contains(&true_count),
            "gen_bool(0.5) should produce roughly 50% true values, got {true_count}/1000"
        );
    }

    #[test_log::test]
    #[should_panic(expected = "InvalidProbability")]
    fn test_gen_bool_panics_on_invalid_probability() {
        let rng = Rng::from_seed(42_u64);
        let _result = rng.gen_bool(1.5); // Invalid probability > 1.0
    }

    #[test_log::test]
    fn test_gen_ratio_zero_numerator() {
        let rng = Rng::from_seed(42_u64);

        let results: Vec<bool> = (0..100).map(|_| rng.gen_ratio(0, 10)).collect();
        assert!(
            results.iter().all(|&x| !x),
            "gen_ratio(0, 10) should always return false"
        );
    }

    #[test_log::test]
    fn test_gen_ratio_equal_numerator_denominator() {
        let rng = Rng::from_seed(42_u64);

        let results: Vec<bool> = (0..100).map(|_| rng.gen_ratio(10, 10)).collect();
        assert!(
            results.iter().all(|&x| x),
            "gen_ratio(10, 10) should always return true"
        );
    }

    #[test_log::test]
    #[should_panic(expected = "InvalidProbability")]
    fn test_gen_ratio_panics_on_numerator_greater_than_denominator() {
        let rng = Rng::from_seed(42_u64);
        let _result = rng.gen_ratio(11, 10);
    }

    #[test_log::test]
    #[should_panic(expected = "InvalidProbability")]
    fn test_gen_ratio_panics_on_zero_denominator() {
        let rng = Rng::from_seed(42_u64);
        let _result = rng.gen_ratio(1, 0);
    }

    #[test_log::test]
    fn test_fill_bytes() {
        let rng = Rng::from_seed(42_u64);
        let mut buffer = [0_u8; 32];

        rng.fill_bytes(&mut buffer);

        // Verify that not all bytes are zero (extremely unlikely with proper RNG)
        assert!(
            buffer.iter().any(|&x| x != 0),
            "Fill should produce non-zero bytes"
        );
    }

    #[test_log::test]
    fn test_try_fill_bytes_success() {
        let rng = Rng::from_seed(42_u64);
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
    fn test_fill_array() {
        let rng = Rng::from_seed(42_u64);
        let mut array = [0_u32; 10];

        rng.fill(&mut array[..]);

        // Verify that not all elements are zero
        assert!(
            array.iter().any(|&x| x != 0),
            "Fill should produce non-zero values"
        );
    }

    #[test_log::test]
    fn test_try_fill_array_success() {
        let rng = Rng::from_seed(42_u64);
        let mut array = [0_u32; 10];

        let result = rng.try_fill(&mut array[..]);
        assert!(result.is_ok(), "try_fill should succeed");

        // Verify that not all elements are zero
        assert!(
            array.iter().any(|&x| x != 0),
            "Fill should produce non-zero values"
        );
    }

    #[test_log::test]
    fn test_random_generates_different_types() {
        let rng = Rng::from_seed(42_u64);

        let _u8_val: u8 = rng.random();
        let _u16_val: u16 = rng.random();
        let _u32_val: u32 = rng.random();
        let _u64_val: u64 = rng.random();
        let _i8_val: i8 = rng.random();
        let _i16_val: i16 = rng.random();
        let _i32_val: i32 = rng.random();
        let _i64_val: i64 = rng.random();
        let _f32_val: f32 = rng.random();
        let _f64_val: f64 = rng.random();

        // Test passes if no panics occur
    }

    #[test_log::test]
    fn test_rng_clone_shares_state() {
        let rng1 = Rng::from_seed(42_u64);
        let rng2 = rng1.clone();

        // Both should advance the same internal state
        let val1 = rng1.next_u32();
        let val2 = rng2.next_u32();

        // They should not be equal because they share state and val1 advanced it
        assert_ne!(val1, val2, "Cloned RNGs share the same state");
    }

    #[test_log::test]
    #[allow(clippy::float_cmp)]
    fn test_f64_convertible_roundtrip_f64() {
        let original = 42.5_f64;
        let converted = original.into_f64();
        let back = f64::from_f64(converted);
        assert_eq!(original, back, "f64 conversion should be lossless");
    }

    #[test_log::test]
    #[allow(clippy::float_cmp)]
    fn test_f64_convertible_roundtrip_f32() {
        let original = 42.5_f32;
        let converted = original.into_f64();
        let back = f32::from_f64(converted);
        assert_eq!(original, back, "f32 conversion should be lossless");
    }

    #[test_log::test]
    fn test_f64_convertible_integer_rounding() {
        let value = 42.7_f64;
        let as_u32 = u32::from_f64(value);
        assert_eq!(as_u32, 43, "Should round 42.7 to 43");

        let value = 42.3_f64;
        let as_u32 = u32::from_f64(value);
        assert_eq!(as_u32, 42, "Should round 42.3 to 42");
    }

    #[test_log::test]
    fn test_next_i32_range() {
        let rng = Rng::from_seed(42_u64);

        // This test simply verifies that next_i32() executes without panicking
        // The range check is redundant since i32 always contains all i32 values
        for _ in 0..100 {
            let _value = rng.next_i32();
            // Any i32 value is valid
        }
    }
}
