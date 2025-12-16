# Handler Macro Test Example

This example validates the handler extractor system implementation, testing various handler signatures with different extractor types across both Actix and Simulator backends.

## What This Example Demonstrates

- **Handler Extractor System**: Testing handlers with 0-2 parameter extractors
- **Backend Compatibility**: Same handler code working with both Actix and Simulator
- **Parameter Extraction**: Working implementations of RequestInfo, Headers, Query, and Path extractors
- **Compilation Validation**: Ensuring all handler signatures compile correctly
- **Route Conversion**: Handlers successfully converting to Route objects

## Prerequisites

- Rust toolchain (see root README)
- Understanding of async Rust and handler patterns
- Basic knowledge of extractor patterns

## Available Test Binaries

- **test_actix**: Tests handler extractors with Actix backend (includes Path extractor)
- **test_simulator**: Tests handler extractors with Simulator backend
- **debug_actix**: Debug version testing IntoHandler trait implementation

## Running the Tests

### Actix Backend Tests

```bash
# From repository root
cargo run -p switchy_web_server_example_handler_macro_test --bin test_actix --features switchy_web_server/actix

# From example directory
cd packages/switchy/web_server/examples/handler_macro_test
cargo run --bin test_actix --features switchy_web_server/actix

# With NixOS
nix develop .#server --command cargo run -p switchy_web_server_example_handler_macro_test --bin test_actix --features switchy_web_server/actix
```

### Simulator Backend Tests

```bash
# From repository root
cargo run -p switchy_web_server_example_handler_macro_test --bin test_simulator --features switchy_web_server/simulator

# From example directory
cd packages/switchy/web_server/examples/handler_macro_test
cargo run --bin test_simulator --features switchy_web_server/simulator

# With NixOS
nix develop .#server --command cargo run -p switchy_web_server_example_handler_macro_test --bin test_simulator --features switchy_web_server/simulator
```

### Debug Mode

```bash
# Debug Actix backend behavior
cargo run -p switchy_web_server_example_handler_macro_test --bin debug_actix --features switchy_web_server/actix
```

### Build All Binaries

```bash
# Test compilation of all binaries
cargo build -p switchy_web_server_example_handler_macro_test --bins --features "switchy_web_server/actix,switchy_web_server/simulator"

# Build specific binary
cargo build -p switchy_web_server_example_handler_macro_test --bin test_actix --features switchy_web_server/actix
```

## Expected Results

### Successful Compilation

All handler signatures should compile without errors:

- 0-parameter handlers
- 1-parameter handlers (with single extractor)
- 2-parameter handlers (with multiple extractors)

### Runtime Behavior

- ✅ **Route Registration**: Handlers successfully convert to Route objects
- ✅ **Backend Compatibility**: Same code works with both backends
- ✅ **Parameter Extraction**: Working extractors for RequestInfo, Headers, Query, and Path
- ✅ **Send-Safe**: All extractors are Send-safe with no async boundary issues

## Code Structure

### Handler Signatures Tested

```rust
// 0 parameters - fully implemented
async fn simple_handler() -> Result<HttpResponse, Error>

// 1 parameter - RequestInfo extractor
async fn info_handler(info: RequestInfo) -> Result<HttpResponse, Error>

// 1 parameter - Headers extractor
async fn headers_handler(headers: Headers) -> Result<HttpResponse, Error>

// 1 parameter - Query extractor
async fn query_handler(Query(query): Query<SearchQuery>) -> Result<HttpResponse, Error>

// 1 parameter - Path extractor (Actix only)
async fn path_handler(Path(user_id): Path<u32>) -> Result<HttpResponse, Error>

// 2 parameters - multiple extractors
async fn multi_handler(info: RequestInfo, headers: Headers) -> Result<HttpResponse, Error>
```

### Route Registration

```rust
// 0 parameters
let route = Route::with_handler(Method::Get, "/hello", simple_handler);

// 1 parameter
let route = Route::with_handler1(Method::Get, "/info", info_handler);

// 2 parameters
let route = Route::with_handler2(Method::Get, "/multi", multi_handler);
```

### Extractors Available

```rust
// RequestInfo - provides request metadata (path, method, etc.)
async fn handler(info: RequestInfo) -> Result<HttpResponse, Error>

// Headers - provides access to HTTP headers
async fn handler(headers: Headers) -> Result<HttpResponse, Error>

// Query<T> - extracts query parameters
async fn handler(Query(params): Query<MyStruct>) -> Result<HttpResponse, Error>

// Path<T> - extracts path parameters (Actix only)
async fn handler(Path(id): Path<u32>) -> Result<HttpResponse, Error>
```

## Current Implementation Status

### ✅ Completed Features

- **Handler Registration**: Handlers with 0-2 parameters successfully compile and convert to Route objects
- **Route Conversion**: `Route::with_handler`, `Route::with_handler1`, and `Route::with_handler2` work correctly
- **Backend Abstraction**: Same handler code works with both Actix and Simulator backends
- **Working Extractors**:
    - `RequestInfo` - request metadata (path, method, etc.)
    - `Headers` - HTTP header access (including user-agent)
    - `Query<T>` - query parameter extraction with serde deserialization
    - `Path<T>` - path parameter extraction from URL segments (Actix only)
- **Multiple Extractors**: Handlers can combine multiple extractors (up to 2 parameters tested)
- **Send-Safe Design**: All extractors work without Send bound issues

### 📝 Known Limitations

- **Path Extractor**: Only available in Actix backend (not yet implemented for Simulator)
- **Parameter Count**: Currently tested up to 2 parameters (not 3+)
- **Macro Syntax**: TODOs reference future `#[get("/path")]` attribute macro syntax

### 🔮 Planned Future Improvements

- Attribute macro syntax for route definitions (e.g., `#[get("/path")]`)
- Path extractor support for Simulator backend
- Additional extractor types (Json body, Form data, etc.)
- Support for 3+ parameter handlers

## Troubleshooting

### Feature Flag Issues

**Problem**: "trait bound not satisfied" errors
**Solution**: Ensure correct backend feature is enabled:

```bash
--features switchy_web_server/actix        # for Actix backend
--features switchy_web_server/simulator    # for Simulator backend
```

### Compilation Errors

**Problem**: Handler trait bound errors
**Solution**: Ensure parameter types implement the `FromRequest` trait and are used with the correct `Route::with_handlerN` method

### Runtime Errors

**Problem**: Path extractor not working in Simulator backend
**Solution**: This is expected - Path extractor is currently only implemented for Actix backend

## Testing Strategy

### Compilation Tests

```bash
# Verify all handler signatures compile
cargo check -p switchy_web_server_example_handler_macro_test --features switchy_web_server/actix
cargo check -p switchy_web_server_example_handler_macro_test --features switchy_web_server/simulator
```

### Runtime Tests

```bash
# Test actual handler execution
cargo run -p switchy_web_server_example_handler_macro_test --bin test_actix --features switchy_web_server/actix
cargo run -p switchy_web_server_example_handler_macro_test --bin test_simulator --features switchy_web_server/simulator
```

### Cross-Backend Validation

```bash
# Ensure same behavior across backends
cargo build -p switchy_web_server_example_handler_macro_test --features "switchy_web_server/actix,switchy_web_server/simulator"
```

## Related Documentation

For more information about the web server abstraction layer and handler system, see:

- The main `switchy_web_server` package documentation
- Other examples in `packages/switchy/web_server/examples/`

## Development Notes

This example serves as a validation tool for the handler extractor system development. It helps ensure that:

1. **Extractor Implementation Works**: All tested extractors (RequestInfo, Headers, Query, Path) function correctly
2. **Backend Compatibility**: Handlers work across Actix and Simulator backends
3. **Send-Safety**: No Send bound issues with extractor implementations
4. **Route Conversion**: Handler functions properly convert to Route objects

The example is particularly useful during development of the web server abstraction layer, providing immediate feedback on extractor implementations and backend compatibility. The debug_actix binary specifically tests the `IntoHandler` trait implementation to verify the underlying trait system works correctly.
