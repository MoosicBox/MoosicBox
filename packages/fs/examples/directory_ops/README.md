# Directory Operations Example

Demonstrates comprehensive directory management operations using the `switchy_fs` package, including creation, reading, walking, and deletion with deterministic sorted ordering.

## What This Example Demonstrates

- Creating nested directory structures with `create_dir_all`
- Checking path existence with `exists`
- Reading directory contents with `read_dir_sorted` (alphabetically sorted)
- Recursively walking directory trees with `walk_dir_sorted`
- Removing directories and all contents with `remove_dir_all`
- Deterministic sorted ordering for reproducible results
- Organizing project-like directory structures

## Prerequisites

- Basic understanding of filesystem concepts (directories, paths)
- Familiarity with file operations (see `basic_file_ops` example)

## Running the Example

```bash
# Run with default features (using standard filesystem)
cargo run --manifest-path packages/fs/examples/directory_ops/Cargo.toml

# Run in simulator mode (in-memory filesystem for testing)
cargo run --manifest-path packages/fs/examples/directory_ops/Cargo.toml --no-default-features --features switchy_fs/simulator,switchy_fs/sync
```

## Expected Output

```
Demo: Directory Operations with switchy_fs

Using temporary directory: /tmp/.tmp[random]

1. Creating nested directory structures:
   Created: /tmp/.tmp[random]/project/src/modules/utils
   Created: /tmp/.tmp[random]/project/docs
   Created: /tmp/.tmp[random]/project/tests

2. Checking if paths exist:
   'project' exists: true
   'project/src' exists: true
   'project/nonexistent' exists: false

3. Creating files in directories:
   Created: project/src/main.rs
   Created: project/src/lib.rs
   Created: project/src/modules/utils/mod.rs
   Created: project/docs/README.md
   Created: project/tests/test1.rs
   Created: project/tests/test2.rs

4. Reading directory contents (sorted by filename):
   Contents of 'project/src':
     - lib.rs (file)
     - main.rs (file)
     - modules (dir)

5. Reading 'project/tests' directory:
     - test1.rs
     - test2.rs

6. Walking entire directory tree (sorted by path):
   All entries under 'project':
     [DIR ] docs
     [FILE] docs/README.md
     [DIR ] src
     [FILE] src/lib.rs
     [FILE] src/main.rs
     [DIR ] src/modules
     [DIR ] src/modules/utils
     [FILE] src/modules/utils/mod.rs
     [DIR ] tests
     [FILE] tests/test1.rs
     [FILE] tests/test2.rs

7. Demonstrating directory removal:
   Created temp_structure with nested levels
   Created 3 files in temp_structure
   'temp_structure' exists before removal: true
   Removed temp_structure and all contents
   'temp_structure' exists after removal: false

8. Demonstrating deterministic sorted order:
   Created files: ["zebra.txt", "apple.txt", "mango.txt", "banana.txt"]
   Reading back (sorted alphabetically):
     - apple.txt
     - banana.txt
     - mango.txt
     - zebra.txt

Demo completed successfully!
Temporary directory will be automatically cleaned up.
```

## Code Walkthrough

### Creating Nested Directories

The `create_dir_all` function creates all parent directories as needed:

```rust
let nested = temp_path
    .join("project")
    .join("src")
    .join("modules")
    .join("utils");
create_dir_all(&nested)?;
```

This creates the entire path `project/src/modules/utils` in one call, similar to `mkdir -p` in Unix.

### Checking Path Existence

The `exists` function checks if a path exists:

```rust
use switchy_fs::exists;

if exists(&project_path) {
    println!("Project directory exists");
}
```

Works for both files and directories.

### Reading Directory Contents (Sorted)

`read_dir_sorted` returns directory entries sorted alphabetically by filename:

```rust
let entries = read_dir_sorted(&src_path)?;

for entry in entries {
    let path = entry.path();
    let name = path.file_name().unwrap().to_string_lossy();
    println!("{}", name);
}
```

**Key benefit**: Results are deterministic, making tests reproducible across runs.

### Walking Directory Trees

`walk_dir_sorted` recursively traverses a directory tree, returning entries sorted by path:

```rust
let entries = walk_dir_sorted(&project_path)?;

for entry in entries {
    let path = entry.path();
    if path.is_dir() {
        println!("Directory: {}", path.display());
    } else {
        println!("File: {}", path.display());
    }
}
```

Visits all directories and files in sorted order for deterministic traversal.

### Removing Directories

`remove_dir_all` deletes a directory and all its contents recursively:

```rust
remove_dir_all(&temp_structure)?;
```

Similar to `rm -rf` in Unix. Use with caution!

## Key Concepts

### Sorted vs Unsorted Operations

`switchy_fs` provides sorted variants of directory operations:

- **`read_dir_sorted`**: Returns entries sorted by filename
- **`walk_dir_sorted`**: Returns entries sorted by full path
- **Standard `fs::read_dir`**: Order is undefined (filesystem-dependent)

### Why Sorted Operations Matter

1. **Reproducible tests**: Same order every run
2. **Deterministic builds**: Consistent output for build systems
3. **Predictable behavior**: Easier to debug and reason about

### Path Building

Use `Path::join()` to build paths portably:

```rust
let path = base_path.join("subdir").join("file.txt");
```

This works correctly on both Unix (/) and Windows (\) systems.

### Directory Existence

Before creating files, ensure parent directories exist:

```rust
let file_path = base.join("nested").join("dir").join("file.txt");
let parent = file_path.parent().unwrap();
create_dir_all(parent)?;  // Create parent directories

// Now safe to create the file
let file = OpenOptions::new().create(true).write(true).open(&file_path)?;
```

### Temporary Directories

This example uses `tempdir()` for automatic cleanup:

```rust
let temp_dir = tempdir()?;
let temp_path = temp_dir.path();

// Use temp_path for operations...

// Directory automatically deleted when temp_dir is dropped
```

## Testing the Example

Experiment with different scenarios:

1. **Create complex structures**: Add more nested levels and files
2. **Test sorted order**: Create files/dirs with various names and verify ordering
3. **Simulator mode**: Run with simulator features to avoid touching disk
4. **Error handling**: Try reading non-existent directories

## Troubleshooting

### "Directory not empty" errors

Use `remove_dir_all` instead of `remove_dir` to delete non-empty directories.

### "No such file or directory" errors

Ensure parent directories exist before creating files. Use `create_dir_all` on the parent path.

### "Permission denied" errors

Verify you have write permissions in the target location. Consider using `tempdir()` for test scenarios.

### Inconsistent ordering

If you need deterministic ordering, always use `read_dir_sorted` and `walk_dir_sorted` instead of standard library equivalents.

## Related Examples

- **basic_file_ops**: File creation and manipulation within directories
- **temp_dir**: Temporary directory management with automatic cleanup
- **async_operations**: Async versions of directory operations
