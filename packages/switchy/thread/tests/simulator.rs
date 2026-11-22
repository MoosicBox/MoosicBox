//! Behaviour of the deterministic scheduler.
#![cfg(feature = "simulator")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::time::Duration;

use switchy_thread::{
    simulator::{self, Config, Outcome},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
};

/// Three threads appending to a shared log: the interleaving depends on the seed.
fn interleaving() -> Vec<u32> {
    let log = Arc::new(Mutex::new(Vec::new()));
    let handles: Vec<_> = (0..3)
        .map(|t| {
            let log = Arc::clone(&log);
            thread::spawn(move || {
                for i in 0..5 {
                    log.lock().unwrap().push(t * 10 + i);
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    Arc::try_unwrap(log).unwrap().into_inner().unwrap()
}

fn outcome<R>(outcome: Outcome<R>) -> (R, u64) {
    assert!(outcome.report.leaked.is_empty(), "{:?}", outcome.report);
    let hash = outcome.report.trace_hash;
    (outcome.result.unwrap(), hash)
}

#[test]
fn a_seed_replays_exactly() {
    for seed in 0..20 {
        let (a, hash_a) = outcome(simulator::run(seed, interleaving));
        let (b, hash_b) = outcome(simulator::run(seed, interleaving));
        assert_eq!(a, b, "seed {seed}");
        assert_eq!(hash_a, hash_b, "seed {seed}");
    }
}

#[test]
fn different_seeds_explore_different_interleavings() {
    let distinct: std::collections::BTreeSet<Vec<u32>> = (0..40)
        .map(|seed| outcome(simulator::run(seed, interleaving)).0)
        .collect();
    assert!(distinct.len() > 5, "only {} interleavings", distinct.len());
}

#[test]
fn a_mutex_excludes_and_a_counter_is_exact() {
    for seed in 0..10 {
        let (count, _) = outcome(simulator::run(seed, || {
            let counter = Arc::new(Mutex::new(0_u32));
            let handles: Vec<_> = (0..4)
                .map(|_| {
                    let counter = Arc::clone(&counter);
                    thread::spawn(move || {
                        for _ in 0..10 {
                            let mut guard = counter.lock().unwrap();
                            let value = *guard;
                            thread::yield_now();
                            *guard = value + 1;
                        }
                    })
                })
                .collect();
            for handle in handles {
                handle.join().unwrap();
            }
            *counter.lock().unwrap()
        }));
        assert_eq!(count, 40);
    }
}

#[test]
fn an_unsynchronized_read_modify_write_race_is_found() {
    // load + store is not atomic: some seed must interleave two threads between them.
    let lost_update = (0..200).any(|seed| {
        let (value, _) = outcome(simulator::run_with(Config::random(), seed, || {
            let counter = Arc::new(AtomicUsize::new(0));
            let handles: Vec<_> = (0..2)
                .map(|_| {
                    let counter = Arc::clone(&counter);
                    thread::spawn(move || {
                        let value = counter.load(Ordering::SeqCst);
                        counter.store(value + 1, Ordering::SeqCst);
                    })
                })
                .collect();
            for handle in handles {
                handle.join().unwrap();
            }
            counter.load(Ordering::SeqCst)
        }));
        value == 1
    });
    assert!(lost_update, "no seed exposed the lost update");
}

#[test]
fn bounded_channels_apply_backpressure_and_keep_order() {
    for seed in 0..10 {
        let (received, _) = outcome(simulator::run(seed, || {
            let (tx, rx) = mpsc::sync_channel(2);
            let producer = thread::spawn(move || {
                for i in 0..20 {
                    tx.send(i).unwrap();
                }
            });
            let received: Vec<i32> = rx.iter().collect();
            producer.join().unwrap();
            received
        }));
        assert_eq!(received, (0..20).collect::<Vec<_>>());
    }
}

#[test]
fn timed_waits_follow_simulated_time() {
    let (result, _) = outcome(simulator::run(1, || {
        let (tx, rx) = mpsc::channel::<()>();
        let start = switchy_time::simulator::instant_now();
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(30)),
            Err(mpsc::RecvTimeoutError::Timeout)
        );
        let waited = switchy_time::simulator::instant_now() - start;
        drop(tx);
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(30)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        );
        waited
    }));
    assert!(result >= Duration::from_secs(30), "{result:?}");
}

#[test]
fn sleeping_threads_never_wake_early_and_wake_in_deadline_order() {
    for seed in 0..10 {
        let (woken, _) = outcome(simulator::run(seed, || {
            let start = switchy_time::simulator::instant_now();
            let woken = Arc::new(Mutex::new(Vec::new()));
            let handles: Vec<_> = [300_u64, 100, 200]
                .into_iter()
                .map(|ms| {
                    let woken = Arc::clone(&woken);
                    thread::spawn(move || {
                        thread::sleep(Duration::from_millis(ms));
                        let at = switchy_time::simulator::instant_now() - start;
                        woken.lock().unwrap().push((ms, at));
                    })
                })
                .collect();
            for handle in handles {
                handle.join().unwrap();
            }
            Arc::try_unwrap(woken).unwrap().into_inner().unwrap()
        }));
        assert_eq!(woken.len(), 3);
        for &(ms, at) in &woken {
            assert!(
                at >= Duration::from_millis(ms),
                "seed {seed}: {ms}ms woke at {at:?}"
            );
        }
        // Time only moves forward, and a later deadline never wakes at an earlier time.
        for pair in woken.windows(2) {
            assert!(pair[0].1 <= pair[1].1, "seed {seed}: {woken:?}");
        }
    }
}

#[test]
fn condvars_deliver_notifications() {
    for seed in 0..10 {
        let (value, _) = outcome(simulator::run(seed, || {
            let pair = Arc::new((Mutex::new(false), Condvar::new()));
            let notifier = {
                let pair = Arc::clone(&pair);
                thread::spawn(move || {
                    *pair.0.lock().unwrap() = true;
                    pair.1.notify_all();
                })
            };
            let guard = pair
                .1
                .wait_while(pair.0.lock().unwrap(), |ready| !*ready)
                .unwrap();
            let value = *guard;
            drop(guard);
            notifier.join().unwrap();
            value
        }));
        assert!(value);
    }
}

#[test]
fn a_deadlock_is_reported_with_every_thread() {
    for seed in 0..10 {
        let outcome = simulator::run(seed, || {
            // `other` waits for a message main never sends; main waits for `other` to finish.
            let (tx, rx) = mpsc::channel::<()>();
            let other = thread::Builder::new()
                .name("other".into())
                .spawn(move || {
                    let _ = rx.recv();
                })
                .unwrap();
            other.join().unwrap();
            drop(tx);
        });
        let panic = outcome.result.unwrap_err();
        let message = panic.downcast_ref::<String>().unwrap();
        assert!(message.contains("deadlock"), "{message}");
        assert!(message.contains("\"other\""), "{message}");
        assert!(message.contains("Recv"), "{message}");
        assert_eq!(outcome.report.leaked, vec!["other".to_owned()]);
    }
}

#[test]
fn pct_preempts_within_its_step_range() {
    // With change points drawn from the first few decisions, PCT finds the race too.
    let lost_update = (0..200).any(|seed| {
        let (value, _) = outcome(simulator::run_with(Config::pct(2, 8), seed, || {
            let counter = Arc::new(AtomicUsize::new(0));
            let handles: Vec<_> = (0..2)
                .map(|_| {
                    let counter = Arc::clone(&counter);
                    thread::spawn(move || {
                        let value = counter.load(Ordering::SeqCst);
                        counter.store(value + 1, Ordering::SeqCst);
                    })
                })
                .collect();
            for handle in handles {
                handle.join().unwrap();
            }
            counter.load(Ordering::SeqCst)
        }));
        value == 1
    });
    assert!(lost_update, "no seed exposed the lost update");
}

#[test]
fn a_panicking_thread_reports_through_join() {
    let (joined, _) = outcome(simulator::run(5, || {
        thread::spawn(|| panic!("boom")).join().is_err()
    }));
    assert!(joined);
}

#[test]
fn unjoined_threads_that_never_finish_are_reported_as_leaked() {
    let outcome = simulator::run(7, || {
        let (tx, rx) = mpsc::channel::<()>();
        let rx = Arc::new(Mutex::new(rx));
        thread::Builder::new()
            .name("stuck".into())
            .spawn(move || {
                let _ = rx.lock().unwrap().recv();
            })
            .unwrap();
        thread::yield_now();
        // The sender stays alive, so the receiver stays blocked.
        std::mem::forget(tx);
    });
    assert!(outcome.result.is_ok());
    assert_eq!(outcome.report.leaked, vec!["stuck".to_owned()]);
}

#[test]
fn run_until_idle_lets_others_run_then_returns() {
    let (seen, _) = outcome(simulator::run(9, || {
        let (tx, rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            for i in 0..3 {
                tx.send(i).unwrap();
            }
        });
        simulator::run_until_idle();
        let seen: Vec<i32> = rx.try_iter().collect();
        worker.join().unwrap();
        seen
    }));
    assert_eq!(seen, vec![0, 1, 2]);
}

#[test]
fn a_decision_hook_sees_every_decision_without_changing_the_run() {
    use std::sync::atomic::AtomicU64;
    let plain = outcome(simulator::run(4, interleaving));
    let seen = std::sync::Arc::new(AtomicU64::new(0));
    let counted = std::sync::Arc::clone(&seen);
    let observed = outcome(simulator::run(4, move || {
        simulator::set_decision_hook(Some(std::sync::Arc::new(
            move |decision: &simulator::Decision,
                  snapshot: &dyn Fn() -> simulator::SchedulerSnapshot| {
                assert_eq!(snapshot().decisions, decision.number + 1);
                counted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            },
        )));
        interleaving()
    }));
    assert_eq!(plain, observed, "observing must not change the run");
    assert!(seen.load(std::sync::atomic::Ordering::Relaxed) > 0);
}

#[test]
fn both_strategies_are_deterministic() {
    for config in [Config::random(), Config::pct(3, 30)] {
        for seed in 0..10 {
            let first = outcome(simulator::run_with(config.clone(), seed, interleaving));
            let second = outcome(simulator::run_with(config.clone(), seed, interleaving));
            assert_eq!(first, second, "{config:?} seed {seed}");
        }
    }
}

fn weak() -> Config {
    Config {
        weak_memory: true,
        ..Config::random()
    }
}

/// Message passing: data written, then a flag; a reader seeing the flag reads the data.
fn message_passing(flag_order: (Ordering, Ordering)) -> Vec<(bool, usize)> {
    (0..300)
        .map(|seed| {
            outcome(simulator::run_with(weak(), seed, move || {
                let data = Arc::new(AtomicUsize::new(0));
                let flag = Arc::new(switchy_thread::sync::atomic::AtomicBool::new(false));
                let writer = {
                    let (data, flag) = (Arc::clone(&data), Arc::clone(&flag));
                    thread::spawn(move || {
                        data.store(42, Ordering::Relaxed);
                        flag.store(true, flag_order.0);
                    })
                };
                let reader = thread::spawn(move || {
                    let seen = flag.load(flag_order.1);
                    (seen, data.load(Ordering::Relaxed))
                });
                writer.join().unwrap();
                reader.join().unwrap()
            }))
            .0
        })
        .collect()
}

#[test]
fn weak_memory_lets_relaxed_message_passing_see_stale_data() {
    let results = message_passing((Ordering::Relaxed, Ordering::Relaxed));
    assert!(
        results.contains(&(true, 0)),
        "relaxed flags must allow seeing the flag without the data"
    );
}

#[test]
fn release_acquire_message_passing_always_sees_the_data() {
    let results = message_passing((Ordering::Release, Ordering::Acquire));
    assert!(!results.contains(&(true, 0)), "{results:?}");
    assert!(results.contains(&(true, 42)));
}

/// Store buffering: each thread stores, then loads the other's location.
fn store_buffering(order: Ordering) -> Vec<(usize, usize)> {
    (0..300)
        .map(|seed| {
            outcome(simulator::run_with(weak(), seed, move || {
                let x = Arc::new(AtomicUsize::new(0));
                let y = Arc::new(AtomicUsize::new(0));
                let a = {
                    let (x, y) = (Arc::clone(&x), Arc::clone(&y));
                    thread::spawn(move || {
                        x.store(1, order);
                        y.load(order)
                    })
                };
                let b = thread::spawn(move || {
                    y.store(1, order);
                    x.load(order)
                });
                (a.join().unwrap(), b.join().unwrap())
            }))
            .0
        })
        .collect()
}

#[test]
fn weak_memory_allows_relaxed_store_buffering_but_not_seq_cst() {
    assert!(store_buffering(Ordering::Relaxed).contains(&(0, 0)));
    assert!(!store_buffering(Ordering::SeqCst).contains(&(0, 0)));
}

#[test]
fn weak_memory_runs_replay_exactly() {
    for seed in 0..20 {
        let first = outcome(simulator::run_with(weak(), seed, interleaving));
        let second = outcome(simulator::run_with(weak(), seed, interleaving));
        assert_eq!(first, second);
    }
}

/// Two workers increment a counter with a separate load and store, among threads that only
/// yield: an update is lost when one worker runs between the other's load and store.
fn lost_update() -> usize {
    let counter = Arc::new(AtomicUsize::new(0));
    let noise: Vec<_> = (0..3)
        .map(|_| {
            thread::spawn(|| {
                for _ in 0..10 {
                    thread::yield_now();
                }
            })
        })
        .collect();
    let workers: Vec<_> = (0..2)
        .map(|_| {
            let counter = Arc::clone(&counter);
            thread::spawn(move || {
                let seen = counter.load(Ordering::SeqCst);
                thread::yield_now();
                counter.store(seen + 1, Ordering::SeqCst);
            })
        })
        .collect();
    for handle in noise.into_iter().chain(workers) {
        handle.join().unwrap();
    }
    counter.load(Ordering::SeqCst)
}

#[test]
fn replaying_a_recorded_schedule_reproduces_the_run() {
    let weak = Config {
        weak_memory: true,
        ..Config::random()
    };
    for config in [Config::random(), Config::pct(3, 30), weak] {
        for seed in 0..20 {
            let recorded = simulator::run_with(config.clone(), seed, interleaving);
            let schedule = recorded.report.schedule.clone();
            let replay = Config {
                weak_memory: config.weak_memory,
                ..Config::replay(schedule.clone())
            };
            let replayed = simulator::run_with(replay, seed, interleaving);
            assert_eq!(
                outcome(recorded),
                outcome(replayed),
                "{config:?} seed {seed} schedule {schedule}"
            );
        }
    }
}

#[test]
fn the_default_schedule_switches_only_when_it_must() {
    let run = simulator::run_with(Config::replay(simulator::Schedule::new()), 1, interleaving);
    assert_eq!(run.report.schedule, simulator::Schedule::new());
    let (log, _) = outcome(run);
    // Each worker runs to completion before the next.
    assert_eq!(log, [0, 1, 2, 3, 4, 10, 11, 12, 13, 14, 20, 21, 22, 23, 24]);
}

#[test]
fn schedules_round_trip_through_text() {
    let schedule: simulator::Schedule = "17=2, 3=1".parse().unwrap();
    assert_eq!(schedule.choices, [(3, 1), (17, 2)]);
    assert_eq!(schedule.to_string(), "3=1,17=2");
    assert_eq!(
        "none".parse::<simulator::Schedule>().unwrap(),
        simulator::Schedule::new()
    );
    assert!("3".parse::<simulator::Schedule>().is_err());
}

#[test]
fn shrinking_leaves_only_switches_the_failure_needs() {
    let loses = |schedule: &simulator::Schedule, seed| {
        let run = simulator::run_with(Config::replay(schedule.clone()), seed, lost_update);
        (*run.result.as_ref().unwrap() != 2).then_some(run.report.schedule)
    };
    let failing: Vec<_> = (0..200)
        .map(|seed| {
            (
                seed,
                simulator::run_with(Config::random(), seed, lost_update),
            )
        })
        .filter(|(_, run)| run.result.as_ref().is_ok_and(|&count| count != 2))
        .take(5)
        .collect();
    assert!(!failing.is_empty(), "some seed loses an update");
    for (seed, run) in failing {
        let original = run.report.schedule;
        let shrunk = simulator::shrink(&original, simulator::SHRINK_ATTEMPTS, |candidate| {
            loses(candidate, seed)
        });
        assert!(
            shrunk.choices.len() < original.choices.len(),
            "seed {seed}: {original} did not shrink"
        );
        assert!(
            loses(&shrunk, seed).is_some(),
            "seed {seed}: {shrunk} passes"
        );
        for i in 0..shrunk.choices.len() {
            let mut fewer = shrunk.clone();
            fewer.choices.remove(i);
            assert!(
                loses(&fewer, seed).is_none(),
                "seed {seed}: {shrunk} still fails without choice {i}"
            );
        }
        eprintln!(
            "seed {seed}: {} choices shrank to {shrunk}",
            original.choices.len()
        );
    }
}

#[test]
fn try_scheduler_finds_only_the_run_s_own_threads() {
    let ((found, others), _) = outcome(simulator::run(3, || {
        // `run` makes this thread the run's main thread.
        assert!(simulator::try_scheduler().is_some());
        let scheduler = simulator::scheduler();
        assert!(!scheduler.others_alive());
        let (tx, rx) = mpsc::channel::<()>();
        let worker = thread::spawn(move || {
            let inside = simulator::try_scheduler().is_some();
            rx.recv().unwrap();
            inside
        });
        let alive = scheduler.others_alive();
        let outside = std::thread::spawn(|| simulator::try_scheduler().is_none())
            .join()
            .unwrap();
        tx.send(()).unwrap();
        let inside = worker.join().unwrap();
        (inside && outside, alive && !scheduler.others_alive())
    }));
    assert!(found);
    assert!(others);
}

/// Records a drop.
struct DropFlag(Arc<AtomicUsize>);

impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// A worker in `group` that counts its loop iterations and holds a [`DropFlag`].
fn worker(
    group: Option<u64>,
    progress: &Arc<AtomicUsize>,
    dropped: &Arc<AtomicUsize>,
) -> thread::JoinHandle<()> {
    let progress = Arc::clone(progress);
    let flag = DropFlag(Arc::clone(dropped));
    let builder = thread::Builder::new();
    let builder = match group {
        Some(group) => builder.group(group),
        None => builder,
    };
    builder
        .spawn(move || {
            let _flag = flag;
            loop {
                progress.fetch_add(1, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(1));
            }
        })
        .unwrap()
}

/// Kill a group of two busy threads (one with a child) while another thread keeps running.
fn kill_scenario() -> (usize, usize, usize, bool) {
    let group = simulator::new_key();
    let progress = Arc::new(AtomicUsize::new(0));
    let dropped = Arc::new(AtomicUsize::new(0));
    let first = worker(Some(group), &progress, &dropped);
    // A child inherits its parent's group.
    let child_progress = Arc::clone(&progress);
    let child_dropped = Arc::clone(&dropped);
    let _parent = thread::Builder::new()
        .group(group)
        .spawn(move || {
            let _child = worker(None, &child_progress, &child_dropped);
            thread::sleep(Duration::from_secs(3600));
        })
        .unwrap();
    // Not in the group: survives.
    let survivor_progress = Arc::new(AtomicUsize::new(0));
    let survivor = {
        let progress = Arc::clone(&survivor_progress);
        thread::spawn(move || {
            for _ in 0..50 {
                progress.fetch_add(1, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(1));
            }
        })
    };
    // A killed thread that never started drops its closure without running it.
    let never_started = Arc::new(AtomicUsize::new(0));
    let unstarted_dropped = Arc::new(AtomicUsize::new(0));
    let unstarted = {
        let ran = Arc::clone(&never_started);
        let flag = DropFlag(Arc::clone(&unstarted_dropped));
        thread::Builder::new()
            .group(group)
            .spawn(move || {
                let _flag = flag;
                ran.fetch_add(1, Ordering::SeqCst);
            })
            .unwrap()
    };
    let _ = unstarted;

    thread::sleep(Duration::from_millis(5));
    simulator::scheduler().kill_group(group);
    let frozen = progress.load(Ordering::SeqCst);
    thread::sleep(Duration::from_millis(20));
    let joined = first.join();
    survivor.join().unwrap();
    assert_eq!(
        progress.load(Ordering::SeqCst),
        frozen,
        "a killed thread ran"
    );
    assert_eq!(survivor_progress.load(Ordering::SeqCst), 50);
    (
        dropped.load(Ordering::SeqCst),
        unstarted_dropped.load(Ordering::SeqCst),
        never_started.load(Ordering::SeqCst),
        joined.is_err_and(|payload| payload.is::<simulator::Killed>()),
    )
}

#[test]
fn killing_a_group_unwinds_its_threads_and_no_other() {
    for seed in 0..10 {
        let outcome = simulator::run(seed, kill_scenario);
        assert!(outcome.report.leaked.is_empty(), "{:?}", outcome.report);
        assert!(outcome.report.panicked.is_empty(), "{:?}", outcome.report);
        let (dropped, unstarted_dropped, unstarted_ran, joined_killed) = outcome.result.unwrap();
        // The first worker and the parent's child (which inherits the group) dropped their
        // flags.
        assert_eq!(dropped, 2, "seed {seed}");
        assert!(joined_killed, "seed {seed}");
        if unstarted_ran == 0 {
            assert_eq!(unstarted_dropped, 1, "seed {seed}");
        }
        let again = simulator::run(seed, kill_scenario);
        assert_eq!(
            outcome.report.trace_hash, again.report.trace_hash,
            "seed {seed}"
        );
    }
}

#[test]
fn a_killed_thread_blocking_in_a_destructor_is_parked_for_good() {
    struct BlockOnDrop(Arc<Mutex<()>>);
    impl Drop for BlockOnDrop {
        fn drop(&mut self) {
            let _guard = self.0.lock();
        }
    }
    let outcome = simulator::run(1, || {
        let group = simulator::new_key();
        let lock = Arc::new(Mutex::new(()));
        let held = lock.lock().unwrap();
        let victim_lock = Arc::clone(&lock);
        let _victim = thread::Builder::new()
            .group(group)
            .spawn(move || {
                let _blocker = BlockOnDrop(victim_lock);
                thread::sleep(Duration::from_secs(3600));
            })
            .unwrap();
        thread::sleep(Duration::from_millis(1));
        simulator::scheduler().kill_group(group);
        drop(held);
    });
    outcome.result.unwrap();
    assert!(outcome.report.leaked.is_empty(), "{:?}", outcome.report);
}
