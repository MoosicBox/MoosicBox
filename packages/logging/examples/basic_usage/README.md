# Basic Usage Example

This example demonstrates the fundamental usage patterns of the MoosicBox logging package, showing how to initialize the logging system and use its features in a real application.

## What This Example Demonstrates

- **Logging Initialization**: Setting up the logging system with file output
- **Standard Log Macros**: Using `info!`, `debug!`, `warn!`, `error!`, and `trace!` macros
- **Conditional Logging**: Using the `debug_or_trace!` macro for adaptive verbosity
- **Environment-Based Filtering**: Configuring log levels via environment variables
- **File Output**: Writing logs to a configured file location

## Prerequisites

- Rust toolchain installed (see root repository README)
- Basic understanding of Rust logging concepts
- Familiarity with environment variables

## Running the Example

```bash
# From the repository root
cargo run --manifest-path packages/logging/examples/basic_usage/Cargo.toml

# Or from the example directory
cd packages/logging/examples/basic_usage
cargo run
```

### Running with Different Log Levels

```bash
# Show all messages including trace-level
RUST_LOG=trace cargo run --manifest-path packages/logging/examples/basic_usage/Cargo.toml

# Show only info and above (default in release builds)
RUST_LOG=info cargo run --manifest-path packages/logging/examples/basic_usage/Cargo.toml

# Show debug and above (default in debug builds)
RUST_LOG=debug cargo run --manifest-path packages/logging/examples/basic_usage/Cargo.toml

# Filter to specific modules
RUST_LOG=moosicbox_logging_basic_usage_example=trace cargo run --manifest-path packages/logging/examples/basic_usage/Cargo.toml
```

## Expected Output

When you run the example, you'll see output similar to:

```
Application started - this is an info message
This is a warning message
This is an error message
perform_calculation called with a=10, b=20
Calculation result: 30
Application finished successfully

Example completed!
Check the log file at: {config_dir}/logs/basic_usage.log

Tip: Run with RUST_LOG=trace to see all messages:
  RUST_LOG=trace cargo run --manifest-path packages/logging/examples/basic_usage/Cargo.toml
```

**Note**: Debug and trace messages may not appear unless you set the appropriate `RUST_LOG` level.

## Code Walkthrough

### Initializing the Logging System

```rust
use moosicbox_logging::{init, InitError};

fn main() -> Result<(), InitError> {
    // Initialize with file output to "{config_dir}/logs/basic_usage.log"
    let _layer = init(Some("basic_usage.log"), None)?;

    // Logging is now configured and ready to use
    // ...
}
```

The `init()` function:

- Takes an optional filename for log file output
- Creates the log file in `{config_dir}/logs/` directory
- Configures filtering based on `MOOSICBOX_LOG` or `RUST_LOG` environment variables
- Returns a `FreeLogLayer` that manages the logging subscription
- Defaults to `trace` level in debug builds, `info` level in release builds

### Using Standard Log Macros

```rust
use moosicbox_logging::log;

log::info!("Application started - this is an info message");
log::debug!("This is a debug message with details: counter = {}", 42);
log::warn!("This is a warning message");
log::error!("This is an error message");
log::trace!("This is a trace message with very detailed information");
```

These are the standard Rust logging macros from the `log` crate, re-exported for convenience.

### Using the debug_or_trace! Macro

```rust
use moosicbox_logging::debug_or_trace;

debug_or_trace!(
    ("Short debug message: operation completed"),
    ("Detailed trace message: operation completed with result = {:?}", "success")
);
```

This macro provides adaptive verbosity:

- If trace logging is enabled, it logs the detailed trace message
- Otherwise, it logs the shorter debug message
- Useful for adding detailed diagnostics without cluttering logs when not needed

### Logging in Functions

```rust
fn perform_calculation(a: i32, b: i32) -> i32 {
    log::debug!("perform_calculation called with a={}, b={}", a, b);

    let result = a + b;

    log::trace!("Calculation details: {} + {} = {}", a, b, result);
    log::info!("Calculation result: {}", result);

    result
}
```

Best practices:

- Use `debug!` for function entry/parameters
- Use `trace!` for detailed internal state
- Use `info!` for significant events
- Use `warn!` for recoverable issues
- Use `error!` for error conditions

## Key Concepts

### Log Levels (from most to least verbose)

1. **TRACE**: Very detailed diagnostic information
2. **DEBUG**: Detailed information for debugging
3. **INFO**: General informational messages
4. **WARN**: Warning messages for potentially problematic situations
5. **ERROR**: Error messages for error conditions

### Environment-Based Filtering

The logging system respects standard Rust logging environment variables:

- `MOOSICBOX_LOG`: Primary filtering variable (checked first)
- `RUST_LOG`: Fallback filtering variable
- Format: `target=level` (e.g., `RUST_LOG=debug` or `RUST_LOG=myapp=trace`)

### File Output

When a filename is provided to `init()`:

- Logs are written to `{config_dir}/logs/{filename}`
- The config directory is platform-specific (varies by OS)
- File logging runs at DEBUG level by default
- Files are created automatically if they don't exist

### Feature Flags

This example uses the default features:

- `api`: Enables API features in `free_log_client`
- `free_log`: Enables the `init()` function and free log integration
- `macros`: Enables the `debug_or_trace!` macro

## Testing the Example

1. **Run with default settings** and observe which messages appear
2. **Run with `RUST_LOG=trace`** to see all messages including trace and debug
3. **Run with `RUST_LOG=info`** to see only info, warn, and error messages
4. **Check the log file** at `{config_dir}/logs/basic_usage.log` to verify file output
5. **Experiment with different log levels** for specific modules

## Troubleshooting

### No debug/trace messages visible

**Problem**: Running the example but not seeing debug or trace messages.

**Solution**: Set the appropriate log level:

```bash
RUST_LOG=trace cargo run --manifest-path packages/logging/examples/basic_usage/Cargo.toml
```

Debug builds default to `trace`, but release builds default to `info`.

### Log file not created

**Problem**: Log file is not being created in the expected location.

**Solution**: Check that:

- You have write permissions to the config directory
- The config directory exists and is accessible
- Check the console for warning messages about config directory issues

### Environment variable not working

**Problem**: Setting `RUST_LOG` doesn't change log output.

**Solution**:

- Ensure the variable is exported: `export RUST_LOG=trace`
- Or set it inline: `RUST_LOG=trace cargo run ...`
- Try using `MOOSICBOX_LOG` instead (it's checked first)

## Related Examples

This is currently the primary example for the `moosicbox_logging` package. As the package evolves, additional examples may be added for:

- Custom tracing layers
- Integration with specific frameworks
- Advanced filtering patterns
- Performance logging

## Further Reading

- [moosicbox_logging README](../../README.md) - Full package documentation
- [free_log_client documentation](https://docs.rs/free_log_client/) - Underlying logging client
- [Rust log crate](https://docs.rs/log/) - Standard logging facade
