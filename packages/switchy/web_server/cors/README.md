# MoosicBox Web Server CORS

Cross-Origin Resource Sharing (CORS) configuration and utilities.

## Overview

The MoosicBox Web Server CORS package provides:

- **CORS Configuration**: Comprehensive CORS policy configuration
- **AllOrSome Pattern**: Flexible allow-all or specific-values pattern
- **Builder Pattern**: Fluent API for CORS configuration
- **HTTP Integration**: Integration with HTTP method types

## Features

### CORS Configuration

- **Origins**: Configure allowed origins with wildcard or specific domains
- **Methods**: Specify allowed HTTP methods
- **Headers**: Control allowed and exposed headers
- **Credentials**: Configure credential support
- **Max Age**: Set preflight cache duration

### AllOrSome Pattern

- **Flexible Permissions**: Allow all or restrict to specific values
- **Type Safety**: Generic pattern for different permission types
- **Default Behavior**: Sensible defaults for security

### Builder API

- **Fluent Interface**: Chain configuration methods
- **Incremental Building**: Add permissions incrementally
- **Method Chaining**: Build complex CORS policies easily

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
switchy_web_server_cors = { path = "../web_server/cors" }
```

## Usage

### Basic CORS Configuration

```rust
use switchy_web_server_cors::Cors;
use switchy_http_models::Method;

// Create restrictive CORS policy
let cors = Cors::default()
    .allow_origin("https://app.moosicbox.com")
    .allow_origin("https://moosicbox.com")
    .allow_method(Method::Get)
    .allow_method(Method::Post)
    .allow_header("Content-Type")
    .allow_header("Authorization")
    .support_credentials()
    .max_age(3600);
```

### Permissive CORS Configuration

```rust
// Create permissive CORS policy
let cors = Cors::default()
    .allow_any_origin()
    .allow_any_method()
    .allow_any_header()
    .expose_any_header()
    .support_credentials();
```

### Incremental Configuration

```rust
// Start with defaults and add permissions
let mut cors = Cors::default();

// Add multiple origins
cors = cors.allowed_origins(vec![
    "https://app.moosicbox.com",
    "https://admin.moosicbox.com",
    "http://localhost:3000",
]);

// Add multiple methods
cors = cors.allowed_methods(vec![
    Method::Get,
    Method::Post,
    Method::Put,
    Method::Delete,
]);

// Add multiple headers
cors = cors.allowed_headers(vec![
    "Content-Type",
    "Authorization",
    "X-Requested-With",
]);
```

### AllOrSome Usage

```rust
use switchy_web_server_cors::AllOrSome;

// Check permission type
match cors.allowed_origins {
    AllOrSome::All => {
        println!("All origins allowed");
    }
    AllOrSome::Some(ref origins) => {
        println!("Specific origins allowed: {:?}", origins);
    }
}

// Access specific values
if let Some(origins) = cors.allowed_origins.as_ref() {
    for origin in origins {
        println!("Allowed origin: {}", origin);
    }
}
```

### Configuration Properties

```rust
// Access CORS configuration
println!("Supports credentials: {}", cors.supports_credentials);
println!("Max age: {:?}", cors.max_age);

// Check if all values are allowed
println!("All origins allowed: {}", cors.allowed_origins.is_all());
println!("All methods allowed: {}", cors.allowed_methods.is_all());
println!("All headers allowed: {}", cors.allowed_headers.is_all());
```

## CORS Structure

### Cors Configuration

- **allowed_origins**: Origins that can make requests
- **allowed_methods**: HTTP methods permitted
- **allowed_headers**: Request headers allowed
- **expose_headers**: Response headers exposed to client
- **supports_credentials**: Whether credentials are supported
- **max_age**: Preflight cache duration in seconds

### AllOrSome<T>

- **All**: Allow everything (equivalent to `*`)
- **Some(T)**: Allow only specific values
- **Type Default**: `AllOrSome<T>` type defaults to `All`, but the `Cors` struct explicitly uses `Some(vec![])` for security

## Security Considerations

### Default Behavior

- **Restrictive Defaults**: Default configuration is restrictive
- **Explicit Permissions**: Require explicit permission grants
- **Credential Handling**: Credentials disabled by default

### Best Practices

- **Specific Origins**: Avoid `allow_any_origin()` in production
- **Minimal Headers**: Only allow necessary headers
- **Credential Security**: Be cautious with credential support

## Dependencies

- **Switchy HTTP Models**: HTTP method types and models

## Integration

This package is designed for:

- **Web Server Middleware**: CORS middleware implementation
- **API Security**: Cross-origin request policy enforcement
- **Development Tools**: Flexible CORS configuration for development
- **Production Security**: Restrictive CORS policies for production
