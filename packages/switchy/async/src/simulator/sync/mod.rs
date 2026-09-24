//! Synchronization primitives for the simulator runtime.
//!
//! This module provides channels, locks, and barriers for coordinating async tasks
//! in the simulator environment.

// Notify only coordinates permits and executor wakers; it does not start a
// runtime, read time, or perform I/O. The selected executor owns task scheduling.
pub use tokio::sync::{
    AcquireError, Mutex, Notify, RwLock, RwLockReadGuard, Semaphore, broadcast, oneshot, watch,
};

pub mod barrier;
pub mod mpmc;
pub mod mpsc;

pub use barrier::{Barrier, BarrierWaitResult};
