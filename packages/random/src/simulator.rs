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
