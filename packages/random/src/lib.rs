#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::{Arc, Mutex};

use ::rand::RngCore;

#[cfg(feature = "rand")]
pub mod rand;

#[cfg(feature = "simulator")]
pub mod simulator;

#[cfg(any(feature = "simulator", feature = "rand"))]
pub static RNG: std::sync::LazyLock<Rng> = std::sync::LazyLock::new(Rng::new);

pub trait GenericRng: Send + Sync + RngCore {
    fn next_u32(&self) -> u32;

    fn next_i32(&self) -> i32;

    fn next_u64(&self) -> u64;

    fn fill_bytes(&self, dest: &mut [u8]);

    /// # Errors
    ///
    /// * If the underlying random implementation fails to fill the bytes
    fn try_fill_bytes(&self, dest: &mut [u8]) -> Result<(), ::rand::Error>;
}

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
    #[inline]
    fn next_u32(&self) -> u32 {
        self.0.lock().unwrap().next_u32()
    }

    #[inline]
    fn next_i32(&self) -> i32 {
        self.0.lock().unwrap().next_i32()
    }

    #[inline]
    fn next_u64(&self) -> u64 {
        self.0.lock().unwrap().next_u64()
    }

    #[inline]
    fn fill_bytes(&self, dest: &mut [u8]) {
        self.0.lock().unwrap().fill_bytes(dest);
    }

    #[inline]
    fn try_fill_bytes(&self, dest: &mut [u8]) -> Result<(), ::rand::Error> {
        self.0.lock().unwrap().try_fill_bytes(dest)
    }
}

#[allow(unused)]
macro_rules! impl_rng {
    ($type:ty $(,)?) => {
        use ::rand::distributions::Distribution as _;

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
                Self(Arc::new(Mutex::new(<$type>::new(seed))))
            }

            #[inline]
            #[must_use]
            pub fn next_u32(&self) -> u32 {
                <Self as GenericRng>::next_u32(self)
            }

            #[inline]
            #[must_use]
            pub fn next_i32(&self) -> i32 {
                <Self as GenericRng>::next_i32(self)
            }

            #[inline]
            #[must_use]
            pub fn next_u64(&self) -> u64 {
                <Self as GenericRng>::next_u64(self)
            }
        }

        impl Rng {
            #[must_use]
            pub fn random<T>(&self) -> T
            where
                ::rand::distributions::Standard: ::rand::prelude::Distribution<T>,
            {
                ::rand::distributions::Standard.sample(&mut *self.0.lock().unwrap())
            }

            pub fn gen_range<T, R>(&self, range: R) -> T
            where
                T: ::rand::distributions::uniform::SampleUniform,
                R: ::rand::distributions::uniform::SampleRange<T>,
            {
                assert!(!range.is_empty(), "cannot sample empty range");
                range.sample_single(&mut *self.0.lock().unwrap())
            }

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

            pub fn sample<T, D: ::rand::prelude::Distribution<T>>(&self, distr: D) -> T {
                distr.sample(&mut *self.0.lock().unwrap())
            }

            pub fn fill<T: ::rand::Fill + ?Sized>(&self, dest: &mut T) {
                dest.try_fill(&mut *self.0.lock().unwrap())
                    .unwrap_or_else(|_| core::panic!("Rng::fill failed"))
            }

            /// # Errors
            ///
            /// * If the underlying `Rng` implementation fails to fill
            pub fn try_fill<T: ::rand::Fill + ?Sized>(
                &self,
                dest: &mut T,
            ) -> Result<(), ::rand::Error> {
                dest.try_fill(&mut *self.0.lock().unwrap())
            }

            #[must_use]
            pub fn gen_bool(&self, p: f64) -> bool {
                let d = ::rand::distributions::Bernoulli::new(p).unwrap();
                self.sample(d)
            }

            #[must_use]
            pub fn gen_ratio(&self, numerator: u32, denominator: u32) -> bool {
                let d =
                    ::rand::distributions::Bernoulli::from_ratio(numerator, denominator).unwrap();
                self.sample(d)
            }
        }
    };
}

pub trait F64Convertible: Sized {
    fn from_f64(f: f64) -> Self;
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

#[must_use]
#[cfg(any(feature = "simulator", feature = "rand"))]
pub fn non_uniform_distribute_f64(value: f64, pow: f64, rng: &Rng) -> f64 {
    value * rng.gen_range(0.0001..1.0f64).powf(pow)
}

#[must_use]
#[cfg(any(feature = "simulator", feature = "rand"))]
pub fn non_uniform_distribute_i32(value: f64, pow: i32, rng: &Rng) -> f64 {
    value * rng.gen_range(0.0001..1.0f64).powi(pow)
}

#[cfg(feature = "simulator")]
impl_rng!(simulator::SimulatorRng);

#[cfg(all(not(feature = "simulator"), feature = "rand"))]
impl_rng!(rand::RandRng);
