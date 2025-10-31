# Simulator Mode Example

This example demonstrates how to use `switchy_fs`'s simulator mode for fast, isolated filesystem testing without touching your real disk.

## Summary

Shows how to use the in-memory filesystem simulator for testing, including resetting state between tests, verifying isolation from the real filesystem, and optionally mixing simulated and real filesystem operations for hybrid testing scenarios.

## What This Example Demonstrates

- Using the filesystem simulator for testing without disk I/O
- Resetting the simulated filesystem to a clean state with `reset_fs()`
- Writing and reading files that only exist in memory
- Verifying complete isolation from the real filesystem
- Using `with_real_fs()` for hybrid testing (simulator + real FS)
- Benefits of simulator mode: speed, isolation, determinism
- How the same code works transparently with both modes

## Prerequisites

- Understanding of filesystem operations and testing concepts
- Familiarity with file I/O in Rust
- Basic knowledge of test isolation and mocking concepts

## Running the Example

```bash
# Run with simulator mode (default)
cargo run --manifest-path packages/fs/examples/simulator_mode/Cargo.toml

# Run with simulator mode + real filesystem support
cargo run --manifest-path packages/fs/examples/simulator_mode/Cargo.toml --features simulator-real-fs
```

## Expected Output

```
switchy_fs Simulator Mode Example

This example demonstrates using the simulator for testing without touching the real filesystem.

✓ Simulator filesystem reset to clean state

=== Example 1: Basic Simulator Usage ===
Saved config to: /tmp/app/config.json
Loaded config from: /tmp/app/config.json
Config content: {"debug": true}

=== Example 2: Testing in Isolation ===
Processing multiple users...
Saved config to: /tmp/users/1/data.txt
Loaded config from: /tmp/users/1/data.txt
Successfully processed data for user 1
Saved config to: /tmp/users/2/data.txt
Loaded config from: /tmp/users/2/data.txt
Successfully processed data for user 2
Saved config to: /tmp/users/3/data.txt
Loaded config from: /tmp/users/3/data.txt
Successfully processed data for user 3

=== Example 3: Simulator Isolation ===
✓ All files exist in simulated filesystem
✓ No files were created on your real disk

=== Example 4: Reset Simulator ===
✓ Simulator reset
✓ All files removed from simulated filesystem

✅ All examples completed successfully!

Key Benefits:
  • No disk I/O - tests run faster
  • No cleanup needed - reset with one call
  • No file conflicts - each test can use same paths
  • No permissions issues - complete control
  • Deterministic - no race conditions from disk
```

## Code Walkthrough

### Example 1: Basic Simulator Usage

```rust
#[cfg(feature = "simulator")]
{
    switchy_fs::simulator::reset_fs();
}

save_config("/tmp/app/config.json", r#"{"debug": true}"#)?;
let config = load_config("/tmp/app/config.json")?;
```

In simulator mode, all file operations happen entirely in memory. The `reset_fs()` function clears the simulated filesystem to a clean state, which is perfect for test setup.

### Example 2: Testing in Isolation

```rust
fn process_user_data(user_id: u32, data: &str) -> std::io::Result<()> {
    let path = format!("/tmp/users/{user_id}/data.txt");
    save_config(&path, data)?;

    let loaded = load_config(&path)?;
    assert_eq!(loaded, data);
    Ok(())
}

process_user_data(1, "Alice's data")?;
process_user_data(2, "Bob's data")?;
```

Each operation is completely isolated. Multiple tests can run in parallel without interfering with each other or leaving files on disk.

### Example 3: Verify Isolation

```rust
#[cfg(feature = "simulator")]
{
    assert!(switchy_fs::exists("/tmp/app/config.json"));
    assert!(switchy_fs::exists("/tmp/users/1/data.txt"));
    // These files only exist in memory!
}
```

The `exists()` function confirms files are present in the simulated filesystem. If you check your real `/tmp` directory, you won't find these files - they never touch the disk.

### Example 4: Reset for Clean State

```rust
switchy_fs::simulator::reset_fs();

// All files are gone
assert!(!switchy_fs::exists("/tmp/app/config.json"));
```

Calling `reset_fs()` instantly clears all simulated files. This is much faster than recursive directory deletion and completely deterministic.

### Example 5: Hybrid Testing with Real FS

```rust
#[cfg(feature = "simulator-real-fs")]
{
    // Normal operations use simulator
    write("/tmp/simulator-only.txt", b"Simulated")?;

    // Temporarily use real filesystem
    switchy_fs::with_real_fs(|| {
        create_dir_all("target/real_fs_test")?;
        write("target/real_fs_test/real-file.txt", b"Real")?;
    });

    // Back to simulator
    assert!(switchy_fs::exists("/tmp/simulator-only.txt"));
}
```

With the `simulator-real-fs` feature, you can temporarily switch to the real filesystem for specific operations. This is useful when you need to test integration with actual files while keeping most operations simulated.

## Key Concepts

### Why Use Simulator Mode?

1. **Speed**: No disk I/O means tests run 10-100x faster
2. **Isolation**: Each test gets a clean filesystem, no interference
3. **Determinism**: No race conditions, no disk full errors, no permission issues
4. **Simplicity**: No cleanup code needed, just call `reset_fs()`
5. **Flexibility**: Same code works in both simulator and real modes

### When to Use Simulator Mode

Use simulator mode for:

- **Unit tests** - Test individual functions that use filesystem operations
- **Integration tests** - Test components that interact with files
- **Rapid prototyping** - Develop without worrying about cleanup
- **CI/CD pipelines** - Faster tests, no disk space concerns
- **Parallel test execution** - No file conflicts between tests

Use real filesystem when:

- **Integration testing with external tools** - Testing actual file formats
- **Performance testing** - Measuring real disk I/O performance
- **End-to-end testing** - Verifying actual file creation
- **Platform-specific behavior** - Testing filesystem quirks

### Feature Flags

- `simulator` - Enables in-memory filesystem simulation
- `simulator-real-fs` - Adds `with_real_fs()` for hybrid testing
- `std` - Uses real filesystem via `std::fs`

The beauty of `switchy_fs` is that **your code doesn't change** - only the feature flags determine which backend is used!

### Testing Best Practices

```rust
#[cfg(test)]
mod tests {
    use switchy_fs::sync::{write, read_to_string};

    #[test]
    fn test_config_save_load() {
        #[cfg(feature = "simulator")]
        switchy_fs::simulator::reset_fs();

        let path = "/tmp/test-config.json";
        write(path, b"{\"key\": \"value\"}").unwrap();
        let content = read_to_string(path).unwrap();

        assert_eq!(content, r#"{"key": "value"}"#);

        // No cleanup needed in simulator mode!
    }
}
```

Always call `reset_fs()` at the start of tests to ensure clean state. No cleanup needed at the end!

## Testing the Example

Try these experiments:

1. **Check your real filesystem**: Run the example, then check `/tmp` - you won't find the files!
2. **Compare speeds**: Time this example vs a version using real filesystem
3. **Parallel execution**: Run multiple instances simultaneously - no conflicts!
4. **Modify and reset**: Make changes, call `reset_fs()`, verify clean state

## Troubleshooting

### Files Persist After Reset

Make sure you're calling `reset_fs()` from the `simulator` module:

```rust
#[cfg(feature = "simulator")]
switchy_fs::simulator::reset_fs();
```

### Cannot Find reset_fs()

Ensure you have the `simulator` feature enabled:

```toml
switchy_fs = { workspace = true, features = ["simulator"] }
```

### with_real_fs() Not Available

Add the `simulator-real-fs` feature:

```bash
cargo run --features simulator-real-fs
```

## Related Examples

- [basic_file_io](../basic_file_io/) - Demonstrates core file operations that work with both modes
- [temp_dir](../temp_dir/) - Shows temporary directory management
