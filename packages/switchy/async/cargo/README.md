# MoosicBox Async Cargo

Cargo tool (and library) reporting async code that `#[inject_yields]` does not cover.

## Overview

The MoosicBox Async Cargo package provides:

- **Lint Tool**: Command-line tool for async code analysis
- **Library**: `uncovered(root)`, for a test to call on its crate
- **Attribute Detection**: Finds async functions, blocks and closures `#[inject_yields]` does not cover
- **Root Source Support**: Analyzes Rust source files under `<root>/src`
- **CI Integration**: Suitable for continuous integration pipelines
- **Syntax Analysis**: Uses syn for accurate Rust code parsing

## Features

### Async Function Analysis

- **Function Detection**: Identifies async functions and methods
- **Block and Closure Detection**: Identifies `async` blocks and closures in any function
- **Attribute Checking**: Verifies an `#[inject_yields]` on the code or an enclosing item
- **Module Support**: Follows the module tree, `#[inject_yields]` modules and `inject_yields_mod!`

### Code Coverage

- **Source Scanning**: Recursively scans Rust source files under `<root>/src`
- **File Filtering**: Processes only `.rs` files
- **Syntax Parsing**: Robust parsing with error handling
- **Path Resolution**: Handles complex file structures

### Reporting

- **Warning Messages**: Clear warnings for uncovered code
- **File Location**: Reports file, line, column and enclosing function
- **Exit Codes**: Returns appropriate exit codes for CI/CD
- **Batch Processing**: Processes multiple files efficiently

## Installation

### As Binary

```bash
# Install from source
cargo install --path packages/switchy/async/cargo

# Or build locally
cd packages/switchy/async/cargo
cargo build --release
```

## Cargo Features

- **`fail-on-warnings`**: Enables strict compilation mode with warnings treated as errors

## Usage

### Command Line

```bash
# Check the crate in CARGO_MANIFEST_DIR
CARGO_MANIFEST_DIR=/path/to/crate switchy_async_cargo

# Check a specific crate, leaving out `#[cfg(test)]` code
switchy_async_cargo --root /path/to/crate --skip-tests
```

### Library

```rust
let findings = switchy_async_cargo::uncovered(std::path::Path::new("path/to/crate"));
for finding in findings.iter().filter(|finding| !finding.cfg_test) {
    eprintln!("{finding}");
}
```

`uncovered(root)` returns a `Finding` (file, line, column, kind, name, whether it is in
`#[cfg(test)]` code) for each:

- `async fn` and async method (in impls, and traits with a body);
- `async` block and `async` closure, in any function, sync or async;
- file it could not read or parse (`Kind::Unreadable`).

### Example Output

```
2 warnings:
warning: src/lib.rs:12:5: async fn `process_data` is not covered by #[inject_yields]
warning: src/handlers.rs:40:9: async block in `spawn_worker` is not covered by #[inject_yields]
```

## Coverage Rules

Code is covered when it, or an item around it (function, impl, trait, module, const or
static), has `#[inject_yields]` (any path ending in `inject_yields`), or when it is in a
module loaded with `inject_yields_mod!(mod name;)`. Async code inside an uncovered `async fn`,
block or closure is reported once, for the outermost one. Code inside macro invocations is
invisible to the attribute and to the check alike.

The check walks the module tree from `src/lib.rs`, `src/main.rs` and `src/bin/*`, following
`mod` declarations (and `#[path]`) as rustc does, then checks the other `.rs` files under
`src` on their own.

## Exit Codes

- **0**: Nothing uncovered
- **1**: Uncovered async code (or files that could not be checked) found

## Dependencies

- **Clap**: Command-line argument parsing
- **Syn**: Rust syntax parsing and AST traversal
- **WalkDir**: Recursive directory traversal
- **Proc Macro2**: Line and column numbers of findings
- **Pretty Env Logger**: Logging implementation

## Integration

This tool is designed for:

- **Development**: Local development workflow
- **CI/CD**: Continuous integration checks
- **Code Quality**: Ensuring consistent async function attributes
- **Team Development**: Enforcing coding standards across teams
- **Testing**: Ensuring proper yield injection for simulation testing

## Why inject_yields?

The `#[inject_yields]` attribute is important for:

- **Simulation Testing**: Enables deterministic async testing
- **Yield Points**: Provides controlled execution points
- **Debugging**: Improves debugging capabilities
- **Testing Reliability**: Ensures consistent test behavior
