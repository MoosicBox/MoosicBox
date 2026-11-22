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
