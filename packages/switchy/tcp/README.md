# Switchy TCP

Generic TCP networking abstraction with Tokio and simulator support.

## Overview

The TCP package provides:

- **Generic TCP Traits**: Abstract TCP listener and stream interfaces
- **Tokio Integration**: Production-ready async TCP networking
- **Simulator Support**: Mock TCP networking for testing
- **Stream Splitting**: Read/write half separation for concurrent operations
- **Address Handling**: Local and peer address access
- **Type Safety**: Generic traits for different TCP implementations

## Features

### Generic TCP Interface

- **GenericTcpListener**: Abstract TCP listener trait
- **GenericTcpStream**: Abstract TCP stream trait with read/write
- **GenericTcpStreamReadHalf**: Abstract read-only stream interface
- **GenericTcpStreamWriteHalf**: Abstract write-only stream interface

### Implementation Support

- **Tokio TCP**: Production async TCP networking
- **Simulator TCP**: Mock networking for testing and development
- **Wrapper Types**: Type-safe wrappers for different implementations

### Stream Operations

- **AsyncRead/AsyncWrite**: Tokio async I/O trait implementations
- **Stream Splitting**: Separate read and write operations
- **Address Information**: Access to local and peer socket addresses
- **Connection Management**: Connect, accept, and close operations

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
switchy_tcp = { path = "../tcp" }

# With specific features (both enabled by default)
switchy_tcp = {
    path = "../tcp",
    default-features = false,
    features = ["tokio"]  # or ["simulator"]
}
```

## Usage

### Basic TCP Server

```rust
use switchy_tcp::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Bind to address
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    println!("Server listening on 127.0.0.1:8080");

    loop {
        // Accept connections
        let (mut stream, addr) = listener.accept().await?;
        println!("New connection from: {}", addr);

        // Spawn task to handle connection
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream).await {
                eprintln!("Connection error: {}", e);
            }
        });
    }
}

async fn handle_connection(mut stream: TcpStream) -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = [0; 1024];

    loop {
        // Read data
        let n = stream.read(&mut buffer).await?;
        if n == 0 {
            break; // Connection closed
        }

        // Echo data back
        stream.write_all(&buffer[..n]).await?;
    }

    Ok(())
}
```

### TCP Client

```rust
use switchy_tcp::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to server
    let mut stream = TcpStream::connect("127.0.0.1:8080").await?;

    // Get connection info
    println!("Connected to: {}", stream.peer_addr()?);
    println!("Local address: {}", stream.local_addr()?);

    // Send data
    stream.write_all(b"Hello, server!").await?;

    // Read response
    let mut buffer = [0; 1024];
    let n = stream.read(&mut buffer).await?;
    println!("Server response: {}", String::from_utf8_lossy(&buffer[..n]));

    Ok(())
}
```

### Stream Splitting

```rust
use switchy_tcp::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn handle_bidirectional_stream(stream: TcpStream) -> Result<(), Box<dyn std::error::Error>> {
    // Split into read and write halves
    let (mut read_half, mut write_half) = stream.into_split();

    // Spawn reader task
    let reader_handle = tokio::spawn(async move {
        let mut buffer = [0; 1024];
        loop {
            match read_half.read(&mut buffer).await {
                Ok(0) => break, // Connection closed
                Ok(n) => {
                    println!("Received: {}", String::from_utf8_lossy(&buffer[..n]));
                }
                Err(e) => {
                    eprintln!("Read error: {}", e);
                    break;
                }
            }
        }
    });

    // Spawn writer task
    let writer_handle = tokio::spawn(async move {
        let messages = ["Hello", "World", "Goodbye"];
        for msg in messages {
            if let Err(e) = write_half.write_all(msg.as_bytes()).await {
                eprintln!("Write error: {}", e);
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    });

    // Wait for both tasks
    let _ = tokio::try_join!(reader_handle, writer_handle)?;
    Ok(())
}
```

### Generic TCP Usage

```rust
use switchy_tcp::{GenericTcpListener, GenericTcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn generic_server<S, R, W, L>(listener: L) -> Result<(), switchy_tcp::Error>
where
    S: GenericTcpStream<R, W>,
    R: switchy_tcp::GenericTcpStreamReadHalf,
    W: switchy_tcp::GenericTcpStreamWriteHalf,
    L: GenericTcpListener<S>,
{
    loop {
        let (mut stream, addr) = listener.accept().await?;
        println!("Connection from: {}", addr);

        tokio::spawn(async move {
            let mut buffer = [0; 1024];
            if let Ok(n) = stream.read(&mut buffer).await {
                let _ = stream.write_all(&buffer[..n]).await;
            }
        });
    }
}
```

### Simulator Mode (Testing)

```rust
#[cfg(feature = "simulator")]
use switchy_tcp::simulator::{TcpListener, TcpStream};

#[tokio::test]
async fn test_tcp_communication() {
    // Use simulator for testing
    let listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();

    // Test client connection
    let client_stream = TcpStream::connect("127.0.0.1:8080").await.unwrap();
    let (server_stream, _) = listener.accept().await.unwrap();

    // Test communication
    // ... test logic using mock TCP streams
}
```

## Generic Traits

### GenericTcpListener

```rust
use switchy_tcp::{GenericTcpListener, Error};
use std::net::SocketAddr;
use async_trait::async_trait;

#[async_trait]
pub trait GenericTcpListener<T>: Send + Sync {
    async fn accept(&self) -> Result<(T, SocketAddr), Error>;
}
```

### GenericTcpStream

```rust
use switchy_tcp::{GenericTcpStream, GenericTcpStreamReadHalf, GenericTcpStreamWriteHalf};
use tokio::io::{AsyncRead, AsyncWrite};
use std::net::SocketAddr;

pub trait GenericTcpStream<R: GenericTcpStreamReadHalf, W: GenericTcpStreamWriteHalf>:
    AsyncRead + AsyncWrite + Send + Sync + Unpin
{
    fn into_split(self) -> (R, W);
    fn local_addr(&self) -> std::io::Result<SocketAddr>;
    fn peer_addr(&self) -> std::io::Result<SocketAddr>;
}
```

## Error Handling

```rust
use switchy_tcp::{Error, TcpStream};

async fn handle_tcp_errors() {
    match TcpStream::connect("invalid-address").await {
        Ok(stream) => {
            // Handle successful connection
        }
        Err(Error::IO(io_err)) => {
            eprintln!("I/O error: {}", io_err);
        }
        Err(Error::AddrParse(parse_err)) => {
            eprintln!("Address parse error: {}", parse_err);
        }
        Err(Error::ParseInt(int_err)) => {
            eprintln!("Integer parse error: {}", int_err);
        }
        #[cfg(feature = "simulator")]
        Err(Error::Send) => {
            eprintln!("Simulator send error");
        }
    }
}
```

## Feature Flags

- **`tokio`**: Enable Tokio-based TCP implementation (enabled by default)
- **`simulator`**: Enable simulator/mock TCP implementation (enabled by default)

**Note**: Both features are enabled by default. When both are enabled, the simulator type aliases (`TcpListener`, `TcpStream`, etc.) are used. To use Tokio types exclusively, disable default features and enable only `tokio`.

## Type Aliases

When features are enabled, convenient type aliases are available:

```rust
// With simulator feature (takes priority when both are enabled)
pub type TcpListener = SimulatorTcpListener;
pub type TcpStream = SimulatorTcpStream;
pub type TcpStreamReadHalf = SimulatorTcpStreamReadHalf;
pub type TcpStreamWriteHalf = SimulatorTcpStreamWriteHalf;

// With tokio feature only (when simulator is disabled)
pub type TcpListener = TokioTcpListener;
pub type TcpStream = TokioTcpStream;
pub type TcpStreamReadHalf = TokioTcpStreamReadHalf;
pub type TcpStreamWriteHalf = TokioTcpStreamWriteHalf;
```

To access specific implementations when both features are enabled:

```rust
use switchy_tcp::{TokioTcpListener, TokioTcpStream};
use switchy_tcp::{SimulatorTcpListener, SimulatorTcpStream};
```

## Dependencies

Core dependencies:

- **switchy_async**: Async runtime abstraction with I/O, macros, sync, time, and util support
- **tokio**: Networking primitives (required, with `net` feature)
- **async-trait**: Async trait support
- **thiserror**: Error handling
- **paste**: Macro utilities
- **log**: Logging

Simulator-specific dependencies (when `simulator` feature is enabled):

- **bytes**: Byte buffer management
- **flume**: MPSC channel implementation
- **scoped-tls**: Thread-local storage for simulator context

## Use Cases

- **Network Servers**: TCP-based server applications
- **Client Applications**: TCP client connections
- **Protocol Implementation**: Custom network protocol development
- **Testing**: Mock network communication in tests
- **Cross-platform Networking**: Abstract over different TCP implementations
- **Microservices**: Service-to-service TCP communication
