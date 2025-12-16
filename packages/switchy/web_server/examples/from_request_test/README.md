# Switchy Web Server FromRequest Test Examples

Test binaries demonstrating the `FromRequest` trait extraction functionality from the `switchy_web_server` crate.

## Overview

This package provides test examples that validate the `FromRequest` trait implementation for extracting typed data from HTTP requests. The examples use the simulator stub to create test HTTP requests and verify extraction behavior.

## Binaries

### test_sync_extraction

Tests synchronous extraction using `from_request_sync`:

- `RequestData` extraction (method, path, query, headers)
- `String` extraction from query strings
- `u32` extraction with validation
- `bool` extraction supporting multiple formats (true/false, 1/0, yes/no, on/off)

### test_async_extraction

Tests asynchronous extraction using `from_request_async`:

- `RequestData` async extraction
- `i32` extraction with negative number support
- Future type resolution
- Consistency between sync and async extraction methods

## Usage

Run the test binaries:

```bash
cargo run --bin test_sync_extraction
cargo run --bin test_async_extraction
```

## License

See the [LICENSE](../../../../../LICENSE) file for details.
