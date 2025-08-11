# Handler Macro Test

This example validates the Step 2.1 handler macro system implementation.

## What it tests

- **0-parameter handlers**: Uses the legacy implementation, should work fully
- **1+ parameter handlers**: Uses the new macro system, compiles but returns "not implemented" errors

## Prerequisites

⚠️ **Important**: These tests require the `serde` feature to be enabled because the FromRequest implementations use `serde_json` for parsing.

## Available Binaries

- **test_actix**: Tests handler macros with Actix backend
- **test_simulator**: Tests handler macros with Simulator backend
- **debug_actix**: Debug version for Actix backend development

## Running the tests

### Actix Backend Tests
```bash
# From repository root
cargo run --bin test_actix -p handler_macro_test --features "moosicbox_web_server/actix,moosicbox_web_server/serde"

# With NixOS
nix-shell --run "cargo run --bin test_actix -p handler_macro_test --features 'moosicbox_web_server/actix,moosicbox_web_server/serde'"

# From example directory
cd packages/web_server/examples/handler_macro_test
cargo run --bin test_actix --features "moosicbox_web_server/actix,moosicbox_web_server/serde"
```

### Simulator Backend Tests
```bash
# From repository root
cargo run --bin test_simulator -p handler_macro_test --features "moosicbox_web_server/simulator,moosicbox_web_server/serde"

# With NixOS
nix-shell --run "cargo run --bin test_simulator -p handler_macro_test --features 'moosicbox_web_server/simulator,moosicbox_web_server/serde'"

# From example directory
cd packages/web_server/examples/handler_macro_test
cargo run --bin test_simulator --features "moosicbox_web_server/simulator,moosicbox_web_server/serde"
```

### Debug Actix Backend
```bash
# From repository root
cargo run --bin debug_actix -p handler_macro_test --features "moosicbox_web_server/actix,moosicbox_web_server/serde"

# With NixOS
nix-shell --run "cargo run --bin debug_actix -p handler_macro_test --features 'moosicbox_web_server/actix,moosicbox_web_server/serde'"
```

### Build All Binaries
```bash
# Build all binaries for testing compilation
cargo build --bins -p handler_macro_test --features "moosicbox_web_server/actix,moosicbox_web_server/simulator,moosicbox_web_server/serde"

# Build specific binary
cargo build --bin test_actix -p handler_macro_test --features "moosicbox_web_server/actix,moosicbox_web_server/serde"
```

## Expected Results

Both tests should:
1. ✅ Compile successfully
2. ✅ Show that handlers can be converted to Routes
3. 📝 Note that actual parameter extraction will be implemented in Step 2.2

## Troubleshooting

### Missing serde feature error
If you see `use of unresolved module or unlinked crate 'serde_json'`, make sure to include the serde feature:
```bash
--features "moosicbox_web_server/serde"
```

### Missing backend feature error
If you see `requires the features: moosicbox_web_server/actix`, make sure to include the backend feature:
```bash
--features "moosicbox_web_server/actix"  # for Actix
--features "moosicbox_web_server/simulator"  # for Simulator
```

## Step 2.1 Status

- ✅ **Macro System**: `impl_handler!` macro generates implementations for 1-16 parameters
- ✅ **Compilation**: All handler signatures compile without errors
- ✅ **Error Handling**: Proper error messages for unimplemented features
- ⏳ **Parameter Extraction**: Will be implemented in Step 2.2 (FromRequest trait updates)
- ⏳ **Send Bounds**: Will be resolved in Step 2.2

This validates that the foundation for the handler system is in place and ready for Step 2.2 completion.
