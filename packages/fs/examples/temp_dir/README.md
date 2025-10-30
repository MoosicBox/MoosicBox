# Temp Directory Example

Demonstrates temporary directory management using the `switchy_fs` package, including automatic cleanup, custom prefixes, and manual lifecycle control.

## What This Example Demonstrates

- Creating temporary directories with automatic cleanup
- Using custom prefixes for temporary directories
- Keeping directories (preventing automatic cleanup)
- Manual cleanup with `close()`
- Behavior differences between standard and simulator modes
- Safe temporary file storage patterns

## Prerequisites

- Basic understanding of filesystem concepts
- Knowledge of RAII (Resource Acquisition Is Initialization) pattern in Rust

## Running the Example

```bash
# Standard mode (uses real filesystem temp directories)
cargo run --manifest-path packages/fs/examples/temp_dir/Cargo.toml

# Simulator mode (in-memory filesystem)
cargo run --manifest-path packages/fs/examples/temp_dir/Cargo.toml --no-default-features --features simulator

# Simulator with real filesystem backing
cargo run --manifest-path packages/fs/examples/temp_dir/Cargo.toml --no-default-features --features simulator-real-fs
```

## Expected Output

```
Demo: switchy_fs temp_dir functionality

1. Basic temp directory creation:
Created temp directory at: /tmp/.tmp[random]
Created file: /tmp/.tmp[random]/example.txt

2. Temp directory with prefix:
Created temp directory at: /tmp/my-app-[random]
Directory name starts with prefix: true

3. Keeping a temp directory:
Created temp directory at: /tmp/.tmp[random]
Kept directory at: /tmp/.tmp[random]
Directory still exists: true
Manually cleaned up kept directory

4. Manual close:
Created temp directory at: /tmp/.tmp[random]
Manually closed temp directory

Demo completed!
```

Note: In simulator mode, paths will be virtual (e.g., `/tmp/...`) but exist only in memory.

## Code Walkthrough

### Basic Temporary Directory Creation

The simplest usage creates a temporary directory that's automatically cleaned up:

```rust
use switchy_fs::tempdir;

let temp_dir = tempdir()?;
let path = temp_dir.path();

// Use the directory
println!("Temp directory: {}", path.display());

// Directory is automatically deleted when temp_dir goes out of scope
```

The `TempDir` type implements `Drop`, which removes the directory when the value is dropped.

### Temporary Directory with Custom Prefix

Add a custom prefix to make temp directories more identifiable:

```rust
use switchy_fs::TempDir;

let temp_dir = TempDir::with_prefix("my-app-")?;
// Creates: /tmp/my-app-[random]/
```

Useful for:

- Identifying which application created the temp directory
- Debugging (easier to spot your temp dirs)
- Organization when multiple apps use temp storage

### Keeping a Temporary Directory

Sometimes you want to prevent automatic cleanup:

```rust
let temp_dir = tempdir()?;
let path = temp_dir.path().to_path_buf();

// Keep the directory (prevent automatic cleanup)
let kept_path = temp_dir.keep();

// kept_path now contains the path, but won't be auto-deleted
// You're responsible for cleanup now
```

Use cases:

- Debugging (inspect contents after program exits)
- Passing temp directory to another process
- Converting temp work into permanent storage

### Manual Cleanup

Explicitly close and clean up a temp directory:

```rust
let temp_dir = tempdir()?;

// Use the directory...

// Manually close (clean up immediately)
temp_dir.close()?;

// Directory is now deleted
```

Benefits:

- Immediate cleanup (don't wait for Drop)
- Handle cleanup errors explicitly
- Control cleanup timing precisely

### Creating Files in Temp Directories

Combine with file operations:

```rust
use std::io::Write;
use switchy_fs::sync::OpenOptions;

let temp_dir = tempdir()?;
let file_path = temp_dir.path().join("data.txt");

let mut file = OpenOptions::new()
    .create(true)
    .write(true)
    .open(&file_path)?;

file.write_all(b"Temporary data")?;
```

## Key Concepts

### RAII and Automatic Cleanup

`TempDir` uses Rust's RAII pattern:

- Resource acquired in constructor (`tempdir()`)
- Resource released in destructor (`Drop`)
- Ensures cleanup even if errors occur

### Standard vs Simulator Mode

**Standard Mode** (`std` feature):

- Uses real filesystem temp directories
- Actual disk I/O
- Survives program crashes (until OS cleanup)

**Simulator Mode** (`simulator` feature):

- In-memory virtual filesystem
- No disk I/O
- Disappears when program exits
- Perfect for unit tests

### Temporary Directory Lifecycle

```rust
{
    let temp_dir = tempdir()?;
    // Directory created here

    // Use directory...

} // temp_dir dropped here, directory deleted automatically
```

### Error Handling

Operations can fail, so use `?` operator:

```rust
let temp_dir = tempdir()?;  // May fail (disk full, permissions)
temp_dir.close()?;           // May fail (permissions, in use)
```

## Testing the Example

Experiment with different scenarios:

1. **Test automatic cleanup**: Create temp dir, let it go out of scope, verify deletion
2. **Test keep**: Use `keep()` and verify directory persists
3. **Test manual close**: Call `close()` and verify immediate cleanup
4. **Test with files**: Create files in temp dir and verify they're deleted too
5. **Compare modes**: Run in standard and simulator mode to see differences

## Troubleshooting

### "Permission denied" errors

Ensure your temp directory location has write permissions. On Unix systems, `/tmp` is usually writable by all users.

### Temp directories not cleaned up

If you're seeing leftover temp directories:

- Check if you called `keep()` (prevents cleanup)
- Look for panics (may prevent Drop from running)
- On Unix, check `/tmp` for orphaned directories

### "Disk full" errors

Temp directories use disk space. If operations fail:

- Clean up old temp files manually
- Use `tempdir_in()` to specify a location with more space
- Consider using simulator mode for testing

### Differences between modes

In **simulator mode**:

- Temp directories are virtual (memory only)
- Paths look like real paths but aren't on disk
- Much faster for testing

In **standard mode**:

- Real filesystem operations
- Actual disk I/O
- Files persist if program crashes before cleanup

## Related Examples

- **basic_file_ops**: Creating and manipulating files (use temp dirs for storage)
- **directory_ops**: Directory creation and management
- **async_operations**: Async file operations (temp dirs work with both sync and async)
