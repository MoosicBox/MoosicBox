# Basic Handler Example

This example demonstrates the fundamental handler implementation using the new `RequestData` abstraction layer. It shows how to create handlers that work with the MoosicBox web server abstraction while maintaining Send-safety.

## What This Example Demonstrates

- **RequestData Usage**: Using `RequestData` instead of `HttpRequest` for Send-safe handlers
- **Handler System**: New `Route::with_handler()` method for registering handlers
- **Request Information Access**: Accessing method, path, query, headers, and remote address
- **Dual Backend Support**: Same handler code works with both Actix and Simulator backends
- **Response Generation**: Creating HTTP responses with formatted content

## Prerequisites

- Rust toolchain (see root README)
- Understanding of async Rust
- Basic HTTP knowledge

## Running the Example

### With Actix Web (Production Backend)
```bash
# From repository root
cargo run --example basic_handler --features actix

# From example directory
cd packages/web_server/examples/basic_handler
cargo run --features actix

# With NixOS
nix-shell --run "cargo run --example basic_handler --features actix"
```

### With Simulator (Testing Backend)
```bash
# From repository root
cargo run --example basic_handler --features simulator

# From example directory
cd packages/web_server/examples/basic_handler
cargo run --features simulator

# With NixOS
nix-shell --run "cargo run --example basic_handler --features simulator"
```

## Expected Output

When you run the example, you'll see:
```
Server configured for 127.0.0.1:8080
=== New Handler System Demonstration ===

HTTP Method: Get
Path: /demo
Query String: None
User-Agent: None
Content-Type: None
All Headers: 0 found
Remote Address: None
Body: Not available in RequestData (use Json<T> extractor for body parsing)

=== Handler Registration Success ===
✓ Handler successfully registered with Route::with_handler()
✓ RequestData provides Send-safe access to request information
✓ Same handler works with both Actix and Simulator backends
```

## Testing the Handler

### Manual Testing with curl

**Basic GET Request**
```bash
curl http://localhost:8080/demo
```

**With Query Parameters**
```bash
curl "http://localhost:8080/demo?page=1&limit=10"
```

**With Headers**
```bash
curl -H "User-Agent: TestClient/1.0" \
     -H "Content-Type: application/json" \
     http://localhost:8080/demo
```

## Code Walkthrough

### Key Components

**RequestData Structure**
```rust
async fn demo_handler(data: RequestData) -> Result<HttpResponse, Error> {
    // RequestData provides Send-safe access to:
    // - data.method: HTTP method
    // - data.path: Request path
    // - data.query: Query string
    // - data.headers: Header collection
    // - data.user_agent: User-Agent header
    // - data.content_type: Content-Type header
    // - data.remote_addr: Client IP address
}
```

**Handler Registration**
```rust
let route = Route {
    path: "/demo",
    method: Method::Get,
    handler: &demo_handler,
};
```

### Why RequestData Instead of HttpRequest?

- **Send Safety**: `RequestData` is Send + Sync, allowing use in async contexts
- **Simplified Access**: Pre-extracted common request information
- **Backend Agnostic**: Same structure works with any backend implementation
- **Performance**: Avoids repeated header lookups and parsing

## Architecture Notes

### Current Implementation

This example demonstrates the current web server abstraction layer:
- Uses `RequestData` for Send-safe request handling
- Requires feature flags to select backend (`actix` or `simulator`)
- Handler registration through `Route::with_handler()`

### Limitations

- **Feature Flag Dependency**: Must choose backend at compile time
- **Limited Body Access**: RequestData doesn't include body (use extractors instead)
- **Backend-Specific Code**: Some conditional compilation still required

### Future Improvements

The web server abstraction is being enhanced to:
- Remove feature flag requirements
- Provide unified server execution API
- Add comprehensive extractor system
- Enable runtime backend selection

## Troubleshooting

### Feature Flag Issues
**Problem**: "trait bound not satisfied" errors
**Solution**: Ensure either `actix` or `simulator` feature is enabled

### Port Conflicts
**Problem**: "address already in use"
**Solution**: Change port or kill existing process:
```bash
lsof -ti:8080 | xargs kill
```

### Compilation Errors
**Problem**: Missing RequestData or handler traits
**Solution**: Check that web server dependencies are correctly configured

## Related Examples

- **handler_macros.rs**: Shows handler macro usage patterns
- **query_extractor.rs**: Demonstrates query parameter extraction
- **json_extractor.rs**: Shows JSON body parsing
- **combined_extractors.rs**: Multiple extractors working together

This example serves as the foundation for understanding the MoosicBox web server abstraction and demonstrates the current state of the handler system implementation.
