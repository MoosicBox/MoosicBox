# switchy_tcp

Generic TCP stream and listener abstractions for async Rust.

## Overview

This crate provides generic traits and implementations for TCP networking that work across different async runtimes. It supports both real tokio-based networking and an in-memory simulator for testing.

## Features

- `tokio` - Real TCP networking using tokio (enabled by default)
- `simulator` - In-memory TCP simulator for testing without actual network I/O (enabled by default)
- `fail-on-warnings` - Treat warnings as errors during compilation

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
switchy_tcp = { version = "0.1.4" }
```

To use only the tokio implementation without the simulator:

```toml
[dependencies]
switchy_tcp = { version = "0.1.4", default-features = false, features = ["tokio"] }
```

## Usage

### Real TCP Networking (tokio)

```rust,no_run
use switchy_tcp::{TokioTcpListener, TokioTcpStream, GenericTcpListener};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a TCP listener
    let listener = TokioTcpListener::bind("127.0.0.1:8080").await?;

    // Accept incoming connections
    let (stream, addr) = listener.accept().await?;
    println!("Connection from: {}", addr);

    // Connect to a server
    let client = TokioTcpStream::connect("127.0.0.1:8080").await?;
    Ok(())
}
```

### In-Memory Simulator (for testing)

```rust,no_run
use switchy_tcp::simulator::{TcpListener, TcpStream, reset};

async fn test_example() -> Result<(), Box<dyn std::error::Error>> {
    // Reset simulator state for clean test isolation
    reset();

    // Bind a simulated listener
    let listener = TcpListener::bind("127.0.0.1:8080").await?;

    // Connect a simulated client
    let client = TcpStream::connect("127.0.0.1:8080").await?;

    // Accept the connection on the server side
    let (server_stream, client_addr) = listener.accept().await?;
    Ok(())
}
```

### Splitting Streams

TCP streams can be split into separate read and write halves for concurrent I/O:

```rust,no_run
use switchy_tcp::{TokioTcpStream, GenericTcpStream};

async fn example(stream: TokioTcpStream) {
    let (read_half, write_half) = stream.into_split();
    // Use read_half and write_half independently
}
```

## Core Types

### Traits

- `GenericTcpListener<T>` - Trait for TCP listeners that can accept connections
- `GenericTcpStream<R, W>` - Trait for TCP streams that can be split into read/write halves
- `GenericTcpStreamReadHalf` - Trait for the read half of a split stream
- `GenericTcpStreamWriteHalf` - Trait for the write half of a split stream

### Tokio Types

- `TokioTcpListener` - TCP listener backed by tokio
- `TokioTcpStream` - TCP stream backed by tokio
- `TokioTcpStreamReadHalf` - Read half of a tokio TCP stream
- `TokioTcpStreamWriteHalf` - Write half of a tokio TCP stream

### Simulator Types

- `SimulatorTcpListener` - In-memory TCP listener for testing
- `SimulatorTcpStream` - In-memory TCP stream for testing
- `SimulatorTcpStreamReadHalf` - Read half of a simulated stream
- `SimulatorTcpStreamWriteHalf` - Write half of a simulated stream

### Default Type Aliases

When both features are enabled, the simulator types are used as defaults:

- `TcpListener` - Alias for `SimulatorTcpListener` (or `TokioTcpListener` if simulator is disabled)
- `TcpStream` - Alias for `SimulatorTcpStream` (or `TokioTcpStream` if simulator is disabled)
- `TcpStreamReadHalf` - Alias for the corresponding read half type
- `TcpStreamWriteHalf` - Alias for the corresponding write half type

## Simulator Utilities

The simulator module provides utilities for test isolation:

- `reset()` - Resets all simulator state (ports, IPs, DNS)
- `reset_next_port()` - Resets the ephemeral port counter
- `reset_next_ip()` - Resets the IP address counter
- `reset_dns()` - Clears all DNS hostname-to-IP mappings
- `next_port()` - Allocates the next ephemeral port
- `next_ip()` - Allocates the next simulated IP address
- `with_host(addr, f)` - Executes a closure with a scoped host address
- `current_host()` - Returns the current scoped host address if set

## License

MPL-2.0
