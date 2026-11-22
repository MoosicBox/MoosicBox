//! Task spawning and execution for the Tokio runtime.
//!
//! This module re-exports Tokio's task spawning functions and types.

pub use tokio::task::{
    AbortHandle, JoinError, JoinHandle, JoinSet, spawn, spawn_blocking, spawn_local, yield_now,
};

#[cfg(feature = "rt-multi-thread")]
pub use tokio::task::block_in_place;

/// A seeded yield point. Only the simulator yields (with its run's probability); here it is
/// ready at once.
#[must_use = "futures do nothing unless awaited"]
pub fn maybe_yield() -> std::future::Ready<()> {
    std::future::ready(())
}

/// Awaits `future` and returns its output. Only the simulator yields after it (with its run's
/// probability); here it is `future` itself.
#[must_use = "futures do nothing unless awaited"]
pub fn yield_after<F: IntoFuture>(future: F) -> F::IntoFuture {
    future.into_future()
}
