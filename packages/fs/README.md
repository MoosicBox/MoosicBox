# Switchy Filesystem (switchy_fs)

Cross-platform filesystem abstraction with sync and async operations.

## Overview

The switchy_fs package provides:

- **Sync and Async APIs**: Both synchronous and asynchronous filesystem operations
- **Cross-platform**: Abstraction over different filesystem implementations
- **Feature-gated Backends**: Standard library, Tokio, and simulator implementations
- **File Operations**: Create, read, write, seek, and delete operations
- **Directory Operations**: Create, remove, read, and walk directories with deterministic ordering
- **Temporary Directories**: Automatic cleanup of temporary directories
- **Flexible Options**: Configurable file open options
- **Path Utilities**: Check path existence across backends

## Features

### Operation Modes

- **Synchronous**: Blocking filesystem operations via `sync` module
- **Asynchronous**: Non-blocking operations via `unsync` module

### Backend Implementations

- **Standard Library**: `std::fs` based implementation
- **Tokio**: `tokio::fs` based async implementation
- **Simulator**: Mock filesystem for testing

### File Operations

- **File Creation**: Create new files with various options
- **File Reading**: Read file contents to string
- **File Writing**: Write data to files
- **File Seeking**: Random access file positioning
- **Directory Management**: Create, remove, read, and walk directories
- **Directory Traversal**: Sorted directory reading and recursive walking
- **Path Checking**: Check if files or directories exist

### Open Options

- **Create**: Create file if it doesn't exist
- **Append**: Append to existing file content
- **Read**: Open file for reading
- **Write**: Open file for writing
- **Truncate**: Clear existing file content

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
switchy_fs = { path = "../fs" }

# With specific features
switchy_fs = {
    path = "../fs",
    features = ["sync", "async", "std", "tokio"]
}

# For testing
switchy_fs = {
    path = "../fs",
    features = ["simulator", "sync", "async"]
}
```

## Usage

### Synchronous File Operations

```rust
use switchy_fs::sync::{File, OpenOptions, read_to_string, create_dir_all, remove_dir_all};
use std::io::{Read, Write, Seek, SeekFrom};

fn sync_file_operations() -> std::io::Result<()> {
    // Create directory
    create_dir_all("./data")?;

    // Create and write to file
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open("./data/example.txt")?;

    file.write_all(b"Hello, World!")?;

    // Read file contents
    let contents = read_to_string("./data/example.txt")?;
    println!("File contents: {}", contents);

    // Append to file
    let mut file = OpenOptions::new()
        .append(true)
        .open("./data/example.txt")?;

    file.write_all(b"\nAppended line")?;

    // Read with seeking
    let mut file = OpenOptions::new()
        .read(true)
        .open("./data/example.txt")?;

    file.seek(SeekFrom::Start(0))?;
    let mut buffer = String::new();
    file.read_to_string(&mut buffer)?;
    println!("Full contents: {}", buffer);

    // Clean up
    remove_dir_all("./data")?;

    Ok(())
}
```

### Asynchronous File Operations

```rust
use switchy_fs::unsync::{File, OpenOptions, read_to_string, create_dir_all, remove_dir_all};
use switchy_async::io::{AsyncReadExt, AsyncWriteExt, AsyncSeekExt};
use std::io::SeekFrom;

async fn async_file_operations() -> std::io::Result<()> {
    // Create directory
    create_dir_all("./async_data").await?;

    // Create and write to file
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open("./async_data/example.txt")
        .await?;

    file.write_all(b"Hello, Async World!").await?;

    // Read file contents
    let contents = read_to_string("./async_data/example.txt").await?;
    println!("File contents: {}", contents);

    // Append to file
    let mut file = OpenOptions::new()
        .append(true)
        .open("./async_data/example.txt")
        .await?;

    file.write_all(b"\nAsync appended line").await?;

    // Read with seeking
    let mut file = OpenOptions::new()
        .read(true)
        .open("./async_data/example.txt")
        .await?;

    file.seek(SeekFrom::Start(0)).await?;
    let mut buffer = String::new();
    file.read_to_string(&mut buffer).await?;
    println!("Full contents: {}", buffer);

    // Clean up
    remove_dir_all("./async_data").await?;

    Ok(())
}
```

### File Open Options

```rust
use switchy_fs::sync::OpenOptions;

// Create new file, fail if exists
let file = OpenOptions::new()
    .create(true)
    .write(true)
    .open("new_file.txt")?;

// Open existing file for reading
let file = OpenOptions::new()
    .read(true)
    .open("existing_file.txt")?;

// Open file for appending
let file = OpenOptions::new()
    .append(true)
    .open("log_file.txt")?;

// Create or truncate file for writing
let file = OpenOptions::new()
    .create(true)
    .write(true)
    .truncate(true)
    .open("output_file.txt")?;

// Read and write access
let file = OpenOptions::new()
    .read(true)
    .write(true)
    .open("data_file.txt")?;
```

### Directory Operations

```rust
use switchy_fs::sync::{create_dir_all, remove_dir_all, read_dir_sorted, walk_dir_sorted};
use switchy_fs::exists;

// Create nested directories
create_dir_all("./deep/nested/directory/structure")?;

// Check if path exists
if exists("./deep") {
    println!("Directory exists");
}

// Read directory entries (sorted by filename)
let entries = read_dir_sorted("./deep")?;
for entry in entries {
    println!("{:?}", entry.path());
}

// Recursively walk directory tree (sorted by path)
let all_entries = walk_dir_sorted("./deep")?;
for entry in all_entries {
    println!("{:?}", entry.path());
}

// Remove directory and all contents
remove_dir_all("./deep")?;

// Async versions
use switchy_fs::unsync::{create_dir_all, remove_dir_all};

create_dir_all("./async/nested/dirs").await?;

remove_dir_all("./async").await?;
```

### Cross-module Compatibility

```rust
// Convert between sync and async options
use switchy_fs::{sync, unsync};

let async_options = unsync::OpenOptions::new()
    .create(true)
    .write(true);

// Convert to sync options
let sync_options: sync::OpenOptions = async_options.into();

// Or use explicit conversion
let sync_options = async_options.into_sync();
```

### Simulator Mode (Testing)

```rust
#[cfg(test)]
mod tests {
    use switchy_fs::sync::{File, OpenOptions, read_to_string};

    #[test]
    fn test_file_operations() {
        // When simulator feature is enabled, all operations use mock filesystem
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .open("test_file.txt")
            .unwrap();

        file.write_all(b"test data").unwrap();

        let contents = read_to_string("test_file.txt").unwrap();
        assert_eq!(contents, "test data");
    }
}
```

### Simulator Real Filesystem Access

With the `simulator-real-fs` feature, you can temporarily access the real filesystem within simulator mode:

```rust
#[cfg(all(feature = "simulator", feature = "simulator-real-fs"))]
use switchy_fs::with_real_fs;

// In simulator mode, but need to access real filesystem temporarily
let real_data = with_real_fs(|| {
    std::fs::read_to_string("/path/to/real/file.txt")
}).unwrap();

// Back to simulated filesystem
let sim_data = read_to_string("/simulated/file.txt")?;
```

### Temporary Directories

The package provides temporary directory support that automatically cleans up when dropped:

```rust
use switchy_fs::{TempDir, tempdir, tempdir_in};

// Create a temporary directory
let temp = tempdir()?;
let path = temp.path();

// Use the directory
std::fs::write(path.join("test.txt"), b"data")?;

// Directory is automatically cleaned up when temp is dropped

// Create temp directory with prefix
let temp = TempDir::with_prefix("my-prefix-")?;

// Create temp directory with suffix
let temp = TempDir::with_suffix("-my-suffix")?;

// Create temp directory in specific location
let temp = tempdir_in("/custom/temp/location")?;

// Persist the directory (prevent automatic cleanup)
let kept_path = temp.keep();
```

## Feature Flags

### Operation Modes

- **`sync`**: Enable synchronous filesystem operations
- **`async`**: Enable asynchronous filesystem operations

### Backend Implementations

- **`std`**: Use standard library filesystem implementation
- **`tokio`**: Use Tokio async filesystem implementation
- **`simulator`**: Use mock filesystem for testing
- **`simulator-real-fs`**: Enable real filesystem access within simulator mode using `with_real_fs`

### Other Features

- **`fail-on-warnings`**: Treat warnings as errors during compilation

**Default features**: `async`, `simulator`, `std`, `sync`, `tokio`

## Backend Selection

The package automatically selects the appropriate backend based on enabled features:

1. **Simulator Mode**: When `simulator` feature is enabled, all operations use mock filesystem
2. **Standard Library**: When `std` feature is enabled (and not simulator), uses `std::fs`
3. **Tokio**: When `tokio` feature is enabled (and not simulator), uses `tokio::fs`

## Error Handling

```rust
use switchy_fs::sync::{File, OpenOptions};
use std::io::ErrorKind;

match OpenOptions::new().read(true).open("nonexistent.txt") {
    Ok(file) => {
        // File opened successfully
    }
    Err(e) => match e.kind() {
        ErrorKind::NotFound => {
            println!("File not found");
        }
        ErrorKind::PermissionDenied => {
            println!("Permission denied");
        }
        _ => {
            println!("Other error: {}", e);
        }
    }
}
```

## Dependencies

- **switchy_async**: Async I/O trait abstractions (optional)
- **bytes**: Byte buffer utilities for simulator backend (optional)
- **scoped-tls**: Thread-local storage for simulator real-fs mode (optional)
- **tempfile**: Temporary file management for standard library backend (optional)
- **tokio**: Async filesystem operations (optional)

## Use Cases

- **Configuration Files**: Read and write application configuration
- **Data Persistence**: Store and retrieve application data
- **Log Files**: Append-only log file operations
- **Temporary Directories**: Create and manage temporary directories with automatic cleanup
- **Testing**: Mock filesystem operations in unit tests with simulator mode
- **Cross-platform Applications**: Unified filesystem interface across different backends
- **Async Applications**: Non-blocking filesystem operations with Tokio
- **Deterministic Testing**: Sorted directory reading and walking for reproducible test results
