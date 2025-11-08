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
