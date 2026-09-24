//! Synchronization primitives for the Tokio runtime.
//!
//! This module provides channels, locks, and barriers for coordinating async tasks.

pub use tokio::sync::{
    AcquireError, Barrier, BarrierWaitResult, Mutex, Notify, RwLock, RwLockReadGuard, Semaphore,
    broadcast, oneshot, watch,
};

pub mod mpmc;
pub mod mpsc;
