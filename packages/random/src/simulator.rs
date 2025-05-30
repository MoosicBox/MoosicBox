use std::{
    cell::RefCell,
    sync::{Arc, LazyLock, Mutex, RwLock},
};

use rand::{Rng, RngCore, SeedableRng, rngs::SmallRng};

use crate::GenericRng;

pub struct SimulatorRng(Arc<Mutex<SmallRng>>);

static INITIAL_SEED: LazyLock<u64> = LazyLock::new(|| {
    std::env::var("SIMULATOR_SEED").ok().map_or_else(
        || SmallRng::from_entropy().next_u64(),
        |x| x.parse::<u64>().unwrap(),
    )
});

static INITIAL_RNG: LazyLock<Mutex<SmallRng>> =
    LazyLock::new(|| Mutex::new(SmallRng::seed_from_u64(*INITIAL_SEED)));

#[must_use]
pub fn initial_seed() -> u64 {
    *INITIAL_SEED
}

thread_local! {
    static SEED: RefCell<RwLock<u64>> = RefCell::new(RwLock::new(*INITIAL_SEED));

    static RNG: RefCell<crate::Rng> = RefCell::new(crate::Rng::new());
}

#[must_use]
pub fn rng() -> crate::Rng {
    RNG.with_borrow(Clone::clone)
}

/// # Panics
///
/// * If fails to get a random `u64`
#[must_use]
pub fn gen_seed() -> u64 {
    INITIAL_RNG.lock().unwrap().next_u64()
}

#[must_use]
pub fn contains_fixed_seed() -> bool {
    std::env::var("SIMULATOR_SEED").is_ok()
}

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

/// # Panics
///
/// * If the `SEED` `RwLock` fails to read from
#[must_use]
pub fn seed() -> u64 {
    SEED.with_borrow(|x| *x.read().unwrap())
}

/// # Panics
///
/// * If the `RNG` `Mutex` fails to lock
pub fn reset_rng() {
    let seed = seed();
    log::debug!("reset_rng to seed={seed}");
    RNG.with_borrow_mut(|x| *x.0.lock().unwrap().0.lock().unwrap() = SmallRng::seed_from_u64(seed));
}

impl SimulatorRng {
    pub fn new<T: Into<u64>, S: Into<Option<T>>>(seed: S) -> Self {
        let seed = seed.into().map(Into::into);
        Self(Arc::new(Mutex::new(SmallRng::seed_from_u64(
            seed.unwrap_or_else(crate::simulator::seed),
        ))))
    }
}

impl GenericRng for SimulatorRng {
    fn next_u32(&self) -> u32 {
        self.0.lock().unwrap().next_u32()
    }

    fn next_i32(&self) -> i32 {
        self.0.lock().unwrap().gen_range(i32::MIN..=i32::MAX)
    }

    fn next_u64(&self) -> u64 {
        self.0.lock().unwrap().next_u64()
    }

    fn fill_bytes(&self, dest: &mut [u8]) {
        self.0.lock().unwrap().fill_bytes(dest);
    }

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
