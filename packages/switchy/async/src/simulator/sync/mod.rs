//! Synchronization primitives for the simulator runtime.
//!
//! This module provides channels, locks, and barriers for coordinating async tasks
//! in the simulator environment.

// These only coordinate permits and executor wakers; they do not start a runtime, read time,
// or perform I/O, and they wake waiters first in, first out (no random or other unseeded
// state), so the selected executor's schedule decides their order. tokio's `watch` is the
// exception (it picks a wait shard with tokio's thread-local generator), so it is replaced.
pub use tokio::sync::{
    AcquireError, Mutex, Notify, RwLock, RwLockReadGuard, Semaphore, broadcast, oneshot,
};

pub mod barrier;
pub mod mpmc;
pub mod mpsc;
pub mod watch;

pub use barrier::{Barrier, BarrierWaitResult};
