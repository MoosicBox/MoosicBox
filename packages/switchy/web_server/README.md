# Switchy Web Server

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
switchy_web_server = "0.1.0"

# Enable features as needed
switchy_web_server = {
    version = "0.1.0",
    features = ["actix", "cors", "compress", "openapi"]
}
```

## Usage

### Basic Server Setup

```rust
use switchy_web_server::{WebServerBuilder, Scope, HttpResponse};

#[tokio::main]
async fn main() {
    let server = WebServerBuilder::new()
        .with_addr("127.0.0.1")
        .with_port(8080)
        .with_scope(
            Scope::new("/api").get("/health", |_req| {
                Box::pin(async {
                    Ok(HttpResponse::ok().with_body("OK"))
                })
            })
        )
        .build();

    server.start().await;
}
```

### Creating Routes and Scopes

```rust
use switchy_web_server::{Scope, HttpResponse, Error};
use switchy_http_models::StatusCode;

fn create_api_routes() -> Scope {
    Scope::new("/api/v1")
        .get("/users", |_req| {
            Box::pin(async move {
                // Handle GET /api/v1/users
                Ok(HttpResponse::ok().with_body(r#"{"users": []}"#))
            })
        })
        .post("/users", |_req| {
            Box::pin(async move {
                // Handle POST /api/v1/users
                Ok(HttpResponse::from_status_code(StatusCode::Created)
                    .with_body(r#"{"created": true}"#))
            })
        })
        .with_scope(
            Scope::new("/admin").get("/stats", |_req| {
                Box::pin(async move {
                    Ok(HttpResponse::ok().with_body(r#"{"stats": {}}"#))
                })
            })
        )
}
```

### Request Handling

```rust
use switchy_web_server::{HttpRequest, HttpResponse, Error};
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

### Path Parameter Extraction

```rust
#[cfg(feature = "serde")]
use switchy_web_server::{Path, HttpResponse, Error};

#[cfg(feature = "serde")]
async fn get_user(Path(user_id): Path<u32>) -> Result<HttpResponse, Error> {
    // Extract single path parameter from routes like "/users/123"
    Ok(HttpResponse::ok().with_body(format!(r#"{{"user_id": {}}}"#, user_id)))
}

#[cfg(feature = "serde")]
async fn get_user_post(Path((username, post_id)): Path<(String, u32)>) -> Result<HttpResponse, Error> {
    // Extract multiple path parameters from routes like "/users/john/posts/456"
    Ok(HttpResponse::ok().with_body(format!(
        r#"{{"username": "{}", "post_id": {}}}"#,
        username, post_id
    )))
}

#[cfg(feature = "serde")]
use serde::Deserialize;

#[cfg(feature = "serde")]
#[derive(Deserialize)]
struct UserPostParams {
    username: String,
    post_id: u32,
}

#[cfg(feature = "serde")]
async fn get_user_post_named(Path(params): Path<UserPostParams>) -> Result<HttpResponse, Error> {
    // Extract named path parameters using a struct
    Ok(HttpResponse::ok().with_body(format!(
        r#"{{"username": "{}", "post_id": {}}}"#,
        params.username, params.post_id
    )))
}
```

### Response Types

```rust
use switchy_web_server::{HttpResponse, HttpResponseBody};
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
        HttpResponse::new(StatusCode::NotFound),

        // With body content
        HttpResponse::ok().with_body("Hello, World!"),
        HttpResponse::ok().with_body(b"Binary data".to_vec()),

        // Convenience methods with automatic Content-Type headers
        HttpResponse::text("Plain text response"),
        HttpResponse::html("<h1>HTML response</h1>"),

        // With location header
        HttpResponse::temporary_redirect().with_location("https://example.com"),

        // Custom responses
        HttpResponse::new(StatusCode::Accepted)
            .with_body(r#"{"status": "accepted"}"#)
            .with_location("/status/123"),
    ]
}

// JSON responses (require 'serde' feature)
#[cfg(feature = "serde")]
fn json_response_examples() -> Result<Vec<HttpResponse>, switchy_web_server::Error> {
    use serde_json::json;

    Ok(vec![
        // Using json() method with automatic Content-Type header
        HttpResponse::json(&json!({"key": "value"}))?,

        // Using with_body() for manual JSON
        HttpResponse::ok().with_body(json!({"manual": true})),
    ])
}
```

### CORS Configuration

```rust
#[cfg(feature = "cors")]
use switchy_web_server::{WebServerBuilder, cors::Cors, Method};

#[cfg(feature = "cors")]
fn server_with_cors() {
    let cors = Cors::default()
        .allow_origin("https://example.com")
        .allowed_methods([Method::Get, Method::Post, Method::Put, Method::Delete])
        .allowed_headers(["Content-Type", "Authorization"]);

    let server = WebServerBuilder::new()
        .with_port(8080)
        .with_cors(cors);
}
```

### Compression Support

```rust
#[cfg(feature = "compress")]
use switchy_web_server::WebServerBuilder;

#[cfg(feature = "compress")]
fn server_with_compression() {
    let server = WebServerBuilder::new()
        .with_port(8080)
        .with_compress(true);
}
```

### Error Handling

```rust
use switchy_web_server::{Error, HttpResponse};
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
use switchy_web_server::{utoipa, openapi};
#[cfg(feature = "openapi")]
use utoipa::openapi::OpenApi;

#[cfg(feature = "openapi")]
fn setup_openapi() -> OpenApi {
    // Build OpenAPI specification
    OpenApi::builder()
        .tags(Some([utoipa::openapi::Tag::builder()
            .name("API")
            .build()]))
        .paths(
            utoipa::openapi::Paths::builder()
                // Add your paths here
                .build(),
        )
        .components(Some(utoipa::openapi::Components::builder().build()))
        .build()
}

#[cfg(feature = "openapi")]
fn create_server_with_openapi() {
    // Set the OpenAPI spec
    *openapi::OPENAPI.write().unwrap() = Some(setup_openapi());

    let server = switchy_web_server::WebServerBuilder::new()
        // Add OpenAPI UI routes
        .with_scope(openapi::bind_services(Scope::new("/openapi")))
        // Add your API routes
        .with_scope(Scope::new("/api").get("/users", |_req| {
            Box::pin(async {
                Ok(HttpResponse::ok().with_body(r#"{"users": []}"#))
            })
        }))
        .build();
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

### Extractors

- **`Path<T>`** - Extract URL path parameters (requires `serde` feature)
- **`Query<T>`** - Extract query parameters (requires `serde` feature)
- **`Json<T>`** - Extract JSON request body (requires `serde` feature)
- **`Headers`** - Extract request headers in a Send-safe way
- **`RequestData`** - Send-safe wrapper containing commonly needed request data
- **`RequestInfo`** - Basic request information (method, path, query, remote address)

### Request Methods

- `path()` - Get request path
- `path_params()` - Get all path parameters as a map
- `path_param(name)` - Get a specific path parameter by name
- `query_string()` - Get raw query string
- `parse_query<T>()` - Parse query string into typed struct
- `header(name)` - Get header value by name
- `method()` - Get HTTP method
- `body()` - Get request body (for simulator backend)
- `cookie(name)` - Get cookie value by name
- `cookies()` - Get all cookies as a map
- `remote_addr()` - Get remote client address

### Response Methods

- `ok()`, `not_found()`, `temporary_redirect()`, `permanent_redirect()` - Common status codes
- `from_status_code()`, `new()` - Custom status codes
- `with_body()` - Set response body
- `with_location()` - Set location header
- `with_header()` - Add a single header
- `with_headers()` - Add multiple headers
- `with_content_type()` - Set Content-Type header
- `json()` - Create JSON response with automatic Content-Type (requires `serde` feature)
- `html()` - Create HTML response with automatic Content-Type
- `text()` - Create plain text response with automatic Content-Type

### Builder Methods

**WebServerBuilder Methods:**

- `with_addr()`, `with_port()` - Server address configuration
- `with_scope()` - Add route scope
- `with_cors()` - Configure CORS (requires `cors` feature)
- `with_compress()` - Enable compression (requires `compress` feature)
- `build()` - Build the web server

**Scope Methods:**

- `new(path)` - Create a new scope with a base path
- `with_route()` - Add a single route
- `with_routes()` - Add multiple routes
- `with_scope()` - Add a nested scope
- `with_scopes()` - Add multiple nested scopes
- `route(method, path, handler)` - Add a route with a specific HTTP method
- `get(path, handler)` - Add a GET route
- `post(path, handler)` - Add a POST route
- `put(path, handler)` - Add a PUT route
- `delete(path, handler)` - Add a DELETE route
- `patch(path, handler)` - Add a PATCH route
- `head(path, handler)` - Add a HEAD route

**Route Methods:**

- `new(method, path, handler)` - Create a new route
- `with_handler(method, path, handler)` - Create route with handler that supports extractors
- `get(path, handler)` - Create a GET route
- `post(path, handler)` - Create a POST route
- `put(path, handler)` - Create a PUT route
- `delete(path, handler)` - Create a DELETE route
- `patch(path, handler)` - Create a PATCH route
- `head(path, handler)` - Create a HEAD route

## Features

Default features: `actix`, `compress`, `cors`, `htmx`, `openapi-all`, `serde`, `tls`

Available features:

- `actix` - Enable Actix Web backend support (enabled by default)
- `simulator` - Enable test simulator backend (for testing without Actix)
- `serde` - Enable JSON serialization/deserialization support (enabled by default)
- `cors` - Enable CORS middleware support (enabled by default)
- `compress` - Enable response compression (enabled by default)
- `htmx` - Enable HTMX integration support (enabled by default)
- `static-files` - Enable static file serving support
- `tls` - Enable TLS/SSL support (OpenSSL) (enabled by default)
- `openapi` - Enable OpenAPI documentation generation
- `openapi-all` - Enable all OpenAPI UI variants (enabled by default)
- `openapi-rapidoc` - Enable RapiDoc OpenAPI UI
- `openapi-redoc` - Enable ReDoc OpenAPI UI
- `openapi-scalar` - Enable Scalar OpenAPI UI
- `openapi-swagger-ui` - Enable SwaggerUI OpenAPI UI

## Error Types

- `Error::Http` - HTTP errors with status codes and source errors
- Built-in constructors for common HTTP status codes
- Automatic conversion from query parsing errors

## Examples

This package includes comprehensive examples demonstrating various web server features and patterns. Examples are located in the `examples/` directory as standalone Cargo projects.

### Prerequisites

- Rust toolchain (see root README)
- Understanding of async Rust
- Basic HTTP knowledge

### Example Structure

Each example is a complete Cargo project with:

- Its own `Cargo.toml` with appropriate dependencies
- Comprehensive `README.md` with usage instructions
- Self-contained code demonstrating specific features
- Support for both Actix and Simulator backends

### Running Examples

The standalone examples are workspace members and can be run directly:

```bash
# Run with default features (simulator)
cargo run -p basic_handler_standalone_example
cargo run -p json_extractor_standalone_example
cargo run -p query_extractor_standalone_example
cargo run -p combined_extractors_standalone_example

# Run with Actix backend
cargo run -p basic_handler_standalone_example --features actix --no-default-features
cargo run -p json_extractor_standalone_example --features actix --no-default-features
```

### Available Examples

#### Standalone Example Projects

Each example is a complete Cargo project with its own dependencies and comprehensive README:

**Basic Handler** (`basic_handler_standalone/`)

- **Purpose**: Demonstrates RequestData extraction without any serde dependencies
- **Run**: `cargo run -p basic_handler_standalone_example`
- **Features**: Simple request handling, multiple extractors, no JSON dependencies
- **[Full Documentation](examples/basic_handler_standalone/README.md)**

**JSON Extractor** (`json_extractor_standalone/`)

- **Purpose**: Shows JSON request/response handling with serde
- **Run**: `cargo run -p json_extractor_standalone_example`
- **Features**: Json<T> extractor, optional fields, JSON responses, error handling
- **[Full Documentation](examples/json_extractor_standalone/README.md)**

**Query Extractor** (`query_extractor_standalone/`)

- **Purpose**: Demonstrates query parameter parsing with serde
- **Run**: `cargo run -p query_extractor_standalone_example`
- **Features**: Query<T> extractor, optional parameters, type-safe parsing
- **[Full Documentation](examples/query_extractor_standalone/README.md)**

**Combined Extractors** (`combined_extractors_standalone/`)

- **Purpose**: Shows multiple extractors working together
- **Run**: `cargo run -p combined_extractors_standalone_example`
- **Features**: Query + RequestData, Json + RequestData combinations, JSON API patterns
- **[Full Documentation](examples/combined_extractors_standalone/README.md)**

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

**From Request Test** (`from_request_test/`)

- **Purpose**: Testing FromRequest trait implementations
- **Shows**: Custom extractors and request data extraction

**Handler Macro Test** (`handler_macro_test/`)

- **Purpose**: Testing handler macros and code generation
- **Shows**: Advanced handler patterns and macro usage

**OpenAPI Integration** (`openapi/`)

- **Purpose**: OpenAPI documentation generation
- **Run**: `cargo run --example openapi --features "actix,openapi-all"`
- **Shows**: API documentation with utoipa integration

### Testing Examples

#### Running Tests

```bash
# Test individual examples
cargo test -p basic_handler_standalone_example
cargo test -p json_extractor_standalone_example

# Test the main web_server package
cargo test -p switchy_web_server --features "actix,serde"
```

#### Manual Testing with curl

The standalone examples include detailed curl examples in their individual READMEs. When running with Actix backend:

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

// After (Switchy abstraction)
Scope::new("/api").with_route(Route {
    path: "/users",
    method: Method::Get,
    handler: &get_users_handler,
})
```

## Dependencies

Core dependencies:

- `switchy_http_models` - HTTP types and status codes
- `serde-querystring` - Query string parsing
- `switchy_web_server_core` - Core server functionality
- `bytes` - Efficient byte buffer handling
- `futures` - Async runtime utilities

Optional dependencies (feature-gated):

- `switchy_web_server_cors` - CORS middleware (with `cors` feature)
- `actix-web` - Actix Web server backend (with `actix` feature)
- `actix-cors` - Actix CORS support (with `cors` feature)
- `actix-htmx` - HTMX integration (with `htmx` feature)
- `serde_json` - JSON serialization (with `serde` feature)
- `utoipa` - OpenAPI specification support (with `openapi` feature)
- `utoipa-swagger-ui`, `utoipa-rapidoc`, `utoipa-redoc`, `utoipa-scalar` - OpenAPI UI variants (with respective `openapi-*` features)
