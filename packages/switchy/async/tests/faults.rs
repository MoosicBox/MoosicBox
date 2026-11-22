//! Seeded executor faults: late wakeups and spurious `Pending` (see the simulator's `faults`).
#![cfg(all(
    feature = "simulator",
    feature = "macros",
    feature = "sync",
    feature = "time"
))]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    task::Poll,
    time::Duration,
};

use switchy_async::{
    simulator::{
        FaultCounts, LATE_WAKEUPS_STREAM, SPURIOUS_PENDING_STREAM, fault_counts,
        runtime::Runtime, set_late_wakeups, set_spurious_pending, set_yield_probability,
    },
    sync::{Barrier, mpmc, mpsc, watch},
    task,
};

type Log = Arc<Mutex<Vec<String>>>;

/// Run `f` in a fresh simulation run seeded with `seed`, without injected yields.
fn in_run<R>(seed: u64, f: impl FnOnce() -> R) -> R {
    switchy_simulation::with_context(switchy_simulation::Context::new(seed), || {
        set_yield_probability(0.0);
        f()
    })
}

/// The run's clock step.
fn period() -> Duration {
    Duration::from_millis(switchy_time::simulator::step_multiplier())
}

/// Drive `runtime` as the harness does, one clock step per tick, until its tasks finish.
/// Returns the ticks it took.
fn drive(runtime: &Runtime) -> usize {
    for ticks in 1..100_000 {
        let _ = switchy_time::simulator::next_step();
        runtime.tick();
        if runtime.try_finish() {
            return ticks;
        }
    }
    panic!("the runtime never finished");
}

/// Whether `stream` of the current run has drawn nothing yet: its next value is the first
/// value of the same stream of a fresh run with the same seed.
fn untouched(stream: &str) -> bool {
    let seed = switchy_simulation::seed();
    let next = switchy_simulation::stream(stream).lock().unwrap().next_u64();
    let fresh = switchy_simulation::Context::new(seed)
        .stream(stream)
        .lock()
        .unwrap()
        .next_u64();
    next == fresh
}

/// Tasks that each wake themselves `wakes` times, logging each poll.
fn self_wakers(runtime: &Runtime, log: &Log, tasks: usize, wakes: usize) {
    for t in 0..tasks {
        let log = Arc::clone(log);
        let mut left = wakes;
        drop(runtime.spawn(std::future::poll_fn(move |cx| {
            log.lock().unwrap().push(format!("{t}"));
            if left == 0 {
                return Poll::Ready(());
            }
            left -= 1;
            cx.waker().wake_by_ref();
            Poll::Pending
        })));
    }
}

/// [`self_wakers`] run to completion with late wakeup probability `probability`: the poll
/// order and the fault counts.
fn late_run(seed: u64, probability: Option<f64>) -> (String, FaultCounts) {
    in_run(seed, || {
        if let Some(probability) = probability {
            set_late_wakeups(probability);
        }
        let runtime = Runtime::new();
        let log = Log::default();
        self_wakers(&runtime, &log, 8, 100);
        runtime.clone().wait().unwrap();
        let order = log.lock().unwrap().join(" ");
        (order, fault_counts())
    })
}

/// Whether `fired / draws` is within `tolerance` of `probability`.
fn near(fired: u64, draws: u64, probability: f64, tolerance: f64) -> bool {
    #[allow(clippy::cast_precision_loss)] // counts far below 2^53
    let rate = fired as f64 / draws as f64;
    (rate - probability).abs() <= tolerance
}

#[test]
fn late_wakeups_fire_at_the_configured_rate_and_replay() {
    for seed in 0..4 {
        let (order, counts) = late_run(seed, Some(0.2));
        // 8 tasks × 100 self-wakes, each drawn while the task is not queued.
        assert_eq!(counts.late_wakeup_draws, 800, "seed {seed}");
        assert!(
            near(counts.late_wakeups, counts.late_wakeup_draws, 0.2, 0.06),
            "seed {seed}: {counts:?}"
        );
        assert_eq!(counts.spurious_pending_draws, 0);
        assert_eq!(late_run(seed, Some(0.2)), (order.clone(), counts), "seed {seed}");
        // Late wakeups reorder the tasks' polls.
        assert_ne!(late_run(seed, None).0, order, "seed {seed}");
    }
}

#[test]
fn a_late_wakeup_can_lose_to_a_timeout() {
    // A task waits for a message with a deadline three clock steps away; the message comes
    // one step after it starts waiting. On time, the message always wins. Late, the task may
    // be polled only once both are ready, and `select!` may then pick the deadline.
    fn race(seed: u64, late: f64) -> Result<u32, &'static str> {
        in_run(seed, || {
            set_late_wakeups(late);
            let runtime = Runtime::new();
            let (tx, rx) = mpmc::unbounded::<u32>();
            let deadline = period() * 3;
            let waiter = runtime.spawn(async move {
                switchy_async::select! {
                    message = rx.recv_async() => Ok(message.unwrap()),
                    () = switchy_async::time::sleep(deadline) => Err("timed out"),
                }
            });
            let mut tx = Some(tx);
            for step in 1..64 {
                let _ = switchy_time::simulator::next_step();
                if step == 2 {
                    tx.take().unwrap().send(7).unwrap();
                }
                runtime.tick();
                if runtime.try_finish() {
                    break;
                }
            }
            let other = Runtime::new();
            let outcome = other.block_on(waiter).unwrap();
            other.wait().unwrap();
            outcome
        })
    }

    for seed in 0..64 {
        assert_eq!(race(seed, 0.0), Ok(7), "seed {seed}");
    }
    let lost: Vec<u64> = (0..64).filter(|&seed| race(seed, 0.5).is_err()).collect();
    assert!(!lost.is_empty(), "no seed let the deadline win");
    for &seed in &lost {
        assert!(race(seed, 0.5).is_err(), "seed {seed} does not replay");
    }
}

/// Producers and consumers over every leaf the spurious `Pending` fault reaches: switchy's
/// own channels, bounded and `mpsc` channels, watch, barrier, sleeps, intervals and join
/// handles (sleeps and intervals only when `timed`: something must move the clock). Returns
/// what correct code computes whatever the schedule: the sum and number of everything received
/// and the barrier's leader count (the watcher must see the last value sent, whichever it is).
async fn workload(timed: bool) -> (u64, u64, usize) {
    let (flume_tx, flume_rx) = mpmc::bounded::<u64>(2);
    let (bounded_tx, mut bounded_rx) = mpsc::bounded::channel::<u64>(2);
    let (mpsc_tx, mut mpsc_rx) = mpsc::unbounded::<u64>();
    let (watch_tx, mut watch_rx) = watch::channel(0_u64);
    let barrier = Arc::new(Barrier::new(3));
    let leaders = Arc::new(AtomicUsize::new(0));

    let mut producers = Vec::new();
    for p in 0..3_u64 {
        let (flume_tx, bounded_tx, mpsc_tx) = (flume_tx.clone(), bounded_tx.clone(), mpsc_tx.clone());
        let (barrier, leaders) = (Arc::clone(&barrier), Arc::clone(&leaders));
        producers.push(task::spawn(async move {
            let mut interval = switchy_async::time::interval(Duration::from_millis(1));
            for i in 0..5 {
                if timed {
                    interval.tick().await;
                }
                flume_tx.send_async(p * 100 + i).await.unwrap();
                bounded_tx.send(p * 1000 + i).await.unwrap();
                mpsc_tx.send(p * 10_000 + i).unwrap();
                if timed {
                    switchy_async::time::sleep(Duration::from_millis(p + 1)).await;
                } else {
                    task::yield_now().await;
                }
            }
            if barrier.wait().await.is_leader() {
                leaders.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }
    drop((flume_tx, bounded_tx, mpsc_tx));
    let watcher = task::spawn(async move {
        let mut last = 0;
        while watch_rx.changed().await.is_ok() {
            last = *watch_rx.borrow_and_update();
        }
        last
    });

    let bounded = task::spawn(async move {
        let (mut sum, mut count) = (0, 0);
        while let Some(value) = bounded_rx.recv().await {
            sum += value;
            count += 1;
        }
        (sum, count)
    });
    let (mut sum, mut count) = (0, 0);
    while let Ok(value) = flume_rx.recv_async().await {
        sum += value;
        count += 1;
        watch_tx.send_replace(value);
    }
    let (bounded_sum, bounded_count) = bounded.await.unwrap();
    sum += bounded_sum;
    count += bounded_count;
    while let Ok(value) = mpsc_rx.recv_async().await {
        sum += value;
        count += 1;
    }
    for producer in producers {
        producer.await.unwrap();
    }
    let last = *watch_tx.borrow();
    drop(watch_tx);
    let watched = watcher.await.unwrap();
    assert_eq!(watched, last);
    (sum, count, leaders.load(Ordering::SeqCst))
}

/// [`workload`] driven with spurious `Pending` probability `probability`: its result, the
/// ticks it took and the fault counts.
fn spurious_run(seed: u64, probability: f64) -> ((u64, u64, usize), usize, FaultCounts) {
    in_run(seed, || {
        set_spurious_pending(probability);
        let runtime = Runtime::new();
        let result = Arc::new(Mutex::new(None));
        let out = Arc::clone(&result);
        drop(runtime.spawn(async move {
            *out.lock().unwrap() = Some(workload(true).await);
        }));
        let ticks = drive(&runtime);
        let result = result.lock().unwrap().take().unwrap();
        (result, ticks, fault_counts())
    })
}

#[test]
fn spurious_pending_fires_at_the_configured_rate_and_replays() {
    let (mut draws, mut fired) = (0, 0);
    for seed in 0..8 {
        let run = spurious_run(seed, 0.2);
        assert_eq!(spurious_run(seed, 0.2), run, "seed {seed}");
        assert_eq!(run.2.late_wakeup_draws, 0);
        draws += run.2.spurious_pending_draws;
        fired += run.2.spurious_pendings;
    }
    assert!(draws > 300, "{draws} draws");
    assert!(near(fired, draws, 0.2, 0.06), "{fired} of {draws}");
}

#[test]
fn a_spurious_pending_is_invisible_to_correct_code() {
    let mut rescheduled = false;
    for seed in 0..8 {
        let (expected, ticks, counts) = spurious_run(seed, 0.0);
        assert_eq!(counts, FaultCounts::default());
        assert_eq!(expected.1, 45, "every value received once");
        assert_eq!(expected.2, 1, "one barrier leader");
        for probability in [0.3, 1.0] {
            let (result, faulty_ticks, counts) = spurious_run(seed, probability);
            assert_eq!(result, expected, "seed {seed} at {probability}");
            assert!(counts.spurious_pendings > 0, "seed {seed} at {probability}");
            rescheduled |= faulty_ticks != ticks;
        }
    }
    assert!(rescheduled, "spurious Pendings never changed the schedule");
}

#[test]
fn faults_off_replay_as_without_them() {
    for seed in 0..4 {
        let unset = late_run(seed, None);
        assert_eq!(late_run(seed, Some(0.0)), unset, "seed {seed}");
        let (result, ticks, counts) = spurious_run(seed, 0.0);
        assert_eq!(counts, FaultCounts::default());
        let without = in_run(seed, || {
            let runtime = Runtime::new();
            let out = Arc::new(Mutex::new(None));
            let slot = Arc::clone(&out);
            drop(runtime.spawn(async move {
                *slot.lock().unwrap() = Some(workload(true).await);
            }));
            let ticks = drive(&runtime);
            assert!(untouched(LATE_WAKEUPS_STREAM) && untouched(SPURIOUS_PENDING_STREAM));
            let result = out.lock().unwrap().take().unwrap();
            (result, ticks)
        });
        assert_eq!(without, (result, ticks), "seed {seed}");
    }
}

#[test]
fn wait_and_block_on_finish_with_late_wakeups() {
    for seed in 0..8 {
        in_run(seed, || {
            set_late_wakeups(1.0);
            let runtime = Runtime::new();
            let log = Log::default();
            self_wakers(&runtime, &log, 4, 20);
            let joined = runtime.block_on({
                let handle = runtime.handle();
                async move {
                    let (tx, rx) = mpmc::unbounded::<u32>();
                    let sender = handle.spawn(async move {
                        for i in 0..10 {
                            tx.send_async(i).await.unwrap();
                            task::yield_now().await;
                        }
                    });
                    let mut sum = 0;
                    while let Ok(i) = rx.recv_async().await {
                        sum += i;
                    }
                    sender.await.unwrap();
                    sum
                }
            });
            assert_eq!(joined, 45);
            runtime.clone().wait().unwrap();
            assert!(runtime.try_finish());
            assert_eq!(log.lock().unwrap().len(), 4 * 21, "seed {seed}");
            assert!(fault_counts().late_wakeups > 0);
        });
    }
}

#[test]
fn a_late_task_counts_until_it_finishes_and_shutdown_drops_it() {
    struct DropFlag(Arc<AtomicUsize>);
    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    in_run(1, || {
        set_late_wakeups(1.0);
        let runtime = Runtime::new();
        let dropped = Arc::new(AtomicUsize::new(0));
        let polls = Arc::new(AtomicUsize::new(0));
        let flag = DropFlag(Arc::clone(&dropped));
        let counted = Arc::clone(&polls);
        let forever = runtime.spawn(async move {
            let _flag = flag;
            std::future::poll_fn(|cx| {
                counted.fetch_add(1, Ordering::SeqCst);
                cx.waker().wake_by_ref();
                Poll::<()>::Pending
            })
            .await;
        });
        // Polled once, then woken late: neither queued nor finished.
        runtime.tick();
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        runtime.tick();
        assert_eq!(polls.load(Ordering::SeqCst), 1, "a late task ran at once");
        assert!(!runtime.try_finish(), "a late task is still a task");
        // Its wait ends within the most steps a late wakeup takes.
        for _ in 0..=switchy_async::simulator::LATE_WAKEUP_MAX_STEPS {
            runtime.tick();
        }
        assert!(polls.load(Ordering::SeqCst) >= 2);
        runtime.shutdown_now();
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
        assert!(runtime.try_finish());
        let polled = polls.load(Ordering::SeqCst);
        for _ in 0..20 {
            runtime.tick();
        }
        assert_eq!(polls.load(Ordering::SeqCst), polled, "a shut-down runtime ran a task");
        let other = Runtime::new();
        assert!(other.block_on(forever).is_err());
        other.wait().unwrap();
    });
}

#[cfg(feature = "thread-simulator")]
mod threads {
    use std::time::Duration;

    use switchy_async::{
        runtime::Builder,
        simulator::{fault_counts, set_late_wakeups, set_spurious_pending},
    };
    use switchy_thread::simulator;

    use super::workload;

    /// [`workload`] on a runtime with `workers` workers, plus blocking work that sleeps on the
    /// run's clock (so the executor idles), in a `switchy_thread` run.
    fn threaded(
        seed: u64,
        workers: usize,
        faults: Option<(f64, f64)>,
    ) -> ((u64, u64, usize), u64, bool) {
        let outcome = simulator::run(seed, || {
            switchy_async::simulator::set_yield_probability(0.0);
            if let Some((late, spurious)) = faults {
                set_late_wakeups(late);
                set_spurious_pending(spurious);
            }
            let runtime = Builder::new().worker_threads(workers).build().unwrap();
            let handle = runtime.handle();
            let result = runtime.block_on(async move {
                let (tx, rx) = switchy_async::sync::mpmc::unbounded::<u64>();
                let blocking = handle.spawn_blocking(move || {
                    switchy_thread::thread::sleep(Duration::from_millis(20));
                    tx.send(5).unwrap();
                });
                let received = rx.recv_async().await.unwrap();
                blocking.await.unwrap();
                assert_eq!(received, 5);
                workload(false).await
            });
            runtime.wait().unwrap();
            let counts = fault_counts();
            (result, counts.late_wakeups + counts.spurious_pendings > 0)
        });
        assert!(outcome.report.leaked.is_empty(), "{:?}", outcome.report);
        let (result, fired) = outcome
            .result
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
        (result, outcome.report.trace_hash, fired)
    }

    #[test]
    fn faults_off_keep_the_trace_hash() {
        for seed in 0..4 {
            for workers in [1, 3] {
                let unset = threaded(seed, workers, None);
                assert!(!unset.2);
                assert_eq!(threaded(seed, workers, Some((0.0, 0.0))), unset, "seed {seed}");
            }
        }
    }

    #[test]
    fn faults_with_workers_and_idle_threads_replay_and_finish() {
        for seed in 0..4 {
            for workers in [1, 3] {
                let (expected, _, _) = threaded(seed, workers, None);
                let faulty = threaded(seed, workers, Some((0.3, 0.3)));
                assert_eq!(faulty.0, expected, "seed {seed}, {workers} workers");
                assert!(faulty.2, "seed {seed}: no fault fired");
                assert_eq!(threaded(seed, workers, Some((0.3, 0.3))), faulty);
            }
        }
    }
}
