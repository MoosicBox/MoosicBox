//! Utility types for the simulator runtime.
//!
//! This module re-exports `tokio_util`'s cancellation token for compatibility with the simulator.

pub use tokio_util::sync::CancellationToken;
