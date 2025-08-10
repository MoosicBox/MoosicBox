# Basic Handler Example

This example demonstrates the unified HttpRequest API working identically across both Actix and Simulator backends.

## What it validates

- **HttpRequest methods**: `method()`, `path()`, `query_string()`, `header()`, `body()`, `cookies()`, `cookie()`, `remote_addr()`
- **Handler trait system**: `IntoHandler` trait working with async functions
- **Dual backend support**: Same API works with both Actix and Simulator

## Running the example

```bash
# With simulator backend (default)
cargo run -p basic_handler_example

# With actix backend
cargo run -p basic_handler_example --features actix

# With both backends
cargo run -p basic_handler_example --features actix,simulator
```

## Current limitations

- **Send bounds**: Actix backend currently requires extracting data before async blocks due to Send constraints
- **TODO**: This will be fixed in Step 2 of the web server enhancement plan

## Purpose

This example serves as validation that Step 1 of the web server enhancement is complete and working correctly.
