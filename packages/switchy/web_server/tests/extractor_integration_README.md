# Extractor Integration Tests

Comprehensive integration tests for the MoosicBox web server extractor system, validating that all extractors work correctly with both Actix and Simulator backends.

## Overview

This test suite validates the dual-mode extractor system that allows the same handler code to work with both synchronous (Actix) and asynchronous (Simulator) backends. The tests focus on compilation safety, type correctness, and backend consistency.

## Test Structure

### Test Modules

- **`actix_tests`** - Tests for Actix backend (synchronous extraction)
- **`simulator_tests`** - Tests for Simulator backend (asynchronous extraction)
- **`consistency_tests`** - Cross-backend consistency validation
- **`edge_case_tests`** - Edge cases and error conditions (simulator only)
- **`performance_tests`** - Performance and stress tests (simulator only)
- **`benchmarks`** - Compilation-time benchmarks

### Test Coverage

| Extractor Type   | Actix Tests | Simulator Tests | Edge Cases | Performance |
| ---------------- | ----------- | --------------- | ---------- | ----------- |
| **Query**        | ✅          | ✅              | ✅         | ✅          |
| **Json**         | ✅          | ✅              | ✅         | ✅          |
| **Path**         | ✅          | ✅              | ✅         | -           |
| **Header**       | ✅          | ✅              | ✅         | ✅          |
| **State**        | ✅          | ✅              | -          | -           |
| **Combinations** | ✅          | ✅              | -          | -           |

## Running Tests

### Basic Test Execution

```bash
# Test with Actix backend (default)
cargo test -p moosicbox_web_server extractor_integration

# Test with Simulator backend
cargo test -p moosicbox_web_server --features simulator extractor_integration

# Test with all features
cargo test -p moosicbox_web_server --all-features extractor_integration
```

### Specific Test Groups

```bash
# Run only Actix tests
cargo test -p moosicbox_web_server --features actix actix_tests

# Run only Simulator tests
cargo test -p moosicbox_web_server --features simulator simulator_tests

# Run consistency tests
cargo test -p moosicbox_web_server --all-features consistency_tests

# Run edge case tests (requires simulator)
cargo test -p moosicbox_web_server --features simulator edge_case_tests

# Run performance tests (requires simulator)
cargo test -p moosicbox_web_server --features simulator performance_tests
```

### Individual Extractor Tests

```bash
# Test specific extractor types
cargo test -p moosicbox_web_server --features actix test_query_extractor_compilation
cargo test -p moosicbox_web_server --features actix test_json_extractor_compilation
cargo test -p moosicbox_web_server --features actix test_path_extractor_compilation
cargo test -p moosicbox_web_server --features actix test_header_extractor_compilation
cargo test -p moosicbox_web_server --features actix test_state_extractor_compilation
```

## Test Results

### Actix Backend Tests

- **7 tests** covering all extractor types
- **Compilation focus**: Validates synchronous extraction with Send bounds
- **All tests passing** ✅

### Simulator Backend Tests

- **8 tests** covering all extractor types + StateContainer integration
- **Compilation focus**: Validates asynchronous extraction
- **All tests passing** ✅

### Total Test Coverage

- **15 compilation tests** across both backends
- **4 consistency tests** ensuring identical behavior
- **8 edge case tests** for error conditions
- **3 performance tests** for stress testing
- **1 benchmark test** for compilation performance

## Test Philosophy

### Compilation-First Approach

These tests prioritize **compilation safety** over runtime validation:

- **Type Safety**: Ensures all extractor combinations compile correctly
- **Trait Bounds**: Validates Send/Sync requirements are met
- **Feature Gates**: Tests conditional compilation based on enabled features
- **Backend Consistency**: Same handler signatures work across backends

### Progressive Enhancement Path

1. **Current**: Compilation safety and type correctness ✅
2. **Future**: Runtime validation with actual request/response testing
3. **Later**: Performance benchmarks and optimization

## Key Test Scenarios

### Individual Extractors

Each extractor type is tested independently:

```rust
// Query extractor
fn handler(_query: Query<String>) -> Result<String, Error> { ... }

// JSON extractor
fn handler(_json: Json<TestData>) -> Result<String, Error> { ... }

// Path extractor
fn handler(_path: Path<String>) -> Result<String, Error> { ... }

// Header extractor
fn handler(_header: Header<String>) -> Result<String, Error> { ... }

// State extractor
fn handler(_state: State<TestState>) -> Result<String, Error> { ... }
```

### Multiple Extractors

Complex handlers with multiple extractors:

```rust
fn complex_handler(
    _query: Query<String>,
    _json: Json<TestData>,
    _path: Path<String>,
    _header: Header<String>,
    _state: State<TestState>,
) -> Result<String, Error> { ... }
```

### Error Handling

Consistent error propagation across backends:

```rust
fn error_handler(_query: Query<String>) -> Result<String, Error> {
    Err("test error".into())
}
```

### Edge Cases

- **Optional extractors**: `Query<Option<String>>`
- **Complex JSON**: Nested structures and arrays
- **Missing data**: Headers, query parameters, path segments
- **Large payloads**: Stress testing with large JSON objects
- **Many parameters**: Handlers with numerous extractors

## Backend-Specific Features

### Actix Backend

- **Synchronous extraction**: No async/await required
- **Send bounds**: All extractors implement Send
- **Direct integration**: Works with existing actix-web handlers

### Simulator Backend

- **Asynchronous extraction**: Uses async/await
- **StateContainer**: Enhanced state management
- **Simulation data**: Pre-loaded request data for testing

## Extending the Tests

### Adding New Extractor Types

1. Add compilation tests in both `actix_tests` and `simulator_tests`
2. Add consistency tests in `consistency_tests`
3. Add edge case tests in `edge_case_tests` if applicable
4. Update this README with the new extractor coverage

### Adding Runtime Tests

Future enhancement to add actual request/response validation:

```rust
#[test]
async fn test_query_extractor_runtime() {
    let request = create_test_request_with_query("name=test");
    let result = extract_query::<String>(request).await;
    assert_eq!(result.unwrap(), "test");
}
```

### Adding Performance Tests

Future enhancement for performance validation:

```rust
#[test]
fn benchmark_extractor_performance() {
    let start = Instant::now();
    // ... perform extraction operations
    let duration = start.elapsed();
    assert!(duration < Duration::from_millis(1));
}
```

## Troubleshooting

### Common Issues

1. **Feature flag errors**: Ensure correct features are enabled
2. **Import errors**: Check that all required imports are present
3. **Compilation errors**: Verify extractor trait bounds are satisfied

### Debug Commands

```bash
# Check compilation without running tests
cargo build -p moosicbox_web_server --tests --all-features

# Check for clippy warnings
cargo clippy -p moosicbox_web_server --tests --all-features

# Verbose test output
cargo test -p moosicbox_web_server --all-features extractor_integration -- --nocapture
```

## Integration with CI/CD

These tests are designed to run in continuous integration:

- **Fast execution**: Compilation-focused tests run quickly
- **Feature matrix**: Tests run with different feature combinations
- **Zero warnings**: All tests pass clippy validation
- **Cross-platform**: Works on all supported platforms

## Future Enhancements

1. **Runtime Validation**: Execute handlers with real requests
2. **Performance Benchmarks**: Measure extraction overhead
3. **Memory Usage**: Track allocation patterns
4. **Error Message Quality**: Validate helpful error messages
5. **Migration Examples**: Real-world usage patterns

---

**Generated with [opencode](https://opencode.ai)**
