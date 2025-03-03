#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

#[cfg(feature = "retained")]
pub mod retained;

#[cfg(feature = "retained")]
pub use retained::*;

#[cfg(feature = "immediate")]
pub mod immediate;

#[cfg(all(not(feature = "retained"), feature = "immediate"))]
pub use immediate::*;
