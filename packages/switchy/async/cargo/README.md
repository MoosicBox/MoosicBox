# MoosicBox Async Cargo

Cargo tool for detecting missing `#[inject_yields]` attributes on async functions.

## Overview

The MoosicBox Async Cargo package provides:

- **Lint Tool**: Command-line tool for async function analysis
- **Attribute Detection**: Finds async functions missing `#[inject_yields]`
- **Workspace Support**: Analyzes entire Rust workspaces
- **CI Integration**: Suitable for continuous integration pipelines
- **Syntax Analysis**: Uses syn for accurate Rust code parsing

## Features

### Async Function Analysis

- **Function Detection**: Identifies async functions and methods
- **Attribute Checking**: Verifies presence of `#[inject_yields]` attribute
- **Impl Block Support**: Analyzes async methods in impl blocks
- **Module Support**: Handles module-level attribute inheritance

### Code Coverage

- **Workspace Scanning**: Recursively scans all Rust source files
- **File Filtering**: Processes only `.rs` files
- **Syntax Parsing**: Robust parsing with error handling
- **Path Resolution**: Handles complex file structures

### Reporting

- **Warning Messages**: Clear warnings for missing attributes
- **File Location**: Reports exact file and function names
- **Exit Codes**: Returns appropriate exit codes for CI/CD
- **Batch Processing**: Processes multiple files efficiently

## Installation

### As Binary

```bash
# Install from source
cargo install --path packages/async/cargo

# Or build locally
cd packages/async/cargo
cargo build --release
```

## Cargo Features

- **`fail-on-warnings`**: Enables strict compilation mode with warnings treated as errors

## Usage

### Command Line

```bash
# Check current workspace
switchy_async_cargo

# Check specific directory
switchy_async_cargo --root /path/to/project

# Use in CI/CD
switchy_async_cargo || exit 1
```

### Example Output

```
2 warnings:
warning: src/lib.rs: async fn `process_data` is missing #[inject_yields]
warning: src/handlers.rs: async method `handle_request` in impl is missing #[inject_yields]
```

### Integration with CI

```yaml
# GitHub Actions example
- name: Check async functions
  run: |
      cargo install --path packages/async/cargo
      switchy_async_cargo
```

## Attribute Rules

### Required Attributes

Functions and methods that need `#[inject_yields]`:

- **Async Functions**: All `async fn` declarations
- **Async Methods**: All `async fn` in impl blocks
- **Public APIs**: Especially important for public async functions

### Exemptions

Attributes that exempt from checking:

- **Function Level**: `#[inject_yields]` on the function
- **Impl Level**: `#[inject_yields]` on the impl block
- **Module Level**: `#[inject_yields]` on the module

### Example Code

```rust
// ✅ Correct: Has inject_yields attribute
#[inject_yields]
async fn good_function() {
    // Function body
}

// ❌ Warning: Missing inject_yields attribute
async fn bad_function() {
    // Function body
}

// ✅ Correct: Impl-level attribute covers all methods
#[inject_yields]
impl MyStruct {
    async fn method1(&self) {
        // Method body
    }

    async fn method2(&self) {
        // Method body
    }
}

// ✅ Correct: Module-level attribute covers all functions
#[inject_yields]
mod my_module {
    async fn function_in_module() {
        // Function body
    }
}
```

## Command Line Options

- **`--root <PATH>`**: Specify root directory to analyze (if not specified, uses `$CARGO_MANIFEST_DIR`)

## Error Handling

The tool handles various error conditions gracefully:

- **Parse Errors**: Continues processing other files if one file fails to parse
- **File Access**: Handles missing or inaccessible files
- **Syntax Errors**: Provides empty AST for unparseable files

## Exit Codes

- **0**: Success, no warnings found
- **1**: Warnings found, missing `#[inject_yields]` attributes detected

## Dependencies

- **Clap**: Command-line argument parsing
- **Syn**: Rust syntax parsing and AST traversal
- **WalkDir**: Recursive directory traversal
- **Log**: Logging facade
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
