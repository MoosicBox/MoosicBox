# Web Server Nested GET Example

Shows how to create a web server with a route nested under a scope.

## What it does

- Creates a web server with CORS configuration
- Creates a scope with prefix "/nested"
- Adds a GET route at "/example" to that scope
- Results in the endpoint being at "/nested/example"

## Prerequisites

⚠️ **Important**: This example requires the `serde` feature to be enabled because the FromRequest implementations use `serde_json` for parsing.

## Running the example

```bash
# From repository root
cargo run -p web_server_nested_get --features "moosicbox_web_server/serde"

# With NixOS
nix-shell --run "cargo run -p web_server_nested_get --features 'moosicbox_web_server/serde'"

# From example directory
cd packages/web_server/examples/nested_get
cargo run --features "moosicbox_web_server/serde"
```

## Build only (for testing compilation)

```bash
# Build the example
cargo build -p web_server_nested_get --features "moosicbox_web_server/serde"
```

## Troubleshooting

### Missing serde feature error
If you see `use of unresolved module or unlinked crate 'serde_json'`, make sure to include the serde feature:
```bash
--features "moosicbox_web_server/serde"
```

### Package name error
The package name is `web_server_nested_get`, not `nested_get`.
