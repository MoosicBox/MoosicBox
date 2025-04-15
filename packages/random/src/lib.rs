#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "rand")]
pub mod rand;

#[cfg(feature = "simulator")]
pub mod simulator;

pub trait GenericRng: Send + Sync {
    fn next_u64(&self) -> u64;
}

pub struct RngWrapper<R: GenericRng>(R);

impl<R: GenericRng> GenericRng for RngWrapper<R> {
    #[inline]
    fn next_u64(&self) -> u64 {
        self.0.next_u64()
    }
}

#[allow(unused)]
macro_rules! impl_rng {
    ($type:ty $(,)?) => {
        pub type Rng = RngWrapper<$type>;

        impl Default for Rng {
            fn default() -> Self {
                Self::new()
            }
        }

        impl Rng {
            #[must_use]
            pub fn new() -> Self {
                Self::from_seed(None)
            }

            pub fn from_seed<S: Into<Option<u64>>>(seed: S) -> Self {
                Self(<$type>::new(seed))
            }

            #[inline]
            #[must_use]
            pub fn next_u64(&self) -> u64 {
                <Self as GenericRng>::next_u64(self)
            }
        }
    };
}

#[cfg(feature = "simulator")]
impl_rng!(simulator::SimulatorRng);

#[cfg(all(not(feature = "simulator"), feature = "rand"))]
impl_rng!(rand::RandRng);
