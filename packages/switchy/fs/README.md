# switchy_fs

Filesystem abstraction layer with support for real and simulated filesystems.

## Overview

This crate provides a unified filesystem API that can switch between different backends:

- **Real filesystem** - Standard filesystem operations using `std::fs` or `tokio::fs`
- **Simulated filesystem** - In-memory filesystem for testing without touching the disk

## Features

- `simulator` - Enables in-memory filesystem simulator (enabled by default)
- `simulator-real-fs` - Allows temporarily using real filesystem within simulator mode
- `std` - Standard library filesystem support (enabled by default)
- `tokio` - Async filesystem operations using tokio (enabled by default)
- `sync` - Synchronous filesystem operations (enabled by default)
- `async` - Asynchronous filesystem operations (enabled by default)

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
switchy_fs = "0.1"
```

## Usage

### Basic File Operations (Sync)

```rust
use switchy_fs::sync::{OpenOptions, read_to_string, create_dir_all};
use std::io::Write;

// Create a directory and write to a file
create_dir_all("/tmp").unwrap();

let mut file = OpenOptions::new()
    .create(true)
    .write(true)
    .open("/tmp/example.txt")
    .unwrap();

file.write_all(b"Hello, world!").unwrap();
drop(file);

// Read the file back
let content = read_to_string("/tmp/example.txt").unwrap();
assert_eq!(content, "Hello, world!");
```

### Temporary Directories

```rust
use switchy_fs::{tempdir, TempDir};

// Create a temporary directory that will be cleaned up when dropped
let temp_dir = tempdir().unwrap();
let temp_path = temp_dir.path();

// Use the temporary directory
println!("Temp directory: {}", temp_path.display());

// Directory is automatically deleted when temp_dir goes out of scope
```

### Temporary Directory with Prefix/Suffix

```rust
use switchy_fs::TempDir;

// Create with prefix
let temp_dir = TempDir::with_prefix("my-app-").unwrap();

// Create with suffix
let temp_dir = TempDir::with_suffix("-data").unwrap();

// Keep the directory (prevent automatic cleanup)
let kept_path = temp_dir.keep();
```

### Simulator Mode (Testing)

When the `simulator` feature is enabled, all filesystem operations run in-memory:

```rust
use switchy_fs::simulator::reset_fs;

// Reset the simulated filesystem before tests
reset_fs();

// All file operations now use the in-memory filesystem
```

### Using Real Filesystem Within Simulator

When you need to access the real filesystem while in simulator mode:

```rust
use switchy_fs::with_real_fs;

// Temporarily use real filesystem
with_real_fs(|| {
    // Operations here use the actual filesystem
});
```

### Seeding Simulator from Real Files

```rust
use switchy_fs::{seed_from_real_fs, seed_from_real_fs_same_path, seed_relative_to};

// Copy real files into the simulator
seed_from_real_fs("/real/path", "/sim/path").unwrap();

// Use the same path in both real and simulated filesystems
seed_from_real_fs_same_path("/path/to/seed").unwrap();

// Seed multiple paths relative to a base directory
seed_relative_to(
    env!("CARGO_MANIFEST_DIR"),
    ["tests/fixtures", "tests/scripts"],
).unwrap();
```

### Runtime Detection

```rust
use switchy_fs::is_simulator_enabled;

if is_simulator_enabled() {
    println!("Using in-memory simulator");
} else {
    println!("Using real filesystem");
}
```

## License

See the [LICENSE](../../../LICENSE) file for details.
