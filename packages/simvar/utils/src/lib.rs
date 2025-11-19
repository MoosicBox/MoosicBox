//! Utility functions for simulation testing and cancellation management.
//!
//! This crate provides utilities for managing worker threads and cancellation tokens
//! in simulation environments. It supports both thread-local and global cancellation,
//! allowing tests to gracefully terminate simulations and async operations.
//!
//! # Features
//!
//! * **Thread Management**: Unique worker thread ID tracking
//! * **Cancellation Tokens**: Thread-local and global cancellation support
//! * **Async Utilities**: Run futures until simulation cancellation
//!
//! # Example
//!
//! ```rust
//! use simvar_utils::{worker_thread_id, run_until_simulation_cancelled};
//!
//! // Get unique thread ID
//! let thread_id = worker_thread_id();
//! println!("Worker thread ID: {}", thread_id);
//!
//! # async fn example() {
//! # async fn simulate_work() -> u32 { 42 }
//! // Run future until cancelled
//! let result = run_until_simulation_cancelled(async {
//!     simulate_work().await
//! }).await;
//!
//! match result {
//!     Some(output) => println!("Completed: {}", output),
//!     None => println!("Cancelled"),
//! }
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    cell::RefCell,
    future::Future,
    sync::{LazyLock, RwLock, atomic::AtomicU64},
};

use switchy::unsync::util::CancellationToken;

static WORKER_THREAD_ID_COUNTER: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(1));

thread_local! {
    static WORKER_THREAD_ID: RefCell<u64> = RefCell::new(WORKER_THREAD_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst));
}

/// Returns the unique identifier for the current worker thread.
///
/// Each thread gets a unique, monotonically increasing ID starting from 1.
#[must_use]
pub fn worker_thread_id() -> u64 {
    WORKER_THREAD_ID.with_borrow(|x| *x)
}

thread_local! {
    static SIMULATOR_CANCELLATION_TOKEN: RefCell<RwLock<CancellationToken>> =
        RefCell::new(RwLock::new(CancellationToken::new()));
}

/// Resets the thread-local simulation cancellation token.
///
/// Creates a new cancellation token for the current thread, clearing any previous
/// cancellation state. Use this to prepare for a new simulation run.
///
/// # Panics
///
/// * If the `SIMULATOR_CANCELLATION_TOKEN` `RwLock` fails to write to
pub fn reset_simulator_cancellation_token() {
    SIMULATOR_CANCELLATION_TOKEN
        .with_borrow_mut(|x| *x.write().unwrap() = CancellationToken::new());
}

/// Checks if the current thread's simulation has been cancelled.
///
/// Returns `true` if either the global or thread-local cancellation token has been triggered.
///
/// # Panics
///
/// * If the `SIMULATOR_CANCELLATION_TOKEN` `RwLock` fails to read from
#[must_use]
pub fn is_simulator_cancelled() -> bool {
    is_global_simulator_cancelled()
        || SIMULATOR_CANCELLATION_TOKEN.with_borrow(|x| x.read().unwrap().is_cancelled())
}

/// Cancels the current thread's simulation.
///
/// Triggers the thread-local cancellation token, causing any futures running with
/// [`run_until_simulation_cancelled`] to terminate.
///
/// # Panics
///
/// * If the `SIMULATOR_CANCELLATION_TOKEN` `RwLock` fails to read from
pub fn cancel_simulation() {
    SIMULATOR_CANCELLATION_TOKEN.with_borrow(|x| x.read().unwrap().cancel());
}

static GLOBAL_SIMULATOR_CANCELLATION_TOKEN: LazyLock<RwLock<CancellationToken>> =
    LazyLock::new(|| RwLock::new(CancellationToken::new()));

/// Resets the global simulation cancellation token.
///
/// Creates a new global cancellation token, clearing any previous cancellation state
/// across all threads. Use this to prepare for a new simulation run.
///
/// # Panics
///
/// * If the `GLOBAL_SIMULATOR_CANCELLATION_TOKEN` `RwLock` fails to write to
pub fn reset_global_simulator_cancellation_token() {
    *GLOBAL_SIMULATOR_CANCELLATION_TOKEN.write().unwrap() = CancellationToken::new();
}

/// Checks if the global simulation has been cancelled.
///
/// Returns `true` if the global cancellation token has been triggered, affecting all threads.
///
/// # Panics
///
/// * If the `GLOBAL_SIMULATOR_CANCELLATION_TOKEN` `RwLock` fails to read from
#[must_use]
pub fn is_global_simulator_cancelled() -> bool {
    GLOBAL_SIMULATOR_CANCELLATION_TOKEN
        .read()
        .unwrap()
        .is_cancelled()
}

/// Cancels all simulations globally.
///
/// Triggers the global cancellation token, affecting all threads and causing any futures
/// running with [`run_until_simulation_cancelled`] to terminate across the entire process.
///
/// # Panics
///
/// * If the `GLOBAL_SIMULATOR_CANCELLATION_TOKEN` `RwLock` fails to read from
pub fn cancel_global_simulation() {
    GLOBAL_SIMULATOR_CANCELLATION_TOKEN.read().unwrap().cancel();
}

/// Runs a future until it completes or simulation is cancelled.
///
/// Returns `Some(output)` if the future completes, or `None` if either the global
/// or thread-local simulation cancellation token is triggered.
///
/// # Panics
///
/// * If the `GLOBAL_SIMULATOR_CANCELLATION_TOKEN` `RwLock` fails to read from
/// * If the `SIMULATOR_CANCELLATION_TOKEN` `RwLock` fails to read from
#[must_use]
pub async fn run_until_simulation_cancelled<F>(fut: F) -> Option<F::Output>
where
    F: Future,
{
    let global_token = GLOBAL_SIMULATOR_CANCELLATION_TOKEN.read().unwrap().clone();
    let local_token = SIMULATOR_CANCELLATION_TOKEN.with_borrow(|x| x.read().unwrap().clone());

    switchy::unsync::select! {
        resp = fut => Some(resp),
        () = global_token.cancelled() => None,
        () = local_token.cancelled() => None,
    }
}
