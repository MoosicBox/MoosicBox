#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

#[cfg(feature = "css")]
pub mod css;

#[cfg(feature = "xml")]
pub mod xml;

#[cfg(feature = "serde")]
pub mod serde;
