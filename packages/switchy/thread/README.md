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
