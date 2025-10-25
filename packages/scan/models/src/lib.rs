//! Data models for file scanning operations.
//!
//! This crate provides types for representing scan paths and related metadata
//! used in the `MoosicBox` file scanning system.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "api")]
pub mod api;
