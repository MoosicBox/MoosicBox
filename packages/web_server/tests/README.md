# MoosicBox Web Server Integration Tests

## Overview

This directory contains integration tests for the MoosicBox web server's handler system, validating dual-mode support for both Actix (production) and Simulator (testing) backends.

The tests ensure that the same handler code works identically across both backends, providing confidence for the broader MoosicBox web server migration from direct Actix usage to the abstracted dual-mode system.

## Test Structure

### `handler_integration.rs`

**Purpose**: Tests the compilation and type safety of the handler system across different backends.

**What it tests:**

- ✅ Handler compilation with 0-16 parameters
- ✅ All extractor types (Query, Json, Path, Header, State)
- ✅ Error handling consistency across backends
- ✅ Backend compatibility (Actix vs Simulator)
- ✅ Type safety and trait bounds

**What it DOESN'T test (yet):**

- ❌ Actual HTTP request/response flow
- ❌ Runtime extraction behavior with real data
- ❌ Performance characteristics
- ❌ Real network operations
- ❌ Error propagation with actual errors

**Test Philosophy**: These are **compilation-focused** tests that validate the handler system works at the type level. They prove that handlers compile correctly but don't execute them with real requests.

## Running Tests

### Basic Test Execution

```bash
# Run with default features (Actix backend only)
cargo test -p moosicbox_web_server --test handler_integration

# Run with simulator feature (both backends)
cargo test -p moosicbox_web_server --test handler_integration --features simulator

# Run all web_server tests
cargo test -p moosicbox_web_server --all-targets --all-features
```

### Development Workflow

```bash
# Check compilation without running tests
cargo build -p moosicbox_web_server --tests

# Run with verbose output
cargo test -p moosicbox_web_server --test handler_integration -- --nocapture

# Run specific test module
cargo test -p moosicbox_web_server --test handler_integration actix_tests

# Run with clippy to check code quality
cargo clippy -p moosicbox_web_server --tests --all-features
```

### Feature-Specific Testing

```bash
# Test only Actix backend (default)
cargo test -p moosicbox_web_server --test handler_integration --no-default-features --features=actix,serde

# Test only Simulator backend
cargo test -p moosicbox_web_server --test handler_integration --no-default-features --features=simulator,serde

# Test both backends
cargo test -p moosicbox_web_server --test handler_integration --features=simulator,serde
```

## Test Coverage

| Backend       | Tests               | Purpose                                     |
| ------------- | ------------------- | ------------------------------------------- |
| **Actix**     | 4 compilation tests | Validates sync extraction with Send bounds  |
| **Simulator** | 5 compilation tests | Validates async extraction + StateContainer |
| **Both**      | 2 consistency tests | Ensures identical handler signatures        |
| **Total**     | **11-12 tests**     | Depends on enabled features                 |

### Test Breakdown

#### Actix Tests (`actix_tests` module)

- `test_0_param_handler_compilation` - Handlers with no parameters
- `test_1_param_handler_compilation` - Single extractor handlers
- `test_multi_param_handler_compilation` - 2-5 parameter handlers
- `test_error_handler_compilation` - Error-producing handlers

#### Simulator Tests (`simulator_tests` module)

- Same compilation tests as Actix
- `test_state_container_functionality` - StateContainer direct testing

#### Consistency Tests (`consistency_tests` module)

- `test_handler_signature_consistency` - Same signatures work on both backends
- `test_error_handler_consistency` - Error handling compiles consistently

## Handler Examples Tested

The tests validate these handler patterns:

```rust
// 0-parameter handler
async fn handler_0_params() -> Result<HttpResponse, Error>

// 1-parameter handlers
async fn handler_query(Query(params): Query<SearchParams>) -> Result<HttpResponse, Error>
async fn handler_json(Json(data): Json<UserData>) -> Result<HttpResponse, Error>
async fn handler_path(Path(id): Path<u64>) -> Result<HttpResponse, Error>
async fn handler_header(Header(auth): Header<String>) -> Result<HttpResponse, Error>
async fn handler_state(State(config): State<AppConfig>) -> Result<HttpResponse, Error>

// Multi-parameter handlers (2-5 parameters)
async fn handler_complex(
    Query(params): Query<SearchParams>,
    Json(data): Json<UserData>,
    Path(id): Path<u64>,
    Header(auth): Header<String>,
    State(config): State<AppConfig>,
) -> Result<HttpResponse, Error>

// Error handlers
async fn handler_with_error(Query(params): Query<SearchParams>) -> Result<HttpResponse, Error>
```

## Extending the Tests

### Adding New Handler Patterns

When adding new extractors or handler patterns:

1. **Add test handler function**:

```rust
pub async fn handler_new_pattern(
    NewExtractor(data): NewExtractor<DataType>
) -> Result<HttpResponse, Error> {
    Ok(test_utils::json_response(&data))
}
```

2. **Add compilation tests**:

```rust
#[test]
fn test_new_extractor_compilation() {
    let _handler = test_handlers::handler_new_pattern.into_handler();
    // If this compiles, the test passes
}
```

3. **Test in both backends** by adding to both `actix_tests` and `simulator_tests` modules.

### Adding Runtime Tests (Future Enhancement)

To add actual runtime testing, create a new test file:

**File: `tests/handler_runtime.rs`**

```rust
#[cfg(all(feature = "simulator", feature = "serde"))]
mod runtime_tests {
    use super::*;

    #[tokio::test]
    async fn test_query_extraction_runtime() {
        let handler = test_handlers::handler_1_param_query.into_handler();
        let req = create_test_request_with_query("name=test&age=30");

        let response = handler(req).await.unwrap();
        assert_eq!(response.status(), 200);
        // TODO: Validate response body contains expected data
    }
}
```

### Adding Performance Benchmarks (Future Enhancement)

**File: `tests/performance_benchmarks.rs`**

```rust
#[cfg(feature = "simulator")]
mod benchmarks {
    use criterion::{black_box, criterion_group, criterion_main, Criterion};

    fn benchmark_handler_overhead(c: &mut Criterion) {
        c.bench_function("4_param_handler", |b| {
            b.iter(|| {
                let handler = test_handlers::handler_4_params.into_handler();
                black_box(handler);
            });
        });
    }
}
```

## Architecture Notes

### Dual-Mode Handler System

The handler system supports two modes:

1. **Actix Mode** (`feature = "actix"`):
    - Uses synchronous extraction (`from_request_sync`)
    - Avoids Send bounds issues with Actix's request handling
    - Production-focused with performance optimizations

2. **Simulator Mode** (`feature = "simulator"`):
    - Uses asynchronous extraction (`from_request_async`)
    - Enables deterministic testing
    - Development and testing focused

### Extractor System

All extractors implement the `FromRequest` trait with dual-mode support:

- **Query**: URL query parameters using `serde_urlencoded`
- **Json**: JSON request body using `serde_json`
- **Path**: URL path segments with flexible extraction
- **Header**: HTTP headers with type conversion
- **State**: Application state with backend-specific storage

### Feature Gates

Tests are carefully feature-gated:

- `#[cfg(feature = "actix")]` - Actix-specific tests
- `#[cfg(feature = "simulator")]` - Simulator-specific tests
- `#[cfg(feature = "serde")]` - Serde-dependent extractors

## Migration Guidance

### From Direct Actix

**Before**:

```rust
async fn search(
    query: web::Query<SearchParams>,
    state: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    // Handler logic
}
```

**After**:

```rust
async fn search(
    Query(params): Query<SearchParams>,
    State(app_state): State<AppState>,
) -> Result<HttpResponse, Error> {
    // Same handler logic
}
```

### Testing Your Migration

1. **Add compilation test** to verify handler compiles with both backends
2. **Add runtime test** to verify behavior matches expectations
3. **Compare performance** if the handler is on a critical path

## Troubleshooting

### Common Issues

**"Method not found: into_handler"**

- Solution: Import `use moosicbox_web_server::handler::IntoHandler;`

**"Feature simulator not enabled"**

- Solution: Run tests with `--features simulator`

**"Tests not running"**

- Check feature gates - some tests only run with specific features enabled

### Debug Commands

```bash
# Check which tests would run
cargo test -p moosicbox_web_server --test handler_integration --features simulator -- --list

# Run with full compiler output
RUST_BACKTRACE=1 cargo test -p moosicbox_web_server --test handler_integration

# Check feature compilation
cargo check -p moosicbox_web_server --tests --no-default-features --features=simulator,serde
```

## Future Enhancements

### Planned Improvements

1. **Runtime Testing**: Execute handlers with real requests and validate responses
2. **Error Testing**: Test actual error conditions and propagation
3. **Performance Benchmarks**: Measure handler overhead and optimization opportunities
4. **Integration Examples**: Real-world migration patterns and examples
5. **Automated Migration Tools**: Scripts to help migrate existing Actix handlers

### Contributing

When adding new tests:

1. Follow the existing pattern of compilation-focused tests
2. Add tests for both Actix and Simulator backends
3. Use appropriate feature gates
4. Update this README with new test descriptions
5. Ensure zero clippy warnings

## Related Documentation

- `../src/handler.rs` - Handler system implementation
- `../src/extractors/` - Extractor implementations
- `../src/from_request.rs` - FromRequest trait definition
- `../README.md` - Web server package overview

---

_These tests are part of the MoosicBox determinism audit Phase 3, enabling migration from direct Actix usage to a dual-mode web server system that supports both production performance and deterministic testing._
