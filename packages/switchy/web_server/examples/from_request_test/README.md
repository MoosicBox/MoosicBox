# Switchy Web Server FromRequest Test Examples

Test binaries demonstrating and validating the `FromRequest` trait extraction
functionality in `switchy_web_server`.

## Overview

This package provides two test binaries that verify the `FromRequest` trait
implementation works correctly for extracting data from HTTP requests:

- **`test_sync_extraction`** - Tests synchronous extraction using
  `from_request_sync`
- **`test_async_extraction`** - Tests asynchronous extraction using
  `from_request_async`

## Features Tested

The test binaries validate extraction of:

- `RequestData` - Full request metadata (method, path, query, headers)
- `String` - Query string extraction
- `u32` / `i32` - Integer parsing from query strings
- `bool` - Boolean parsing with support for multiple formats (`true`/`false`,
  `1`/`0`, `yes`/`no`, `on`/`off`)

## Running the Tests

```bash
# Run synchronous extraction tests
cargo run --bin test_sync_extraction

# Run asynchronous extraction tests
cargo run --bin test_async_extraction
```

## Dependencies

- `switchy_async` - Async runtime macros (with `tokio` backend)
- `switchy_web_server` - Web server with `simulator` feature for creating test
  requests

## License

See the [LICENSE](../../../../../LICENSE) file for details.
