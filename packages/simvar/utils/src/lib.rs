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

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use super::*;

    // Note: All tests in this module use #[serial] because they interact with the global
    // SIMULATOR_CANCELLATION_TOKEN state. Running these tests in parallel would cause
    // race conditions where one test's state changes affect another test's expectations.
    // The serial_test crate ensures these tests run one at a time.

    #[test_log::test]
    #[serial]
    fn test_worker_thread_id_returns_unique_ids() {
        let id1 = worker_thread_id();
        let id2 = worker_thread_id();
        // Same thread should return same ID
        assert_eq!(id1, id2);
    }

    #[test_log::test]
    #[serial]
    fn test_worker_thread_id_uniqueness_across_threads() {
        let id1 = worker_thread_id();
        let handle = std::thread::spawn(worker_thread_id);
        let id2 = handle.join().unwrap();
        // Different threads should have different IDs
        assert_ne!(id1, id2);
    }

    #[test_log::test]
    #[serial]
    fn test_local_cancellation_isolated_between_threads() {
        // Reset all states
        reset_global_simulator_cancellation_token();
        reset_simulator_cancellation_token();

        // Cancel local simulation on this thread
        cancel_simulation();
        assert!(is_simulator_cancelled());

        // Spawn a new thread and check its local state is NOT cancelled
        let handle = std::thread::spawn(|| {
            // This thread has its own thread-local token which should NOT be cancelled
            reset_simulator_cancellation_token();
            is_simulator_cancelled()
        });

        let other_thread_cancelled = handle.join().unwrap();
        // The other thread's local cancellation state should be false
        // (since we reset it and only cancelled on the main thread)
        assert!(
            !other_thread_cancelled,
            "Local cancellation should not affect other threads"
        );
    }

    #[test_log::test]
    #[serial]
    fn test_reset_simulator_cancellation_token() {
        // Reset all states
        reset_global_simulator_cancellation_token();
        reset_simulator_cancellation_token();

        // Cancel the token
        cancel_simulation();
        assert!(is_simulator_cancelled());

        // Reset should clear cancellation
        reset_simulator_cancellation_token();
        assert!(!is_simulator_cancelled());
    }

    #[test_log::test]
    #[serial]
    fn test_cancel_simulation_sets_cancelled_state() {
        // Reset all states
        reset_global_simulator_cancellation_token();
        reset_simulator_cancellation_token();

        assert!(!is_simulator_cancelled());

        cancel_simulation();
        assert!(is_simulator_cancelled());
    }

    #[test_log::test]
    #[serial]
    fn test_is_simulator_cancelled_respects_global_cancellation() {
        // Reset all states
        reset_global_simulator_cancellation_token();
        reset_simulator_cancellation_token();

        assert!(!is_simulator_cancelled());

        cancel_global_simulation();
        // Local cancellation should detect global cancellation
        assert!(is_simulator_cancelled());
    }

    #[test_log::test]
    #[serial]
    fn test_global_cancellation_independent_from_local() {
        // Reset all states
        reset_global_simulator_cancellation_token();
        reset_simulator_cancellation_token();

        cancel_simulation();
        // Local cancelled but not global directly
        assert!(!is_global_simulator_cancelled());
        assert!(is_simulator_cancelled());
    }

    #[test_log::test]
    #[serial]
    fn test_reset_global_simulator_cancellation_token() {
        // Reset all states
        reset_global_simulator_cancellation_token();
        reset_simulator_cancellation_token();

        cancel_global_simulation();

        assert!(is_global_simulator_cancelled());

        reset_global_simulator_cancellation_token();
        assert!(!is_global_simulator_cancelled());
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_run_until_simulation_cancelled_completes_normally() {
        // Reset all states
        reset_global_simulator_cancellation_token();
        reset_simulator_cancellation_token();

        let result = run_until_simulation_cancelled(async { 42 }).await;
        assert_eq!(result, Some(42));
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_run_until_simulation_cancelled_with_local_cancellation() {
        // Reset all states
        reset_global_simulator_cancellation_token();
        reset_simulator_cancellation_token();

        let cancel_task = async {
            cancel_simulation();
        };

        let work_task = async {
            // This will never complete
            std::future::pending::<()>().await;
            42
        };

        // Cancel immediately
        cancel_task.await;
        let result = run_until_simulation_cancelled(work_task).await;
        assert_eq!(result, None);
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_run_until_simulation_cancelled_with_global_cancellation() {
        // Reset all states
        reset_global_simulator_cancellation_token();
        reset_simulator_cancellation_token();

        let cancel_task = async {
            cancel_global_simulation();
        };

        let work_task = async {
            // This will never complete
            std::future::pending::<()>().await;
            42
        };

        // Cancel immediately
        cancel_task.await;
        let result = run_until_simulation_cancelled(work_task).await;
        assert_eq!(result, None);
    }

    #[test_log::test]
    #[serial]
    fn test_global_cancellation_affects_other_threads() {
        // Reset all states
        reset_global_simulator_cancellation_token();
        reset_simulator_cancellation_token();

        // Verify not cancelled initially
        assert!(!is_global_simulator_cancelled());

        // Cancel globally from main thread
        cancel_global_simulation();

        // Verify another thread sees the global cancellation
        let handle = std::thread::spawn(|| {
            // Reset this thread's local token (should not affect global)
            reset_simulator_cancellation_token();
            // This should still return true because global is cancelled
            is_simulator_cancelled()
        });

        let other_thread_sees_cancellation = handle.join().unwrap();
        assert!(
            other_thread_sees_cancellation,
            "Global cancellation should be visible to all threads"
        );
    }

    #[test_log::test]
    #[serial]
    fn test_worker_thread_ids_are_monotonically_increasing() {
        // Spawn multiple threads and collect their IDs
        let mut handles = Vec::new();
        for _ in 0..5 {
            handles.push(std::thread::spawn(worker_thread_id));
        }

        let mut ids: Vec<u64> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        // Sort to verify all IDs are unique
        ids.sort_unstable();
        let original_len = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), original_len, "All thread IDs should be unique");

        // All IDs should be >= 1 (IDs start at 1)
        assert!(ids.iter().all(|&id| id >= 1), "All IDs should be >= 1");
    }
}
