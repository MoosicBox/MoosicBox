#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    cell::RefCell,
    sync::{LazyLock, RwLock, atomic::AtomicU64},
};

use switchy::unsync::{futures::FutureExt as _, util::CancellationToken};

static WORKER_THREAD_ID_COUNTER: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(1));

thread_local! {
    static WORKER_THREAD_ID: RefCell<u64> = RefCell::new(WORKER_THREAD_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst));
}

#[must_use]
pub fn worker_thread_id() -> u64 {
    WORKER_THREAD_ID.with_borrow(|x| *x)
}

thread_local! {
    static SIMULATOR_CANCELLATION_TOKEN: RefCell<RwLock<CancellationToken>> =
        RefCell::new(RwLock::new(CancellationToken::new()));
}

/// # Panics
///
/// * If the `SIMULATOR_CANCELLATION_TOKEN` `RwLock` fails to write to
pub fn reset_simulator_cancellation_token() {
    SIMULATOR_CANCELLATION_TOKEN
        .with_borrow_mut(|x| *x.write().unwrap() = CancellationToken::new());
}

/// # Panics
///
/// * If the `SIMULATOR_CANCELLATION_TOKEN` `RwLock` fails to read from
#[must_use]
pub fn is_simulator_cancelled() -> bool {
    is_global_simulator_cancelled()
        || SIMULATOR_CANCELLATION_TOKEN.with_borrow(|x| x.read().unwrap().is_cancelled())
}

/// # Panics
///
/// * If the `SIMULATOR_CANCELLATION_TOKEN` `RwLock` fails to read from
pub fn cancel_simulation() {
    SIMULATOR_CANCELLATION_TOKEN.with_borrow(|x| x.read().unwrap().cancel());
}

static GLOBAL_SIMULATOR_CANCELLATION_TOKEN: LazyLock<RwLock<CancellationToken>> =
    LazyLock::new(|| RwLock::new(CancellationToken::new()));

/// # Panics
///
/// * If the `GLOBAL_SIMULATOR_CANCELLATION_TOKEN` `RwLock` fails to write to
pub fn reset_global_simulator_cancellation_token() {
    *GLOBAL_SIMULATOR_CANCELLATION_TOKEN.write().unwrap() = CancellationToken::new();
}

/// # Panics
///
/// * If the `GLOBAL_SIMULATOR_CANCELLATION_TOKEN` `RwLock` fails to read from
pub fn is_global_simulator_cancelled() -> bool {
    GLOBAL_SIMULATOR_CANCELLATION_TOKEN
        .read()
        .unwrap()
        .is_cancelled()
}

/// # Panics
///
/// * If the `GLOBAL_SIMULATOR_CANCELLATION_TOKEN` `RwLock` fails to read from
pub fn cancel_global_simulation() {
    GLOBAL_SIMULATOR_CANCELLATION_TOKEN.read().unwrap().cancel();
}

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
        resp = fut.fuse() => Some(resp),
        () = global_token.cancelled().fuse() => None,
        () = local_token.cancelled().fuse() => None,
    }
}
