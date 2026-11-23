# MoosicBox Simulation Testing Harness

Comprehensive simulation testing framework for deterministic testing and validation.

## Overview

The MoosicBox Simulation Testing Harness provides:

- **Simulation Orchestration**: Multi-threaded simulation execution and management
- **Host/Client Testing**: Network simulation with host and client actors
- **Deterministic Testing**: Reproducible test runs with controlled randomness
- **TUI Interface**: Optional text-based user interface for simulation monitoring
- **Parallel Execution**: Configurable parallel simulation runs
- **Event System**: Simulation lifecycle event hooks and callbacks

## Features

### Simulation Management

- **Multi-Run Support**: Execute multiple simulation runs with parallel execution
- **Cancellation Support**: Graceful simulation cancellation and cleanup
- **Thread Management**: Worker thread allocation and management
- **Progress Tracking**: Real-time simulation progress monitoring

### Testing Framework

- **Host Simulation**: Server-side simulation with async action support
- **Client Simulation**: Client-side simulation and interaction testing
- **Result Aggregation**: Simulation result collection and analysis
- **Network Bouncing**: Planned: Host restart and failover simulation

### User Interface

- **TUI Mode**: Optional terminal-based monitoring interface
- **Logging Integration**: Comprehensive logging with environment logger
- **Progress Display**: Real-time simulation status and progress
- **Error Reporting**: Detailed error reporting and stack traces

### Configuration

- **Environment Variables**: Runtime configuration via environment variables
- **Bootstrap System**: Configurable simulation initialization
- **Properties**: Custom simulation properties and metadata
- **Lifecycle Hooks**: Before/after simulation event handlers

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
simvar_harness = { path = "../simvar/harness" }
```

## Usage

### Basic Simulation

```rust
use simvar_harness::{run_simulation, SimBootstrap, Sim, SimConfig};
use std::time::Duration;

struct MySimulation;

impl SimBootstrap for MySimulation {
    fn build_sim(&self, mut config: SimConfig) -> SimConfig {
        config.duration(Duration::from_secs(10)); // 10 second simulation
        config
    }

    fn on_start(&self, sim: &mut impl Sim) {
        // Add hosts and clients
        sim.host("server", || async {
            // Server simulation logic
            Ok(())
        });

        sim.client("client1", async {
            // Client simulation logic
            Ok(())
        });
    }
}

// Run simulation
let results = run_simulation(MySimulation)?;

// Analyze results
for result in results {
    if result.is_success() {
        println!("Run {} succeeded", result.props().run_number);
    } else {
        println!("Run {} failed", result.props().run_number);
    }
}
```

### Advanced Configuration

```rust
use simvar_harness::{SimBootstrap, SimConfig, Sim};
use std::time::Duration;

struct AdvancedSimulation;

impl SimBootstrap for AdvancedSimulation {
    fn props(&self) -> Vec<(String, String)> {
        vec![
            ("test_type".to_string(), "load_test".to_string()),
            ("target_rps".to_string(), "1000".to_string()),
        ]
    }

    fn build_sim(&self, mut config: SimConfig) -> SimConfig {
        config.duration(Duration::from_secs(30));
        config
    }

    fn on_start(&self, sim: &mut impl Sim) {
        // Add multiple hosts for load balancing
        for i in 0..3 {
            sim.host(format!("server_{}", i), move || async move {
                // Server instance simulation
                simulate_server_load().await
            });
        }

        // Add multiple clients for load generation
        for i in 0..10 {
            sim.client(format!("client_{}", i), async move {
                // Client load generation
                simulate_client_requests().await
            });
        }
    }

    fn on_step(&self, sim: &mut impl Sim) {
        // Per-step simulation logic
    }

    fn on_end(&self, sim: &mut impl Sim) {
        // Cleanup and final validation
    }

    fn on_result(&self, result: &SimResult) {
        // Each run's result, once its threads ended (a guided fleet steers by it)
    }
}
```

### Host and Client Actors

```rust
// Host actor (server simulation)
sim.host("api_server", || async {
    // Start server
    let server = start_test_server().await?;

    // Wait for requests
    server.wait_for_shutdown().await?;

    Ok(())
});

// Client actor (client simulation)
sim.client("load_client", async {
    // Generate load
    for _ in 0..100 {
        let response = make_request().await?;
        assert!(response.is_success());
    }

    Ok(())
});
```

### Fault Injection

The `fault` module crashes, restarts and bounces hosts, and breaks the simulated
network (`switchy::network`), from `on_step` or from inside any actor. A
*nemesis* client is the usual way to inject faults on a schedule:

```rust
use std::time::Duration;

use simvar_harness::{fault, switchy};

sim.on_crash("db", |host| {
    // Crash whatever else the host owns, e.g. its simulated filesystem.
});

sim.client("nemesis", async {
    switchy::unsync::time::sleep(Duration::from_secs(1)).await;
    fault::network::partition(["n1"], ["n2", "n3"]);
    fault::bounce_after("n2", Duration::from_millis(500));
    switchy::unsync::time::sleep(Duration::from_secs(1)).await;
    fault::network::heal();
    Ok(())
});
```

- `fault::crash(host)` takes the host's network node down (dropping its
  messages in flight and resetting its TCP connections), drops its action and
  every task it spawned (including `JoinSet` tasks) at once, in spawn order and
  without polling them again, kills the simulated threads they started (with
  the `thread` feature: each is unwound from its current switch point, dropping
  what it holds, and runs none of its own code again), then runs its crash
  hooks. The host stays down until `fault::restart(host)`.
- `fault::bounce(host)` / `fault::bounce_after(host, delay)` crash and restart
  it from its factory on a fresh runtime, at once or after `delay` of simulated
  time.
- `fault::on_crash(host, hook)` (or `Sim::on_crash`) registers a hook for every
  crash of a host; `host::on_crash(hook)` registers one, from inside a host, for
  its current incarnation only.
- `fault::network` partitions (symmetric and one-way), isolates and heals nodes,
  and sets link policies (latency, loss, duplication, reordering). Hosts and
  clients are nodes named after themselves.

Lifecycle changes apply at the start of the next step; network changes at once.
Everything random is drawn from the run's seed, so a seed replays the same
faults.

## Environment Configuration

### Environment Variables

- `SIMULATOR_RUNS`: Number of simulation runs (default: 1), or `forever` to run until stopped (only failed runs' results are then kept)
- `SIMULATOR_MAX_PARALLEL`: Maximum parallel runs (default: CPU cores)
- `SIMULATOR_ON_FAILURE`: `stop` (default) ends the simulation at the first failed run; `continue` records and reports it and keeps starting runs
- `SIMULATOR_DURATION`: Simulation duration with unit suffix (e.g., "10s", "5000ms", "1000µs", "1000ns"; default: unlimited)
- `SIMULATOR_SEED`: Fixed seed for deterministic runs
- `SIMULATOR_EPOCH_OFFSET`: Fixed simulated epoch offset override (time feature)
- `SIMULATOR_EPOCH_MIN` / `SIMULATOR_EPOCH_MAX`: Inclusive bounded random epoch offset (time feature)
- `SIMULATOR_EPOCH_RANGE_PROFILE`: Epoch profile `low` | `wide` | `full` (time feature)
- `SIMULATOR_VERIFY_DETERMINISM`: replay every run with its seed and fail if the replay observed anything differently (the run's digest: the thread schedule, processes and terminal output, the final file tree, random draws, the clock and `record`ed values). `1` replays in the same process; `process` in a fresh, perturbed process of the simulation (`SIMULATOR_PERTURB` starts process-wide counters elsewhere), which also catches a run depending on what else its process ran. A failure names the first observation that differs, with the text around the difference
- `SIMULATOR_DIGEST_BACKTRACE_AT`: add a backtrace to this observation of a run's digest, to see what made it
- `SIMULATOR_SHRINK` (thread feature): shrink a failing run's thread schedule (`1` for up to 1000 replays, or a number of replays) and print how to replay the result
- `SIMULATOR_SCHEDULE` (thread feature): replay a thread schedule, as printed by `SIMULATOR_SHRINK`
- `SIMULATOR_INTERLEAVING_COVERAGE` (thread feature): `1` records each run's interleaving coverage (the ordered pairs of conflicting operations of different simulated threads, by code location; see `switchy_thread`) in its result's `SimRunProperties::coverage`, which `SimBootstrap::on_result` sees, so a guided fleet can keep the inputs whose runs added pairs
- `NO_TUI`: Disable TUI interface when set at compile or run time
- `RUST_LOG`: Log level configuration (e.g., "debug", "info", "warn")

When a run fails, the harness prints a copy/paste rerun command that includes both
`SIMULATOR_SEED` and the failed run's concrete `SIMULATOR_EPOCH_OFFSET` so replay is exact.

`simvar_harness::control()` pauses, resumes, resizes and stops a running
simulation, and starts in-process replay and shrink jobs beside it (see the
crate docs).

### Example Configuration

```bash
# Run 10 simulations with 4 parallel workers
export SIMULATOR_RUNS=10
export SIMULATOR_MAX_PARALLEL=4

# Disable TUI interface (set before compilation)
export NO_TUI=1

cargo test simulation_test
```

## Features

The package supports the following features:

### Default Features

- **pretty_env_logger**: Environment-based logging configuration
- **tui**: Terminal user interface for simulation monitoring

### TUI Interface

When built with the `tui` feature (enabled by default), the harness shows
[`simvar_ratatui`](../ratatui)'s progress display whenever standard output is
a terminal. The display is an ordinary observer (see `simvar_observer`): the
run loop only sends events, and any other front end can subscribe to the same
ones with `add_observer`.

Disable by excluding default features, or by setting `NO_TUI` at compile or
run time.

### Optional Features

- **async**: Async simulation support (via switchy)
- **database**: Database connection simulation (via switchy)
- **fs**: File system simulation (via switchy)
- **fs-simulator-real-fs**: Enable real filesystem seeding helpers for simulator setup (via switchy)
- **http**: HTTP simulation (via switchy)
- **mdns**: mDNS simulation (via switchy)
- **random**: Randomness simulation controls (via switchy)
- **tcp**: TCP network simulation (via switchy)
- **telemetry**: Telemetry support (via switchy)
- **time**: Time manipulation for simulation (via switchy)
- **upnp**: UPnP simulation (via switchy)
- **web-server**: Web server simulation support

### Logging Integration

- Environment-based log level configuration
- Structured logging with simulation context
- Error tracking and stack traces
- Performance metrics

## Error Handling

Comprehensive error types:

- **Step Errors**: Simulation step execution failures
- **Join Errors**: Thread joining and coordination failures
- **IO Errors**: File system and network operation failures

## Dependencies

- **switchy**: Async runtime and simulation utilities
- **simvar_utils**: Simulation utilities and helpers
- **color-backtrace**: Enhanced error reporting
- **simvar_ratatui**: Optional terminal display (with `tui` feature)
- **pretty_env_logger**: Optional environment logger integration (default feature)

## Integration

This package is designed for:

- **Load Testing**: High-throughput simulation testing
- **Integration Testing**: Multi-service interaction testing
- **Chaos Engineering**: Failure injection and recovery testing
- **Performance Testing**: Latency and throughput validation
