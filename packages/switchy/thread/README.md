# Switchy Thread

Threads and blocking synchronization (`Mutex`, `RwLock`, `Condvar`, `mpsc`,
atomics) with the API of `std::thread` and `std::sync`, plus a deterministic
simulator backend.

- **`std` backend**: re-exports of `std`, no overhead.
- **`simulator` backend**: threads are still real OS threads, but a seeded
  scheduler lets exactly one run at a time. Every synchronization operation is
  a switch point where the scheduler picks the next thread, so the order of
  execution depends only on the seed and a failing seed replays exactly.

The simulator also provides:

- **Simulated time**: `sleep`, `recv_timeout` and `wait_timeout` use the run's
  `switchy_time` clock, which jumps to the earliest deadline when every thread
  is waiting.
- **Deadlock detection**: when every thread is blocked, the run fails with the
  state of every thread.
- **Leak reports**: threads still blocked when the run ends are listed.
- **Trace hash**: a running hash of every decision, for checking that a seed
  replays identically.
- **Two strategies**: uniformly random, and PCT (probabilistic concurrency
  testing). `SIMULATOR_SCHEDULE_STRATEGY` selects `random`, `pct`,
  `alternate` (the default: by seed parity) or `swarm` (the strategy and PCT's
  depth and steps drawn from each run's seed). `SIMULATOR_WEAK_MEMORY=swarm`
  models weak memory in one run in four.
- **Replay and shrinking**: every run records its schedule (the threads it
  picked wherever it had a choice, beyond a default that switches only when it
  must). `SIMULATOR_SCHEDULE` (or `Strategy::Replay`) replays one without the
  scheduling generator, and `shrink` reduces a failing one by delta debugging
  until dropping any remaining choice makes the run pass.
- **Failed spawns**: `SIMULATOR_THREAD_SPAWN_FAILURES=<probability>` (or
  `set_spawn_failures` for the current run) makes `thread::Builder::spawn`
  fail with `WouldBlock` ("Resource temporarily unavailable") at that seeded
  rate, and `thread::spawn` panic, as `std`'s do. Off by default.
- **Exhaustive exploration**: `explore` runs a short scenario under every
  schedule that differs from the default by at most a given number of
  preemptions (switching away from a thread that could go on; switches forced
  by blocking or finishing are free), depth first as CHESS does, and reports
  the first failing schedule, which replays like any other. Sleep sets skip
  schedules that only reorder independent operations (on different locks,
  channels, atomics or threads). `explore_runs` does the same for harnesses
  that create their runs themselves.
- **Interleaving coverage**: with `SIMULATOR_INTERLEAVING_COVERAGE=1` (or
  `Config::interleaving_coverage`), a run records which ordered pairs of
  conflicting operations it made, by code location. See
  [Interleaving coverage](#interleaving-coverage).

It depends only on `switchy_simulation` and `switchy_time`, neither of which has
external dependencies in simulator builds.

## Installation

```toml
[dependencies]
switchy_thread = { version = "0.5.0", default-features = false, features = ["simulator"] }
```

## Usage

```rust
use switchy_thread::{simulator, sync::{Arc, Mutex}, thread};

let outcome = simulator::run(42, || {
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
    log.lock().unwrap().clone()
});

// The same seed always produces the same order and the same trace hash.
let order = outcome.result.unwrap();
assert!(outcome.report.leaked.is_empty());
# let _ = order;
```

Harnesses that advance time themselves (such as simvar) call
`simulator::run_until_idle()` from the run's main thread each tick: it runs the
other threads until none can make progress at the current simulated time.

A thread that serves the run until it ends (a runtime's worker, a service loop) can be a
daemon: `thread::Builder::new().daemon(true)`. When the run ends, an unfinished daemon is killed
(it unwinds, dropping what it holds) and is not reported as leaked.

Each simulated thread keeps its OS thread until its run ends. When the operating system has
no thread to spare (many runs at once), a spawn waits for other runs to free some, and panics
if none do: whether the machine has threads left never changes what a run does.

## Interleaving coverage

A run with `SIMULATOR_INTERLEAVING_COVERAGE=1` (`1` or `true`), or with
`Config::interleaving_coverage` set, records its interleaving coverage: the
ordered pairs `(a, b)` of code locations where one thread operated on an object
at `a` and later a different thread made a conflicting operation on the same
object at `b`. The objects are locks, atomics, channels, condition variables,
threads (finishing, and joining or asking whether one finished), and other
simulators' kernel objects (`WaitKey::Io` keys). Two operations conflict when at
least one writes the object, by the same reads and writes `explore` uses for
sleep sets: taking a lock, even for reading, writes it; an atomic load reads.
The order of the scheduler's own seeded wake-ups is not covered.

Locations come from `#[track_caller]` on the public switch points (`lock`,
`try_lock`, `read`, `write`, condition variable waits and notifications, every
atomic operation, `send`, `recv`, channel creation and `clone`,
`thread::spawn`, `join`, `is_finished`, `sleep`, `yield_now`, `fence`, and the
`Scheduler` hooks other simulators call: `yield_point`, `block`, `wake_all`,
`wake_one`), so each is the caller's `file:line:column` (a `Site`). Unlocking is
attributed to where the lock was taken, dropping a channel end and receiving
through its iterators to where it was made, and a thread's finishing to where
it was spawned. Code that wraps a switch point can be `#[track_caller]` too, to
attribute it to its own callers.

The scheduler keeps, per object, the last thread that wrote it and the threads
that read it since, each with its site, and adds a pair whenever an operation
conflicts with one of them. `ShutdownReport::coverage` holds the result
(`simulator::coverage()` gives it so far, during the run). It is sorted, and
each pair has a stable hash (`Coverage::pair_hash`, the same in every run of
the same build), so a fleet keeps the hashes it has seen and asks each run what
it added:

```rust
use std::collections::BTreeSet;
use switchy_thread::simulator::{self, Config};

let mut seen = BTreeSet::new();
for seed in 0..4 {
    let config = Config { interleaving_coverage: true, ..Config::random() };
    let outcome = simulator::run_with(config, seed, || { /* the scenario */ });
    let coverage = outcome.report.coverage.unwrap();
    // `new_pairs(&seen)` counts without adding.
    let added = coverage.merge_into(&mut seen);
    println!("seed {seed} added {added} pairs");
}
```

Coverage only observes: the run, its schedule and its trace hash are the same
with it on or off. Off, a switch point costs a thread-local swap and restore. On,
each operation also locks the scheduler and updates its object's entry in a
map (a lookup, and a pass over the readers since the object's last write).
