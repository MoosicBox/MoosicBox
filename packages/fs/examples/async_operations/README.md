# Async Operations Example

Demonstrates asynchronous file operations using the `switchy_fs` package with Tokio, including concurrent operations and async I/O patterns.

## What This Example Demonstrates

- Creating and writing files asynchronously
- Reading file contents with async/await
- Appending to files asynchronously
- Async seek operations for random access
- Concurrent file operations using Tokio tasks
- Simultaneous read/write operations in async context
- Async directory creation and management
- Proper async error handling with `?` operator

## Prerequisites

- Understanding of Rust's async/await syntax
- Basic familiarity with Tokio runtime
- Knowledge of async I/O concepts
- Familiarity with file operations (see `basic_file_ops` example)

## Running the Example

```bash
# Run with default features (using Tokio async filesystem)
cargo run --manifest-path packages/fs/examples/async_operations/Cargo.toml

# Run in simulator mode (in-memory async filesystem for testing)
cargo run --manifest-path packages/fs/examples/async_operations/Cargo.toml --no-default-features --features switchy_fs/simulator,switchy_fs/async,switchy_async/io,switchy_async/macros
```

## Expected Output

```
Demo: Async File Operations with switchy_fs

Using temporary directory: /tmp/.tmp[random]

1. Creating and writing to a new file (async):
   Created and wrote to: /tmp/.tmp[random]/async_example.txt

2. Reading file contents (async):
   File contents:
   Hello, async switchy_fs!
   This is an async operation.

3. Appending to existing file (async):
   Updated contents:
   Hello, async switchy_fs!
   This is an async operation.
   This line was appended asynchronously.

4. Using async seek to read specific parts:
   Read 5 bytes from position 7: async
   Read 14 bytes from 15 bytes before end: synchronously.

5. Performing concurrent file operations:
   Created: file1.txt
   Created: file2.txt
   Created: file3.txt

6. Async read-write operations:
   Current contents: Initial content
   Updated contents:
   Initial content
   Added via async read-write

7. Async directory operations:
   Created directory structure: /tmp/.tmp[random]/async/nested/dirs
   Created file: /tmp/.tmp[random]/async/nested/dirs/async_data.txt
   Cleaned up nested directories

Demo completed successfully!
Temporary directory will be automatically cleaned up.
```

## Code Walkthrough

### Async File Creation and Writing

The example uses `.await` for async operations:

```rust
let mut file = OpenOptions::new()
    .create(true)
    .write(true)
    .open(&file_path)
    .await?;  // Await the async open operation

file.write_all(b"Hello, async switchy_fs!\n").await?;  // Await the write
```

All I/O operations are non-blocking, allowing the runtime to handle other tasks while waiting for I/O.

### Async Reading

Reading files asynchronously uses the `AsyncReadExt` trait:

```rust
use switchy_async::io::AsyncReadExt;

let mut file = OpenOptions::new()
    .read(true)
    .open(&file_path)
    .await?;

let mut contents = String::new();
file.read_to_string(&mut contents).await?;
```

### Async Seek Operations

Seeking in async context uses `AsyncSeekExt`:

```rust
use switchy_async::io::{AsyncSeekExt, SeekFrom};

// Seek from start
file.seek(SeekFrom::Start(7)).await?;

// Seek from end
file.seek(SeekFrom::End(-15)).await?;
```

### Concurrent File Operations

One of the key benefits of async is the ability to perform operations concurrently:

```rust
let mut tasks = Vec::new();

for (i, filename) in files.iter().enumerate() {
    let file_path = temp_path.join(filename);
    let task = tokio::spawn(async move {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .open(&file_path)
            .await?;

        file.write_all(format!("Content for file {}\n", i + 1).as_bytes())
            .await?;
        Result::Ok(filename.to_string())
    });
    tasks.push(task);
}

// Wait for all tasks to complete
for task in tasks {
    let filename = task.await.expect("Task panicked")?;
    println!("Created: {}", filename);
}
```

This allows multiple files to be created simultaneously without blocking each other.

### Async Directory Operations

Directory operations are also async:

```rust
use switchy_fs::unsync::{create_dir_all, remove_dir_all};

// Create directories asynchronously
create_dir_all(&nested_path).await?;

// Remove directories asynchronously
remove_dir_all(&async_path).await?;
```

## Key Concepts

### Async vs Sync

The main differences between async and sync operations:

- **Async (unsync module)**: Non-blocking, allows other tasks to run while waiting for I/O
- **Sync (sync module)**: Blocking, thread waits for I/O to complete

### The Tokio Runtime

This example uses `#[tokio::main]` to set up the async runtime:

```rust
#[tokio::main]
async fn main() -> Result<()> {
    // Async operations can be used here
}
```

### Concurrent vs Parallel

Async operations are **concurrent** (interleaved) but not necessarily **parallel** (truly simultaneous). The Tokio runtime manages task scheduling.

### Error Handling

Async error handling uses the same `?` operator as sync code:

```rust
let file = OpenOptions::new()
    .read(true)
    .open(&path)
    .await?;  // Propagates errors up
```

### Module Choice

- Use `switchy_fs::unsync` for async operations
- Use `switchy_fs::sync` for synchronous operations
- Both modules provide similar APIs with different execution models

## Testing the Example

Experiment with different scenarios:

1. **Increase concurrency**: Modify the concurrent operations example to create more files
2. **Add delays**: Use `tokio::time::sleep()` to simulate slow I/O
3. **Error scenarios**: Try opening non-existent files without `create(true)`
4. **Simulator mode**: Run with simulator features to test without touching disk

## Troubleshooting

### "Runtime context not found" errors

Ensure you're running async code within a Tokio runtime. The `#[tokio::main]` macro handles this for `main()`.

### "Future not awaited" warnings

All async operations must be `.await`ed. Don't forget the `.await?` on every async call.

### Compilation errors about traits

Ensure you have the correct imports:

```rust
use switchy_async::io::{AsyncReadExt, AsyncWriteExt, AsyncSeekExt};
```

### Performance concerns

For truly parallel I/O, consider using `tokio::task::spawn_blocking` for CPU-intensive operations.

## Related Examples

- **basic_file_ops**: Synchronous version of these operations
- **temp_dir**: Temporary directory management (works with both sync and async)
- **directory_ops**: Directory traversal and sorted reading
