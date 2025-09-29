#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! # `MoosicBox` Opus Codec
//!
//! `RFC 6716` compliant Opus audio codec decoder for Symphonia.
//!
//! This crate is under development.

pub mod error;

pub use error::{Error, Result};
