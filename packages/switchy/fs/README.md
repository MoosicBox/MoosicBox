# Switchy Filesystem (switchy_fs)

Cross-platform filesystem abstraction with sync and async operations.

## User directory locations

`directories::{home_dir, config_dir, data_dir, data_local_dir}` return
`Option<PathBuf>` without creating directories. Enable `native-directories` for
lookup without an executor; `std` and `tokio` also enable it. Through `switchy`,
use `fs-native-directories`, `fs-std`, or `fs-tokio`.

Native Unix home resolution uses nonempty `HOME`, then the user account database
(except Android/iOS/Emscripten). Linux-style config/data lookup accepts only absolute
XDG overrides. macOS uses `Library/Application Support`. Windows uses native Known
Folder APIs, including roaming/local storage and redirected folders. Unavailable
locations return `None`; account lookup retries are bounded to 1 MiB of storage.

**The simulator takes precedence**, including with both native backends enabled.
Default features include the simulator. For native production lookup, disable default
features and ensure dependencies do not unify in `simulator`.

Configure immutable `DirectoryLocations` using
`simulator::Filesystem::with_directory_locations`. The default filesystem has no
user locations. Locations follow `with_filesystem` and `scope_filesystem`, including
nested scopes, each async poll, and cancellation cleanup. Spawned tasks must be
scoped explicitly. Resetting filesystem contents preserves configured locations.
Paths are supplied verbatim and need not exist. `simulator-real-fs` does **not**
switch directory resolution to host paths: permission for native I/O is not
permission to discover the host user's directories.

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
- **Create New**: Exclusive creation (`O_EXCL`)
- **No-follow / Directory / Mode**: `O_NOFOLLOW`, `O_DIRECTORY` and creation permission bits (Unix)

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
        .write(true)
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
        .write(true)
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
    .write(true)
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
    use std::io::Write;

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

### POSIX Simulation Model

The simulator is an in-memory inode filesystem built to run storage code under
deterministic simulation:

- **Inodes and paths**: directories map names to inodes; paths resolve one component
  at a time and follow symbolic links (40 at most, then `ELOOP`). Handles refer to
  inodes, so renaming or unlinking an open file behaves as on POSIX. Hard links,
  symbolic links, permission bits, ownership and a simulated effective user
  (`Filesystem::with_credentials`, `current_uid`) are modeled. Errors carry the host
  `errno` (`ENOENT`, `EEXIST`, `ENOTDIR`, `EISDIR`, `ENOTEMPTY`, `ELOOP`, `EACCES`, ...).
- **Durability**: file data becomes durable on `sync_data`/`sync_all`; directory changes
  (create, link, unlink, rename) become durable only when their directory is
  synchronized (`Dir::sync` or `sync_directory`). A rename is atomic across crashes.
- **Crashes**: `Filesystem::crash(&CrashPolicy)` reverts to durable state, invalidates
  open handles (`ESTALE`) and releases locks. `CrashPolicy::power_loss()` (the default)
  keeps a random torn prefix of unsynchronized writes and an ordered random subset of
  unsynchronized directory changes; `drop_all_pending()` and `process_crash()` are the
  deterministic extremes.
- **Faults**: `FaultPlan` injects `EIO`, `ENOSPC` and short writes per `OpClass` with a
  seeded probability, `Filesystem::inject_fault` queues one-shot faults, and
  `Filesystem::set_quota` limits stored bytes. A failed file synchronization forgets the
  pending data (Linux "fsyncgate" semantics).
- **Timestamps**: `Metadata::accessed`/`modified`/`created` come from the simulated clock
  (`switchy_time::now`) and change as on POSIX: creation sets all three, writing or
  truncating a file updates `modified`, reading a file, listing a directory or reading a
  symbolic link updates `accessed`, and creating, removing or renaming an entry updates
  its directory's `modified`. Inspecting metadata changes nothing. Timestamps are not
  crash-modeled (a crash keeps each surviving inode's current timestamps).
- **Personalities**: `Filesystem::set_personality` (or `set_run_personality` for the run's
  default namespace) selects `Personality::Posix` (the default) or
  `Personality::Windows`. Under Windows, removing a file or renaming another file over it
  while a handle on it is open fails with a sharing violation (raw os error 32 on Windows
  hosts, `PermissionDenied`/`EACCES` elsewhere; renaming the open file itself is allowed),
  names are case-insensitive but case-preserving (a rename to another spelling of the
  same name changes its case), and creating a symbolic link fails for lack of privilege
  (raw os error 1314 on Windows hosts, `PermissionDenied`/`EPERM` elsewhere).
- **Operation hook**: `set_operation_hook(Some(hook))` installs a per-run
  `Arc<dyn Fn(OpClass) + Send + Sync>` called at the start of every simulated operation
  that touches a namespace (not when closing handles), before any filesystem lock is
  taken. A deterministic scheduler can place a thread switch point there; operations the
  hook performs itself do not call it again.
- **Determinism**: every random choice draws from named streams of the current
  `switchy_simulation` run (`switchy_fs::crash`, `switchy_fs::faults`, `switchy_fs::temp`),
  and each run has its own default `Filesystem`, operation hook and temporary directory
  names. Nothing is shared between runs or kept per OS thread, so simulated threads of one
  run (real OS threads taking turns) see the same state and parallel runs never interfere.

```rust
use std::{io::Write as _, sync::Arc};
use switchy_fs::simulator::{CrashPolicy, Filesystem, with_filesystem};
use switchy_fs::sync::{Dir, OpenOptions, read};

let fs = Arc::new(Filesystem::new());
with_filesystem(&fs, || {
    let dir = Dir::open("/").unwrap();
    let mut file = dir
        .open_file_at("state", &OpenOptions::new().write(true).create_new(true))
        .unwrap();
    file.write_all(b"v1").unwrap();
    file.sync_all().unwrap(); // data durable
    dir.sync().unwrap(); // directory entry durable
    file.write_all(b" unsynced").unwrap();
});

fs.crash(&CrashPolicy::drop_all_pending());

with_filesystem(&fs, || assert_eq!(read("/state").unwrap(), b"v1"));
```

### Directory Handles and Publishing

`sync::Dir` is a capability-style directory handle: it is opened without following a
final symbolic link, and `open_file_at`, `create_dir_at`, `metadata_at` (no-follow),
`remove_file_at`, `remove_dir_at`, `rename_at`, `rename_noreplace_at`, `hard_link_at`,
`symlink_at`, `read_link_at`, `entries` and `sync` take single entry names. On Unix the
real backend uses `openat`-family system calls; on Windows `Dir` returns `Unsupported`.

`sync::rename_noreplace` and `sync::publish_file_noclobber` publish a file only if the
destination does not exist (`renameat2(RENAME_NOREPLACE)` on Linux, `renameatx_np` on
Apple platforms, link-and-unlink elsewhere). `sync::tempfile`/`tempfile_in` create
anonymous temporary files.

The conformance suite (`tests/conformance.rs`) runs the same cases against the `std`
backend in a real temporary directory and against the simulator.

### Simulator Real Filesystem Access

With the `simulator-real-fs` feature, you can temporarily access the real filesystem within simulator mode:

```rust
#[cfg(all(feature = "simulator", feature = "simulator-real-fs"))]
use switchy_fs::{sync::read_to_string, with_real_fs};

// In simulator mode, but need to access real filesystem temporarily
let real_data = with_real_fs(|| {
    std::fs::read_to_string("/path/to/real/file.txt")
}).unwrap();

// Back to simulated filesystem
let sim_data = read_to_string("/simulated/file.txt")?;
```

### Seeding Simulator from Real Filesystem

When loading fixtures into the simulator, use the seeding APIs:

- `seed_from_real_fs(real_path, sim_path)`: Recursively copy a real filesystem path into a simulator path
- `seed_from_real_fs_same_path(path)`: Seed using the same path for both real and simulator locations
- `seed_relative_to(base, relative_paths)`: Seed multiple paths relative to a base directory
- `is_simulator_enabled()`: Check whether the crate was compiled with simulator support

```rust
use switchy_fs::{seed_from_real_fs, seed_relative_to};

// Seed one path from real filesystem into a simulator path
seed_from_real_fs("./tests/fixtures", "/fixtures")?;

// Seed multiple paths relative to a project root
seed_relative_to(
    env!("CARGO_MANIFEST_DIR"),
    ["tests/fixtures", "tests/scripts"],
)?;
```

### Temporary Directories

The package provides temporary directory support that automatically cleans up when dropped.
In the simulator, names draw random alphanumeric characters from the run's
`switchy_fs::temp` stream (so a seed reproduces them), creation retries with a fresh name
when one exists, and a `TempDir` removes itself from the namespace it was created in,
whichever thread drops it:

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

- **log**: Logging facade for diagnostic messages
- **switchy_async**: Async I/O trait abstractions (optional)
- **switchy_simulation**: Per-run seed, RNG streams and default namespace for the simulator (optional)
- **rustix**: `openat`-family calls, `errno` values and effective user id on Unix (optional)
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
