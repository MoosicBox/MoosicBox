# MoosicBox Async Macros

Procedural macros for async function transformation and yield injection.

## Overview

The MoosicBox Async Macros package provides:

- **Yield Injection**: Automatic yield point insertion for simulation testing
- **Async Transformation**: Transform async functions for deterministic execution
- **Proc Macros**: `#[inject_yields]` and `inject_yields_mod!` macros
- **Test Macros**: Simulator-aware test attribute macros (`#[test]`, `#[unsync_test]`, etc.)
- **Feature-Gated**: Simulator features are optional (included in default features for IDE support only)
- **AST Manipulation**: Sophisticated syntax tree transformation

## Features

### Yield Injection

- **Automatic Yields**: Insert yield points after every `.await`
- **Deterministic Testing**: Enable predictable async execution in tests
- **Simulation Support**: Required for simulation-based testing
- **Non-Intrusive**: No overhead when simulator feature is disabled

### Macro Types

#### Yield Injection Macros

- **`#[inject_yields]`**: Attribute macro for individual functions and impl blocks
- **`inject_yields_mod!`**: Procedural macro for entire modules
- **Conditional**: Only active with `simulator` feature flag

#### Test Macros

- **`#[test]`**: External test macro for `switchy_async` tests with optional `real_time`, `real_fs`, `no_simulator` parameters (simulator feature only)
- **`#[unsync_test]`**: Test macro for `switchy::unsync` with optional `real_time`, `real_fs`, `no_simulator` parameters (simulator feature only)
- **`#[tokio_test_wrapper]`**: Tokio-compatible test wrapper (always available)

#### Main Macros

- **`#[main]`**: Main function macro for `switchy_async` runtime (simulator feature only)

### AST Transformation

- **Await Wrapping**: Wraps `.await` expressions with yield points
- **Function Support**: Handles async functions and methods
- **Module Support**: Process entire modules recursively
- **Impl Block Support**: Transform all async methods in impl blocks

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
# With default features (includes simulator for IDE support)
switchy_async_macros = { path = "../async/macros" }

# Production usage: disable default features to exclude simulator
switchy_async_macros = {
    path = "../async/macros",
    default-features = false
}

# Explicitly enable simulator for testing
switchy_async_macros = {
    path = "../async/macros",
    default-features = false,
    features = ["simulator"]
}
```

## Usage

### Function-Level Macro

```rust
use switchy_async_macros::inject_yields;

// Original function
#[inject_yields]
async fn my_async_function() {
    let result1 = some_async_operation().await;
    let result2 = another_async_operation().await;
    result1 + result2
}

// With simulator feature enabled, transforms to:
// async fn my_async_function() {
//     let result1 = {
//         let __yield_res = some_async_operation().await;
//         switchy::unsync::task::yield_now().await;
//         __yield_res
//     };
//     let result2 = {
//         let __yield_res = another_async_operation().await;
//         switchy::unsync::task::yield_now().await;
//         __yield_res
//     };
//     result1 + result2
// }
```

### Impl Block Macro

```rust
use switchy_async_macros::inject_yields;

#[inject_yields]
impl MyStruct {
    async fn method1(&self) -> i32 {
        self.async_operation().await
    }

    async fn method2(&self) -> String {
        self.another_operation().await
    }

    // Non-async methods are unaffected
    fn sync_method(&self) -> bool {
        true
    }
}
```

### Module-Level Macro

```rust
use switchy_async_macros::inject_yields_mod;

// Transform entire module (reads from src/my_module.rs)
inject_yields_mod! {
    mod my_module;
}
```

The macro reads the module source from `src/{module_name}.rs`, transforms all async functions within, and emits the transformed module.

**Note:** This approach has limited LSP integration. Since the module is transformed at compile time, IDE features like "go to definition" may not work correctly for items defined within `inject_yields_mod!` blocks.

### Feature-Gated Behavior

```rust
// Without simulator feature - no transformation
#[cfg(not(feature = "simulator"))]
#[inject_yields]
async fn my_function() {
    // Executes normally without yield injection
    some_operation().await;
}

// With simulator feature - yield injection enabled
#[cfg(feature = "simulator")]
#[inject_yields]
async fn my_function() {
    // Automatically transformed with yield points
    some_operation().await; // -> wrapped with yield_now()
}
```

## Transformation Details

### Await Expression Transformation

**Before:**

```rust
let result = async_call().await;
```

**After (with simulator feature):**

```rust
let result = {
    let __yield_res = async_call().await;
    switchy::unsync::task::yield_now().await;
    __yield_res
};
```

### Supported Constructs

- **Async Functions**: `async fn` declarations
- **Async Methods**: Methods in impl blocks
- **Nested Modules**: Recursive module processing
- **Complex Expressions**: Handles complex await expressions

### Unsupported/Unchanged

- **Sync Functions**: Non-async functions remain unchanged
- **Await in Macros**: Await expressions inside macro calls
- **Non-Simulator**: No transformation without simulator feature

## Use Cases

### Simulation Testing

```rust
#[cfg(test)]
mod tests {
    use switchy_async_macros::inject_yields;

    #[inject_yields]
    async fn test_function() {
        // Deterministic execution for testing
        let result = async_operation().await;
        assert_eq!(result, expected_value);
    }
}
```

### Library Development

```rust
// Library functions that need deterministic testing
#[inject_yields]
pub async fn library_function() -> Result<Data, Error> {
    let data = fetch_data().await?;
    let processed = process_data(data).await?;
    Ok(processed)
}
```

### Async Main Functions

```rust
use switchy_async_macros::main;

#[main]
async fn main() {
    // Async main using switchy_async runtime
    println!("Hello from async main!");
}

// With Result return type
#[main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}
```

## Feature Flags

- **`simulator`**: Enable yield injection transformation and test macros (included in default features for IDE support; disable for production)
- **`fail-on-warnings`**: Deny all warnings during compilation

## Dependencies

- **proc-macro2**: Procedural macro support library
- **syn**: Rust syntax parsing and AST manipulation (with `extra-traits`, `full`, `parsing`, `visit-mut` features)
- **quote**: Code generation utilities
- **log**: Logging facade

## Integration

This package is designed for:

- **Testing**: Deterministic async testing with simulation
- **Development**: Consistent async behavior during development
- **Library Development**: Libraries that need predictable async execution
- **Debugging**: Easier debugging of async code with controlled execution

## Performance

- **Zero Cost**: No runtime overhead when simulator feature is disabled
- **Compile Time**: Transformation happens at compile time
- **Minimal Impact**: Only affects functions with the attribute
- **Conditional**: Completely conditional based on feature flags
