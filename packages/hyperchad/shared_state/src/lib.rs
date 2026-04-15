#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod error;
pub mod fanout;
pub mod runtime;
pub mod storage;
pub mod traits;

pub use error::SharedStateError;
