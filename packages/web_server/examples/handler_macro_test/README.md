# Handler Macro Test Example

This example validates the handler macro system implementation, testing various handler signatures with different parameter counts across both Actix and Simulator backends.

## What This Example Demonstrates

- **Handler Macro System**: Testing the `impl_handler!` macro for 0-16 parameter handlers
- **Backend Compatibility**: Same handler code working with both Actix and Simulator
- **Parameter Extraction**: Current state of parameter extraction implementation
- **Compilation Validation**: Ensuring all handler signatures compile correctly
- **Error Handling**: Proper error messages for unimplemented features

## Prerequisites

- Rust toolchain (see root README)
- Understanding of async Rust and handler patterns
- Basic knowledge of macro systems

## Available Test Binaries

- **test_actix**: Tests handler macros with Actix backend
- **test_simulator**: Tests handler macros with Simulator backend
- **debug_actix**: Debug version for Actix backend development

## Running the Tests

### Actix Backend Tests
```bash
# From repository root
cargo run --bin test_actix --example handler_macro_test --features actix

# From example directory
cd packages/web_server/examples/handler_macro_test
cargo run --bin test_actix --features actix

# With NixOS
nix develop .#server --command cargo run --bin test_actix --example handler_macro_test --features actix
```

### Simulator Backend Tests
```bash
# From repository root
cargo run --bin test_simulator --example handler_macro_test --features simulator

# From example directory
cd packages/web_server/examples/handler_macro_test
cargo run --bin test_simulator --features simulator

# With NixOS
nix develop .#server --command cargo run --bin test_simulator --example handler_macro_test --features simulator
```

### Debug Mode
```bash
# Debug Actix backend behavior
cargo run --bin debug_actix --example handler_macro_test --features actix
```

### Build All Binaries
```bash
# Test compilation of all binaries
cargo build --bins --example handler_macro_test --features "actix,simulator"

# Build specific binary
cargo build --bin test_actix --example handler_macro_test --features actix
```

## Expected Results

### Successful Compilation
All handler signatures should compile without errors:
- 0-parameter handlers (legacy implementation)
- 1-16 parameter handlers (macro-generated implementations)

### Runtime Behavior
- ✅ **Route Registration**: Handlers successfully convert to Route objects
- ✅ **Backend Compatibility**: Same code works with both backends
- 📝 **Parameter Extraction**: Currently returns "not implemented" for 1+ parameters
- 📝 **Send Bounds**: Some limitations with async parameter extraction

## Code Structure

### Handler Signatures Tested
```rust
// 0 parameters - fully implemented
async fn handler_0() -> Result<HttpResponse, Error>

// 1 parameter - macro generated
async fn handler_1(param1: Type1) -> Result<HttpResponse, Error>

// 2 parameters - macro generated
async fn handler_2(param1: Type1, param2: Type2) -> Result<HttpResponse, Error>

// ... up to 16 parameters
```

### Macro System
```rust
// The impl_handler! macro generates implementations like:
impl_handler!(T1);
impl_handler!(T1, T2);
impl_handler!(T1, T2, T3);
// ... up to 16 parameters
```

### Backend Testing
```rust
#[cfg(feature = "actix")]
fn test_actix_handlers() {
    // Test handlers with Actix backend
}

#[cfg(feature = "simulator")]
fn test_simulator_handlers() {
    // Test handlers with Simulator backend
}
```

## Current Implementation Status

### ✅ Completed Features
- **Macro Generation**: All handler signatures (0-16 params) compile
- **Route Conversion**: Handlers successfully convert to Route objects
- **Backend Abstraction**: Same handler code works with both backends
- **Error Handling**: Proper error messages for unimplemented features

### 📝 Known Limitations
- **Parameter Extraction**: 1+ parameter handlers return "not implemented"
- **Send Bounds**: Some async parameter extraction limitations
- **Type Safety**: Limited compile-time parameter validation

### 🔄 Future Improvements
- Complete parameter extraction implementation
- Resolve Send bound issues
- Add compile-time parameter validation
- Implement more extractor types

## Troubleshooting

### Feature Flag Issues
**Problem**: "trait bound not satisfied" errors
**Solution**: Ensure correct backend feature is enabled:
```bash
--features actix        # for Actix backend
--features simulator    # for Simulator backend
```

### Compilation Errors
**Problem**: Handler macro compilation failures
**Solution**: Check that parameter types implement required traits

### Runtime Errors
**Problem**: "not implemented" errors for parameterized handlers
**Solution**: This is expected behavior - parameter extraction is not yet fully implemented

## Testing Strategy

### Compilation Tests
```bash
# Verify all handler signatures compile
cargo check --example handler_macro_test --features actix
cargo check --example handler_macro_test --features simulator
```

### Runtime Tests
```bash
# Test actual handler execution
cargo run --bin test_actix --example handler_macro_test --features actix
cargo run --bin test_simulator --example handler_macro_test --features simulator
```

### Cross-Backend Validation
```bash
# Ensure same behavior across backends
cargo build --example handler_macro_test --features "actix,simulator"
```

## Related Examples

- **handler_macros.rs**: Demonstrates working handler macro usage
- **basic_handler**: Shows RequestData-based handlers
- **query_extractor**: Parameter extraction patterns
- **json_extractor**: Request body handling
- **combined_extractors**: Multiple parameter extraction

## Development Notes

This example serves as a validation tool for the handler macro system development. It helps ensure that:

1. **Macro Generation Works**: All parameter combinations compile
2. **Backend Compatibility**: Handlers work across different backends
3. **Error Handling**: Proper error messages for incomplete features
4. **Future Readiness**: Foundation is ready for full parameter extraction

The example is particularly useful during development of the web server abstraction layer, providing immediate feedback on macro system changes and backend compatibility.
