# Basic Logging Usage Example

This example demonstrates the basic usage patterns of the `moosicbox_logging` package, showing how to initialize the logging system and use various logging macros.

## Summary

A complete, runnable example showing how to set up logging with file output and use both standard log macros and the conditional `debug_or_trace!` macro.

## What This Example Demonstrates

- Initializing the logging system with the `init()` function
- Configuring file output for logs
- Using standard log macros (`error!`, `warn!`, `info!`, `debug!`, `trace!`)
- Using the `debug_or_trace!` macro for conditional verbose logging
- Logging with formatted strings and variables

## Prerequisites

- Rust toolchain installed
- Basic understanding of Rust logging concepts
- Familiarity with the `log` crate facade

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/logging/examples/basic_usage/Cargo.toml
```

To see trace-level messages, set the `MOOSICBOX_LOG` or `RUST_LOG` environment variable:

```bash
MOOSICBOX_LOG=trace cargo run --manifest-path packages/logging/examples/basic_usage/Cargo.toml
```

## Expected Output

When you run the example, you should see output similar to:

```
Initializing logging system...
Logging initialized successfully!

Demonstrating standard log macros:

Demonstrating debug_or_trace! macro:

All log messages have been written!
Check the log file at: {config_dir}/logs/basic_usage_example.log
```

The actual log messages are written to:

1. **Console output** (to stderr by default)
2. **Log file** at `{config_dir}/logs/basic_usage_example.log`

The log file will contain entries like:

```
[ERROR] This is an error message
[WARN] This is a warning message
[INFO] This is an info message
[DEBUG] This is a debug message
[TRACE] This is a trace message
[DEBUG] Short debug message: Processing started
[INFO] Starting data processing with 42 items
[DEBUG] Processing item 1
[DEBUG] Item 1 processed
[DEBUG] Processing item 2
[DEBUG] Item 2 processed
[DEBUG] Processing item 3
[DEBUG] Item 3 processed
[INFO] Completed 3 of 42 items
```

## Code Walkthrough

### 1. Importing Dependencies

```rust
use moosicbox_logging::{debug_or_trace, init, log, InitError};
```

- `init`: Function to initialize the logging system
- `log`: Re-exported `log` crate for standard logging macros
- `debug_or_trace`: Macro for conditional logging
- `InitError`: Error type for initialization failures

### 2. Initializing the Logging System

```rust
let _layer = init(Some("basic_usage_example.log"), None)?;
```

The `init()` function:

- **First parameter**: Optional filename for log output
    - When provided, creates a log file in `{config_dir}/logs/{filename}`
    - When `None`, logs only to console
- **Second parameter**: Optional custom tracing layers
    - Advanced use case for adding custom logging behavior
    - Use `None` for standard setup

The function returns a `FreeLogLayer` that manages the logging subscription. We store it in `_layer` to keep it alive for the program's duration.

### 3. Using Standard Log Macros

```rust
log::error!("This is an error message");
log::warn!("This is a warning message");
log::info!("This is an info message");
log::debug!("This is a debug message");
log::trace!("This is a trace message");
```

These are standard macros from the `log` crate, re-exported by `moosicbox_logging` for convenience. They log at different severity levels.

### 4. Using the `debug_or_trace!` Macro

```rust
debug_or_trace!(
    ("Short debug message: Processing started"),
    ("Detailed trace message: Processing started with full context and details")
);
```

This macro is unique to `moosicbox_logging`:

- If trace-level logging is enabled, it logs the **second** message at trace level
- If trace-level logging is disabled, it logs the **first** message at debug level
- Useful for providing detailed information only when needed, reducing noise in debug mode

### 5. Formatted Logging

```rust
let count = 42;
let operation = "data processing";
log::info!("Starting {} with {} items", operation, count);
```

All log macros support format string syntax, just like `println!`.

## Key Concepts

### Log Levels

From most to least severe:

1. **ERROR**: Critical errors that need immediate attention
2. **WARN**: Warning messages for potentially problematic situations
3. **INFO**: Informational messages about application progress
4. **DEBUG**: Detailed information for debugging
5. **TRACE**: Very detailed trace information

### Environment-Based Filtering

The `init()` function reads log level configuration from environment variables:

- `MOOSICBOX_LOG`: MoosicBox-specific log filter
- `RUST_LOG`: Standard Rust log filter (fallback)
- Default in debug builds: `trace` level
- Default in release builds: `info` level

You can set fine-grained filters like:

```bash
# Show only info and above for most modules, but trace for moosicbox_logging
MOOSICBOX_LOG="info,moosicbox_logging=trace" cargo run --manifest-path packages/logging/examples/basic_usage/Cargo.toml
```

### Log File Location

Log files are written to the system's config directory:

- **Linux**: `~/.config/moosicbox/logs/`
- **macOS**: `~/Library/Application Support/moosicbox/logs/`
- **Windows**: `C:\Users\<user>\AppData\Roaming\moosicbox\logs\`

## Testing the Example

1. **Run with default settings** to see info-level and above messages
2. **Set `MOOSICBOX_LOG=trace`** to see all messages including trace level
3. **Set `MOOSICBOX_LOG=warn`** to see only warnings and errors
4. **Check the log file** in your config directory to verify file output is working
5. **Run multiple times** to see how log entries accumulate in the file

## Troubleshooting

### No log file created

- Check that your system's config directory exists and is writable
- Look for warning messages in the console output
- Verify the path shown in the output message

### Not seeing debug or trace messages

- Set the `MOOSICBOX_LOG` or `RUST_LOG` environment variable to `trace` or `debug`
- Remember that release builds default to `info` level
- Check that you're looking in the right place (console vs. file)

### Permission errors

- Ensure the config directory (`~/.config/moosicbox/logs/` on Linux) exists and is writable
- Try running with elevated permissions if necessary
- Check disk space availability

## Related Examples

This is currently the only example for `moosicbox_logging`. Future examples may demonstrate:

- Custom tracing layers for advanced logging scenarios
- Integration with application servers
- Structured logging with fields and metadata
