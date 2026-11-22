# MoosicBox Simvar

Simulation variable system and testing harness for deterministic async testing.

## Overview

The MoosicBox Simvar package is a facade that re-exports functionality from:

- **Simvar Harness**: Core simulation testing framework (`simvar_harness`)
- **Simvar Utils**: Simulation utilities (`simvar_utils`, with `utils` feature)
- **Switchy**: Underlying simulation and switching framework

This package provides a unified interface for simulation-based testing with deterministic async execution.

## Core Functionality

The package re-exports from `simvar_harness`:

- **`run_simulation`**: Main function to run simulation tests
- **`SimBootstrap` trait**: Trait for configuring simulation lifecycle hooks (`init`, `on_start`, `on_step`, `on_end`, `props`, `build_sim`)
- **`Sim` trait**: Interface for managing hosts and clients in simulations
- **Configuration types**: `SimConfig`, `SimProperties`, `SimResult`, `SimRunProperties`
- **Modules**: `client`, `host`, `plan`
- **`utils` module**: Re-exports `simvar_utils` functionality
- **`switchy`**: Underlying simulation and switching framework

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
simvar = { path = "../simvar" }

# Or with specific features
simvar = {
    path = "../simvar",
    features = ["async", "tcp", "time"]
}
```

## Usage

### Basic Simulation

```rust
use simvar::*;

struct MySimBootstrap;

impl SimBootstrap for MySimBootstrap {
    fn on_start(&self, sim: &mut impl Sim) {
        // Setup hosts and clients
        sim.host("my-host", || async {
            // Host logic
            Ok(())
        });
    }
}

fn main() {
    let results = run_simulation(MySimBootstrap).unwrap();
    // Process results
}
```

### With Utilities

```rust
#[cfg(feature = "utils")]
use simvar::utils;
```

## Feature Flags

By default, the `all` feature is enabled. Individual features can be selected:

- **`all`** (default): Enable the primary simulation feature set
- **`async`**: Async runtime support
- **`database`**: Database simulation
- **`fs`**: Filesystem simulation
- **`fs-simulator-real-fs`**: Enable real filesystem seeding helpers for simulator filesystem setup
- **`http`**: HTTP client/server simulation
- **`mdns`**: mDNS simulation
- **`random`**: Random number generation simulation
- **`tcp`**: TCP connection simulation
- **`telemetry`**: Telemetry support
- **`time`**: Time simulation
- **`tui`**: Terminal UI for simulation visualization
- **`upnp`**: UPnP simulation
- **`utils`**: Simulation utilities module
- **`web-server`**: Web server simulation
- **`pretty_env_logger`**: Pretty logging output
- **`fail-on-warnings`**: Treat warnings as errors

## Environment Variables

- **`SIMULATOR_RUNS`**: Number of simulation runs to execute (default: 1), or
  `forever` to run until stopped (only failed runs' results are then kept)
- **`SIMULATOR_MAX_PARALLEL`**: Maximum parallel runs (default: number of CPUs)
- **`SIMULATOR_ON_FAILURE`**: `stop` (default) ends the simulation at the first
  failed run; `continue` records and reports it and keeps starting runs
- **`SIMULATOR_SEED`**: The first run's seed (default random)
- **`SIMULATOR_VERIFY_DETERMINISM`**: `1` replays every run and fails if its trace differs
- **`SIMULATOR_SHRINK`** (`thread` feature): shrink a failing run's thread
  schedule (`1` for up to 1000 replays, or a number) and print how to replay it
- **`SIMULATOR_SCHEDULE`** (`thread` feature): replay a shrunk schedule
- **`NO_TUI`**: Disable terminal UI when set

## Steering a Running Simulation

`simvar::control()` steers the simulation from any thread (an observer, a
dashboard, a test) while it runs:

- `pause()` / `resume()`: hold workers from starting new runs; runs in progress finish
- `set_max_parallel(n)`: grow or shrink the pool of workers (has no effect when
  the simulation started with `SIMULATOR_MAX_PARALLEL=0`)
- `stop()`: end the simulation, as `end_sim()` does
- `status()`: a `FleetStatus` of paused, `max_parallel`, active, completed and failed runs
- `replay(ReplayRequest { seed, epoch_offset, control })`: re-run an earlier run
  exactly, in-process, on a thread of its own
- `shrink(seed, epoch_offset)` (`thread` feature): shrink an earlier failed run's
  thread schedule, reporting `SimEvent::Shrink` progress and finally the
  schedule for `SIMULATOR_SCHEDULE`

Replay and shrink jobs work while the fleet runs, while it is paused and after it
finished. They report the usual events under worker `0` with
`RunKind::Replay`/`RunKind::Shrink`, and never count toward the fleet. A stop, or
a failure under `SIMULATOR_ON_FAILURE=stop`, also cancels jobs in progress, so a
long-lived dashboard should use `SIMULATOR_ON_FAILURE=continue`. Inside a run,
`current_run()` and `current_replay()` say which run the calling thread belongs
to and which `ReplayControl` (if any) holds it.

## Dependencies

- **simvar_harness** (workspace): Core simulation testing framework
- **simvar_utils** (workspace, optional): Simulation utilities

## Integration

This package is designed for:

- **Deterministic Testing**: Predictable async test execution with controlled time and randomness
- **Concurrent Systems Testing**: Test multi-host, multi-client distributed systems
- **Regression Testing**: Reproducible test scenarios for complex async applications
- **Development**: Testing framework for async applications with simulated I/O

## Note

This package serves as a facade for the simulation variable system, re-exporting functionality from the core harness and providing optional utilities. The `simvar_harness` package provides the actual implementation of the simulation framework.
