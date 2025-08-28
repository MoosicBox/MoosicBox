# FromRequest Trait Test Example

This example provides comprehensive validation of the dual-mode FromRequest trait implementation, testing both synchronous and asynchronous parameter extraction with real data and error cases.

## What This Example Demonstrates

- **Dual-Mode Extraction**: Testing both sync and async FromRequest implementations
- **RequestData Extraction**: Complete validation of RequestData field extraction
- **Type Conversion**: Testing extraction of various primitive types (String, u32, i32, bool)
- **Error Handling**: Comprehensive error case testing with proper error messages
- **Consistency Validation**: Ensuring sync and async methods produce identical results
- **Real Test Coverage**: Actual method calls with assertions, not just compilation tests

## Prerequisites

- Rust toolchain (see root README)
- Understanding of async Rust and trait systems
- Basic knowledge of HTTP request parameter extraction

## Available Test Binaries

- **test_sync_extraction**: Tests synchronous parameter extraction
- **test_async_extraction**: Tests asynchronous parameter extraction

## Running the Tests

### Synchronous Extraction Tests
```bash
# From repository root
cargo run --bin test_sync_extraction --example from_request_test --features actix

# From example directory
cd packages/web_server/examples/from_request_test
cargo run --bin test_sync_extraction --features actix

# With NixOS
nix develop .#server --command cargo run --bin test_sync_extraction --example from_request_test --features actix
```

### Asynchronous Extraction Tests
```bash
# From repository root
cargo run --bin test_async_extraction --example from_request_test --features actix

# From example directory
cd packages/web_server/examples/from_request_test
cargo run --bin test_async_extraction --features actix

# With NixOS
nix develop .#server --command cargo run --bin test_async_extraction --example from_request_test --features actix
```

### Build All Tests
```bash
# Test compilation of both binaries
cargo build --bins --example from_request_test --features actix

# Build specific test
cargo build --bin test_sync_extraction --example from_request_test --features actix
```

## Test Coverage

### Synchronous Extraction Tests
- **RequestData Extraction**: Validates all RequestData fields (method, path, query, headers, etc.)
- **String Extraction**: Tests query string parameter extraction
- **u32 Extraction**: Tests numeric parsing with valid/invalid inputs
- **bool Extraction**: Tests boolean variants (true/1/yes/on vs false/0/no/off)
- **Error Cases**: Validates proper error handling for invalid inputs

### Asynchronous Extraction Tests
- **RequestData Async**: Tests async extraction of RequestData
- **i32 Async Extraction**: Tests negative number parsing asynchronously
- **Future Types**: Validates Future associated types work correctly
- **Consistency Check**: Compares sync vs async results for identical behavior
- **Async Error Handling**: Tests error propagation through async extraction

## Expected Test Results

### Successful Validation
✅ **Data Extraction**: All extraction methods successfully parse valid inputs
✅ **Field Validation**: RequestData fields match expected values
✅ **Type Conversion**: Primitive types correctly parsed from strings
✅ **Error Handling**: Invalid inputs produce appropriate error messages
✅ **Sync/Async Consistency**: Both modes produce identical results

### Test Output Example
```
=== Sync Extraction Tests ===
✓ RequestData extraction successful
✓ String extraction: "test_value"
✓ u32 extraction: 42
✓ bool extraction: true
✓ Error handling: Invalid u32 rejected

=== Async Extraction Tests ===
✓ RequestData async extraction successful
✓ i32 async extraction: -123
✓ Sync/async consistency verified
✓ Async error handling working
```

## Code Structure

### Mock Request Creation
```rust
fn create_mock_request() -> HttpRequest {
    // Creates HttpRequest with test data:
    // - Method: GET
    // - Path: /test
    // - Query: param=value&number=42
    // - Headers: User-Agent, Content-Type
}
```

### Extraction Testing
```rust
// Sync extraction
let data = RequestData::from_request_sync(&req)?;
assert_eq!(data.method, Method::Get);
assert_eq!(data.path, "/test");

// Async extraction
let data = RequestData::from_request_async(&req).await?;
assert_eq!(data.method, Method::Get);
```

### Error Case Testing
```rust
// Test invalid number parsing
let result = u32::from_request_sync(&invalid_req);
assert!(result.is_err());
assert!(result.unwrap_err().to_string().contains("invalid"));
```

## Test Quality Features

### Real Method Calls
- 🎯 **Actual Execution**: Tests call real FromRequest methods
- 🔍 **Result Validation**: Assertions verify extracted values
- ❌ **Error Testing**: Invalid inputs tested for proper error handling
- 📊 **Behavior Comparison**: Sync vs async consistency validation

### Comprehensive Coverage
- **All Types**: Tests RequestData, String, u32, i32, bool extraction
- **Valid Cases**: Tests successful extraction with various inputs
- **Invalid Cases**: Tests error handling with malformed data
- **Edge Cases**: Tests boundary conditions and special values

### Production Readiness
- **Real Data**: Uses realistic HTTP request data
- **Error Messages**: Validates error message quality
- **Performance**: Tests both sync and async performance characteristics
- **Consistency**: Ensures reliable behavior across extraction modes

## Troubleshooting

### Feature Flag Issues
**Problem**: FromRequest trait not available
**Solution**: Ensure correct feature flags are enabled:
```bash
--features actix        # for Actix backend
--features simulator    # for Simulator backend
```

### Test Failures
**Problem**: Extraction tests failing
**Solution**: Check that mock request data matches expected format

### Compilation Errors
**Problem**: FromRequest trait not found
**Solution**: Verify web server dependencies are correctly configured

## Development Usage

### Validation Tool
This example serves as a validation tool for:
- FromRequest trait implementations
- Parameter extraction logic
- Error handling consistency
- Sync/async behavior parity

### Regression Testing
Use this example to:
- Verify changes don't break extraction
- Test new parameter types
- Validate error handling improvements
- Ensure backend compatibility

### Performance Testing
The example can be extended for:
- Extraction performance benchmarks
- Memory usage validation
- Async overhead measurement
- Error handling performance

## Related Examples

- **handler_macro_test**: Tests handler macro system
- **query_extractor**: Demonstrates Query<T> extractor usage
- **json_extractor**: Shows JSON body extraction
- **combined_extractors**: Multiple parameter extraction patterns
- **basic_handler**: RequestData usage in handlers

## Implementation Notes

### Dual-Mode Design
The FromRequest trait supports both sync and async extraction:
- **Sync Mode**: Immediate extraction for simple types
- **Async Mode**: Future-based extraction for complex operations
- **Consistency**: Both modes produce identical results

### Error Handling
Comprehensive error handling includes:
- **Type Conversion Errors**: Invalid string-to-type conversions
- **Missing Parameters**: Required parameters not found
- **Format Errors**: Malformed request data
- **Descriptive Messages**: Clear error descriptions for debugging

This example provides the foundation for understanding and validating the parameter extraction system in the MoosicBox web server framework.
