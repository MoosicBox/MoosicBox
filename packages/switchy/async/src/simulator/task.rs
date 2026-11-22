//! Task spawning and execution for the simulator runtime.
//!
//! This module provides task spawning functions and types for the simulator backend.

pub use super::join_set::JoinSet;
pub use super::runtime::{AbortHandle, JoinHandle, spawn, spawn_blocking, spawn_local};

pub use tokio::task::yield_now;

pub use super::yields::{MaybeYield, YieldAfter, maybe_yield, yield_after};

/// Run blocking `f` in place. The simulator runs every task on the calling thread already,
/// and futures nested with `Handle::block_on` drive the runtime's other tasks meanwhile.
pub fn block_in_place<F: FnOnce() -> R, R>(f: F) -> R {
    f()
}

/// Error returned when a task fails to join.
///
/// In the simulator a task fails to join only when it is cancelled: aborted, or dropped with
/// its runtime. A panicking task is not caught; the panic unwinds through the executor and
/// fails the run (where `tokio` would report it as a [`JoinError`] with `is_panic`).
#[derive(Debug, Clone, thiserror::Error, Default)]
pub struct JoinError;

impl JoinError {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self
    }

    /// Whether the task was cancelled. Always `true` in the simulator (see [`JoinError`]).
    #[must_use]
    #[allow(clippy::unused_self)] // mirrors `tokio::task::JoinError::is_cancelled`
    pub const fn is_cancelled(&self) -> bool {
        true
    }

    /// Whether the task panicked. Always `false` in the simulator (see [`JoinError`]).
    #[must_use]
    #[allow(clippy::unused_self)] // mirrors `tokio::task::JoinError::is_panic`
    pub const fn is_panic(&self) -> bool {
        false
    }
}

impl std::fmt::Display for JoinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("JoinError")
    }
}
