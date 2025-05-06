#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "std")]
pub mod standard;

#[cfg(feature = "simulator")]
pub mod simulator;

#[allow(unused)]
macro_rules! impl_time {
    ($module:ident $(,)?) => {
        pub use $module::now;
    };
}

#[cfg(feature = "simulator")]
impl_time!(simulator);

#[cfg(all(not(feature = "simulator"), feature = "std"))]
impl_time!(standard);
