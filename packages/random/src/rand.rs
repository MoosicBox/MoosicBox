use std::sync::{Arc, Mutex};

use rand::{RngCore, SeedableRng, rngs::SmallRng};

use crate::GenericRng;

pub struct RandRng(Arc<Mutex<SmallRng>>);

impl RandRng {
    pub fn new<T: Into<u64>, S: Into<Option<T>>>(seed: S) -> Self {
        Self(Arc::new(Mutex::new(
            seed.into()
                .map(Into::into)
                .map_or_else(SmallRng::from_entropy, SmallRng::seed_from_u64),
        )))
    }
}

impl GenericRng for RandRng {
    fn next_u64(&self) -> u64 {
        self.0.lock().unwrap().next_u64()
    }
}
