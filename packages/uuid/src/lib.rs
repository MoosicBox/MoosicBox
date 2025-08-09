#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "uuid")]
pub mod uuid;

#[cfg(feature = "simulator")]
pub mod simulator;

#[allow(unused)]
macro_rules! impl_uuid {
    ($module:ident $(,)?) => {
        pub use $module::{new_v4, new_v4_string};
    };
}

#[cfg(feature = "simulator")]
impl_uuid!(simulator);

#[cfg(all(not(feature = "simulator"), feature = "uuid"))]
impl_uuid!(uuid);
