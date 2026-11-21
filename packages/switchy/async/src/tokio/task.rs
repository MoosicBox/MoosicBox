//! Task spawning and execution for the Tokio runtime.
//!
//! This module re-exports Tokio's task spawning functions and types.

pub use tokio::task::{JoinError, JoinHandle, spawn, spawn_blocking, spawn_local, yield_now};

#[cfg(feature = "rt-multi-thread")]
pub use tokio::task::block_in_place;
