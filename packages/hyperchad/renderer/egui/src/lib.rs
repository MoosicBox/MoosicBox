#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

#[cfg(feature = "calc")]
pub mod calc;

#[cfg(feature = "calc")]
pub use calc::*;

#[cfg(feature = "retained")]
pub mod retained;

#[cfg(all(not(feature = "calc"), feature = "retained"))]
pub use retained::*;
