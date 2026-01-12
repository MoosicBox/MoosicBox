# Switchy HTTP Models

HTTP models and types for Switchy.

This crate provides common HTTP types including methods and status codes that work
across different HTTP libraries. It includes optional conversions for popular frameworks
like `actix-web` and `reqwest`.

## Features

- `actix` - Enables conversions to/from `actix-web` types (enabled by default)
- `reqwest` - Enables conversions to/from `reqwest` types (enabled by default)
- `serde` - Enables serialization/deserialization support (enabled by default)

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
switchy_http_models = "0.1.4"
```

## Usage

```rust
use switchy_http_models::{Method, StatusCode};

let method = Method::Get;
assert_eq!(method.to_string(), "GET");

let status = StatusCode::Ok;
assert_eq!(status.as_u16(), 200);
assert!(status.is_success());
```

### Parsing Methods

```rust
use std::str::FromStr;
use switchy_http_models::Method;

let method = Method::from_str("GET").unwrap();
assert_eq!(method, Method::Get);
```

### Status Code Categories

```rust
use switchy_http_models::StatusCode;

let status = StatusCode::NotFound;
assert!(status.is_client_error());
assert!(!status.is_success());

let server_error = StatusCode::InternalServerError;
assert!(server_error.is_server_error());
```

### Converting from u16

```rust
use switchy_http_models::StatusCode;

let status = StatusCode::try_from_u16(404).unwrap();
assert_eq!(status, StatusCode::NotFound);
```

## License

MPL-2.0
