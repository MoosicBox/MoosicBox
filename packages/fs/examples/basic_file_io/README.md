# Basic File I/O Example

This example demonstrates the core file and directory operations provided by `switchy_fs`, including reading, writing, seeking, and directory management.

## Summary

A comprehensive demonstration of `switchy_fs`'s fundamental file system operations, showing how to create directories, read and write files, append data, seek within files, and clean up resources. The same code works seamlessly with both real and simulated filesystems.

## What This Example Demonstrates

- Creating directories and nested directory structures with `create_dir_all`
- Writing files using the simple `write` helper function
- Reading file contents using `read_to_string`
- Using `OpenOptions` for fine-grained control over file operations
- Appending data to existing files
- Reading files with seeking operations (`Read` + `Seek` traits)
- Listing directory contents with `read_dir_sorted`
- Removing directories recursively with `remove_dir_all`
- Switching between real and simulated filesystems with the same code

## Prerequisites

- Basic understanding of Rust file I/O concepts
- Familiarity with standard `std::io` traits (`Read`, `Write`, `Seek`)
- Understanding of filesystem paths and directory structures

## Running the Example

```bash
# Run with standard filesystem (default)
cargo run --manifest-path packages/fs/examples/basic_file_io/Cargo.toml

# Run with simulated in-memory filesystem
cargo run --manifest-path packages/fs/examples/basic_file_io/Cargo.toml --no-default-features --features simulator
```

## Expected Output

```
switchy_fs Basic File I/O Example

This example demonstrates core file operations using switchy_fs.
The same code works with both real and simulated filesystems!

Note: Running in STANDARD mode - using real filesystem

=== Example 1: Creating Directories ===
Created directory: target/switchy_example
Created nested directories: target/switchy_example/subdir/nested

=== Example 2: Writing Files (Simple) ===
Wrote: target/switchy_example/hello.txt
Wrote: target/switchy_example/subdir/data.txt

=== Example 3: Reading Files (Simple) ===
Read from hello.txt: "Hello, World!"
Read from data.txt: "This is nested data"

=== Example 4: Using OpenOptions ===
Created and wrote to: target/switchy_example/config.txt
Config contents:
# Configuration File
setting1=value1
setting2=value2

=== Example 5: Appending to Files ===
Appended line to config.txt
Updated config:
# Configuration File
setting1=value1
setting2=value2
setting3=appended

=== Example 6: Reading with Seeking ===
First 5 bytes: Hello
Full content after seeking: "Hello, World!"

=== Example 7: Reading Directories ===
Files in target/switchy_example:
  - config.txt
  - hello.txt
  - subdir

=== Example 8: Removing Directories ===
Removed directory: target/switchy_example

✅ All examples completed successfully!
```

## Code Walkthrough

### Example 1: Creating Directories

```rust
create_dir_all(base_path)?;
create_dir_all(format!("{base_path}/subdir/nested"))?;
```

The `create_dir_all` function creates a directory and all its parent directories if they don't exist, similar to `mkdir -p` in Unix.

### Example 2-3: Simple File I/O

```rust
write(format!("{base_path}/hello.txt"), b"Hello, World!")?;
let content = read_to_string(format!("{base_path}/hello.txt"))?;
```

The `write` and `read_to_string` helpers provide the simplest way to write and read file contents.

### Example 4: Using OpenOptions

```rust
let mut file = OpenOptions::new()
    .create(true)
    .write(true)
    .truncate(true)
    .open(format!("{base_path}/config.txt"))?;

writeln!(file, "# Configuration File")?;
```

`OpenOptions` provides fine-grained control over how files are opened:

- `.create(true)` - Create the file if it doesn't exist
- `.write(true)` - Open for writing
- `.truncate(true)` - Clear existing contents
- `.append(true)` - Append to existing contents
- `.read(true)` - Open for reading

### Example 5: Appending to Files

```rust
let mut append_file = OpenOptions::new()
    .append(true)
    .open(format!("{base_path}/config.txt"))?;

writeln!(append_file, "setting3=appended")?;
```

Appending adds content to the end of a file without overwriting existing data.

### Example 6: Seeking

```rust
let mut read_file = OpenOptions::new()
    .read(true)
    .open(format!("{base_path}/hello.txt"))?;

let mut buffer = vec![0u8; 5];
read_file.read_exact(&mut buffer)?;

read_file.seek(SeekFrom::Start(0))?;
```

Files returned by `switchy_fs` implement standard `Read` and `Seek` traits, allowing you to:

- Read specific amounts of data with `read_exact`
- Navigate within files using `seek`
- Use `SeekFrom::Start`, `SeekFrom::End`, or `SeekFrom::Current`

### Example 7: Reading Directories

```rust
let entries = read_dir_sorted(base_path)?;
for entry in entries {
    let entry = entry?;
    println!("  - {}", entry.file_name().to_string_lossy());
}
```

`read_dir_sorted` returns directory entries in sorted order, making output predictable and testable.

### Example 8: Cleanup

```rust
remove_dir_all(base_path)?;
```

`remove_dir_all` recursively removes a directory and all its contents, similar to `rm -rf`.

## Key Concepts

### Filesystem Abstraction

`switchy_fs` provides a unified API that works with multiple backends:

- **Standard mode (`std` feature)**: Operations use the real filesystem via `std::fs`
- **Simulator mode (`simulator` feature)**: Operations use an in-memory filesystem, perfect for testing

The beauty of `switchy_fs` is that **the same code works with both backends** - just change the feature flags!

### Standard Traits

All file types implement standard Rust traits:

- `std::io::Read` - Read bytes from files
- `std::io::Write` - Write bytes to files
- `std::io::Seek` - Navigate within files
- `Send + Sync` - Safe to use across threads

This means you can use `switchy_fs` files with any Rust code that expects these standard traits.

### Error Handling

All operations return `std::io::Result<T>`, consistent with Rust's standard library. Use the `?` operator for clean error propagation.

## Testing the Example

Try modifying the example to experiment with different operations:

1. **Test error handling**: Try reading a non-existent file and handle the error
2. **Experiment with seeking**: Use `SeekFrom::End(-5)` to read the last 5 bytes
3. **Compare modes**: Run with both `--features std` and `--features simulator` to see the difference
4. **Add nested operations**: Create deeper directory structures and verify they work

## Troubleshooting

### Permission Denied Errors (Standard Mode)

If you get permission errors in standard mode, ensure the target directory is writable:

```bash
chmod -R u+w target/
```

### File Not Found After Writing

Make sure to drop the file handle before reading:

```rust
drop(file);  // Close the file
let content = read_to_string(path)?;  // Now we can read
```

### Simulator Mode Differences

In simulator mode, paths start with `/` (Unix-style) while standard mode may use relative paths. The example handles this automatically by using different base paths.

## Related Examples

- [simulator_mode](../simulator_mode/) - Shows how to use the simulator for testing
- [temp_dir](../temp_dir/) - Demonstrates temporary directory creation and management
