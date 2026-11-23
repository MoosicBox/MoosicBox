//! The simulator's own locks, semaphore, notifications, channels and cancellation tokens in a
//! simulation run: waiters get through in the order they started waiting, every run of a seed
//! replays exactly, and the spurious `Pending` fault reaches each of them without changing what
//! correct code computes.
#![cfg(all(
    feature = "simulator",
    feature = "macros",
    feature = "sync",
    feature = "time",
    feature = "util"
))]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex as StdMutex},
};

use switchy_async::{
    simulator::{fault_counts, runtime::Runtime, set_spurious_pending, set_yield_probability},
    sync::{Mutex, Notify, RwLock, Semaphore, broadcast, oneshot},
    task,
    util::CancellationToken,
};

type Log = Arc<StdMutex<Vec<String>>>;

fn note(log: &Log, line: String) {
    log.lock().unwrap().push(line);
}

/// Run `f` in a fresh simulation run seeded with `seed`, without injected yields, on a new
/// runtime, until every task finished.
fn in_run<R>(seed: u64, f: impl FnOnce(&Runtime) -> R) -> R {
    switchy_simulation::with_context(switchy_simulation::Context::new(seed), || {
        set_yield_probability(0.0);
        let runtime = Runtime::new();
        let result = f(&runtime);
        runtime.wait().unwrap();
        result
    })
}

/// Let every other task run until it waits.
async fn settle() {
    for _ in 0..64 {
        task::yield_now().await;
    }
}

const WAITERS: usize = 6;

type Wait = Box<dyn Fn(usize) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// `WAITERS` tasks that each note `wait i`, run `wait(i)` and note `got i`, released one at a
/// time (or all at once) by `release`. Returns the log.
fn fifo_run(
    seed: u64,
    wait: Wait,
    release: impl FnOnce() -> Pin<Box<dyn Future<Output = ()>>>,
) -> Vec<String> {
    in_run(seed, |runtime| {
        let log = Log::default();
        let wait = Arc::new(wait);
        runtime.block_on({
            let log = Arc::clone(&log);
            async move {
                let mut tasks = Vec::new();
                for i in 0..WAITERS {
                    let (log, wait) = (Arc::clone(&log), Arc::clone(&wait));
                    tasks.push(task::spawn(async move {
                        note(&log, format!("wait {i}"));
                        wait(i).await;
                        note(&log, format!("got {i}"));
                    }));
                }
                settle().await;
                release().await;
                for task in tasks {
                    task.await.unwrap();
                }
            }
        });
        log.lock().unwrap().clone()
    })
}

/// Whether the waiters got through in the order they started waiting.
fn in_order(log: &[String]) -> bool {
    let order = |prefix: &str| -> Vec<String> {
        log.iter()
            .filter_map(|line| line.strip_prefix(prefix))
            .map(str::to_string)
            .collect()
    };
    let (waited, got) = (order("wait "), order("got "));
    waited.len() == WAITERS && waited == got
}

/// Each primitive's run for `seed`. The waiters of the first four are released one at a time,
/// so they get through in the order they waited; those of the rest are woken at once (in the
/// order they waited, see the contract tests), and the executor picks among them by seed.
fn runs(seed: u64) -> Vec<(&'static str, Vec<String>)> {
    let mut runs = handoff_runs(seed);
    runs.extend(wake_all_runs(seed));
    runs
}

/// The runs of the primitives that release one waiter at a time.
fn handoff_runs(seed: u64) -> Vec<(&'static str, Vec<String>)> {
    let mut runs = Vec::new();

    let mutex = Arc::new(Mutex::new(Vec::new()));
    let held = mutex.clone().try_lock_owned().unwrap();
    let waiting = Arc::clone(&mutex);
    runs.push((
        "mutex",
        fifo_run(
            seed,
            Box::new(move |i| {
                let mutex = Arc::clone(&waiting);
                Box::pin(async move { mutex.lock().await.push(i) })
            }),
            move || Box::pin(async move { drop(held) }),
        ),
    ));

    let lock = Arc::new(RwLock::new(0));
    let held = lock.clone().try_read_owned().unwrap();
    let waiting = Arc::clone(&lock);
    runs.push((
        "rwlock",
        fifo_run(
            seed,
            Box::new(move |i| {
                let lock = Arc::clone(&waiting);
                Box::pin(async move { *lock.write().await += i })
            }),
            move || Box::pin(async move { drop(held) }),
        ),
    ));

    let semaphore = Arc::new(Semaphore::new(0));
    let waiting = Arc::clone(&semaphore);
    runs.push((
        "semaphore",
        fifo_run(
            seed,
            Box::new(move |_| {
                let semaphore = Arc::clone(&waiting);
                Box::pin(async move { semaphore.acquire().await.unwrap().forget() })
            }),
            move || {
                Box::pin(async move {
                    for _ in 0..WAITERS {
                        semaphore.add_permits(1);
                        task::yield_now().await;
                    }
                })
            },
        ),
    ));

    let notify = Arc::new(Notify::new());
    let waiting = Arc::clone(&notify);
    runs.push((
        "notify_one",
        fifo_run(
            seed,
            Box::new(move |_| {
                let notify = Arc::clone(&waiting);
                Box::pin(async move { notify.notified().await })
            }),
            move || {
                Box::pin(async move {
                    for _ in 0..WAITERS {
                        notify.notify_one();
                        task::yield_now().await;
                    }
                })
            },
        ),
    ));

    runs
}

/// The runs of the primitives that wake every waiter at once.
fn wake_all_runs(seed: u64) -> Vec<(&'static str, Vec<String>)> {
    let mut runs = Vec::new();

    let notify = Arc::new(Notify::new());
    let waiting = Arc::clone(&notify);
    runs.push((
        "notify_waiters",
        fifo_run(
            seed,
            Box::new(move |_| {
                let notify = Arc::clone(&waiting);
                Box::pin(async move { notify.notified().await })
            }),
            move || Box::pin(async move { notify.notify_waiters() }),
        ),
    ));

    let (sender, _) = broadcast::channel::<u32>(4);
    let receivers: Arc<StdMutex<Vec<_>>> = Arc::new(StdMutex::new(
        (0..WAITERS).map(|_| Some(sender.subscribe())).collect(),
    ));
    runs.push((
        "broadcast",
        fifo_run(
            seed,
            Box::new(move |i| {
                let mut receiver = receivers.lock().unwrap()[i].take().unwrap();
                Box::pin(async move {
                    receiver.recv().await.unwrap();
                })
            }),
            move || {
                Box::pin(async move {
                    sender.send(1).unwrap();
                })
            },
        ),
    ));

    let token = CancellationToken::new();
    let waiting = token.clone();
    runs.push((
        "cancellation",
        fifo_run(
            seed,
            Box::new(move |_| {
                let token = waiting.clone();
                Box::pin(async move { token.cancelled().await })
            }),
            move || Box::pin(async move { token.cancel() }),
        ),
    ));

    runs
}

#[test]
fn waiters_get_through_in_the_order_they_waited_and_runs_replay() {
    for seed in 0..8 {
        let first = runs(seed);
        for (name, log) in &first[..4] {
            assert!(in_order(log), "{name}, seed {seed}: {log:?}");
        }
        assert_eq!(first, runs(seed), "seed {seed}");
    }
}

/// Producers and consumers over each of the simulator's own primitives. Returns what they
/// computed.
async fn workload() -> Vec<u64> {
    let mutex = Arc::new(Mutex::new(0_u64));
    let lock = Arc::new(RwLock::new(0_u64));
    let semaphore = Arc::new(Semaphore::new(2));
    let notify = Arc::new(Notify::new());
    let token = CancellationToken::new();
    let (sender, _) = broadcast::channel::<u64>(16);
    let mut tasks = Vec::new();
    for t in 0..4_u64 {
        let (mutex, lock, semaphore, notify, token) = (
            Arc::clone(&mutex),
            Arc::clone(&lock),
            Arc::clone(&semaphore),
            Arc::clone(&notify),
            token.clone(),
        );
        let mut receiver = sender.subscribe();
        let (reply, answer) = oneshot::channel();
        tasks.push((
            task::spawn(async move {
                let mut sum = 0;
                for i in 0..5 {
                    let _permit = semaphore.acquire().await.unwrap();
                    *mutex.lock().await += t * 10 + i;
                    *lock.write().await += 1;
                    assert!(*lock.read().await > 0);
                    sum += receiver.recv().await.unwrap();
                }
                notify.notified().await;
                token.cancelled().await;
                reply.send(sum).unwrap();
            }),
            answer,
        ));
    }
    for value in 0..5 {
        settle().await;
        sender.send(value).unwrap();
    }
    token.cancel();
    let mut results = Vec::new();
    for (task, mut answer) in tasks {
        // One notification per task (a surplus one stays stored).
        let sum = loop {
            notify.notify_one();
            settle().await;
            match answer.try_recv() {
                Ok(sum) => break sum,
                Err(oneshot::error::TryRecvError::Empty) => {}
                Err(error) => panic!("{error}"),
            }
        };
        task.await.unwrap();
        results.push(sum);
    }
    results.push(*mutex.lock().await);
    results.push(*lock.read().await);
    results
}

/// [`workload`] in a run with spurious `Pending` probability `probability`: what it computed
/// and how many spurious `Pending`s were drawn and fired.
fn spurious_run(seed: u64, probability: f64) -> (Vec<u64>, u64, u64) {
    in_run(seed, |runtime| {
        set_spurious_pending(probability);
        let result = runtime.block_on(workload());
        let counts = fault_counts();
        (
            result,
            counts.spurious_pending_draws,
            counts.spurious_pendings,
        )
    })
}

#[test]
fn spurious_pendings_reach_the_primitives_without_changing_results() {
    for seed in 0..4 {
        let (expected, draws, _) = spurious_run(seed, 0.0);
        assert_eq!(draws, 0, "seed {seed}");
        for probability in [0.2, 1.0] {
            let run = spurious_run(seed, probability);
            assert_eq!(run.0, expected, "seed {seed} at {probability}");
            assert!(
                run.1 > 0 && run.2 > 0,
                "seed {seed} at {probability}: {run:?}"
            );
            assert_eq!(spurious_run(seed, probability), run, "seed {seed}");
        }
    }
}
