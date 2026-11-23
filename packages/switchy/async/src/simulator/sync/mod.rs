//! Synchronization primitives for the simulator runtime.
//!
//! This module provides channels, locks, and barriers for coordinating async tasks
//! in the simulator environment.
//!
//! Every primitive here is the simulator's own, with tokio's API: `Mutex`, `RwLock`,
//! `Semaphore`, `Notify`, `Barrier` and the `oneshot`, `broadcast`, `watch` and `mpmc` channels
//! (`mpsc` wraps tokio's unbounded channel, whose single receiver has no wake order to decide).
//! They are deterministic by construction: each keeps its waiters in one list ordered by when
//! they started waiting and wakes them in a defined order (first in, first out, unless the API
//! says otherwise), with no shards, no random choice and nothing that depends on addresses. A
//! ready poll of each of their futures is a switch point of the spurious `Pending` fault (see
//! the simulator's `faults`), and with `thread-simulator` their `blocking_*` methods wait on
//! `switchy_thread`'s scheduler from a simulated thread instead of blocking its OS thread.

pub mod barrier;
mod blocking;
pub mod broadcast;
pub mod mpmc;
pub mod mpsc;
mod mutex;
mod notify;
pub mod oneshot;
mod rwlock;
mod semaphore;
pub mod watch;

pub use barrier::{Barrier, BarrierWaitResult};
pub use mutex::{
    MappedMutexGuard, Mutex, MutexGuard, OwnedMappedMutexGuard, OwnedMutexGuard, TryLockError,
};
pub use notify::Notify;
pub use rwlock::{
    OwnedRwLockMappedWriteGuard, OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock,
    RwLockMappedWriteGuard, RwLockReadGuard, RwLockWriteGuard,
};
pub use semaphore::{
    AcquireError, OwnedSemaphorePermit, Semaphore, SemaphorePermit, TryAcquireError,
};

/// Named future types, as `tokio::sync::futures`.
pub mod futures {
    pub use super::notify::{Notified, OwnedNotified};
}
