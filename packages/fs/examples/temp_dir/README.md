# Temp Directory Example

This example demonstrates `switchy_fs`'s temporary directory functionality, showing how to create, use, and manage temporary directories that are automatically cleaned up.

## Summary

A comprehensive demonstration of temporary directory creation and management using `switchy_fs`, including basic creation, custom prefixes/suffixes, manual cleanup, and keeping directories beyond their normal lifetime. Works seamlessly with both real and simulated filesystems.

## What This Example Demonstrates

- Creating temporary directories with `tempdir()`
- Using custom prefixes with `TempDir::with_prefix()`
- Using custom suffixes with `TempDir::with_suffix()`
- Preventing automatic cleanup with `keep()`
- Manual cleanup with `close()`
- Creating temp directories in specific locations with `tempdir_in()`
- Differences between standard and simulator modes
- Automatic cleanup when `TempDir` is dropped

## Prerequisites

- Basic understanding of Rust ownership and RAII (Resource Acquisition Is Initialization)
- Familiarity with filesystem paths
- Understanding of temporary file concepts

## Running the Example

```bash
# Run with standard filesystem (default)
cargo run --manifest-path packages/fs/examples/temp_dir/Cargo.toml

# Run with simulated filesystem
cargo run --manifest-path packages/fs/examples/temp_dir/Cargo.toml --no-default-features --features simulator

# Run with simulator backed by real filesystem
cargo run --manifest-path packages/fs/examples/temp_dir/Cargo.toml --no-default-features --features simulator-real-fs
```

## Expected Output

```
Demo: switchy_fs temp_dir functionality

1. Basic temp directory creation:
Created temp directory at: target/switchy_example/tmp.abc123xyz
Created file: target/switchy_example/tmp.abc123xyz/example.txt

2. Temp directory with prefix:
Created temp directory at: target/switchy_example/my-app-def456uvw
Directory name starts with prefix: true

3. Keeping a temp directory:
Created temp directory at: target/switchy_example/tmp.ghi789rst
Kept directory at: target/switchy_example/tmp.ghi789rst
Directory still exists: true
Manually cleaned up kept directory

4. Manual close:
Created temp directory at: target/switchy_example/tmp.jkl012mno
Manually closed temp directory

Demo completed!
```

## Code Walkthrough

### Example 1: Basic Temp Directory Creation

```rust
let temp_dir = tempdir()?;
let path = temp_dir.path();
println!("Created temp directory at: {}", path.display());

// Create a file in the temp directory
let file_path = path.join("example.txt");
let mut file = switchy_fs::sync::OpenOptions::new()
    .create(true)
    .write(true)
    .open(&file_path)?;
writeln!(file, "Hello from switchy_fs temp directory!")?;

// Directory will be cleaned up when temp_dir is dropped
```

The `tempdir()` function creates a new temporary directory with a random name. The directory is automatically deleted when the `TempDir` value goes out of scope (RAII pattern).

### Example 2: Custom Prefix

```rust
let temp_dir = TempDir::with_prefix("my-app-")?;
println!("Created temp directory at: {}", temp_dir.path().display());
```

Use `with_prefix()` to add a custom prefix to the directory name. This makes it easier to identify your application's temporary directories.

### Example 3: Keeping a Directory

```rust
let temp_dir = tempdir()?;
let path = temp_dir.path().to_path_buf();

// Keep the directory (prevent automatic cleanup)
let kept_path = temp_dir.keep();
println!("Kept directory at: {}", kept_path.display());

// Directory still exists and won't be automatically deleted
// You're now responsible for cleanup
switchy_fs::sync::remove_dir_all(kept_path)?;
```

Sometimes you want to inspect the temp directory after your program finishes. The `keep()` method prevents automatic cleanup and returns the path. **Important**: After calling `keep()`, you're responsible for cleaning up the directory.

### Example 4: Manual Close

```rust
let temp_dir = tempdir()?;
println!("Created temp directory at: {}", temp_dir.path().display());

// Manually close (clean up immediately)
temp_dir.close()?;
println!("Manually closed temp directory");
```

Use `close()` to explicitly clean up a temp directory before it goes out of scope. This is useful when you want to ensure cleanup happens at a specific point.

### Custom Location with `tempdir_in()`

```rust
// First create a temp directory to use as parent
let parent_temp = tempdir()?;
let parent_path = parent_temp.path();

// Create temp directory inside it
let temp_dir = tempdir_in(parent_path)?;
assert!(temp_dir.path().starts_with(parent_path));
```

By default, temp directories are created in the system's temp location. Use `tempdir_in()` to create them in a specific directory.

## Key Concepts

### RAII and Automatic Cleanup

`TempDir` follows Rust's RAII pattern:

```rust
{
    let temp = tempdir()?;
    // Use the temp directory
    // ...
} // temp is dropped here, directory is automatically deleted
```

This ensures resources are cleaned up even if errors occur or early returns happen.

### Standard vs Simulator Mode

**Standard mode (`std` feature)**:

- Creates real directories on disk (usually in `/tmp` or `C:\Temp`)
- Uses `tempfile` crate under the hood
- Files persist on disk until cleanup

**Simulator mode (`simulator` feature)**:

- Creates directories in simulated in-memory filesystem
- No actual disk I/O
- Much faster for testing
- Can be reset instantly with `reset_fs()`

**The same code works with both modes!** Just change the feature flags.

### When to Use Temporary Directories

Use temporary directories for:

- Test fixtures and test data
- Intermediate processing files
- Temporary caches
- Build artifacts
- Downloaded content that's not needed long-term

### Prefix and Suffix Best Practices

Use descriptive prefixes to:

- Identify your application's temp files
- Make debugging easier
- Help with manual cleanup if needed

```rust
TempDir::with_prefix("myapp-cache-")?;  // Good
TempDir::with_prefix("tmp-")?;          // Less useful
```

## Testing the Example

Try these experiments:

1. **Verify cleanup**: Add a `sleep()` before the program ends and check if the temp directory exists
2. **Test `keep()`**: Use `keep()` and verify the directory persists after the program ends
3. **Compare modes**: Run with both `--features std` and `--features simulator` to see the differences
4. **Inspect paths**: Print the paths and see where temp directories are created in each mode

## Troubleshooting

### Permission Denied (Standard Mode)

If you get permission errors, ensure the system temp directory is writable:

```bash
# Linux/Mac
ls -la /tmp

# Windows
dir %TEMP%
```

### Temp Directory Not Cleaned Up

If using `keep()`, cleanup is your responsibility:

```rust
let kept_path = temp_dir.keep();
// Don't forget to clean up!
remove_dir_all(&kept_path)?;
```

### Different Paths in Different Modes

In simulator mode, paths start with `/tmp`. In standard mode, they use the system's actual temp directory. This is expected behavior.

## Related Examples

- [basic_file_io](../basic_file_io/) - Demonstrates file operations to use within temp directories
- [simulator_mode](../simulator_mode/) - Shows how to use the simulator for testing
