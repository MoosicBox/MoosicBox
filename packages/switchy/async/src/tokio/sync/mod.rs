//! Synchronization primitives for the Tokio runtime.
//!
//! This module provides channels, locks, and barriers for coordinating async tasks.

pub use tokio::sync::{
    AcquireError, Barrier, BarrierWaitResult, MappedMutexGuard, Mutex, MutexGuard, Notify,
    OwnedMappedMutexGuard, OwnedMutexGuard, OwnedRwLockMappedWriteGuard, OwnedRwLockReadGuard,
    OwnedRwLockWriteGuard, OwnedSemaphorePermit, RwLock, RwLockMappedWriteGuard, RwLockReadGuard,
    RwLockWriteGuard, Semaphore, SemaphorePermit, TryAcquireError, TryLockError, broadcast,
    futures, oneshot, watch,
};

pub mod mpmc;
pub mod mpsc;
