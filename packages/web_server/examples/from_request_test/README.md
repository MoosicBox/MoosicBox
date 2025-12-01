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
# From example directory
cd packages/web_server/examples/from_request_test
cargo run --bin test_sync_extraction

# From repository root
cargo run --package from_request_test --bin test_sync_extraction

# With NixOS
nix develop .#server --command cargo run --package from_request_test --bin test_sync_extraction
```

### Asynchronous Extraction Tests

```bash
# From example directory
cd packages/web_server/examples/from_request_test
cargo run --bin test_async_extraction

# From repository root
cargo run --package from_request_test --bin test_async_extraction

# With NixOS
nix develop .#server --command cargo run --package from_request_test --bin test_async_extraction
```

### Build All Tests

```bash
# Test compilation of both binaries
cargo build --bins

# Build specific test
cargo build --bin test_sync_extraction
```

## Test Coverage

### Synchronous Extraction Tests

(Implemented in `src/test_sync_extraction.rs`)

- **RequestData Extraction**: Validates all RequestData fields (method, path, query, headers, user_agent, content_type, remote_addr)
- **String Extraction**: Tests query string extraction
- **u32 Extraction**: Tests numeric parsing with valid inputs and error handling for invalid inputs
- **bool Extraction**: Tests boolean variants (true/1/yes/on vs false/0/no/off/anything_else)
- **Error Cases**: Validates proper error handling with "Failed to parse" error messages

### Asynchronous Extraction Tests

(Implemented in `src/test_async_extraction.rs`)

- **RequestData Async**: Tests async extraction of RequestData with POST method
- **i32 Async Extraction**: Tests negative number parsing asynchronously with error handling
- **Future Types**: Validates Future resolution for String and RequestData types
- **Consistency Check**: Compares sync vs async results for identical behavior with same query strings
- **Async Error Handling**: Tests error propagation through async extraction with "Failed to parse" errors

## Expected Test Results

### Successful Validation

✅ **Data Extraction**: All extraction methods successfully parse valid inputs
✅ **Field Validation**: RequestData fields match expected values
✅ **Type Conversion**: Primitive types correctly parsed from strings
✅ **Error Handling**: Invalid inputs produce appropriate error messages
✅ **Sync/Async Consistency**: Both modes produce identical results

### Test Output Example

```
🧪 Testing synchronous extraction with FromRequest trait...

Testing RequestData sync extraction...
✅ RequestData extracted successfully
  Method: Get
  Path: /test/path
  Query: name=john&age=30&active=true
  Headers count: 3
✅ All RequestData fields extracted correctly

Testing String extraction...
✅ String extracted: 'hello world'

Testing u32 extraction...
✅ u32 extracted: 42
✅ u32 extraction properly failed for invalid input: Failed to parse...

Testing bool extraction...
✅ bool('true') = true
✅ bool('1') = true
[... additional bool test cases ...]

🎉 All synchronous FromRequest tests passed!

---

🧪 Testing asynchronous extraction with FromRequest trait...

Testing RequestData async extraction...
✅ RequestData extracted asynchronously
  Method: Post
  Path: /api/users
  Query: filter=active&limit=10
  Headers count: 3
✅ All RequestData fields extracted correctly via async

Testing async vs sync extraction consistency...
✅ Sync result: 'consistency_test=123'
✅ Async result: 'consistency_test=123'
✅ Sync and async extraction produce identical results

Testing i32 async extraction...
✅ i32 extracted asynchronously: -42
✅ i32 async extraction properly failed for invalid input: Failed to parse...

Testing Future types are properly implemented...
✅ Future<String> resolved correctly: 'future_test'
✅ Future<RequestData> resolved correctly
  Method: Post

🎉 All asynchronous FromRequest tests passed!
```

## Code Structure

### Test Request Creation

```rust
fn create_test_request() -> HttpRequest {
    // Creates HttpRequest with test data:
    // - Method: Varies by test (GET/POST)
    // - Path: Varies by test
    // - Query: Varies by test
    // - Headers: user-agent, content-type, etc.
}
```

### Extraction Testing

```rust
// Sync extraction
let data = RequestData::from_request_sync(&req)?;
assert_eq!(data.method, Method::Get);
assert_eq!(data.path, "/test");

// Async extraction
let data = RequestData::from_request_async(req).await?;
assert_eq!(data.method, Method::Get);
```

### Error Case Testing

```rust
// Test invalid number parsing
let result = u32::from_request_sync(&invalid_req);
assert!(result.is_err());
assert!(result.unwrap_err().to_string().contains("Failed to parse"));
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

### Dependency Issues

**Problem**: FromRequest trait not available
**Solution**: The package uses the `simulator` feature by default (configured in Cargo.toml). No additional feature flags are needed for basic usage.

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
- **query_extractor_standalone**: Demonstrates Query<T> extractor usage
- **json_extractor_standalone**: Shows JSON body extraction
- **combined_extractors_standalone**: Multiple parameter extraction patterns
- **basic_handler**: RequestData usage in handlers
- **basic_handler_standalone**: Standalone basic handler example

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
