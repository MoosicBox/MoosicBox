# Switchy

Feature-gated re-exports of platform-specific implementations for cross-platform compatibility.

## Overview

The Switchy package provides:

- **Conditional Re-exports**: Feature-gated access to platform-specific modules
- **Unified Interface**: Single import point for cross-platform functionality
- **Module Organization**: Structured access to async, database, filesystem, and network modules
- **Feature Flexibility**: Enable only the modules you need

## Features

### Available Modules
- **`async`**: Async runtime utilities (via `switchy_async`)
- **`async-macros`**: Async macro utilities (via `switchy_async_macros`)
- **`database`**: Database abstraction layer (via `switchy_database`)
- **`database-connection`**: Database connection management (via `switchy_database_connection`)
- **`fs`**: Filesystem operations (via `switchy_fs`)
- **`mdns`**: mDNS service discovery (via `switchy_mdns`)
- **`random`**: Random number generation (via `switchy_random`)
- **`tcp`**: TCP networking (via `switchy_tcp`)
- **`telemetry`**: Telemetry and monitoring (via `switchy_telemetry`)
- **`time`**: Time utilities (via `switchy_time`)
- **`upnp`**: UPnP device discovery (via `switchy_upnp`)
- **`http`**: HTTP client/server (via `switchy_http` and `switchy_http_models`)

### Module Structure
- **Individual Features**: Each module is behind its own feature flag
- **Nested Modules**: HTTP module contains both client and models
- **Clean Imports**: Simple import paths for all functionality

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
switchy = { path = "../switchy", features = ["async", "tcp", "http"] }

# Or enable all features
switchy = { path = "../switchy", features = [
    "async",
    "async-macros",
    "database",
    "database-connection",
    "fs",
    "mdns",
    "random",
    "tcp",
    "telemetry",
    "time",
    "upnp",
    "http",
    "http-models"
] }
```

## Usage

### Async Utilities

```rust
// Enable with features = ["async"]
use switchy::unsync;

// Use async utilities
```

### Database Operations

```rust
// Enable with features = ["database", "database-connection"]
use switchy::{database, database_connection};

// Use database functionality
```

### Filesystem Operations

```rust
// Enable with features = ["fs"]
use switchy::fs;

// Use filesystem operations
```

### Network Operations

```rust
// Enable with features = ["tcp", "mdns", "upnp"]
use switchy::{tcp, mdns, upnp};

// Use networking functionality
```

### HTTP Client/Server

```rust
// Enable with features = ["http", "http-models"]
use switchy::http;
use switchy::http::models;

// Use HTTP functionality
```

### Telemetry and Monitoring

```rust
// Enable with features = ["telemetry"]
use switchy::telemetry;

// Use telemetry functionality
```

### Time Utilities

```rust
// Enable with features = ["time"]
use switchy::time;

// Use time utilities
```

### Random Number Generation

```rust
// Enable with features = ["random"]
use switchy::random;

// Use random number generation
```

## Feature Flags

- **`async`**: Enable async runtime utilities
- **`async-macros`**: Enable async macro utilities
- **`database`**: Enable database abstraction layer
- **`database-connection`**: Enable database connection management
- **`fs`**: Enable filesystem operations
- **`mdns`**: Enable mDNS service discovery
- **`random`**: Enable random number generation
- **`tcp`**: Enable TCP networking
- **`telemetry`**: Enable telemetry and monitoring
- **`time`**: Enable time utilities
- **`upnp`**: Enable UPnP device discovery
- **`http`**: Enable HTTP client/server
- **`http-models`**: Enable HTTP model types

## Dependencies

This package re-exports functionality from:
- `switchy_async` - Async runtime utilities
- `switchy_async_macros` - Async macro utilities
- `switchy_database` - Database abstraction
- `switchy_database_connection` - Database connections
- `switchy_fs` - Filesystem operations
- `switchy_mdns` - mDNS service discovery
- `switchy_random` - Random number generation
- `switchy_tcp` - TCP networking
- `switchy_telemetry` - Telemetry and monitoring
- `switchy_time` - Time utilities
- `switchy_upnp` - UPnP device discovery
- `switchy_http` - HTTP client/server
- `switchy_http_models` - HTTP model types

## Use Cases

- **Cross-platform Applications**: Single interface for platform-specific functionality
- **Feature-gated Libraries**: Enable only needed functionality to reduce binary size
- **Modular Architecture**: Clean separation of concerns across different domains
- **Testing**: Easy mocking and testing with feature flags
