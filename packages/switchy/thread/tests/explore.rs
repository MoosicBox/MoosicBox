//! Exhaustive, preemption-bounded exploration of short scenarios.

#![cfg(feature = "simulator")]

use switchy_thread::{
    simulator::{self, Config, ExploreOptions},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};

/// `threads` threads each increment a counter with a separate load and store; fails when the
/// counter ends at `lost_at` or below.
fn racy_increments(threads: usize, lost_at: usize) -> Result<(), String> {
    let counter = Arc::new(AtomicUsize::new(0));
    let workers: Vec<_> = (0..threads)
        .map(|_| {
            let counter = Arc::clone(&counter);
            thread::spawn(move || {
                let value = counter.load(Ordering::SeqCst);
                counter.store(value + 1, Ordering::SeqCst);
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    let total = counter.load(Ordering::SeqCst);
    if total <= lost_at {
        return Err(format!("lost updates: {total} of {threads}"));
    }
    Ok(())
}

#[test]
fn a_lost_update_is_found_within_one_preemption() {
    for prune in [true, false] {
        for seed in 0..4 {
            let options = ExploreOptions::new(1).prune(prune);
            let exploration = simulator::explore(seed, &options, || racy_increments(2, 1));
            let failure = exploration
                .failure
                .unwrap_or_else(|| panic!("not found (prune {prune}, seed {seed})"));
            assert!(failure.message.contains("lost updates: 1 of 2"));
            assert!(!exploration.complete);
        }
    }
}

#[test]
fn a_lost_update_needs_a_preemption() {
    let exploration = simulator::explore(0, &ExploreOptions::new(0), || racy_increments(2, 1));
    assert_eq!(exploration.failure, None);
    assert!(exploration.complete);
}

/// Three threads each increment a counter with a separate load and store; fails when all
/// three loaded 0 (each loaded before any stored), which needs two preemptions: one alone
/// lets the others finish (losing two updates, but not with three stale loads).
fn three_stale_loads() -> Result<(), String> {
    let counter = Arc::new(AtomicUsize::new(0));
    let stale = Arc::new(AtomicUsize::new(0));
    let workers: Vec<_> = (0..3)
        .map(|_| {
            let (counter, stale) = (Arc::clone(&counter), Arc::clone(&stale));
            thread::spawn(move || {
                let value = counter.load(Ordering::SeqCst);
                if value == 0 {
                    stale.fetch_add(1, Ordering::SeqCst);
                }
                counter.store(value + 1, Ordering::SeqCst);
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    if stale.load(Ordering::SeqCst) == 3 {
        return Err("all three loads were stale".to_owned());
    }
    Ok(())
}

#[test]
fn a_bug_needing_two_preemptions_is_found_at_two_but_not_at_one() {
    for prune in [true, false] {
        let one = simulator::explore(0, &ExploreOptions::new(1).prune(prune), three_stale_loads);
        assert_eq!(one.failure, None, "prune {prune}");
        assert!(one.complete);
        let two = simulator::explore(0, &ExploreOptions::new(2).prune(prune), three_stale_loads);
        let failure = two.failure.expect("found at two preemptions");
        assert!(failure.message.contains("stale"), "{failure:?}");
    }
}

/// Two threads take two locks in opposite orders.
fn opposite_order_locks() -> Result<(), String> {
    let a = Arc::new(Mutex::new(0));
    let b = Arc::new(Mutex::new(0));
    let first = {
        let (a, b) = (Arc::clone(&a), Arc::clone(&b));
        thread::spawn(move || {
            let _a = a.lock().unwrap();
            let _b = b.lock().unwrap();
        })
    };
    let second = thread::spawn(move || {
        let _b = b.lock().unwrap();
        let _a = a.lock().unwrap();
    });
    first.join().unwrap();
    second.join().unwrap();
    Ok(())
}

#[test]
fn an_opposite_order_lock_deadlock_is_found() {
    for prune in [true, false] {
        let exploration = simulator::explore(0, &ExploreOptions::new(1).prune(prune), || {
            opposite_order_locks()
        });
        let failure = exploration.failure.expect("deadlock found");
        assert!(failure.message.contains("deadlock"), "{failure:?}");
    }
}

/// Correct code: increments under a lock and with `fetch_add`.
fn correct_increments() -> Result<(), String> {
    let locked = Arc::new(Mutex::new(0));
    let atomic = Arc::new(AtomicUsize::new(0));
    let workers: Vec<_> = (0..3)
        .map(|_| {
            let (locked, atomic) = (Arc::clone(&locked), Arc::clone(&atomic));
            thread::spawn(move || {
                *locked.lock().unwrap() += 1;
                atomic.fetch_add(1, Ordering::SeqCst);
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    let (locked, atomic) = (*locked.lock().unwrap(), atomic.load(Ordering::SeqCst));
    if (locked, atomic) == (3, 3) {
        Ok(())
    } else {
        Err(format!("counted {locked} and {atomic}"))
    }
}

#[test]
fn correct_code_explores_fully_with_a_stable_count() {
    for prune in [true, false] {
        let options = ExploreOptions::new(2).prune(prune);
        let first = simulator::explore(0, &options, correct_increments);
        assert_eq!(first.failure, None);
        assert!(first.complete);
        assert!(first.runs > 1);
        let second = simulator::explore(7, &options, correct_increments);
        assert_eq!(first, second, "the same space whatever the seed");
    }
}

#[test]
fn more_preemptions_explore_more_schedules() {
    let runs: Vec<usize> = (0..=2)
        .map(|bound| {
            let exploration =
                simulator::explore(0, &ExploreOptions::new(bound).prune(false), || {
                    racy_increments(2, 0)
                });
            assert!(exploration.complete);
            exploration.runs
        })
        .collect();
    assert!(runs[0] < runs[1] && runs[1] < runs[2], "{runs:?}");
}

#[test]
fn a_run_limit_stops_the_exploration() {
    let options = ExploreOptions::new(2).prune(false).max_runs(3);
    let exploration = simulator::explore(0, &options, correct_increments);
    assert_eq!(exploration.runs, 3);
    assert!(!exploration.complete);
    assert_eq!(exploration.failure, None);
}

#[test]
fn the_failing_schedule_replays_the_failure() {
    for bound in [1, 2] {
        let scenario = || {
            if bound == 1 {
                racy_increments(2, 1)
            } else {
                three_stale_loads()
            }
        };
        let failure = simulator::explore(3, &ExploreOptions::new(bound), scenario)
            .failure
            .expect("a failure");
        for _ in 0..3 {
            let replayed =
                simulator::run_with(Config::replay(failure.schedule.clone()), 3, scenario);
            assert_eq!(replayed.result.unwrap(), Err(failure.message.clone()));
            assert_eq!(replayed.report.schedule, failure.schedule);
        }
        // Its text form (what `SIMULATOR_SCHEDULE` takes) is the same schedule.
        let text = failure.schedule.to_string();
        assert_eq!(
            text.parse::<simulator::Schedule>().unwrap(),
            failure.schedule
        );
    }
}

#[test]
fn a_deadlock_schedule_replays_the_deadlock() {
    let failure = simulator::explore(0, &ExploreOptions::new(1), opposite_order_locks)
        .failure
        .expect("a deadlock");
    let replayed = simulator::run_with(Config::replay(failure.schedule), 0, opposite_order_locks);
    let panic = replayed.result.expect_err("deadlocks again");
    let message = panic.downcast_ref::<String>().expect("a message");
    assert!(message.contains("deadlock"), "{message}");
}

/// Threads that each work on their own lock and atomic, touching nothing another touches.
fn independent_work() -> Result<(), String> {
    let workers: Vec<_> = (0..3)
        .map(|_| {
            let lock = Arc::new(Mutex::new(0));
            let atomic = Arc::new(AtomicUsize::new(0));
            thread::spawn(move || {
                *lock.lock().unwrap() += 1;
                atomic.fetch_add(1, Ordering::SeqCst);
                *lock.lock().unwrap() += 1;
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    Ok(())
}

#[test]
fn pruning_skips_reorderings_of_independent_operations() {
    for bound in [1, 2] {
        let full = simulator::explore(
            0,
            &ExploreOptions::new(bound).prune(false),
            independent_work,
        );
        let pruned = simulator::explore(0, &ExploreOptions::new(bound), independent_work);
        assert!(full.complete && pruned.complete);
        assert_eq!(full.pruned, 0);
        assert!(pruned.pruned > 0, "bound {bound}: {pruned:?}");
        assert!(
            pruned.runs < full.runs,
            "bound {bound}: {} pruned runs, {} without",
            pruned.runs,
            full.runs
        );
    }
}

#[test]
fn pruning_keeps_every_outcome_of_dependent_operations() {
    // Every final order of three appends to one shared log is reachable with two
    // preemptions; pruning must not lose any.
    let outcomes = |prune: bool| {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(std::collections::BTreeSet::new()));
        let record = std::sync::Arc::clone(&seen);
        let exploration = simulator::explore(0, &ExploreOptions::new(2).prune(prune), move || {
            let log = Arc::new(Mutex::new(Vec::new()));
            let workers: Vec<_> = (0..3)
                .map(|i| {
                    let log = Arc::clone(&log);
                    thread::spawn(move || log.lock().unwrap().push(i))
                })
                .collect();
            for worker in workers {
                worker.join().unwrap();
            }
            let order = log.lock().unwrap().clone();
            record.lock().unwrap().insert(order);
            Ok::<_, String>(())
        });
        assert!(exploration.complete);
        let seen = seen.lock().unwrap().clone();
        (seen, exploration.runs)
    };
    let (full, full_runs) = outcomes(false);
    let (pruned, pruned_runs) = outcomes(true);
    assert_eq!(full.len(), 6, "{full:?}");
    assert_eq!(pruned, full);
    assert!(pruned_runs <= full_runs);
}

#[test]
fn a_traced_run_records_every_decision() {
    let outcome = simulator::run_with(Config::explore(simulator::Schedule::new()), 0, || {
        racy_increments(2, 0)
    });
    let trace = outcome.report.trace.expect("traced");
    assert_eq!(trace.decisions() as u64, outcome.report.decisions);
    assert!(trace.choice_points() > 0);
    assert!(!trace.untracked());
    let untraced = simulator::run_with(Config::random(), 0, || racy_increments(2, 0));
    assert_eq!(untraced.report.trace, None);
}

#[test]
fn a_run_using_public_switch_points_is_untracked() {
    let outcome = simulator::run_with(Config::explore(simulator::Schedule::new()), 0, || {
        simulator::scheduler().yield_point(simulator::Op::Io);
    });
    assert!(outcome.report.trace.expect("traced").untracked());
}
