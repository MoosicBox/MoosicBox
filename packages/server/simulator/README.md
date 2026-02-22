# MoosicBox Server Simulator

Deterministic simulator for testing the MoosicBox server.

This crate provides a simulation harness for testing the MoosicBox server under
various conditions including fault injection and health monitoring. It uses
deterministic simulation to enable reproducible testing of distributed system
behaviors.

## Features

- **Host Simulation** - Run the MoosicBox server in a simulated environment
- **Health Checker** - Periodically verify server health status via HTTP
- **Fault Injector** - Inject random faults (server restarts/bounces) to test
  resilience
- **HTTP Utilities** - Make requests and parse responses in simulations

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_server_simulator = { path = "packages/server/simulator" }
```

## Usage

### Running the Simulator Binary

```sh
cargo run -p moosicbox_server_simulator
```

#### Environment Variables

- `PORT` - Optional port number for the MoosicBox server (default: auto-selected)

### Using as a Library

```rust,no_run
use moosicbox_server_simulator::{client, handle_actions, host};
use simvar::{Sim, SimBootstrap, run_simulation};

struct MySimulator;

impl SimBootstrap for MySimulator {
    fn on_start(&self, sim: &mut impl Sim) {
        // Start the MoosicBox server in the simulation
        host::moosicbox_server::start(sim, None);

        // Start client simulators
        client::health_checker::start(sim);
        client::fault_injector::start(sim);
    }

    fn on_step(&self, sim: &mut impl Sim) {
        // Handle queued actions (like bounces)
        handle_actions(sim);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let results = run_simulation(MySimulator)?;
    Ok(())
}
```

## API

### Root Functions

- `queue_bounce(host)` - Queue a bounce (restart) action for a host
- `handle_actions(sim)` - Handle all queued actions in the simulation
- `try_connect(addr, max_attempts)` - Attempt to connect to a TCP stream with
  retries

### Modules

- `client::health_checker` - Health check client that periodically verifies
  server status
- `client::fault_injector` - Fault injection client that generates random faults
- `host::moosicbox_server` - MoosicBox server host simulation
- `http` - HTTP utilities for requests and response parsing

### HTTP Utilities

- `HttpResponse` - Struct containing status code, headers, and body
- `http_request(method, stream, path)` - Make an HTTP request over a TCP stream
- `parse_http_response(raw)` - Parse a raw HTTP response string
- `headers_contains_in_order(expected, actual)` - Check if headers contain
  expected key-value pairs

## Cargo Features

- `default` - Enables `player`, `sqlite`, `telemetry`, and `upnp`
- `player` - Enable player functionality
- `sqlite` - Enable SQLite database support
- `telemetry` - Enable telemetry/metrics
- `upnp` - Enable UPnP support (requires `player`)

## License

See the LICENSE file in the repository root.
