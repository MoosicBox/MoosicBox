//! Threads and blocking synchronization primitives that can run deterministically.
//!
//! The API mirrors `std::thread` and `std::sync`. With the `std` backend everything here is a
//! re-export of `std`. With the `simulator` backend (which takes precedence when both are
//! enabled), threads are still real OS threads, but a seeded scheduler lets exactly one of them
//! run at a time and decides at every synchronization operation which thread runs next:
//!
//! * Every blocking operation (`lock`, `recv`, `join`, `sleep`, `wait`, ...) and every
//!   non-blocking synchronization operation (`try_recv`, atomic accesses, unlocking, ...) is a
//!   *switch point* where the scheduler may hand the baton to another thread.
//! * Timed waits use the simulation run's clock (`switchy_time`); when every thread is blocked
//!   and some wait has a deadline, the clock jumps to the earliest one.
//! * The order of execution depends only on the run's seed, so a run replays exactly, and its
//!   decisions are summarized in a trace hash. Deadlocks are detected and reported with the
//!   state of every thread.
//!
//! See [`simulator`] (with the `simulator` feature) for running code under the scheduler.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "simulator")]
pub mod simulator;

/// Threads: spawning, joining, sleeping and yielding.
pub mod thread {
    #[cfg(feature = "simulator")]
    pub use crate::simulator::thread::{
        Builder, JoinHandle, current, panicking, sleep, spawn, yield_now,
    };
    #[cfg(feature = "simulator")]
    pub use std::thread::{Result, Thread, ThreadId};

    #[cfg(all(not(feature = "simulator"), feature = "std"))]
    pub use std::thread::{
        Builder, JoinHandle, Result, Thread, ThreadId, current, panicking, sleep, spawn, yield_now,
    };
}

/// Blocking synchronization: mutexes, condition variables, channels and atomics.
pub mod sync {
    #[cfg(feature = "simulator")]
    pub use crate::simulator::sync::{
        Condvar, Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard, WaitTimeoutResult,
        atomic, mpsc,
    };
    #[cfg(feature = "simulator")]
    pub use std::sync::{Arc, LockResult, PoisonError, TryLockError, TryLockResult, Weak};

    #[cfg(all(not(feature = "simulator"), feature = "std"))]
    pub use std::sync::{
        Arc, Condvar, LockResult, Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard,
        RwLockWriteGuard, TryLockError, TryLockResult, WaitTimeoutResult, Weak, atomic, mpsc,
    };
}
