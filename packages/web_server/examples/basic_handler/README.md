# Basic Handler Example

This example demonstrates the unified HttpRequest API working identically across both Actix and Simulator backends.

## What it validates

- **HttpRequest methods**: `method()`, `path()`, `query_string()`, `header()`, `body()`, `cookies()`, `cookie()`, `remote_addr()`
- **Handler trait system**: `IntoHandler` trait working with async functions
- **Dual backend support**: Same API works with both Actix and Simulator

## Prerequisites

⚠️ **Important**: This example requires the `serde` feature to be enabled because the FromRequest implementations use `serde_json` for parsing.

## Running the example

```bash
# With simulator backend (default)
cargo run -p basic_handler_example --features "moosicbox_web_server/serde"

# With actix backend
cargo run -p basic_handler_example --features "actix,moosicbox_web_server/serde"

# With NixOS (simulator)
nix-shell --run "cargo run -p basic_handler_example --features 'moosicbox_web_server/serde'"

# With NixOS (actix)
nix-shell --run "cargo run -p basic_handler_example --features 'actix,moosicbox_web_server/serde'"

# From example directory
cd packages/web_server/examples/basic_handler
cargo run --features "moosicbox_web_server/serde"
cargo run --features "actix,moosicbox_web_server/serde"
```

## Build only (for testing compilation)

```bash
# Build with simulator backend
cargo build -p basic_handler_example --features "moosicbox_web_server/serde"

# Build with actix backend
cargo build -p basic_handler_example --features "actix,moosicbox_web_server/serde"

# Build with both backends
cargo build -p basic_handler_example --features "actix,simulator,moosicbox_web_server/serde"
```

## Troubleshooting

### Missing serde feature error
If you see `use of unresolved module or unlinked crate 'serde_json'`, make sure to include the serde feature:
```bash
--features "moosicbox_web_server/serde"
```

### Package name error
The package name is `basic_handler_example`, not `basic_handler`.

## Current limitations

- **Send bounds**: Actix backend currently requires extracting data before async blocks due to Send constraints
- **TODO**: This will be fixed in Step 2 of the web server enhancement plan

## Purpose

This example serves as validation that Step 1 of the web server enhancement is complete and working correctly.
