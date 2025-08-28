# Simple GET Example

This example demonstrates how to create a basic web server with a single GET endpoint using the MoosicBox web server abstraction. It shows the fundamental patterns for server setup, CORS configuration, and route definition.

## What This Example Demonstrates

- **WebServerBuilder Usage**: Creating a web server with the builder pattern
- **CORS Configuration**: Setting up permissive CORS for development
- **Route Definition**: Using `Scope::get()` for simple route creation
- **Request Handling**: Accessing request path and query string
- **Response Generation**: Creating HTTP responses with dynamic content

## Prerequisites

- Rust toolchain (see root README)
- Understanding of async Rust
- Basic HTTP and CORS knowledge

## Running the Example

### With Actix Web (Production Backend)
```bash
# From repository root
cargo run --example simple_get --features actix

# From example directory
cd packages/web_server/examples/simple_get
cargo run --features actix

# With NixOS
nix develop .#server --command cargo run --example simple_get --features actix
```

### With Simulator (Testing Backend)
```bash
# From repository root
cargo run --example simple_get --features simulator

# From example directory
cd packages/web_server/examples/simple_get
cargo run --features simulator

# With NixOS
nix develop .#server --command cargo run --example simple_get --features simulator
```

## Expected Output

When you run the example, the server will start and listen on the default port. You can then make requests to test the endpoint.

## Testing the Server

### Manual Testing with curl

**Basic GET Request**
```bash
curl http://localhost:8080/example
# Expected: hello, world! path=/example query=
```

**With Query Parameters**
```bash
curl "http://localhost:8080/example?name=test&value=123"
# Expected: hello, world! path=/example query=name=test&value=123
```

**Test CORS Headers**
```bash
curl -H "Origin: https://example.com" \
     -H "Access-Control-Request-Method: GET" \
     -X OPTIONS \
     http://localhost:8080/example
# Should return CORS headers allowing the request
```

## Code Walkthrough

### Server Configuration
```rust
let server = moosicbox_web_server::WebServerBuilder::new()
    .with_cors(cors)                    // Enable CORS
    .with_scope(                        // Add route scope
        Scope::new("")                  // Root scope
            .get("/example", handler)   // GET route
    )
    .build();
```

### CORS Setup
```rust
let cors = moosicbox_web_server::cors::Cors::default()
    .allow_any_origin()     // Allow requests from any origin
    .allow_any_method()     // Allow any HTTP method
    .allow_any_header()     // Allow any request headers
    .expose_any_header();   // Expose any response headers
```

### Route Handler
```rust
.get("/example", |req| {
    let path = req.path().to_string();      // Extract path
    let query = req.query_string().to_string(); // Extract query
    Box::pin(async move {
        Ok(HttpResponse::ok()
            .with_body(format!("hello, world! path={path} query={query}")))
    })
})
```

## Key Concepts

### WebServerBuilder Pattern
- **Fluent API**: Chain configuration methods for clean setup
- **Flexible Configuration**: Add CORS, scopes, middleware as needed
- **Backend Agnostic**: Same API works with different server implementations

### Scope-Based Routing
- **Hierarchical Organization**: Group related routes under scopes
- **Method Shortcuts**: `.get()`, `.post()`, `.put()`, `.delete()` for common methods
- **Path Composition**: Scope path + route path = full endpoint path

### Request Information Access
- **Path Access**: `req.path()` returns the request path
- **Query String**: `req.query_string()` returns raw query parameters
- **Header Access**: `req.header("name")` for specific headers

## Differences from basic_handler

| Aspect | simple_get | basic_handler |
|--------|------------|---------------|
| **Route Registration** | Uses `Scope::get()` shortcut | Uses `Route` struct with handler |
| **Request Type** | Uses `HttpRequest` directly | Uses `RequestData` for Send-safety |
| **Handler Style** | Inline closure | Separate async function |
| **Information Access** | Direct method calls | Pre-extracted fields |

## Architecture Notes

### Current Implementation
- Uses the older `HttpRequest` API (pre-RequestData)
- Demonstrates the scope-based routing system
- Shows CORS integration with the web server

### Migration Path
This example could be updated to use:
- `RequestData` instead of `HttpRequest` for better Send-safety
- Handler macros for cleaner syntax
- Extractors for more sophisticated request parsing

## Troubleshooting

### Feature Flag Issues
**Problem**: "trait bound not satisfied" errors
**Solution**: Ensure either `actix` or `simulator` feature is enabled

### Port Conflicts
**Problem**: "address already in use"
**Solution**: Kill existing process or change port:
```bash
lsof -ti:8080 | xargs kill
```

### CORS Issues
**Problem**: Browser blocks requests due to CORS
**Solution**: This example uses permissive CORS settings, but check browser dev tools for specific errors

## Related Examples

- **basic_handler**: Shows RequestData usage and handler registration
- **nested_get**: Demonstrates nested scopes and route organization
- **query_extractor**: Shows typed query parameter parsing
- **openapi**: Adds API documentation to similar endpoints

This example provides a foundation for understanding the MoosicBox web server's routing system and demonstrates how to create simple HTTP endpoints with CORS support.
