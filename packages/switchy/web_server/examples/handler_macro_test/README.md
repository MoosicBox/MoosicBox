# Switchy Web Server Handler Macro Test

Test examples for the `switchy_web_server` handler macro system.

## Overview

This package provides test binaries that verify the handler macro system works correctly with different web server backends. It tests that handler functions with various extractor combinations compile and convert to routes without `Send` bound issues.

## Binaries

### test_actix

Tests handler macros with the Actix backend.

```bash
cargo run --bin test_actix --features "switchy_web_server/actix"
```

### test_simulator

Tests handler macros with the Simulator backend.

```bash
cargo run --bin test_simulator --features "switchy_web_server/simulator"
```

### debug_actix

Debug tests for `IntoHandler` trait implementation with Actix backend.

```bash
cargo run --bin debug_actix --features "switchy_web_server/actix"
```

## Tested Extractors

The test binaries verify handlers using these extractors:

- **No parameters**: Simple handlers with no extractors
- **`RequestInfo`**: Request path and method information
- **`Headers`**: HTTP headers access
- **`Query<T>`**: Query string parameter extraction
- **`Path<T>`**: Path parameter extraction (Actix only)
- **Multiple extractors**: Combinations of the above

## License

See the workspace root for license information.
