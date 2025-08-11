# MoosicBox Web Server

A web server abstraction library providing a unified interface for HTTP server functionality with support for routing, middleware, and multiple backend implementations.

## Features

- **Server Abstraction**: Unified web server interface with pluggable backends
- **Routing Support**: Define scopes and routes with HTTP method handling
- **Request/Response Types**: Unified HTTP request and response abstractions
- **Query Parsing**: Built-in query string parsing with serde support
- **CORS Support**: Optional CORS middleware configuration
- **Compression**: Optional response compression support
- **OpenAPI Integration**: Optional OpenAPI documentation generation
- **Multiple Backends**: Support for different server implementations (Actix Web)
- **Error Handling**: Structured error types with HTTP status codes

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_web_server = "0.1.1"

# Enable features as needed
moosicbox_web_server = {
    version = "0.1.1",
    features = ["actix", "cors", "compress", "openapi"]
}
```

## Usage

### Basic Server Setup

```rust
use moosicbox_web_server::{WebServerBuilder, Scope, Route, Method, HttpRequest, HttpResponse};
use std::future::Future;
use std::pin::Pin;

fn main() {
    let server = WebServerBuilder::new()
        .with_addr("127.0.0.1")
        .with_port(8080)
        .with_scope(
            Scope::new("/api")
                .with_route(Route {
                    path: "/health",
                    method: Method::Get,
                    handler: &|_req| Box::pin(async {
                        Ok(HttpResponse::ok().with_body("OK"))
                    }),
                })
        );

    println!("Server configured for 127.0.0.1:8080");
}
```

### Creating Routes and Scopes

```rust
use moosicbox_web_server::{Scope, Route, Method, HttpRequest, HttpResponse, Error};

fn create_api_routes() -> Scope {
    Scope::new("/api/v1")
        .with_routes([
            Route {
                path: "/users",
                method: Method::Get,
                handler: &|req| Box::pin(async move {
                    // Handle GET /api/v1/users
                    Ok(HttpResponse::ok().with_body(r#"{"users": []}"#))
                }),
            },
            Route {
                path: "/users",
                method: Method::Post,
                handler: &|req| Box::pin(async move {
                    // Handle POST /api/v1/users
                    Ok(HttpResponse::from_status_code(201).with_body(r#"{"created": true}"#))
                }),
            },
        ])
        .with_scope(
            Scope::new("/admin")
                .with_route(Route {
                    path: "/stats",
                    method: Method::Get,
                    handler: &|req| Box::pin(async move {
                        Ok(HttpResponse::ok().with_body(r#"{"stats": {}}"#))
                    }),
                })
        )
}
```

### Request Handling

```rust
use moosicbox_web_server::{HttpRequest, HttpResponse, Error};
use serde::Deserialize;

#[derive(Deserialize)]
struct QueryParams {
    page: Option<u32>,
    limit: Option<u32>,
}

async fn handle_request(req: HttpRequest) -> Result<HttpResponse, Error> {
    // Access request properties
    let path = req.path();
    let query_string = req.query_string();

    // Parse query parameters
    let params: QueryParams = req.parse_query()?;
    let page = params.page.unwrap_or(1);
    let limit = params.limit.unwrap_or(10);

    // Access headers
    if let Some(auth_header) = req.header("Authorization") {
        println!("Auth header: {}", auth_header);
    }

    // Return response
    Ok(HttpResponse::ok().with_body(format!(
        r#"{{"path": "{}", "page": {}, "limit": {}}}"#,
        path, page, limit
    )))
}
```

### Response Types

```rust
use moosicbox_web_server::{HttpResponse, HttpResponseBody};
use switchy_http_models::StatusCode;

fn response_examples() -> Vec<HttpResponse> {
    vec![
        // Basic responses
        HttpResponse::ok(),
        HttpResponse::not_found(),
        HttpResponse::temporary_redirect(),
        HttpResponse::permanent_redirect(),

        // Custom status codes
        HttpResponse::from_status_code(StatusCode::Created),
        HttpResponse::new(404),

        // With body content
        HttpResponse::ok().with_body("Hello, World!"),
        HttpResponse::ok().with_body(b"Binary data".to_vec()),
        HttpResponse::ok().with_body(serde_json::json!({"key": "value"})),

        // With location header
        HttpResponse::temporary_redirect().with_location("https://example.com"),

        // Custom responses
        HttpResponse::new(StatusCode::Accepted)
            .with_body(r#"{"status": "accepted"}"#)
            .with_location("/status/123"),
    ]
}
```

### CORS Configuration

```rust
#[cfg(feature = "cors")]
use moosicbox_web_server::{WebServerBuilder, cors::Cors};

#[cfg(feature = "cors")]
fn server_with_cors() {
    let cors = Cors::default()
        .allow_origin("https://example.com")
        .allow_methods(["GET", "POST", "PUT", "DELETE"])
        .allow_headers(["Content-Type", "Authorization"]);

    let server = WebServerBuilder::new()
        .with_port(8080)
        .with_cors(cors);
}
```

### Compression Support

```rust
#[cfg(feature = "compress")]
use moosicbox_web_server::WebServerBuilder;

#[cfg(feature = "compress")]
fn server_with_compression() {
    let server = WebServerBuilder::new()
        .with_port(8080)
        .with_compress(true);
}
```

### Error Handling

```rust
use moosicbox_web_server::{Error, HttpResponse};
use switchy_http_models::StatusCode;

fn error_examples() -> Vec<Error> {
    vec![
        Error::bad_request("Invalid input data".into()),
        Error::unauthorized("Missing authentication".into()),
        Error::not_found("Resource not found".into()),
        Error::internal_server_error("Database connection failed".into()),

        Error::from_http_status_code(
            StatusCode::UnprocessableEntity,
            "Validation failed"
        ),

        Error::from_http_status_code_u16(
            429,
            "Rate limit exceeded"
        ),
    ]
}

async fn error_handler() -> Result<HttpResponse, Error> {
    // Return different error types
    if some_condition() {
        return Err(Error::bad_request("Invalid request".into()));
    }

    if another_condition() {
        return Err(Error::not_found("Resource not found".into()));
    }

    Ok(HttpResponse::ok())
}

fn some_condition() -> bool { false }
fn another_condition() -> bool { false }
```

### OpenAPI Integration

```rust
#[cfg(feature = "openapi")]
use moosicbox_web_server::utoipa;

#[cfg(feature = "openapi")]
mod openapi_example {
    use super::*;

    #[utoipa::path(
        get,
        path = "/api/users",
        responses(
            (status = 200, description = "List of users")
        )
    )]
    async fn get_users() -> Result<HttpResponse, Error> {
        Ok(HttpResponse::ok().with_body(r#"{"users": []}"#))
    }
}
```

## API Reference

### Core Types

- **`WebServerBuilder`** - Builder for configuring web servers
- **`HttpRequest`** - Unified HTTP request interface
- **`HttpResponse`** - HTTP response builder
- **`Scope`** - Route grouping and nesting
- **`Route`** - Individual route definition
- **`Error`** - HTTP error types with status codes

### Request Methods

- `path()` - Get request path
- `query_string()` - Get raw query string
- `parse_query<T>()` - Parse query string into typed struct
- `header(name)` - Get header value by name

### Response Methods

- `ok()`, `not_found()`, `temporary_redirect()` - Common status codes
- `from_status_code()`, `new()` - Custom status codes
- `with_body()` - Set response body
- `with_location()` - Set location header

### Builder Methods

- `with_addr()`, `with_port()` - Server address configuration
- `with_scope()` - Add route scope
- `with_cors()` - Configure CORS (requires `cors` feature)
- `with_compress()` - Enable compression (requires `compress` feature)

## Features

- `actix` - Enable Actix Web backend support
- `cors` - Enable CORS middleware support
- `compress` - Enable response compression
- `openapi` - Enable OpenAPI documentation generation

## Error Types

- `Error::Http` - HTTP errors with status codes and source errors
- Built-in constructors for common HTTP status codes
- Automatic conversion from query parsing errors

## Examples

This package includes comprehensive examples demonstrating various web server features and patterns. Examples are located in the `examples/` directory.

### Prerequisites

- Rust toolchain (see root README)
- Understanding of async Rust
- Basic HTTP knowledge

### Running Examples

Examples require feature flags to select the backend implementation:

```bash
# With Actix Web (default backend)
cargo run --example [example_name] --features actix

# With Simulator (for testing)
cargo run --example [example_name] --features simulator
```

### Available Examples

#### Standalone Examples (Single Files)

**Handler Macros** (`handler_macros.rs`)
- **Purpose**: Demonstrates handler macro usage for 0-2 parameter handlers
- **Run**: `cargo run --example handler_macros --features actix`
- **Shows**: Different handler signatures and macro expansions

**Query Extractor** (`query_extractor.rs`)
- **Purpose**: Shows Query<T> extractor for URL parameters
- **Run**: `cargo run --example query_extractor --features actix`
- **Shows**: Parsing query strings into typed structs with serde

**JSON Extractor** (`json_extractor.rs`)
- **Purpose**: Demonstrates Json<T> extractor for request bodies
- **Run**: `cargo run --example json_extractor --features actix`
- **Shows**: JSON deserialization and response handling

**Combined Extractors** (`combined_extractors.rs`)
- **Purpose**: Shows multiple extractors working together
- **Run**: `cargo run --example combined_extractors --features actix`
- **Shows**: Complex handlers with Path, Query, and Json extractors

#### Directory Examples (With Individual READMEs)

**Basic Handler** (`basic_handler/`)
- **Purpose**: Fundamental handler implementation using RequestData
- **Run**: `cargo run --example basic_handler --features actix`
- **Shows**: Basic request/response handling with the new abstraction layer

**Simple GET** (`simple_get/`)
- **Purpose**: Simple GET endpoint implementation
- **Run**: `cargo run --example simple_get --features actix`
- **Shows**: Basic routing and response generation

**Nested GET** (`nested_get/`)
- **Purpose**: Demonstrates nested route structures
- **Run**: `cargo run --example nested_get --features actix`
- **Shows**: Route organization and scope nesting

**OpenAPI Integration** (`openapi/`)
- **Purpose**: OpenAPI documentation generation
- **Run**: `cargo run --example openapi --features "actix,openapi-all"`
- **Shows**: API documentation with utoipa integration

### Testing Examples

#### Unit Tests
```bash
cargo test --example [example_name] --features actix
```

#### Manual Testing with curl

**GET Requests**
```bash
curl http://localhost:8080/endpoint
```

**POST with JSON**
```bash
curl -X POST http://localhost:8080/endpoint \
  -H "Content-Type: application/json" \
  -d '{"key": "value"}'
```

**Query Parameters**
```bash
curl "http://localhost:8080/endpoint?page=1&limit=10"
```

### Troubleshooting

#### Feature Flag Issues
**Problem**: "trait bound not satisfied" errors
**Solution**: Ensure correct feature flags are enabled (`actix` or `simulator`)

#### Port Conflicts
**Problem**: "address already in use"
**Solution**: Change port in example or kill existing process with `lsof -ti:8080 | xargs kill`

#### Compilation Errors
**Problem**: Missing traits or types
**Solution**: Check feature dependencies and ensure all required features are enabled

### Current Architecture Limitations

The web server abstraction currently requires feature flags to select between Actix and Simulator backends. This is a known limitation that will be addressed in future versions.

Examples must use conditional compilation:
- `#[cfg(feature = "actix")]` for Actix-specific code
- `#[cfg(feature = "simulator")]` for test simulator code

Future versions will provide a unified API that removes this requirement.

### Migration Guide

#### From Raw Actix Web

**Handler Changes**
- Replace `HttpRequest` with `RequestData` for Send-safety
- Use handler macros instead of manual implementations
- Extractors remain mostly the same but work through the abstraction layer

**Route Registration**
```rust
// Before (raw Actix)
App::new().route("/api/users", web::get().to(get_users))

// After (MoosicBox abstraction)
Scope::new("/api").with_route(Route {
    path: "/users",
    method: Method::Get,
    handler: &get_users_handler,
})
```

## Dependencies

- `switchy_http_models` - HTTP types and status codes
- `serde_querystring` - Query string parsing
- `moosicbox_web_server_core` - Core server functionality
- `moosicbox_web_server_cors` - CORS middleware (optional)
- `utoipa` - OpenAPI support (optional)
