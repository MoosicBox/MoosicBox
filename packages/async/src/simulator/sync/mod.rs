//! Synchronization primitives for the simulator runtime.
//!
//! This module provides channels, locks, and barriers for coordinating async tasks
//! in the simulator environment.

pub use tokio::sync::{Mutex, RwLock, RwLockReadGuard, oneshot};

pub mod barrier;
pub mod mpmc;
pub mod mpsc;

pub use barrier::{Barrier, BarrierWaitResult};
