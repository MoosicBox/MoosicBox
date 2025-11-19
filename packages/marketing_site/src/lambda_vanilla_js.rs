//! AWS Lambda entry point with vanilla JavaScript renderer support.
//!
//! This binary provides an AWS Lambda handler for the `MoosicBox` marketing site
//! with vanilla JavaScript client-side rendering enabled. It delegates to the
//! shared lambda module for the actual implementation.

#![cfg_attr(
    all(not(debug_assertions), not(feature = "windows-console")),
    windows_subsystem = "windows"
)]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod lambda;

/// Runs the AWS Lambda handler for the marketing site.
///
/// # Errors
///
/// * If application building fails
/// * If Lambda handler setup fails
///
/// # Panics
///
/// * If static asset route registration fails (via `lambda::run`)
fn main() -> Result<(), Box<dyn std::error::Error>> {
    lambda::run()
}
