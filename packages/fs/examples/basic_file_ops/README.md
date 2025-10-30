# Basic File Operations Example

Demonstrates fundamental synchronous file operations using the `switchy_fs` package, including creating, reading, writing, seeking, and managing files.

## What This Example Demonstrates

- Creating new files with `OpenOptions`
- Reading file contents
- Writing and appending to files
- Using seek operations for random file access
- Truncating and overwriting files
- Opening files for simultaneous read/write operations
- Creating and managing nested directory structures
- Automatic cleanup with temporary directories

## Prerequisites

- Basic understanding of Rust's I/O traits (`Read`, `Write`, `Seek`)
- Familiarity with file operations concepts (create, read, write, append)

## Running the Example

```bash
# Run with default features (using standard filesystem)
cargo run --manifest-path packages/fs/examples/basic_file_ops/Cargo.toml

# Run in simulator mode (in-memory filesystem for testing)
cargo run --manifest-path packages/fs/examples/basic_file_ops/Cargo.toml --no-default-features --features switchy_fs/simulator,switchy_fs/sync
```

## Expected Output

```
Demo: Basic File Operations with switchy_fs

Using temporary directory: /tmp/.tmp[random]

1. Creating and writing to a new file:
   Created and wrote to: /tmp/.tmp[random]/example.txt

2. Reading file contents:
   File contents:
   Hello, switchy_fs!
   This is line 2.

3. Appending to existing file:
   Updated contents:
   Hello, switchy_fs!
   This is line 2.
   This line was appended.

4. Using seek to read specific parts of the file:
   Read 10 bytes from position 7: switchy_fs
   Read 9 bytes from 10 bytes before end: appended.

5. Truncating and overwriting file:
   New contents:
   File has been completely overwritten.

6. Opening file for both reading and writing:
   Current contents: File has been completely overwritten.
   Updated contents:
   File has been completely overwritten.
   Added via read-write mode.

7. Creating files in nested directories:
   Created directory structure: /tmp/.tmp[random]/data/files/output
   Created file: /tmp/.tmp[random]/data/files/output/data.txt
   Cleaned up nested directories

Demo completed successfully!
Temporary directory will be automatically cleaned up.
```

## Code Walkthrough

### Creating and Writing Files

The example starts by creating a temporary directory and demonstrating file creation:

```rust
let mut file = OpenOptions::new()
    .create(true)    // Create the file if it doesn't exist
    .write(true)     // Open for writing
    .open(&file_path)?;

file.write_all(b"Hello, switchy_fs!\n")?;
```

The `OpenOptions` builder pattern allows you to configure exactly how a file should be opened.

### Reading Files

Reading is straightforward with the `read(true)` option:

```rust
let mut file = OpenOptions::new()
    .read(true)
    .open(&file_path)?;

let mut contents = String::new();
file.read_to_string(&mut contents)?;
```

### Appending to Files

The `append(true)` option ensures new data is written to the end of the file:

```rust
let mut file = OpenOptions::new()
    .append(true)
    .open(&file_path)?;

file.write_all(b"This line was appended.\n")?;
```

### Seeking Within Files

Seek operations allow random access to file contents:

```rust
// Seek from start
file.seek(SeekFrom::Start(7))?;

// Seek from end
file.seek(SeekFrom::End(-10))?;

// Seek relative to current position
file.seek(SeekFrom::Current(5))?;
```

### Truncating Files

The `truncate(true)` option clears existing file contents:

```rust
let mut file = OpenOptions::new()
    .write(true)
    .truncate(true)  // Clear file to 0 length
    .open(&file_path)?;
```

### Simultaneous Read/Write

Files can be opened for both reading and writing:

```rust
let mut file = OpenOptions::new()
    .read(true)
    .write(true)
    .open(&file_path)?;

// Read current contents
let mut contents = String::new();
file.read_to_string(&mut contents)?;

// Write additional data
file.write_all(b"Added content\n")?;
```

## Key Concepts

### OpenOptions Builder Pattern

`OpenOptions` provides a flexible builder pattern for configuring file access:

- **`create(true)`**: Create the file if it doesn't exist
- **`append(true)`**: Write to the end of the file
- **`read(true)`**: Allow reading from the file
- **`write(true)`**: Allow writing to the file
- **`truncate(true)`**: Clear existing file contents to 0 bytes

### File Lifecycle Management

Files are automatically closed when dropped. You can explicitly close a file by calling `drop(file)` or letting it go out of scope.

### Temporary Directories

The example uses `tempdir()` to create a temporary directory that's automatically cleaned up when dropped, ensuring no leftover files.

### Cross-Platform Abstraction

`switchy_fs` provides a unified API that works across different backends:

- **Standard filesystem** (default): Uses `std::fs`
- **Simulator mode**: In-memory filesystem for testing
- **Tokio**: Async operations (see `async_operations` example)

## Testing the Example

You can modify the example to experiment with different scenarios:

1. **Test error handling**: Try opening a non-existent file without `create(true)`
2. **Test permissions**: Try reading a file opened only for writing
3. **Test seek boundaries**: Try seeking beyond file end
4. **Compare modes**: Run in both standard and simulator mode to see behavior differences

## Troubleshooting

### "File not found" errors

Ensure you're using `create(true)` when creating new files, or that the file already exists when opening without the create flag.

### "Permission denied" errors

Check that you've opened the file with the appropriate permissions:

- Reading requires `read(true)`
- Writing requires `write(true)` or `append(true)`

### "Invalid seek" errors

Ensure you're not seeking to negative positions or beyond reasonable file boundaries.

## Related Examples

- **temp_dir**: Demonstrates temporary directory management and automatic cleanup
- **async_operations**: Shows async versions of these operations using `tokio`
- **directory_ops**: Demonstrates directory creation, reading, and walking
