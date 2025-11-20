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
    use super::*;
    use serial_test::serial;

    #[test_log::test]
    fn test_worker_thread_id_is_unique() {
        let id1 = worker_thread_id();
        let id2 = worker_thread_id();

        // Same thread should return same ID
        assert_eq!(id1, id2);

        // Different threads should have different IDs
        let handle = std::thread::spawn(worker_thread_id);
        let id3 = handle.join().unwrap();

        assert_ne!(id1, id3);
    }

    #[test_log::test]
    fn test_worker_thread_id_is_monotonic() {
        let mut ids: Vec<u64> = (0..5)
            .map(|_| std::thread::spawn(worker_thread_id))
            .map(|h| h.join().unwrap())
            .collect();
        ids.sort_unstable();

        // All IDs should be unique
        for window in ids.windows(2) {
            assert_ne!(window[0], window[1]);
        }
    }

    #[test_log::test]
    #[serial]
    fn test_reset_simulator_cancellation_token() {
        reset_global_simulator_cancellation_token();
        cancel_simulation();
        assert!(is_simulator_cancelled());

        reset_simulator_cancellation_token();
        assert!(!is_simulator_cancelled());
    }

    #[test_log::test]
    #[serial]
    fn test_cancel_simulation() {
        reset_simulator_cancellation_token();
        reset_global_simulator_cancellation_token();
        assert!(!is_simulator_cancelled());

        cancel_simulation();
        assert!(is_simulator_cancelled());

        // Clean up
        reset_simulator_cancellation_token();
    }

    #[test_log::test]
    #[serial]
    fn test_reset_global_simulator_cancellation_token() {
        cancel_global_simulation();
        assert!(is_global_simulator_cancelled());

        reset_global_simulator_cancellation_token();
        assert!(!is_global_simulator_cancelled());
    }

    #[test_log::test]
    #[serial]
    fn test_cancel_global_simulation() {
        reset_global_simulator_cancellation_token();
        assert!(!is_global_simulator_cancelled());

        cancel_global_simulation();
        assert!(is_global_simulator_cancelled());

        // Clean up for other tests
        reset_global_simulator_cancellation_token();
    }

    #[test_log::test]
    #[serial]
    fn test_global_cancellation_affects_is_simulator_cancelled() {
        reset_simulator_cancellation_token();
        reset_global_simulator_cancellation_token();
        assert!(!is_simulator_cancelled());

        cancel_global_simulation();
        assert!(is_simulator_cancelled());
        assert!(is_global_simulator_cancelled());

        // Clean up for other tests
        reset_global_simulator_cancellation_token();
    }

    #[test_log::test]
    #[serial]
    fn test_thread_local_cancellation_does_not_affect_global() {
        reset_simulator_cancellation_token();
        reset_global_simulator_cancellation_token();

        cancel_simulation();
        assert!(is_simulator_cancelled());
        assert!(!is_global_simulator_cancelled());
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_run_until_simulation_cancelled_completes() {
        // Ensure clean state
        reset_simulator_cancellation_token();
        reset_global_simulator_cancellation_token();

        let result = run_until_simulation_cancelled(async { 42 }).await;

        assert_eq!(result, Some(42), "Future should complete when not cancelled");
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_run_until_simulation_cancelled_with_local_cancellation() {
        reset_simulator_cancellation_token();
        reset_global_simulator_cancellation_token();

        // Cancel immediately before starting the future
        cancel_simulation();

        let result = run_until_simulation_cancelled(async {
            switchy_async::time::sleep(std::time::Duration::from_secs(10)).await;
            42
        })
        .await;

        // Clean up
        reset_simulator_cancellation_token();
        assert_eq!(result, None);
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_run_until_simulation_cancelled_with_global_cancellation() {
        reset_simulator_cancellation_token();
        reset_global_simulator_cancellation_token();

        // Cancel immediately before starting the future
        cancel_global_simulation();

        let result = run_until_simulation_cancelled(async {
            switchy_async::time::sleep(std::time::Duration::from_secs(10)).await;
            42
        })
        .await;

        // Clean up for other tests
        reset_global_simulator_cancellation_token();
        assert_eq!(result, None);
    }

    #[test_log::test]
    #[serial]
    fn test_thread_local_cancellation_isolation() {
        reset_global_simulator_cancellation_token();

        // Thread 1: cancel simulation
        let handle1 = std::thread::spawn(|| {
            reset_simulator_cancellation_token();
            cancel_simulation();
            is_simulator_cancelled()
        });

        // Thread 2: don't cancel, check isolation
        let handle2 = std::thread::spawn(|| {
            reset_simulator_cancellation_token();
            is_simulator_cancelled()
        });

        let thread1_cancelled = handle1.join().unwrap();
        let thread2_cancelled = handle2.join().unwrap();

        assert!(thread1_cancelled);
        assert!(!thread2_cancelled);
    }

    #[test_log::test]
    #[serial]
    fn test_multiple_resets_work_correctly() {
        reset_global_simulator_cancellation_token();

        // Test that multiple reset cycles work correctly
        for _ in 0..3 {
            reset_simulator_cancellation_token();
            assert!(!is_simulator_cancelled());

            cancel_simulation();
            assert!(is_simulator_cancelled());
        }

        // Final cleanup
        reset_simulator_cancellation_token();
    }
}
