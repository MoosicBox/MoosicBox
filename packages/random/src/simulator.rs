use std::sync::{Arc, Mutex};

use moosicbox_simulator_utils::SEED;
use rand::{RngCore, SeedableRng, rngs::SmallRng};

use crate::GenericRng;

pub struct SimulatorRng(Arc<Mutex<SmallRng>>);

impl SimulatorRng {
    pub fn new<T: Into<u64>, S: Into<Option<T>>>(seed: S) -> Self {
        Self(Arc::new(Mutex::new(
            seed.into()
                .map(Into::into)
                .map_or_else(|| SmallRng::seed_from_u64(*SEED), SmallRng::seed_from_u64),
        )))
    }
}

impl GenericRng for SimulatorRng {
    fn next_u64(&self) -> u64 {
        self.0.lock().unwrap().next_u64()
    }
}

impl ::rand::RngCore for SimulatorRng {
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
