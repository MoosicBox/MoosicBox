# MoosicBox Simulation Utilities

Utility functions for simulation testing and cancellation management.

## Overview

The MoosicBox Simulation Utilities package provides:

- **Thread Management**: Worker thread ID tracking and management
- **Cancellation Tokens**: Local and global simulation cancellation
- **Async Utilities**: Future cancellation and timeout support
- **Testing Support**: Simulation state management for testing

## Features

### Thread Management
- **Worker Thread IDs**: Unique thread identification for simulation workers
- **Thread-Local Storage**: Per-thread state management

### Cancellation Management
- **Local Cancellation**: Per-thread simulation cancellation
- **Global Cancellation**: System-wide simulation termination
- **Token Reset**: Cancellation token lifecycle management

### Async Support
- **Future Cancellation**: Run futures until simulation cancellation
- **Cancellation Detection**: Check if simulation is cancelled
- **Graceful Shutdown**: Clean simulation termination

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_simvar_utils = { path = "../simvar/utils" }
```

## Usage

### Thread Management

```rust
use moosicbox_simvar_utils::worker_thread_id;

// Get unique thread ID for current worker
let thread_id = worker_thread_id();
println!("Worker thread ID: {}", thread_id);
```

### Cancellation Management

```rust
use moosicbox_simvar_utils::{
    cancel_simulation, cancel_global_simulation,
    is_simulator_cancelled, is_global_simulator_cancelled,
    reset_simulator_cancellation_token, reset_global_simulator_cancellation_token
};

// Check cancellation status
if is_simulator_cancelled() || is_global_simulator_cancelled() {
    println!("Simulation cancelled");
    return;
}

// Cancel local simulation
cancel_simulation();

// Cancel global simulation
cancel_global_simulation();

// Reset local cancellation token for new simulation
reset_simulator_cancellation_token();

// Reset global cancellation token for new simulation
reset_global_simulator_cancellation_token();
```

### Running Futures with Cancellation

```rust
use moosicbox_simvar_utils::run_until_simulation_cancelled;

// Run future until simulation is cancelled
let result = run_until_simulation_cancelled(async {
    // Your simulation work here
    simulate_work().await
}).await;

match result {
    Some(output) => println!("Simulation completed: {:?}", output),
    None => println!("Simulation was cancelled"),
}
```

## Dependencies

- **Switchy**: Threading and cancellation utilities
- **Standard Library**: Thread-local storage and atomic operations

## Integration

This package is designed for use with:

- **MoosicBox Simvar Harness**: Simulation testing framework
- **Test Suites**: Deterministic testing with cancellation
- **Background Workers**: Cancellable async operations
