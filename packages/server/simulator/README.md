# MoosicBox Server Simulator

Simulation utilities for testing MoosicBox server components and network interactions.

## Overview

The MoosicBox Server Simulator package provides:

- **Action Queue System**: Centralized action management for simulation steps
- **Host Bouncing**: Server restart and failover simulation via `queue_bounce` and `handle_actions`
- **Connection Utilities**: Robust TCP connection establishment with retry logic
- **Simulation Actors**: Pre-built client actors (health checker, fault injector) and host implementations (MoosicBox server)
- **HTTP Utilities**: HTTP request generation, response parsing, and header validation

## Features

### Action Management

- **Queue System**: Thread-safe action queuing with VecDeque
- **Action Processing**: Batch action execution during simulation steps
- **Host Bouncing**: Simulate server restarts and failovers

### Connection Utilities

- **Retry Logic**: Automatic connection retry with backoff
- **Timeout Handling**: 5-second timeout per connection attempt
- **Error Recovery**: Handle connection refused and reset scenarios

### Simulation Integration

- **Simvar Integration**: Built on the simvar deterministic simulation framework
- **TCP Streams**: Simulated network connection support
- **Multi-Attempt Logic**: Robust connection establishment with configurable retries

### Cargo Features

- **default**: Enables `player`, `sqlite`, `telemetry`, and `upnp`
- **player**: Enables player-related server functionality through `moosicbox_server/player`
- **sqlite**: Enables SQLite-backed server functionality through `moosicbox_server/sqlite`
- **telemetry**: Enables telemetry integration through `moosicbox_server/telemetry` and `simvar/telemetry`
- **upnp**: Enables UPnP integration through `moosicbox_server/upnp` and `simvar/upnp` (also enables `player`)
- **fail-on-warnings**: Propagates strict warning-as-error behavior to simulator dependencies

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_server_simulator = { path = "../server/simulator" }
```

## Usage

### Action Queue Management

```rust
use moosicbox_server_simulator::{queue_bounce, handle_actions};

// Queue a host bounce action
queue_bounce("api_server");

// Process queued actions in simulation step
handle_actions(&mut sim);
```

### Connection Testing

```rust
use moosicbox_server_simulator::try_connect;

// Attempt connection with retry logic
let stream = try_connect("localhost:8080", 5).await?;

// Use the established connection
perform_test_operations(stream).await;
```

### Integration with Simvar

```rust
use simvar::Sim;
use moosicbox_server_simulator::handle_actions;

// In simulation step handler
fn on_step(sim: &mut impl Sim) {
    // Process any queued actions
    handle_actions(sim);

    // Continue with simulation logic
    perform_simulation_step(sim);
}
```

### Full Simulator Bootstrap

```rust
use moosicbox_server_simulator::{client, handle_actions, host};
use simvar::{Sim, SimBootstrap};

struct Simulator;

impl SimBootstrap for Simulator {
    fn on_start(&self, sim: &mut impl Sim) {
        host::moosicbox_server::start(sim, None);
        client::health_checker::start(sim);
        client::fault_injector::start(sim);
    }

    fn on_step(&self, sim: &mut impl Sim) {
        handle_actions(sim);
    }
}
```

### HTTP Utilities

```rust
use moosicbox_server_simulator::{http, try_connect};

let mut stream = try_connect("127.0.0.1:1234", 5).await?;
let raw = http::http_request("GET", &mut stream, "/health").await?;
let parsed = http::parse_http_response(&raw)?;

assert_eq!(parsed.status_code, 200);
```

### Running the Simulator Binary

```bash
cargo run -p moosicbox_server_simulator
```

Optional environment variable:

- `PORT`: service port used by `host::moosicbox_server::start` in the simulator binary

## Modules

### client

Client-side simulation actors and utilities:

- **health_checker**: Health check client with interaction plans
- **fault_injector**: Fault injection client for testing resilience

### host

Host/server-side simulation implementations:

- **moosicbox_server**: MoosicBox server host actor with TCP proxying

### http

HTTP request and response utilities:

- HTTP request generation
- HTTP response parsing
- Header validation helpers

## Error Handling

### Connection Errors

- **ConnectionRefused**: Retry with backoff
- **ConnectionReset**: Retry with backoff
- **TimedOut**: Timeout after maximum duration
- **Other Errors**: Immediate failure

### Retry Logic

- **Max Attempts**: Configurable retry limit
- **Backoff**: 5-second delay between attempts
- **Timeout**: 5-second timeout per attempt

## Dependencies

### Core Dependencies

- **simvar**: Deterministic simulation framework with async, TCP, HTTP, and network support
- **tokio**: Async runtime and networking
- **actix-web**: Web server framework (used by moosicbox_server host)
- **openport**: Port allocation utilities

### MoosicBox Dependencies

- **moosicbox_assert**: Assertion utilities
- **moosicbox_config**: Configuration management
- **moosicbox_env_utils**: Environment variable utilities
- **moosicbox_logging**: Logging utilities
- **moosicbox_server**: MoosicBox server implementation

### Switchy Dependencies

- **switchy_async**: Async runtime abstraction
- **switchy_env**: Environment variable abstraction
- **switchy_telemetry**: Telemetry support

### Other Dependencies

- **log**: Logging facade
- **net2**: Network utilities for TCP configuration
- **serde_json**: JSON serialization
- **strum**: Enum utilities

## Integration

This package enables:

- **Failover Testing**: Simulating server restarts and testing high availability scenarios
- **Network Simulation**: Testing connection failure and recovery behaviors
- **Integration Testing**: Running multi-component system simulations with deterministic network and timing
