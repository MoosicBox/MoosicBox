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
- **Task Management**: Task spawning and joining
- **IO Operations**: Async I/O primitives (feature-gated)
- **Synchronization**: Async synchronization primitives (feature-gated)
- **Timers**: Async timing utilities (feature-gated)

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
switchy_async = { path = "../async" }

# Enable specific features
switchy_async = {
    path = "../async",
    features = ["tokio", "rt-multi-thread", "io", "sync", "time", "macros"]
}

# For testing with simulation
switchy_async = {
    path = "../async",
    features = ["simulator", "macros"]
}
```

## Usage

### Runtime Creation

```rust
use switchy_async::{Builder, GenericRuntime};

// Create runtime with builder
let runtime = Builder::new()
    .max_blocking_threads(Some(4))
    .build()?;

// Use generic runtime interface
runtime.block_on(async {
    println!("Hello from async runtime!");
});

// Wait for runtime to complete
runtime.wait()?;
```

### Backend-Specific Usage

```rust
// Tokio backend (when tokio feature enabled)
#[cfg(feature = "tokio")]
use switchy_async::{task, time, io, sync};

#[cfg(feature = "tokio")]
{
    // Spawn tasks
    let handle = task::spawn(async {
        time::sleep(time::Duration::from_millis(100)).await;
        "Task completed"
    });

    let result = handle.await?;
    println!("{}", result);
}
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

### Thread Management

```rust
use switchy_async::thread_id;

// Get unique thread ID
let id = thread_id();
println!("Current thread ID: {}", id);
```

### Macros and Utilities

```rust
#[cfg(feature = "macros")]
use switchy_async::{select, inject_yields, inject_yields_mod};

#[cfg(feature = "macros")]
{
    // Use async macros
    select! {
        result1 = async_operation_1() => {
            println!("Operation 1 completed: {:?}", result1);
        }
        result2 = async_operation_2() => {
            println!("Operation 2 completed: {:?}", result2);
        }
    }
}

// Inject yields for simulation testing
#[inject_yields]
async fn my_async_function() {
    // Function body with automatic yield injection
}
```

### Error Handling

```rust
use switchy_async::Error;

match runtime.wait() {
    Ok(()) => println!("Runtime completed successfully"),
    Err(Error::IO(io_err)) => println!("I/O error: {}", io_err),
    Err(Error::Join(join_err)) => println!("Join error: {}", join_err),
}
```

## Feature Flags

### Backend Selection
- **`tokio`**: Enable Tokio async runtime
- **`simulator`**: Enable simulation runtime for testing

### Tokio Features
- **`rt-multi-thread`**: Multi-threaded Tokio runtime
- **`io`**: Async I/O operations
- **`sync`**: Synchronization primitives
- **`time`**: Timing utilities
- **`util`**: Additional utilities
- **`macros`**: Async macros (select!, etc.)

### Additional Features
- **`macros`**: Enable async macros and yield injection

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

- **Futures**: Core Future trait and utilities
- **Tokio**: Optional Tokio runtime (feature-gated)
- **Switchy Async Macros**: Macro utilities
- **ThisError**: Error handling

## Integration

This package is designed for:
- **Application Runtime**: Main async runtime for applications
- **Testing**: Deterministic async testing with simulation
- **Library Development**: Runtime-agnostic async libraries
- **Performance**: High-performance async applications
- **Cross-Platform**: Consistent async behavior across platforms
