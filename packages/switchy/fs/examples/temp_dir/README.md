# Temp Dir Example

Example demonstrating temporary directory functionality using `switchy_fs`.

## Description

This example shows how to use `switchy_fs` temporary directory features including:

- Creating basic temporary directories that auto-cleanup on drop
- Using custom prefixes for temp directory names
- Keeping temp directories (preventing automatic cleanup)
- Manually closing temp directories for immediate cleanup

## Usage

```bash
# Run with default std feature
cargo run -p temp_dir_example

# Run with simulator mode
cargo run -p temp_dir_example --no-default-features --features simulator

# Run with simulator-real-fs mode
cargo run -p temp_dir_example --no-default-features --features simulator-real-fs

# Run with async feature
cargo run -p temp_dir_example --features async
```

## Features

| Feature             | Description                                |
| ------------------- | ------------------------------------------ |
| `default`           | Enables `std`                              |
| `std`               | Standard filesystem with sync operations   |
| `async`             | Async filesystem operations with tokio     |
| `simulator`         | Simulated filesystem (sync)                |
| `simulator-real-fs` | Simulator backed by real filesystem (sync) |
| `fail-on-warnings`  | Treat warnings as errors                   |

## Examples Demonstrated

1. **Basic temp directory creation** - Creates a temporary directory using `tempdir()` that automatically cleans up when dropped
2. **Temp directory with prefix** - Creates a temp directory with a custom prefix using `TempDir::with_prefix("my-app-")`
3. **Keeping a temp directory** - Shows how to use `temp_dir.keep()` to prevent automatic cleanup
4. **Manual close** - Demonstrates immediate cleanup using `temp_dir.close()`

## License

Copyright (c) 2024 MoosicBox

This file is part of MoosicBox.

MoosicBox is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.

MoosicBox is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.

You should have received a copy of the GNU General Public License along with MoosicBox. If not, see <http://www.gnu.org/licenses/>.
