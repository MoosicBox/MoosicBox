# Temp Directory Example

This example demonstrates the `switchy_fs` temp directory functionality across different feature configurations.

## Features

- **`std`** - Standard filesystem operations
- **`simulator`** - Simulated filesystem for testing
- **`simulator-real-fs`** - Simulator with real filesystem backing

## Usage

Run with different feature combinations:

```bash
# Standard mode (default)
cargo run

# Simulator mode
cargo run --no-default-features --features simulator

# Simulator with real filesystem
cargo run --no-default-features --features simulator-real-fs
```

## Examples

The demo shows:

1. Basic temp directory creation
2. Temp directory with custom prefix
3. Keeping a directory (preventing automatic cleanup)
4. Manual close (immediate cleanup)

Examples include conditional code to demonstrate behavior differences between std and simulator modes.
