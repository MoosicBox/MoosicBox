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

# Optional: Enable TUI interface
simvar_harness = {
    path = "../simvar/harness",
    features = ["tui"]
}
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

**Note:** The `bounce` method is currently a placeholder and does not restart hosts. Full host bouncing functionality is planned for a future release.

## Environment Configuration

### Runtime Variables

- `SIMULATOR_RUNS`: Number of simulation runs (default: 1)
- `SIMULATOR_MAX_PARALLEL`: Maximum parallel runs (default: CPU cores)
- `SIMULATOR_DURATION`: Simulation duration with unit suffix (e.g., "10s", "5000ms", "1000µs", "1000ns"; default: unlimited)
- `SIMULATOR_SEED`: Fixed seed for deterministic runs
- `NO_TUI`: Disable TUI interface (compile-time only, set when building)
- `RUST_LOG`: Log level configuration (e.g., "debug", "info", "warn")

### Example Configuration

```bash
# Run 10 simulations with 4 parallel workers
SIMULATOR_RUNS=10 SIMULATOR_MAX_PARALLEL=4 cargo test simulation_test

# Disable TUI interface (must be set at compile time)
NO_TUI=1 cargo build
```

## Features

The package supports the following features:

### Default Features

- **pretty_env_logger**: Environment-based logging configuration
- **tui**: Terminal user interface for simulation monitoring

### TUI Interface

When built with the `tui` feature (enabled by default), provides:

- Real-time simulation progress
- Run status and results
- Error display and debugging
- Interactive simulation monitoring

Disable with `NO_TUI=1` at compile time or by excluding default features.

### Optional Features

- **async**: Async simulation support (via switchy)
- **database**: Database connection simulation (via switchy)
- **fs**: File system simulation (via switchy)
- **http**: HTTP simulation (via switchy)
- **mdns**: mDNS simulation (via switchy)
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
- **ratatui**: Optional terminal interface (with `tui` feature)
- **pretty_env_logger**: Optional environment logger integration (default feature)

## Integration

This package is designed for:

- **Load Testing**: High-throughput simulation testing
- **Integration Testing**: Multi-service interaction testing
- **Chaos Engineering**: Failure injection and recovery testing
- **Performance Testing**: Latency and throughput validation
