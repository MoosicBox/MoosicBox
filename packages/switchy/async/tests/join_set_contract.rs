//! `task::JoinSet` semantics shared by both backends, and the simulator's deterministic
//! completion order.
#![cfg(all(feature = "sync", any(feature = "simulator", feature = "tokio")))]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::collections::BTreeSet;

use switchy_async::{sync::oneshot, task::JoinSet};

#[switchy_async::test]
async fn join_next_yields_every_output_then_none() {
    let mut set = JoinSet::new();
    assert!(set.is_empty());
    for i in 0..8_u32 {
        set.spawn(async move { i * 2 });
    }
    assert_eq!(set.len(), 8);
    let mut outputs = BTreeSet::new();
    while let Some(outcome) = set.join_next().await {
        outputs.insert(outcome.unwrap());
        assert_eq!(set.len(), 8 - outputs.len());
    }
    assert_eq!(outputs, (0..8).map(|i| i * 2).collect());
    assert!(set.is_empty());
    assert!(set.join_next().await.is_none());
}

#[switchy_async::test]
async fn join_next_waits_for_a_pending_task() {
    let mut set = JoinSet::new();
    let (tx, rx) = oneshot::channel::<u32>();
    set.spawn(async move { rx.await.unwrap() + 1 });
    assert!(set.try_join_next().is_none());
    tx.send(41).unwrap();
    assert_eq!(set.join_next().await.unwrap().unwrap(), 42);
}

#[switchy_async::test]
async fn abort_all_cancels_unfinished_tasks_but_keeps_finished_results() {
    let mut set = JoinSet::new();
    set.spawn(async { 1_u32 });
    // Let the ready task finish before aborting.
    let first = set.join_next().await.unwrap().unwrap();
    assert_eq!(first, 1);

    set.spawn(std::future::pending::<u32>());
    set.spawn(std::future::pending::<u32>());
    set.abort_all();
    assert_eq!(set.len(), 2);
    for _ in 0..2 {
        let error = set.join_next().await.unwrap().unwrap_err();
        assert!(error.is_cancelled());
        assert!(!error.is_panic());
    }
    assert!(set.join_next().await.is_none());
}

#[switchy_async::test]
async fn abort_handle_cancels_one_task() {
    let mut set = JoinSet::new();
    let pending = set.spawn(std::future::pending::<u32>());
    set.spawn(async { 7_u32 });
    pending.abort();
    let mut ok = Vec::new();
    let mut cancelled = 0;
    while let Some(outcome) = set.join_next().await {
        match outcome {
            Ok(value) => ok.push(value),
            Err(error) => {
                assert!(error.is_cancelled());
                cancelled += 1;
            }
        }
    }
    assert_eq!((ok, cancelled), (vec![7], 1));
    assert!(pending.is_finished());
}

#[switchy_async::test]
async fn dropping_the_set_aborts_its_tasks() {
    let (tx, rx) = oneshot::channel::<()>();
    let mut set = JoinSet::new();
    set.spawn(async move {
        let _tx = tx;
        std::future::pending::<()>().await;
    });
    switchy_async::task::yield_now().await;
    drop(set);
    // The aborted task drops its future, and with it the sender.
    assert!(rx.await.is_err());
}

#[switchy_async::test]
async fn shutdown_aborts_and_drains() {
    let mut set = JoinSet::new();
    for _ in 0..3 {
        set.spawn(std::future::pending::<()>());
    }
    set.shutdown().await;
    assert!(set.is_empty());
}

#[switchy_async::test]
async fn spawn_blocking_joins_like_a_task() {
    let mut set = JoinSet::new();
    set.spawn_blocking(|| 5_u32);
    set.spawn(async { 6_u32 });
    let mut outputs = Vec::new();
    while let Some(outcome) = set.join_next().await {
        outputs.push(outcome.unwrap());
    }
    outputs.sort_unstable();
    assert_eq!(outputs, [5, 6]);
}

#[switchy_async::test]
async fn join_all_collects_outputs() {
    let set: JoinSet<u32> = (0..4_u32).map(|i| async move { i }).collect();
    let mut outputs = set.join_all().await;
    outputs.sort_unstable();
    assert_eq!(outputs, [0, 1, 2, 3]);
}

#[switchy_async::test]
async fn detach_all_empties_the_set_and_lets_tasks_finish() {
    let (tx, rx) = oneshot::channel::<u32>();
    let mut set = JoinSet::new();
    set.spawn(async move {
        tx.send(3).unwrap();
    });
    set.detach_all();
    assert!(set.is_empty());
    assert!(set.join_next().await.is_none());
    assert_eq!(rx.await.unwrap(), 3);
}

/// Non-`Send` futures join through the set too (tokio needs a `LocalSet` for this).
#[cfg(feature = "simulator")]
#[switchy_async::test]
async fn simulator_spawn_local_joins() {
    let mut set = JoinSet::new();
    let local = std::rc::Rc::new(4_u32);
    set.spawn_local(async move { *local + 1 });
    assert_eq!(set.join_next().await.unwrap().unwrap(), 5);
}

/// The order tasks of one set finish in under the simulator.
#[cfg(feature = "simulator")]
fn completion_order() -> Vec<u32> {
    let runtime = switchy_async::simulator::runtime::Runtime::new();
    let order = runtime.block_on(async {
        let mut set = JoinSet::new();
        for i in 0..16_u32 {
            set.spawn(async move {
                for _ in 0..(i % 3) {
                    switchy_async::task::yield_now().await;
                }
                i
            });
        }
        let mut order = Vec::new();
        while let Some(outcome) = set.join_next().await {
            order.push(outcome.unwrap());
        }
        order
    });
    runtime.wait().unwrap();
    order
}

/// Threads that were never given a simulation context each start from the process's initial
/// seed, so they see the same completion order.
#[cfg(feature = "simulator")]
#[test]
fn simulator_completion_order_is_reproducible() {
    let first = std::thread::spawn(completion_order).join().unwrap();
    let second = std::thread::spawn(completion_order).join().unwrap();
    assert_eq!(first, second);
    let mut sorted = first;
    sorted.sort_unstable();
    assert_eq!(sorted, (0..16).collect::<Vec<_>>());
}

/// The completion order follows the run's seed: the same for a seed, and not the same for
/// every seed.
#[cfg(feature = "thread-simulator")]
#[test]
fn simulator_completion_order_follows_the_seed() {
    fn order(seed: u64) -> Vec<u32> {
        switchy_thread::simulator::run(seed, || {
            switchy_random::simulator::reset_rng();
            completion_order()
        })
        .result
        .unwrap()
    }
    let first = order(7);
    assert_eq!(first, order(7));
    let distinct: BTreeSet<Vec<u32>> = (0..8).map(order).collect();
    assert!(distinct.len() > 1, "{distinct:?}");
}

/// A blocking closure on a simulated thread joins through the set, deterministically.
#[cfg(feature = "thread-simulator")]
#[test]
fn simulator_spawn_blocking_on_simulated_threads_is_deterministic() {
    fn order(seed: u64) -> Vec<u32> {
        switchy_thread::simulator::run(seed, || {
            switchy_random::simulator::reset_rng();
            let runtime = switchy_async::simulator::runtime::Runtime::new();
            let order = runtime.block_on(async {
                let mut set = JoinSet::new();
                for i in 0..6_u32 {
                    if i % 2 == 0 {
                        set.spawn_blocking(move || i);
                    } else {
                        set.spawn(async move { i });
                    }
                }
                let mut order = Vec::new();
                while let Some(outcome) = set.join_next().await {
                    order.push(outcome.unwrap());
                }
                order
            });
            runtime.wait().unwrap();
            order
        })
        .result
        .unwrap()
    }
    let first = order(3);
    assert_eq!(first, order(3));
    let mut sorted = first;
    sorted.sort_unstable();
    assert_eq!(sorted, (0..6).collect::<Vec<_>>());
}
