# MoosicBox Server Simulator

Simulation utilities for testing MoosicBox server components and network interactions.

## Overview

The MoosicBox Server Simulator package provides:

- **Action Queue System**: Centralized action management for simulations
- **Host Bouncing**: Server restart and failover simulation
- **Connection Utilities**: Robust TCP connection establishment
- **Client/Host Modules**: Simulation actor implementations
- **HTTP Testing**: HTTP-based simulation support

## Features

### Action Management
- **Queue System**: Thread-safe action queuing with VecDeque
- **Action Processing**: Batch action execution during simulation steps
- **Host Bouncing**: Simulate server restarts and failovers

### Connection Utilities
- **Retry Logic**: Automatic connection retry with backoff
- **Timeout Handling**: Configurable connection timeouts
- **Error Recovery**: Handle connection refused and reset scenarios

### Simulation Integration
- **Simvar Integration**: Built on MoosicBox simulation framework
- **TCP Streams**: Network connection simulation
- **Multi-Attempt Logic**: Robust connection establishment

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

## Modules

### client
Client-side simulation utilities and implementations.

### host
Host/server-side simulation utilities and implementations.

### http
HTTP-specific simulation testing utilities.

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

- **Simvar**: MoosicBox simulation framework
- **Tokio**: Async runtime and networking
- **Standard Library**: Core functionality

## Integration

This package is designed for:
- **Load Testing**: Server performance under load
- **Failover Testing**: High availability scenario testing
- **Network Simulation**: Connection failure and recovery
- **Integration Testing**: Multi-component system testing
