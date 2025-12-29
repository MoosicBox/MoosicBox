//! Task spawning and execution for the simulator runtime.
//!
//! This module provides task spawning functions and types for the simulator backend.

pub use super::runtime::{JoinHandle, spawn, spawn_blocking, spawn_local};

pub use tokio::task::yield_now;

/// Error returned when a task fails to join.
#[derive(Debug, Clone, thiserror::Error, Default)]
pub struct JoinError;

impl JoinError {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl std::fmt::Display for JoinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("JoinError")
    }
}
