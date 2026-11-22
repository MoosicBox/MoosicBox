//! Interleaving coverage: the ordered pairs of conflicting operations a run made.
#![cfg(feature = "simulator")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::collections::BTreeSet;

use switchy_thread::{
    simulator::{self, Config, Coverage, WaitKey},
    sync::{
        Arc, Condvar, Mutex, RwLock,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
};

fn covering(config: Config) -> Config {
    Config {
        interleaving_coverage: true,
        ..config
    }
}

/// The lines of the pairs whose sites are both in this file.
fn lines(coverage: &Coverage) -> BTreeSet<(u32, u32)> {
    coverage
        .pairs()
        .filter(|(a, b)| a.file() == file!() && b.file() == file!())
        .map(|(a, b)| (a.line(), b.line()))
        .collect()
}

/// Two threads taking one lock, each from its own line: the lines of the two locks.
fn race() -> (u32, u32) {
    let lock = Arc::new(Mutex::new(0));
    let theirs = Arc::clone(&lock);
    let child = thread::spawn(move || {
        let (mut guard, line) = (theirs.lock().unwrap(), line!());
        *guard += 1;
        line
    });
    let (mut guard, mine) = (lock.lock().unwrap(), line!());
    *guard += 1;
    drop(guard);
    (child.join().unwrap(), mine)
}

#[test]
fn two_threads_racing_on_a_lock_record_both_orders_across_seeds() {
    let mut orders = BTreeSet::new();
    for seed in 0..40 {
        let outcome = simulator::run_with(covering(Config::random()), seed, race);
        let (theirs, mine) = outcome.result.unwrap();
        let coverage = outcome.report.coverage.expect("coverage was recorded");
        let pairs = lines(&coverage);
        let forward = pairs.contains(&(theirs, mine));
        let backward = pairs.contains(&(mine, theirs));
        assert!(forward || backward, "seed {seed}: {coverage}");
        if forward {
            orders.insert("theirs first");
        }
        if backward {
            orders.insert("mine first");
        }
    }
    assert_eq!(orders.len(), 2, "only {orders:?}");
}

#[test]
fn reads_that_do_not_conflict_record_nothing() {
    for seed in 0..20 {
        let outcome = simulator::run_with(covering(Config::random()), seed, || {
            let value = Arc::new(AtomicUsize::new(7));
            let theirs = Arc::clone(&value);
            let child = thread::spawn(move || {
                let (_, line) = (theirs.load(Ordering::SeqCst), line!());
                line
            });
            let (_, mine) = (value.load(Ordering::SeqCst), line!());
            (child.join().unwrap(), mine)
        });
        let (theirs, mine) = outcome.result.unwrap();
        let coverage = outcome.report.coverage.unwrap();
        assert!(
            lines(&coverage)
                .iter()
                .all(|&(a, b)| ![a, b].contains(&theirs) && ![a, b].contains(&mine)),
            "seed {seed}: {coverage}"
        );
    }
}

#[test]
fn a_read_after_another_threads_write_is_a_pair() {
    let mut seen = BTreeSet::new();
    for seed in 0..40 {
        let outcome = simulator::run_with(covering(Config::random()), seed, || {
            let value = Arc::new(AtomicUsize::new(0));
            let theirs = Arc::clone(&value);
            let child = thread::spawn(move || {
                let ((), line) = (theirs.store(1, Ordering::SeqCst), line!());
                line
            });
            let (_, read) = (value.load(Ordering::SeqCst), line!());
            (child.join().unwrap(), read)
        });
        let (write, read) = outcome.result.unwrap();
        seen.extend(
            lines(&outcome.report.coverage.unwrap())
                .into_iter()
                .filter(|&pair| pair == (write, read) || pair == (read, write)),
        );
    }
    assert_eq!(seen.len(), 2, "{seen:?}");
}

/// Locks, condition variables, channels and atomics shared by three threads.
fn workload() -> usize {
    let counter = Arc::new(AtomicUsize::new(0));
    let shared = Arc::new((Mutex::new(0), Condvar::new()));
    let table = Arc::new(RwLock::new(0));
    let (tx, rx) = mpsc::channel();
    let handles: Vec<_> = (0..3)
        .map(|t| {
            let (counter, shared, table, tx) = (
                Arc::clone(&counter),
                Arc::clone(&shared),
                Arc::clone(&table),
                tx.clone(),
            );
            thread::spawn(move || {
                counter.fetch_add(1, Ordering::SeqCst);
                let _ = *table.read().unwrap();
                *table.write().unwrap() += t;
                let (lock, condvar) = &*shared;
                *lock.lock().unwrap() += 1;
                condvar.notify_all();
                tx.send(t).unwrap();
            })
        })
        .collect();
    drop(tx);
    {
        let (lock, condvar) = &*shared;
        drop(
            condvar
                .wait_while(lock.lock().unwrap(), |n| *n < 3)
                .unwrap(),
        );
    }
    let sum: usize = rx.iter().sum();
    for handle in handles {
        handle.join().unwrap();
    }
    sum + counter.load(Ordering::SeqCst)
}

#[test]
fn coverage_replays_identically_by_seed() {
    for config in [Config::random(), Config::pct(3, 100)] {
        let mut distinct = BTreeSet::new();
        for seed in 0..20 {
            let first = simulator::run_with(covering(config.clone()), seed, workload);
            let second = simulator::run_with(covering(config.clone()), seed, workload);
            let first = first.report.coverage.unwrap();
            let second = second.report.coverage.unwrap();
            assert!(!first.is_empty(), "seed {seed}");
            assert_eq!(first, second, "{config:?} seed {seed}");
            assert_eq!(first.hash(), second.hash());
            assert_eq!(first.to_string(), second.to_string());
            distinct.insert(first.hash());
        }
        assert!(
            distinct.len() > 1,
            "{config:?}: every seed covered the same pairs"
        );
    }
}

#[test]
fn coverage_off_leaves_the_run_and_its_trace_hash_unchanged() {
    let mut weak = Config::random();
    weak.weak_memory = true;
    for config in [Config::random(), Config::pct(3, 100), weak] {
        for seed in 0..20 {
            let off = simulator::run_with(config.clone(), seed, workload);
            let on = simulator::run_with(covering(config.clone()), seed, workload);
            assert!(off.report.coverage.is_none());
            assert!(on.report.coverage.is_some());
            assert_eq!(off.report.trace_hash, on.report.trace_hash, "seed {seed}");
            assert_eq!(off.report.decisions, on.report.decisions);
            assert_eq!(off.report.schedule, on.report.schedule);
            assert_eq!(off.result.unwrap(), on.result.unwrap());
        }
    }
}

#[test]
fn a_fleet_counts_the_pairs_it_has_not_seen() {
    let mut seen = BTreeSet::new();
    let mut added = 0;
    for seed in 0..20 {
        let outcome = simulator::run_with(covering(Config::random()), seed, workload);
        let coverage = outcome.report.coverage.unwrap();
        let new = coverage.new_pairs(&seen);
        assert_eq!(coverage.merge_into(&mut seen), new);
        assert_eq!(coverage.new_pairs(&seen), 0);
        assert!(coverage.hashes().is_subset(&seen));
        added += new;
    }
    assert_eq!(added, seen.len());
    assert!(added > 0);
}

#[test]
fn another_simulators_kernel_objects_are_covered_at_their_callers() {
    let mut orders = BTreeSet::new();
    for seed in 0..40 {
        let outcome = simulator::run_with(covering(Config::random()), seed, || {
            let key = simulator::new_key();
            let child = thread::spawn(move || {
                let scheduler = simulator::scheduler();
                let ((), line) = (scheduler.wake_all(WaitKey::Io(key)), line!());
                line
            });
            let scheduler = simulator::scheduler();
            scheduler.yield_point(simulator::Op::Io);
            let ((), mine) = (scheduler.wake_all(WaitKey::Io(key)), line!());
            (child.join().unwrap(), mine)
        });
        let (theirs, mine) = outcome.result.unwrap();
        let pairs = lines(&outcome.report.coverage.unwrap());
        orders.extend(
            pairs
                .into_iter()
                .filter(|&pair| pair == (theirs, mine) || pair == (mine, theirs)),
        );
    }
    assert_eq!(orders.len(), 2, "{orders:?}");
}

#[test]
fn coverage_so_far_is_readable_during_the_run() {
    let outcome = simulator::run_with(covering(Config::random()), 1, || {
        let before = simulator::coverage().unwrap();
        workload();
        let after = simulator::coverage().unwrap();
        (before.len(), after.len())
    });
    let (before, after) = outcome.result.unwrap();
    assert_eq!(before, 0);
    assert!(after > 0);
    assert!(outcome.report.coverage.unwrap().len() >= after);
    assert!(
        simulator::run(1, simulator::coverage)
            .result
            .unwrap()
            .is_none()
    );
}
