# MoosicBox Web Server Core

Core abstractions and traits for web server implementations.

## Overview

The MoosicBox Web Server Core package provides:

- **WebServer Trait**: Abstract interface for web server implementations
- **Lifecycle Management**: Standard start/stop operations for servers
- **Async Support**: Full async/await support with Future-based operations
- **Implementation Agnostic**: Framework-independent server abstractions

## Features

### WebServer Trait

- **start()**: Async server startup operation
- **stop()**: Async server shutdown operation
- **Future-based**: Returns pinned futures for async operations
- **Lifecycle Management**: Standard server lifecycle interface

### Async Operations

- **Pin<Box<dyn Future>>**: Boxed futures for dynamic dispatch
- **Async/Await**: Full async support for server operations
- **Non-blocking**: Async operations don't block execution

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_web_server_core = { path = "../web_server/core" }
```

## Usage

### Implementing WebServer

```rust
use moosicbox_web_server_core::WebServer;
use std::pin::Pin;
use std::future::Future;

struct MyWebServer {
    port: u16,
}

impl WebServer for MyWebServer {
    fn start(&self) -> Pin<Box<dyn Future<Output = ()>>> {
        let port = self.port;
        Box::pin(async move {
            // Start server implementation
            println!("Starting server on port {}", port);
            // Server startup logic here
        })
    }

    fn stop(&self) -> Pin<Box<dyn Future<Output = ()>>> {
        Box::pin(async move {
            // Stop server implementation
            println!("Stopping server");
            // Server shutdown logic here
        })
    }
}
```

### Using WebServer

```rust
async fn run_server(server: impl WebServer) {
    // Start the server
    server.start().await;

    // Server is now running...

    // Stop the server
    server.stop().await;
}
```

## Design Principles

### Framework Agnostic

- **Abstract Interface**: No dependency on specific web frameworks
- **Implementation Freedom**: Implementers can use any underlying technology
- **Consistent API**: Standard interface regardless of implementation

### Async First

- **Future-based**: All operations return futures
- **Non-blocking**: Designed for async runtimes
- **Composable**: Easy to integrate with async applications

## Dependencies

- **Standard Library**: Core Future and Pin types
- **moosicbox_assert**: Internal assertion utilities
- **Minimal Dependencies**: Lightweight dependency footprint

## Integration

This package is designed for:

- **Web Framework Abstraction**: Common interface for different web servers
- **Server Management**: Lifecycle management for web services
- **Testing**: Mock server implementations for testing
- **Plugin Systems**: Pluggable server implementations
