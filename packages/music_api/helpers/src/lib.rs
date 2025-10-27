//! Helper utilities for working with `MoosicBox` music APIs.
//!
//! This crate provides high-level helper functions for common music API operations,
//! simplifying tasks like enabling scanning, checking scan status, and performing
//! scans across different music sources.
//!
//! # Features
//!
//! * `scan` (default) - Enables music library scanning functionality

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "scan")]
pub mod scan;
