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
- **`uuid`**: UUID generation utilities (via `switchy_uuid`)
- **`web-server`**: Web server abstractions (via `switchy_web_server`)
- **`web-server-core`**: Core web server types and traits (via `switchy_web_server_core`)
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
    "uuid",
    "web-server",
    "web-server-core",
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

### Top-Level Features

- **`all`** (default): Enable all base module features
- **`simulator`**: Enable simulator mode across all modules
- **`fail-on-warnings`**: Treat warnings as errors

### Base Module Features

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
- **`uuid`**: Enable UUID generation utilities
- **`web-server`**: Enable web server abstractions
- **`web-server-core`**: Enable core web server types
- **`http`**: Enable HTTP client/server
- **`http-models`**: Enable HTTP model types

### Convenience "All" Features

Each module has an `all-*` feature that enables all sub-features for that module:

- **`all-async`**: All async features
- **`all-async-macros`**: All async macro features
- **`all-database`**: All database features
- **`all-database-connection`**: All database connection features
- **`all-fs`**: All filesystem features
- **`all-http`**: All HTTP features
- **`all-http-models`**: All HTTP model features
- **`all-mdns`**: All mDNS features
- **`all-random`**: All random features
- **`all-tcp`**: All TCP features
- **`all-telemetry`**: All telemetry features
- **`all-time`**: All time features
- **`all-upnp`**: All UPnP features
- **`all-uuid`**: All UUID features
- **`all-web-server`**: All web server features

### Sub-Features

Each module has additional sub-features for fine-grained control:

**Async**: `async-fs`, `async-io`, `async-net`, `async-process`, `async-rt-multi-thread`, `async-sync`, `async-time`, `async-tokio`, `async-util`

**Database**: `database-api`, `database-mysql`, `database-mysql-sqlx`, `database-postgres`, `database-postgres-raw`, `database-postgres-sqlx`, `database-schema`, `database-simulator`, `database-sqlite`, `database-sqlite-rusqlite`, `database-sqlite-sqlx`, `database-sqlx`, `database-tls`, `database-turso`

**Database Connection**: `database-connection-creds`, `database-connection-mysql`, `database-connection-mysql-sqlx`, `database-connection-postgres`, `database-connection-postgres-native-tls`, `database-connection-postgres-openssl`, `database-connection-postgres-raw`, `database-connection-postgres-sqlx`, `database-connection-simulator`, `database-connection-sqlite`, `database-connection-sqlite-rusqlite`, `database-connection-sqlite-sqlx`, `database-connection-sqlx`, `database-connection-tls`, `database-connection-turso`

**Filesystem**: `fs-async`, `fs-simulator`, `fs-simulator-real-fs`, `fs-std`, `fs-sync`, `fs-tokio`

**HTTP**: `http-brotli`, `http-deflate`, `http-gzip`, `http-json`, `http-reqwest`, `http-serde`, `http-simulator`, `http-stream`, `http-zstd`

**HTTP Models**: `http-models-actix`, `http-models-reqwest`, `http-models-serde`

**mDNS**: `mdns-scanner`, `mdns-simulator`

**Random**: `random-rand`, `random-simulator`

**TCP**: `tcp-simulator`, `tcp-tokio`

**Telemetry**: `telemetry-actix`, `telemetry-simulator`

**Time**: `time-simulator`, `time-std`

**UPnP**: `upnp-api`, `upnp-listener`, `upnp-openapi`, `upnp-player`, `upnp-simulator`

**UUID**: `uuid-serde`, `uuid-simulator`, `uuid-uuid`

**Web Server**: `web-server-actix`, `web-server-compress`, `web-server-core`, `web-server-cors`, `web-server-htmx`, `web-server-openapi`, `web-server-openapi-all`, `web-server-openapi-rapidoc`, `web-server-openapi-redoc`, `web-server-openapi-scalar`, `web-server-openapi-swagger-ui`, `web-server-serde`, `web-server-simulator`, `web-server-static-files`, `web-server-tls`

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
- `switchy_uuid` - UUID generation utilities
- `switchy_web_server` - Web server abstractions
- `switchy_web_server_core` - Core web server types
- `switchy_http` - HTTP client/server
- `switchy_http_models` - HTTP model types

## Use Cases

- **Cross-platform Applications**: Single interface for platform-specific functionality
- **Feature-gated Libraries**: Enable only needed functionality to reduce binary size
- **Modular Architecture**: Clean separation of concerns across different domains
- **Testing**: Easy mocking and testing with feature flags
