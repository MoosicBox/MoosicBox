//! The simulator executor together with `switchy_thread`'s simulated threads.
#![cfg(feature = "thread-simulator")]

use std::time::Duration;

use switchy_async::simulator::runtime::Runtime;
use switchy_thread::{
    simulator,
    sync::{Arc, Mutex, mpsc},
    thread,
};

fn run<R>(seed: u64, f: impl FnOnce(&Runtime) -> R) -> (R, u64) {
    let outcome = simulator::run(seed, || {
        let runtime = Runtime::new();
        let result = f(&runtime);
        runtime.wait().unwrap();
        result
    });
    assert!(outcome.report.leaked.is_empty(), "{:?}", outcome.report);
    let result = outcome
        .result
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
    (result, outcome.report.trace_hash)
}

#[test]
fn blocking_work_can_wait_on_an_async_task() {
    for seed in 0..20 {
        let (value, _) = run(seed, |runtime| {
            let handle = runtime.handle();
            runtime.block_on(async move {
                let (tx, rx) = mpsc::channel::<u32>();
                let doubled = handle.spawn_blocking(move || rx.recv().unwrap() * 2);
                handle.spawn(async move { tx.send(21).unwrap() });
                doubled.await.unwrap()
            })
        });
        assert_eq!(value, 42, "seed {seed}");
    }
}

#[test]
fn blocking_sleep_moves_the_run_clock() {
    let (elapsed, _) = run(1, |runtime| {
        let handle = runtime.handle();
        runtime.block_on(async move {
            let start = switchy_time::instant_now();
            let slept = handle.spawn_blocking(|| {
                thread::sleep(Duration::from_millis(50));
                7
            });
            assert_eq!(slept.await.unwrap(), 7);
            switchy_time::instant_now().duration_since(start)
        })
    });
    assert!(elapsed >= Duration::from_millis(50), "{elapsed:?}");
}

/// Blocking threads and async tasks appending to one log.
fn interleaved(runtime: &Runtime) -> Vec<u32> {
    let handle = runtime.handle();
    let log = Arc::new(Mutex::new(Vec::new()));
    runtime.block_on({
        let log = Arc::clone(&log);
        async move {
            let mut blocking = Vec::new();
            for t in 0..3 {
                let log = Arc::clone(&log);
                blocking.push(handle.spawn_blocking(move || {
                    for i in 0..4 {
                        log.lock().unwrap().push(t * 10 + i);
                        thread::yield_now();
                    }
                }));
            }
            for t in 3..5 {
                let log = Arc::clone(&log);
                handle.spawn(async move {
                    for i in 0..4 {
                        log.lock().unwrap().push(t * 10 + i);
                    }
                });
            }
            for handle in blocking {
                handle.await.unwrap();
            }
        }
    });
    runtime.clone().wait().unwrap();
    log.lock().unwrap().clone()
}

#[test]
fn interleavings_replay_by_seed() {
    let mut distinct = std::collections::BTreeSet::new();
    for seed in 0..12 {
        let first = run(seed, interleaved);
        let second = run(seed, interleaved);
        assert_eq!(first, second, "seed {seed}");
        assert_eq!(first.0.len(), 20);
        distinct.insert(first.0);
    }
    assert!(distinct.len() > 1, "every seed gave the same interleaving");
}

#[test]
fn waiting_for_the_runtime_waits_for_blocking_work() {
    let (finished, _) = run(5, |runtime| {
        let done = Arc::new(Mutex::new(false));
        let flag = Arc::clone(&done);
        drop(runtime.handle().spawn_blocking(move || {
            thread::sleep(Duration::from_millis(10));
            *flag.lock().unwrap() = true;
        }));
        runtime.clone().wait().unwrap();
        *done.lock().unwrap()
    });
    assert!(finished);
}

#[test]
fn aborting_started_blocking_work_keeps_its_result() {
    for seed in 0..10 {
        let (value, _) = run(seed, |runtime| {
            let handle = runtime.handle();
            runtime.block_on(async move {
                let (started_tx, started_rx) = mpsc::channel::<()>();
                let (go_tx, go_rx) = mpsc::channel::<()>();
                let work = handle.spawn_blocking(move || {
                    started_tx.send(()).unwrap();
                    go_rx.recv().unwrap();
                    42
                });
                // Let the executor start the work, then wait (blocking this thread, so the
                // work's thread runs) until it has.
                switchy_async::simulator::task::yield_now().await;
                started_rx.recv().unwrap();
                work.abort();
                go_tx.send(()).unwrap();
                work.await
            })
        });
        assert_eq!(value.unwrap(), 42, "seed {seed}");
    }
}

#[test]
fn aborting_queued_blocking_work_drops_it_unrun() {
    let (ran, _) = run(2, |runtime| {
        let ran = Arc::new(Mutex::new(false));
        let flag = Arc::clone(&ran);
        let work = runtime.handle().spawn_blocking(move || {
            *flag.lock().unwrap() = true;
        });
        work.abort();
        assert!(runtime.block_on(work).is_err());
        *ran.lock().unwrap()
    });
    assert!(!ran);
}

#[test]
fn block_on_wakes_for_a_future_completed_by_another_thread() {
    for seed in 0..10 {
        let (elapsed, _) = run(seed, |runtime| {
            let (tx, rx) = futures::channel::oneshot::channel::<u32>();
            // The sender lives on long after sending: only the waker can end the wait early.
            let sender = thread::spawn(move || {
                tx.send(5).unwrap();
                thread::sleep(Duration::from_secs(3600));
            });
            let start = switchy_time::instant_now();
            assert_eq!(runtime.block_on(rx).unwrap(), 5);
            let elapsed = switchy_time::instant_now().duration_since(start);
            sender.join().unwrap();
            elapsed
        });
        assert!(
            elapsed < Duration::from_secs(3600),
            "seed {seed}: {elapsed:?}"
        );
    }
}

#[test]
fn a_task_spawned_from_another_thread_wakes_the_executor() {
    for seed in 0..10 {
        let (value, _) = run(seed, |runtime| {
            let (tx, rx) = futures::channel::oneshot::channel::<u32>();
            let handle = runtime.handle();
            // Another thread hands the executor work, then lives on.
            let spawner = thread::spawn(move || {
                drop(handle.spawn(async move {
                    let _ = tx.send(9);
                }));
                thread::sleep(Duration::from_secs(3600));
            });
            let value = runtime.block_on(rx).unwrap();
            spawner.join().unwrap();
            value
        });
        assert_eq!(value, 9, "seed {seed}");
    }
}

#[test]
fn a_task_can_spawn_onto_another_runtime_and_drive_it() {
    for seed in 0..5 {
        let (value, _) = run(seed, |runtime| {
            runtime.block_on(async {
                let other = Runtime::new();
                let done = other.handle().spawn(async { 3 });
                let mut done = Box::pin(done);
                loop {
                    other.tick();
                    if let Some(value) = futures::FutureExt::now_or_never(&mut done) {
                        break value.unwrap();
                    }
                    switchy_async::task::yield_now().await;
                }
            })
        });
        assert_eq!(value, 3, "seed {seed}");
    }
}

/// Several workers (a multi-thread runtime needs `rt-multi-thread`).
#[cfg(feature = "rt-multi-thread")]
mod workers {
    use super::*;

    /// A multi-thread runtime of `workers` workers.
    fn multi(workers: usize) -> Runtime {
        let mut builder = switchy_async::Builder::new();
        builder
            .flavor(switchy_async::RuntimeFlavor::MultiThread)
            .worker_threads(workers);
        builder.build().unwrap()
    }

    /// Two tasks take two locks in opposite orders, with no await in between: only tasks running
    /// at once can interleave them. Whether the run deadlocked.
    fn opposite_lock_orders(seed: u64, workers: usize) -> bool {
        let outcome = simulator::run(seed, || {
            let runtime = multi(workers);
            let (a, b) = (Arc::new(Mutex::new(())), Arc::new(Mutex::new(())));
            runtime.block_on(async move {
                let first = {
                    let (a, b) = (Arc::clone(&a), Arc::clone(&b));
                    switchy_async::task::spawn(async move {
                        let _a = a.lock().unwrap();
                        let _b = b.lock().unwrap();
                    })
                };
                let second = switchy_async::task::spawn(async move {
                    let _b = b.lock().unwrap();
                    let _a = a.lock().unwrap();
                });
                first.await.unwrap();
                second.await.unwrap();
            });
        });
        outcome.result.is_err()
    }

    #[test]
    fn several_workers_interleave_tasks_between_their_awaits() {
        let deadlocks = |workers| {
            (0..40)
                .filter(|&seed| opposite_lock_orders(seed, workers))
                .count()
        };
        assert_eq!(
            deadlocks(1),
            0,
            "one worker runs a task to its next await alone"
        );
        assert!(deadlocks(2) > 0, "two workers interleave two tasks' locks");
    }

    /// Two tasks increment a counter by reading and then writing it, with no await in between.
    fn lost_update(seed: u64, workers: usize) -> (usize, u64) {
        let (count, trace) = run_multi(seed, workers, |runtime| {
            let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let shared =
                switchy_thread::sync::Arc::new(switchy_thread::sync::atomic::AtomicUsize::new(0));
            let _ = counter;
            runtime.block_on(async move {
                let handles: Vec<_> = (0..2)
                    .map(|_| {
                        let shared = switchy_thread::sync::Arc::clone(&shared);
                        switchy_async::task::spawn(async move {
                            use switchy_thread::sync::atomic::Ordering;
                            let value = shared.load(Ordering::SeqCst);
                            shared.store(value + 1, Ordering::SeqCst);
                        })
                    })
                    .collect();
                for handle in handles {
                    handle.await.unwrap();
                }
                shared.load(switchy_thread::sync::atomic::Ordering::SeqCst)
            })
        });
        (count, trace)
    }

    fn run_multi<R>(seed: u64, workers: usize, f: impl FnOnce(&Runtime) -> R) -> (R, u64) {
        let outcome = simulator::run(seed, || {
            let runtime = multi(workers);
            let result = f(&runtime);
            runtime.wait().unwrap();
            result
        });
        assert!(outcome.report.leaked.is_empty(), "{:?}", outcome.report);
        let result = outcome
            .result
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
        (result, outcome.report.trace_hash)
    }

    #[test]
    fn several_workers_lose_an_update_one_never_does() {
        assert!((0..40).all(|seed| lost_update(seed, 1).0 == 2));
        assert!((0..40).any(|seed| lost_update(seed, 2).0 == 1));
    }

    #[test]
    fn several_workers_replay_by_seed() {
        for seed in 0..10 {
            assert_eq!(lost_update(seed, 3), lost_update(seed, 3), "seed {seed}");
        }
    }

    #[test]
    fn shutting_down_stops_the_workers() {
        for seed in 0..5 {
            let outcome = simulator::run(seed, || {
                let runtime = multi(3);
                let handle = runtime.handle();
                runtime.block_on(async move {
                    for _ in 0..3 {
                        drop(handle.spawn(async {
                            switchy_async::time::sleep(Duration::from_secs(3600)).await;
                        }));
                    }
                    switchy_async::task::yield_now().await;
                });
                runtime.shutdown_now();
            });
            assert!(
                outcome.report.leaked.is_empty(),
                "seed {seed}: {:?}",
                outcome.report
            );
            outcome.result.unwrap();
        }
    }
}

#[cfg(feature = "rt-multi-thread")]
#[test]
fn shutting_down_a_runtime_while_another_worker_polls_its_task_does_not_wait_for_it() {
    for seed in 0..20 {
        let outcome = simulator::run(seed, || {
            let server = Runtime::new();
            let client = {
                let mut builder = switchy_async::Builder::new();
                builder
                    .flavor(switchy_async::RuntimeFlavor::MultiThread)
                    .worker_threads(2);
                builder.build().unwrap()
            };
            // A server task that blocks a while within one poll (a switch point inside).
            drop(server.handle().spawn(async {
                let lock = Arc::new(Mutex::new(()));
                for _ in 0..50 {
                    drop(lock.lock().unwrap());
                }
                switchy_async::time::sleep(Duration::from_secs(3600)).await;
            }));
            let (pumped, killed) = (server.clone(), server.clone());
            client.block_on(async move {
                let pump = switchy_async::task::spawn(async move {
                    for _ in 0..20 {
                        pumped.tick();
                        switchy_async::task::yield_now().await;
                    }
                });
                let crash = switchy_async::task::spawn(async move {
                    switchy_async::task::yield_now().await;
                    killed.shutdown_now();
                });
                pump.await.unwrap();
                crash.await.unwrap();
            });
            assert!(server.is_shut_down());
        });
        assert!(
            outcome.report.leaked.is_empty(),
            "seed {seed}: {:?}",
            outcome.report
        );
        outcome.result.unwrap();
    }
}
