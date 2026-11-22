//! `#[inject_yields]`, `inject_yields_mod!` and `select!` let other tasks run, with each
//! simulation run's yield probability.

#![cfg(all(feature = "simulator", feature = "macros"))]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    future::Future,
    pin::pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};

use switchy_async::{
    inject_yields, inject_yields_mod,
    simulator::{runtime::Runtime, set_yield_probability, yield_probability},
    task,
};

inject_yields_mod!(
    mod yielding;
);

type Log = Arc<Mutex<Vec<String>>>;

/// Records `name` and awaits a ready future, `count` times.
#[inject_yields]
async fn steps(log: Log, name: &'static str, count: usize) {
    for i in 0..count {
        log.lock().unwrap().push(format!("{name}{i}"));
        std::future::ready(()).await;
    }
}

/// A sync function whose `async move` block the attribute reaches.
#[inject_yields]
fn spawn_steps(log: Log, name: &'static str, count: usize) -> task::JoinHandle<()> {
    task::spawn(async move {
        for i in 0..count {
            log.lock().unwrap().push(format!("{name}{i}"));
            std::future::ready(()).await;
        }
    })
}

/// Runs `a` and `b` as two tasks in a run seeded with `seed`, with `probability`, returning
/// the order they recorded their steps in. The executor picks among ready tasks with the
/// run's seed.
fn interleave<A, B>(
    seed: u64,
    probability: f64,
    a: impl FnOnce(Log) -> A,
    b: impl FnOnce(Log) -> B,
) -> String
where
    A: Future<Output = ()> + Send + 'static,
    B: Future<Output = ()> + Send + 'static,
{
    switchy_simulation::with_context(switchy_simulation::Context::new(seed), || {
        let runtime = Runtime::new();
        set_yield_probability(probability);
        let log = Log::default();
        let first = runtime.spawn(a(log.clone()));
        let second = runtime.spawn(b(log.clone()));
        runtime.block_on(async {
            first.await.unwrap();
            second.await.unwrap();
        });
        runtime.wait().unwrap();
        let log = log.lock().unwrap();
        log.join(" ")
    })
}

/// Whether `order` has each task's `steps` steps back to back (no task ran between them).
fn contiguous(order: &str, steps: usize) -> bool {
    ["a", "b"].iter().all(|name| {
        let run = (0..steps)
            .map(|i| format!("{name}{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        order.contains(&run)
    })
}

/// How many of `orders` interleave the tasks' steps: the executor picks among ready tasks
/// by seed, so a task that yields may still run again next.
fn interleaved(orders: &[String]) -> usize {
    orders.iter().filter(|order| !contiguous(order, 3)).count()
}

/// Over 32 seeds with `probability`, the orders `a` and `b` record their `steps` in: each
/// replayed once to check it is the same for its seed.
fn orders<A, B>(
    probability: f64,
    steps: usize,
    a: impl Fn(Log) -> A + Copy,
    b: impl Fn(Log) -> B + Copy,
) -> Vec<String>
where
    A: Future<Output = ()> + Send + 'static,
    B: Future<Output = ()> + Send + 'static,
{
    (0..32)
        .map(|seed| {
            let order = interleave(seed, probability, a, b);
            assert_eq!(order, interleave(seed, probability, a, b), "seed {seed}");
            assert_eq!(order.split(' ').count(), steps * 2, "{order}");
            order
        })
        .collect()
}

#[test]
fn an_injected_yield_lets_a_ready_task_run_between_awaits() {
    let orders = orders(1.0, 3, |log| steps(log, "a", 3), |log| steps(log, "b", 3));
    assert!(interleaved(&orders) >= 8, "{orders:?}");
}

#[test]
fn probability_zero_never_yields() {
    let orders = orders(0.0, 3, |log| steps(log, "a", 3), |log| steps(log, "b", 3));
    assert!(
        orders.iter().all(|order| contiguous(order, 3)),
        "{orders:?}"
    );
}

#[test]
fn reaches_async_blocks_in_sync_functions() {
    let spawned = |log: Log, name| async move { spawn_steps(log, name, 3).await.unwrap() };
    let orders = orders(
        1.0,
        3,
        move |log| spawned(log, "a"),
        move |log| spawned(log, "b"),
    );
    assert!(interleaved(&orders) >= 8, "{orders:?}");
}

#[test]
fn reaches_module_files() {
    let orders = orders(
        1.0,
        3,
        |log| yielding::steps(log, "a", 3),
        |log| yielding::steps(log, "b", 3),
    );
    assert!(interleaved(&orders) >= 8, "{orders:?}");
}

#[inject_yields]
async fn selecting(log: Log, name: &'static str) {
    for i in 0..3 {
        log.lock().unwrap().push(format!("{name}{i}"));
        switchy_async::select! {
            () = std::future::ready(()) => {}
        }
    }
}

#[test]
fn select_yields_after_a_ready_branch() {
    let yielding = orders(1.0, 3, |log| selecting(log, "a"), |log| selecting(log, "b"));
    assert!(interleaved(&yielding) >= 8, "{yielding:?}");
    let not = orders(0.0, 3, |log| selecting(log, "a"), |log| selecting(log, "b"));
    assert!(not.iter().all(|order| contiguous(order, 3)), "{not:?}");
}

/// A `select!` whose every handler leaves still compiles (and does not warn).
#[inject_yields]
async fn select_that_always_leaves() -> u32 {
    loop {
        switchy_async::select! {
            value = std::future::ready(1) => return value,
            () = std::future::pending::<()>() => continue,
        }
    }
}

#[test]
fn select_handlers_may_leave() {
    let value = Runtime::new().block_on(select_that_always_leaves());
    assert_eq!(value, 1);
}

/// Polls `future` once with a waker that counts its wakes.
fn poll_once<F: Future>(future: F) -> (Poll<F::Output>, usize) {
    struct Count(Mutex<usize>);
    impl std::task::Wake for Count {
        fn wake(self: Arc<Self>) {
            *self.0.lock().unwrap() += 1;
        }
    }
    let count = Arc::new(Count(Mutex::new(0)));
    let waker = Waker::from(count.clone());
    let poll = pin!(future).poll(&mut Context::from_waker(&waker));
    let woken = *count.0.lock().unwrap();
    (poll, woken)
}

#[test]
fn probability_zero_is_ready_at_once() {
    switchy_simulation::with_context(switchy_simulation::Context::new(1), || {
        set_yield_probability(0.0);
        for _ in 0..100 {
            assert_eq!(poll_once(task::maybe_yield()), (Poll::Ready(()), 0));
            assert_eq!(
                poll_once(task::yield_after(async { 7 })),
                (Poll::Ready(7), 0)
            );
        }
        set_yield_probability(1.0);
        assert_eq!(poll_once(task::maybe_yield()), (Poll::Pending, 1));
        assert_eq!(
            poll_once(task::yield_after(async { 7 })),
            (Poll::Pending, 1)
        );
    });
}

/// Which of 64 points yield with probability ½, in a run seeded with `seed`.
fn half_draws(seed: u64) -> Vec<bool> {
    switchy_simulation::with_context(switchy_simulation::Context::new(seed), || {
        set_yield_probability(0.5);
        (0..64)
            .map(|_| poll_once(task::maybe_yield()).0.is_pending())
            .collect()
    })
}

#[test]
fn draws_are_deterministic_per_seed() {
    let first = half_draws(7);
    assert_eq!(first, half_draws(7));
    assert_ne!(first, half_draws(8));
    let yielded = first.iter().filter(|yielded| **yielded).count();
    assert!((16..=48).contains(&yielded), "{first:?}");
}

#[test]
fn each_run_draws_its_probability_from_its_seed() {
    let probability = |seed| {
        switchy_simulation::with_context(switchy_simulation::Context::new(seed), yield_probability)
    };
    if std::env::var_os(switchy_async::simulator::YIELD_PROBABILITY_ENV).is_some() {
        return;
    }
    let drawn: Vec<f64> = (0..64).map(probability).collect();
    assert_eq!(drawn, (0..64).map(probability).collect::<Vec<_>>());
    for expected in [0.0, 0.25, 0.5, 1.0] {
        assert!(drawn.contains(&expected), "{drawn:?}");
    }
}
