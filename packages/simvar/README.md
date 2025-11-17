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

By default, all features are enabled. Individual features can be selected:

- **`all`** (default): Enable all features
- **`async`**: Async runtime support
- **`database`**: Database simulation
- **`fs`**: Filesystem simulation
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
