# Filesystem (FS)

Cross-platform filesystem abstraction with sync and async operations.

## Overview

The FS package provides:

- **Sync and Async APIs**: Both synchronous and asynchronous filesystem operations
- **Cross-platform**: Abstraction over different filesystem implementations
- **Feature-gated Backends**: Standard library, Tokio, and simulator implementations
- **File Operations**: Create, read, write, seek, and delete operations
- **Directory Operations**: Create and remove directories recursively
- **Flexible Options**: Configurable file open options

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
- **Directory Management**: Create and remove directories

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
fs = { path = "../fs" }

# With specific features
fs = {
    path = "../fs",
    features = ["sync", "async", "std", "tokio"]
}

# For testing
fs = {
    path = "../fs",
    features = ["simulator", "sync", "async"]
}
```

## Usage

### Synchronous File Operations

```rust
use fs::sync::{File, OpenOptions, read_to_string, create_dir_all, remove_dir_all};
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
use fs::unsync::{File, OpenOptions, read_to_string, create_dir_all, remove_dir_all};
use switchy_async::io::{AsyncReadExt, AsyncWriteExt, AsyncSeekExt, SeekFrom};

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
use fs::sync::OpenOptions;

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
use fs::sync::{create_dir_all, remove_dir_all};

// Create nested directories
create_dir_all("./deep/nested/directory/structure")?;

// Remove directory and all contents
remove_dir_all("./deep")?;

// Async versions
use fs::unsync::{create_dir_all, remove_dir_all};

create_dir_all("./async/nested/dirs").await?;
remove_dir_all("./async").await?;
```

### Cross-module Compatibility

```rust
// Convert between sync and async options
use fs::{sync, unsync};

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
    use fs::sync::{File, OpenOptions, read_to_string};

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

### Generic File Traits

```rust
use fs::{GenericSyncFile, GenericAsyncFile};
use std::io::{Read, Write, Seek};

// Function that works with any sync file implementation
fn process_sync_file<F: GenericSyncFile>(mut file: F) -> std::io::Result<String> {
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

// Function that works with any async file implementation
async fn process_async_file<F: GenericAsyncFile>(mut file: F) -> std::io::Result<String> {
    let mut contents = String::new();
    file.read_to_string(&mut contents).await?;
    Ok(contents)
}
```

## Feature Flags

### Operation Modes
- **`sync`**: Enable synchronous filesystem operations
- **`async`**: Enable asynchronous filesystem operations

### Backend Implementations
- **`std`**: Use standard library filesystem implementation
- **`tokio`**: Use Tokio async filesystem implementation
- **`simulator`**: Use mock filesystem for testing

## Backend Selection

The package automatically selects the appropriate backend based on enabled features:

1. **Simulator Mode**: When `simulator` feature is enabled, all operations use mock filesystem
2. **Standard Library**: When `std` feature is enabled (and not simulator), uses `std::fs`
3. **Tokio**: When `tokio` feature is enabled (and not simulator), uses `tokio::fs`

## Error Handling

```rust
use fs::sync::{File, OpenOptions};
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

- **Switchy Async**: Async I/O trait abstractions
- **Standard Library**: `std::fs` and `std::io` (optional)
- **Tokio**: `tokio::fs` and `tokio::io` (optional)

## Use Cases

- **Configuration Files**: Read and write application configuration
- **Data Persistence**: Store and retrieve application data
- **Log Files**: Append-only log file operations
- **Temporary Files**: Create and manage temporary files
- **Testing**: Mock filesystem operations in unit tests
- **Cross-platform Applications**: Unified filesystem interface
- **Async Applications**: Non-blocking filesystem operations
