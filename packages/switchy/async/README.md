# MoosicBox Async

Async runtime abstraction and utilities for MoosicBox applications.

## Overview

The MoosicBox Async package provides:

- **Runtime Abstraction**: Generic async runtime interface
- **Multi-Backend**: Support for Tokio and simulation runtimes
- **Builder Pattern**: Flexible runtime configuration
- **Feature-Gated**: Modular async functionality
- **Thread Management**: Thread ID tracking and management

## Features

### Runtime Abstraction

- **GenericRuntime**: Common interface for all async runtimes
- **Runtime Builder**: Configurable runtime construction
- **Backend Selection**: Choose between Tokio and simulation runtimes
- **Future Support**: Standard Future trait integration

### Backend Support

- **Tokio**: Production async runtime
- **Simulator**: Deterministic simulation runtime for testing
- **Feature-Gated**: Enable only needed backends

### Async Utilities

- **Thread ID**: Unique thread identification
- **Task Management**: Task spawning and joining, and `task::JoinSet` / `task::AbortHandle`
- **IO Operations**: Async I/O primitives (feature-gated)
- **Synchronization**: Async synchronization primitives (feature-gated)
- **Timers**: Async timing utilities (feature-gated)

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
switchy_async = "0.1.4"
```

With Tokio backend features:

```toml
[dependencies]
switchy_async = { version = "0.1.4", default-features = false, features = ["tokio", "rt-multi-thread", "io", "sync", "time", "macros"] }
```

For testing with simulation:

```toml
[dependencies]
switchy_async = { version = "0.1.4", default-features = false, features = ["simulator", "macros"] }
```

## Usage

### Runtime Creation

```rust
use switchy_async::{Builder, GenericRuntime};

// Create runtime with default settings (current-thread)
let runtime = Builder::new().build()?;

// Use generic runtime interface
runtime.block_on(async {
    println!("Hello from async runtime!");
});

// Wait for runtime to complete
runtime.wait()?;

// With multi-threaded runtime (requires rt-multi-thread feature)
#[cfg(feature = "rt-multi-thread")]
{
    let runtime = Builder::new()
        .max_blocking_threads(Some(4))
        .build()?;
    runtime.block_on(async {
        println!("Hello from multi-threaded runtime!");
    });
    runtime.wait()?;
}
```

### Backend-Specific Usage

```rust
// Tokio backend (when tokio feature enabled)
#[cfg(all(feature = "tokio", feature = "time", not(feature = "simulator")))]
use switchy_async::{Builder, task, time};

#[cfg(all(feature = "tokio", feature = "time", not(feature = "simulator")))]
{
    let runtime = Builder::new().build()?;

    // Spawn tasks
    runtime.block_on(async {
        let handle = task::spawn(async {
            time::sleep(time::Duration::from_millis(100)).await;
            "Task completed"
        });

        let result = handle.await.expect("task should complete");
        println!("{}", result);
    });

    runtime.wait()?;
}

// Additional modules available with features
#[cfg(feature = "io")]
use switchy_async::io;  // Async I/O traits and utilities

#[cfg(feature = "sync")]
use switchy_async::sync;  // Synchronization primitives

#[cfg(feature = "util")]
use switchy_async::util;  // Additional utilities
```

### Simulation Backend

```rust
// Simulation backend (when simulator feature enabled)
#[cfg(feature = "simulator")]
use switchy_async::simulator;

#[cfg(feature = "simulator")]
{
    let runtime = Builder::new().build()?;

    runtime.block_on(async {
        // Deterministic async execution for testing
        println!("Simulation runtime");
    });
}
```

Under the simulator, `#[inject_yields]` (and `inject_yields_mod!`) rewrites every `.await` in
the item to `task::yield_after(x).await`, and `select!` calls `task::maybe_yield().await` once
it completes. Each yields (lets other tasks run) with the run's probability: 0, ¼, ½ or 1,
drawn from the run's seed (`simulator::yield_probability`). `SIMULATOR_YIELD_PROBABILITY`
overrides it with a number in `0..=1`, or `swarm` (draw per run, the default), and
`simulator::set_yield_probability` overrides it for the current run. With probability 0 an
injected point polls no yield at all. Outside the simulator both functions are no-ops.

### Task Sets

`task::JoinSet` collects spawned tasks and yields their results as they finish. With the
Tokio backend it is `tokio::task::JoinSet`; the simulator provides the same API (`spawn`,
`spawn_blocking`, `spawn_local`, `join_next`, `try_join_next`, `abort_all`, `detach_all`,
`shutdown`, `join_all`, `len`, `is_empty`; dropping the set aborts its tasks). Under the
simulator, tasks finish in an order fixed by the run's seed, so `join_next` replays exactly.

```rust,ignore
use switchy_async::task::JoinSet;

let mut set = JoinSet::new();
for i in 0..4 {
    set.spawn(async move { i * 2 });
}
while let Some(result) = set.join_next().await {
    let value = result?;
    // ...
}
```

The simulator's `JoinSet` does not provide task ids (`join_next_with_id`), `spawn_on`
variants or `poll_join_next`, and a panicking task fails the run rather than joining with a
`JoinError` (simulator join errors are always cancellations).

### Synchronization Primitives

With the `sync` feature, `sync` provides `Mutex`, `RwLock`, `Semaphore`, `Notify`,
`Barrier`, and the `oneshot`, `mpsc`, `mpmc`, `broadcast` and `watch` channels. With the
Tokio backend all but `mpsc` and `mpmc` are Tokio's. Under the simulator every one is the
simulator's own, with Tokio's API (`mpsc` wraps Tokio's unbounded channel):

- `Mutex` (`lock`, `lock_owned`, `try_lock`, `try_lock_owned`, `blocking_lock`,
  `blocking_lock_owned`, `get_mut`, `into_inner`; `MutexGuard`, `OwnedMutexGuard` and their
  `map` / `try_map` to `MappedMutexGuard` / `OwnedMappedMutexGuard`; `TryLockError`)
- `RwLock` (`read`, `write`, their `_owned`, `try_` and `blocking_` forms,
  `with_max_readers`; `RwLockReadGuard`, `RwLockWriteGuard`, `RwLockMappedWriteGuard` and the
  owned guards, with `map`, `try_map`, `downgrade`, `downgrade_map` and `into_mapped`)
- `Semaphore` (`acquire`, `acquire_many`, their `_owned`, `try_` and `blocking_` forms,
  `add_permits`, `forget_permits`, `close`; `SemaphorePermit` and `OwnedSemaphorePermit` with
  `forget`, `merge`, `split`; `AcquireError`, `TryAcquireError`)
- `Notify` (`notified`, `notified_owned`, `notify_one`, `notify_last`, `notify_waiters`;
  `futures::{Notified, OwnedNotified}` with `enable`)
- `oneshot` (`Sender::{send, closed, poll_closed, is_closed}`, `Receiver::{close, try_recv,
  blocking_recv, is_empty, is_terminated}`, `error::{RecvError, TryRecvError}`)
- `broadcast` (`channel`, `Sender::{new, send, subscribe, downgrade, len, is_empty,
  receiver_count, closed, strong_count, weak_count}`, `WeakSender`, `Receiver::{recv,
  try_recv, blocking_recv, resubscribe, len, is_empty, is_closed}`, `error::{SendError,
  RecvError, TryRecvError}`)
- `watch` (`channel`, `Sender::{send, send_replace, send_modify, send_if_modified, borrow,
  subscribe, receiver_count, sender_count, is_closed, closed}`, `Receiver::{borrow,
  borrow_and_update, changed, wait_for, has_changed, is_closed, mark_changed, mark_unchanged,
  same_channel}`, `Ref` and `error::{SendError, RecvError}`)

With the `util` feature, `util::CancellationToken` is likewise the simulator's own, with
tokio-util's API (`child_token`, `cancel`, `is_cancelled`, `cancelled`, `cancelled_owned`,
`run_until_cancelled`, `run_until_cancelled_owned`, `drop_guard`, `drop_guard_ref`).

They are deterministic by construction: each keeps its waiters in one list ordered by when
they started waiting and wakes them in a defined order, with no shards, no random choice and
nothing that depends on addresses. As Tokio's, the locks and the semaphore are fair (waiters
get the lock or permits first in, first out, and a cancelled waiter gives up its place and
any permits it was given); `notify_one` wakes the oldest waiter and `notify_last` the newest;
`notify_waiters`, a broadcast send and a cancellation wake every waiter, oldest first. Tokio's
`watch`, by contrast, puts each waiting receiver in one of eight queues picked by Tokio's
thread-local random number generator, which no seed controls. Contract tests
(`tests/sync_tokio_contract.rs`) run each against Tokio's and tokio-util's, comparing results
and wake order.

Each of their futures is subject to the spurious `Pending` fault (see below). With
`thread-simulator`, their `blocking_*` methods called from a simulated thread wait on the run's
scheduler instead of blocking its OS thread, as does a simulated thread that finds a `watch`
value locked by another (one holding a `Ref`).

### Thread Management

```rust
use switchy_async::thread_id;

// Get unique thread ID
let id = thread_id();
println!("Current thread ID: {}", id);
```

### Macros and Utilities

```rust
// Async macros (requires macros feature)
#[cfg(feature = "macros")]
use switchy_async::{select, join, try_join};

#[cfg(feature = "macros")]
{
    // Use select! macro
    select! {
        result1 = async_operation_1() => {
            println!("Operation 1 completed: {:?}", result1);
        }
        result2 = async_operation_2() => {
            println!("Operation 2 completed: {:?}", result2);
        }
    }

    // Use join! macro
    let (result1, result2) = join!(
        async_operation_1(),
        async_operation_2()
    );

    // Use try_join! macro for Results
    let (result1, result2) = try_join!(
        fallible_async_operation_1(),
        fallible_async_operation_2()
    )?;
}

// Yield injection for simulation testing (requires macros feature)
#[cfg(feature = "macros")]
use switchy_async::{inject_yields, inject_yields_mod};

#[cfg(feature = "macros")]
#[inject_yields]
async fn my_async_function() {
    // Function body with automatic yield injection for deterministic testing
}

// Runtime entry-point macros
#[cfg(feature = "macros")]
#[switchy_async::main]
async fn main() {
    println!("Runs with the selected switchy_async backend");
}

#[cfg(feature = "macros")]
#[switchy_async::test]
async fn my_test() {
    assert_eq!(2 + 2, 4);
}
```

### Error Handling

```rust
use switchy_async::Error;

// Runtime errors
match runtime.wait() {
    Ok(()) => println!("Runtime completed successfully"),
    Err(Error::IO(io_err)) => println!("I/O error: {}", io_err),
    Err(Error::Join(join_err)) => println!("Join error: {}", join_err),
}

// Task join errors (when using task handles)
use switchy_async::task::JoinError;

let handle = runtime.spawn(async { /* ... */ });
match handle.await {
    Ok(result) => println!("Task completed: {:?}", result),
    Err(e) => println!("Task failed: {}", e),
}
```

## Feature Flags

### Backend Selection

- **`tokio`**: Enable Tokio async runtime
- **`simulator`**: Enable simulation runtime for testing

### Simulated Threads

- **`thread-simulator`**: The simulator together with `switchy_thread`'s
  simulated threads. `spawn_blocking` work runs on a simulated thread (started
  when the executor reaches it, so aborting it before then still drops it
  unrun), and an executor with nothing to run blocks on the run's scheduler so
  those threads run, on the run's clock, until a task's waker wakes it. Blocking
  in `spawn_blocking` work then blocks only its own thread, and every
  interleaving of tasks and threads replays by seed.

### Several Workers

With `thread-simulator`, a simulator runtime polls up to `n` ready tasks at once each step,
on simulated threads, so the synchronous code between two tasks' awaits overlaps as it does on
a multi-thread runtime (lock-order deadlocks and read-modify-write races between tasks become
reachable). The seeded scheduler decides every switch, so runs stay deterministic. `n` is 1 for
a current-thread runtime, `worker_threads` for a multi-thread one, else the run's
`SIMULATOR_ASYNC_WORKERS`: a number, or `swarm` (one run in four gets 1, the others 2 to 4).
Unset, it is 1. A runtime's workers start when a step first needs them and live until it is
shut down or the run ends (they are `switchy_thread` daemons), so a run starts at most `n - 1`
threads per runtime.

### Late Wakeups and Spurious `Pending`

Two seeded executor faults, both off unless configured. Each takes a probability per run:
`switchy_async::simulator::set_late_wakeups(p)` / `set_spurious_pending(p)` (for the current
run), else `SIMULATOR_LATE_WAKEUPS` / `SIMULATOR_SPURIOUS_PENDING`: a number in `0..=1`, or
`swarm` to draw it per run from `{0, 0.01, 0.05, 0.2}`. Unset, it is 0. Every decision is drawn
from the fault's own named stream (`switchy_async::late_wakeups`,
`switchy_async::spurious_pending`), and nothing is drawn while a fault is off, so other draws
and existing seeds replay exactly as before. `simulator::fault_counts()` tells how often each
fault was drawn and fired in the run.

- **Late wakeups**: a woken task is sometimes queued only after 1 to `LATE_WAKEUP_MAX_STEPS`
  (8) further steps of its runtime (each `tick`, each turn of `block_on`, `wait` or a blocking
  task's loop), as on a busy executor. Under the harness the clock moves one step per tick, so
  a wakeup can lose a race it would win at once, against a timeout say. A task waiting late
  still counts as unfinished (`wait`, `try_finish`), `shutdown_now` drops it, and an executor
  with late tasks steps on instead of idling, so none is lost.
- **Spurious `Pending`**: a ready leaf future of the simulator (sleeps and timeouts,
  `futures::Instant`, intervals, its own `Mutex`, `RwLock`, `Semaphore`, `Notify`, `oneshot`,
  `broadcast`, `mpmc` and `watch` channels, `Barrier` and `CancellationToken`, the `mpsc` and
  bounded channels it wraps, and join handles) sometimes returns `Pending` once and wakes
  itself at once, before consuming anything. Correct code cannot tell; code that assumes a
  ready future completes on this poll can. It reaches awaits `#[inject_yields]` does not.
  After a spurious `Pending`, the task's next ready leaf completes without a draw, so even a
  probability of 1 makes progress. tokio's own futures (its I/O, `task::yield_now`) are not
  reached.

### Runtime Features

- **`rt-multi-thread`**: Multi-threaded runtime support
- **`fs`**: File system operations
- **`io`**: Async I/O operations
- **`net`**: Networking support
- **`process`**: Process management
- **`sync`**: Synchronization primitives (includes channels)
- **`time`**: Timing utilities
- **`util`**: Additional utilities
- **`macros`**: Async macros (select!, join!, try_join!, etc.)

### Macro Features

- **`macros`**: Enable async macros and yield injection utilities

## Runtime Comparison

### Tokio Runtime

- **Production**: Optimized for production use
- **Performance**: High-performance async execution
- **Ecosystem**: Full Tokio ecosystem support
- **Threading**: Multi-threaded execution

### Simulation Runtime

- **Testing**: Deterministic execution for tests
- **Reproducible**: Consistent behavior across runs
- **Debugging**: Easier debugging and tracing
- **Controlled**: Precise control over execution order

## Dependencies

Core dependencies:

- **thiserror**: Error handling
- **pin-project-lite**: Pin projection utilities
- **scoped-tls**: Scoped thread-local storage
- **log**: Logging facade

Feature-gated dependencies:

- **futures**: Core Future trait and utilities (enabled with backend features)
- **tokio**: Tokio async runtime (optional, enabled with `tokio` feature)
- **tokio-util**: Additional Tokio utilities (optional, enabled with the `tokio` feature; the
  simulator's `util` does not use it)
- **flume**: MPSC/MPMC channel implementation (optional, enabled with `sync` feature)
- **switchy_async_macros**: Macro utilities (optional, enabled with `macros` feature)
- **switchy_random**: Random number generation for simulator (optional, enabled with `simulator` feature)
- **switchy_time**: Time utilities for simulator (optional, enabled with `simulator` feature)

## Integration

This package is designed for:

- **Application Runtime**: Main async runtime for applications
- **Testing**: Deterministic async testing with simulation
- **Library Development**: Runtime-agnostic async libraries
- **Performance**: High-performance async applications
- **Cross-Platform**: Consistent async behavior across platforms
