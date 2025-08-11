# Web Server Simple GET Example

Shows how to create a web server with a single GET endpoint.

## What it does

- Creates a web server using moosicbox_web_server::WebServerBuilder
- Configures CORS to allow all origins, methods, and headers
- Defines one GET route at /example using the dynamic route builder

## Prerequisites

⚠️ **Important**: This example requires the `serde` feature to be enabled because the FromRequest implementations use `serde_json` for parsing.

## Running the example

```bash
# From repository root
cargo run -p web_server_simple_get --features "moosicbox_web_server/serde"

# With NixOS
nix-shell --run "cargo run -p web_server_simple_get --features 'moosicbox_web_server/serde'"

# From example directory
cd packages/web_server/examples/simple_get
cargo run --features "moosicbox_web_server/serde"
```

## Build only (for testing compilation)

```bash
# Build the example
cargo build -p web_server_simple_get --features "moosicbox_web_server/serde"
```

## Troubleshooting

### Missing serde feature error
If you see `use of unresolved module or unlinked crate 'serde_json'`, make sure to include the serde feature:
```bash
--features "moosicbox_web_server/serde"
```

### Package name error
The package name is `web_server_simple_get`, not `simple_get`.
