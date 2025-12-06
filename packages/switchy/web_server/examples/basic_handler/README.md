# Basic Handler Example

This example demonstrates the fundamental handler implementation using the `Route::with_handler1()` method and the `RequestData` abstraction layer. It shows how to create handlers that work with the MoosicBox web server abstraction while maintaining Send-safety.

## What This Example Demonstrates

- **RequestData Usage**: Using `RequestData` instead of `HttpRequest` for Send-safe handlers
- **Handler System**: New `Route::with_handler1()` method for registering handlers with one parameter
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
cargo run -p basic_handler_example --features actix

# With NixOS
nix develop .#server --command cargo run -p basic_handler_example --features actix
```

### With Simulator (Testing Backend - Default)

```bash
# From repository root
cargo run -p basic_handler_example --features simulator

# Or simply (simulator is the default feature)
cargo run -p basic_handler_example

# With NixOS
nix develop .#server --command cargo run -p basic_handler_example --features simulator
```

## Expected Output

### Actix Backend

When you run with the `actix` feature:

```
🎯 Basic Handler Example - Route::with_handler() Method
=====================================================

🚀 Running Actix Backend Example...
✅ Route created successfully with new handler system:
   Method: Post
   Path: /demo
   Handler: Clean async function (no Box::pin!)
   Backend: Actix Web

✅ Basic Handler Example Complete!
   - Route::with_handler1() method working
   - Clean async function syntax (no Box::pin boilerplate)
   - Works identically with both Actix and Simulator backends
   - RequestData provides Send-safe access to request information
   - Ready for production use with the new handler system
```

### Simulator Backend

When you run with the `simulator` feature (or default):

```
🎯 Basic Handler Example - Route::with_handler() Method
=====================================================

🧪 Running Simulator Backend Example...
✅ Route created successfully with new handler system:
   Method: Post
   Path: /demo
   Handler: Clean async function (no Box::pin!)
   Backend: Simulator

📋 Handler would receive RequestData:
   Method: Post
   Path: /demo
   Query: test=1&debug=true
   User-Agent: Some("MoosicBox-Test/1.0")
   Content-Type: Some("application/json")
   Remote Address: Some(192.168.1.100:54321)
   Headers: 2 total

✅ RequestData extraction successful!
   Handler would process this data and return an HttpResponse
   Note: Full async execution requires an async runtime

✅ Basic Handler Example Complete!
   - Route::with_handler1() method working
   - Clean async function syntax (no Box::pin boilerplate)
   - Works identically with both Actix and Simulator backends
   - RequestData provides Send-safe access to request information
   - Ready for production use with the new handler system
```

**Note**: This example demonstrates the handler registration mechanism but does not start an actual HTTP server. For a complete server example, see the `basic_handler_standalone` example.

## Testing the Handler

This example demonstrates the handler registration API but does not start an HTTP server. To test a working handler with actual HTTP requests, use the `basic_handler_standalone` example which includes a complete server setup.

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
// Using the new Route::with_handler1() method for handlers with one parameter
let route = Route::with_handler1(Method::Post, "/demo", demo_handler);
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
- Handler registration through `Route::with_handler1()` for single-parameter handlers
- Does not start an actual HTTP server (for full server examples, see `basic_handler_standalone`)

### Limitations

- **Feature Flag Dependency**: Must choose backend at compile time
- **Limited Body Access**: RequestData doesn't include body (use extractors instead)
- **Backend-Specific Code**: Some conditional compilation still required
- **No Server Execution**: This example only demonstrates handler registration without running a server

### Future Improvements

Planned: The web server abstraction is being enhanced to:

- Remove feature flag requirements
- Provide unified server execution API
- Add comprehensive extractor system
- Enable runtime backend selection

## Troubleshooting

### Feature Flag Issues

**Problem**: "trait bound not satisfied" errors
**Solution**: Ensure either `actix` or `simulator` feature is enabled:

```bash
cargo run -p basic_handler_example --features actix
# or
cargo run -p basic_handler_example --features simulator
```

### Compilation Errors

**Problem**: Missing RequestData or handler traits
**Solution**: Check that web server dependencies are correctly configured in workspace

## Related Examples

- **basic_handler_standalone**: Complete example with running HTTP server
- **handler_macro_test**: Shows handler macro usage patterns
- **query_extractor_standalone**: Demonstrates query parameter extraction
- **json_extractor_standalone**: Shows JSON body parsing
- **combined_extractors_standalone**: Multiple extractors working together
- **from_request_test**: Tests the FromRequest trait implementation

This example serves as the foundation for understanding the MoosicBox web server abstraction and demonstrates the current state of the handler registration API using `Route::with_handler1()`.
