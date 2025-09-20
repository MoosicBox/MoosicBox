#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

// Phase 2.1: First module added
#[cfg(feature = "simulator")]
pub mod simulator;

// Modules will be added in later phases:
// - Phase 3.1: mod types; mod traits;
// - Phase 4.1: (extend types with thiserror)
// - Phase 5.1: mod router;
