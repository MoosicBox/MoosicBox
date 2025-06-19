# MoosicBox Task

Simple task management utilities for the MoosicBox ecosystem, providing basic async task spawning with optional naming and profiling support for Tokio-based applications.

## Features

- **Named Task Spawning**: Spawn async tasks with names for debugging
- **Blocking Task Support**: Execute blocking operations without blocking the async runtime
- **Local Task Spawning**: Spawn tasks on local task sets
- **Runtime Flexibility**: Spawn tasks on specific Tokio runtime handles
- **Optional Profiling**: Integrate with profiling tools when enabled
- **Debug Logging**: Optional trace logging for task lifecycle

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_task = "0.1.1"

# Enable profiling support
moosicbox_task = { version = "0.1.1", features = ["profiling"] }
```

## Usage

### Basic Task Spawning

```rust
use moosicbox_task::{spawn, spawn_blocking};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Spawn async task with a name
    let async_task = spawn("background-processing", async {
        println!("Running async task");
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        "async result"
    });

    // Spawn blocking task with a name
    let blocking_task = spawn_blocking("cpu-intensive-work", || {
        println!("Running blocking task");
        std::thread::sleep(std::time::Duration::from_secs(1));
        "blocking result"
    });

    // Wait for both tasks to complete
    let async_result = async_task.await?;
    let blocking_result = blocking_task.await?;

    println!("Async result: {}", async_result);
    println!("Blocking result: {}", blocking_result);

    Ok(())
}
```

### Spawning on Specific Runtimes

```rust
use moosicbox_task::{spawn_on, spawn_blocking_on};
use tokio::runtime::Handle;

async fn spawn_on_runtime() -> Result<(), Box<dyn std::error::Error>> {
    let handle = Handle::current();

    // Spawn on specific runtime handle
    let task = spawn_on("named-task", &handle, async {
        println!("Running on specific runtime");
        "result"
    });

    // Spawn blocking on specific runtime
    let blocking_task = spawn_blocking_on("blocking-task", &handle, || {
        println!("Blocking work on specific runtime");
        42
    });

    let result = task.await?;
    let blocking_result = blocking_task.await?;

    println!("Results: {}, {}", result, blocking_result);

    Ok(())
}
```

### Local Task Sets

```rust
use moosicbox_task::{spawn_local, spawn_local_on};
use tokio::task::LocalSet;

async fn local_task_example() -> Result<(), Box<dyn std::error::Error>> {
    let local_set = LocalSet::new();

    // Spawn task on local set
    let task = local_set.run_until(async {
        let task = spawn_local("local-task", async {
            println!("Running on local task set");
            "local result"
        });

        task.await
    }).await?;

    println!("Local task result: {}", task);

    // Or spawn on specific local set
    let result = spawn_local_on("named-local-task", &local_set, async {
        "local set result"
    });

    // Run the local set
    let output = local_set.run_until(result).await?;
    println!("Local set output: {}", output);

    Ok(())
}
```

### Blocking in Async Context

```rust
use moosicbox_task::block_on;

// Block on async operation (useful in sync contexts)
fn sync_function() -> Result<String, Box<dyn std::error::Error>> {
    let result = block_on("sync-to-async", async {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        "converted to sync"
    });

    Ok(result)
}
```

### Optional Runtime Spawning

```rust
use moosicbox_task::spawn_on_opt;

async fn maybe_spawn_on_handle(handle: Option<&tokio::runtime::Handle>) {
    // Spawn on handle if provided, otherwise use current runtime
    let task = spawn_on_opt("flexible-task", handle, async {
        "flexible result"
    });

    let result = task.await.unwrap();
    println!("Result: {}", result);
}
```

### With Profiling (Optional)

When the `profiling` feature is enabled, tasks automatically get profiling scopes:

```rust
use moosicbox_task::spawn;

// This task will have profiling information when profiling is enabled
let task = spawn("profiled-task", async {
    // Work here will be tracked in profiling tools
    expensive_computation().await
});
```

## Core Functions

### Task Spawning
- `spawn(name, future)`: Spawn named async task on current runtime
- `spawn_on(name, handle, future)`: Spawn on specific runtime handle
- `spawn_on_opt(name, handle, future)`: Spawn on optional handle

### Blocking Tasks
- `spawn_blocking(name, function)`: Spawn blocking task on current runtime
- `spawn_blocking_on(name, handle, function)`: Spawn blocking task on specific runtime

### Local Tasks
- `spawn_local(name, future)`: Spawn task on current local set
- `spawn_local_on(name, local_set, future)`: Spawn on specific local set

### Blocking Operations
- `block_on(name, future)`: Block current thread until future completes
- `block_on_runtime(name, handle, future)`: Block using specific runtime

## Features

- **profiling**: Enables automatic profiling scope creation for spawned tasks

## Dependencies

- `tokio`: For async task spawning and runtime management
- `futures`: For Future trait
- `log`: For optional debug logging
- `profiling`: For optional profiling support

This library provides simple, named task spawning utilities that make debugging and profiling easier in async Rust applications.
