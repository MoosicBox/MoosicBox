//! JavaScript bundler utilities using esbuild or SWC.
//!
//! This crate provides functionality to bundle JavaScript and TypeScript files
//! using either esbuild or SWC as the underlying bundler.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(any(feature = "esbuild", feature = "swc"))]
pub(crate) mod bundler;

#[cfg(any(feature = "esbuild", feature = "swc"))]
pub use bundler::*;

#[cfg(feature = "esbuild")]
pub mod esbuild;

#[cfg(feature = "node")]
pub mod node;

#[cfg(feature = "swc")]
pub mod swc;
