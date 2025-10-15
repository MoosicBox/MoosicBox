# MoosicBox Assert

A conditional assertion library providing enhanced assertion macros with colorized output and stack traces, controlled by environment variables.

## Features

- **Conditional Assertions**: Enable/disable assertions via environment variable
- **Colorized Output**: Red background with white text for assertion failures
- **Stack Traces**: Automatic backtrace capture on assertion failures
- **Multiple Assert Types**: Different assertion behaviors (exit, error, panic, unimplemented)
- **Flexible Error Handling**: Convert assertions to errors, warnings, or panics
- **Environment Control**: Runtime control over assertion behavior

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_assert = "0.1.4"
```

## Usage

### Basic Assertions

```rust
use moosicbox_assert::{assert, die};

fn main() {
    // Set environment variable to enable assertions
    std::env::set_var("ENABLE_ASSERT", "1");

    let value = 42;

    // Basic assertion - exits process on failure
    assert!(value > 0);
    assert!(value == 42, "Expected 42, got {}", value);

    // Unconditional death (only when assertions enabled)
    if value < 0 {
        die!("Value cannot be negative: {}", value);
    }
}
```

### Assert with Error Return

```rust
use moosicbox_assert::assert_or_err;

#[derive(Debug)]
enum MyError {
    InvalidValue,
    OutOfRange,
}

fn validate_input(value: i32) -> Result<(), MyError> {
    // Convert assertion failure to error return
    assert_or_err!(value >= 0, MyError::InvalidValue);
    assert_or_err!(value <= 100, MyError::OutOfRange, "Value {} is out of range", value);

    Ok(())
}

fn main() {
    std::env::set_var("ENABLE_ASSERT", "1");

    match validate_input(-5) {
        Ok(()) => println!("Input is valid"),
        Err(e) => println!("Validation error: {:?}", e),
    }
}
```

### Assert with Logging

```rust
use moosicbox_assert::assert_or_error;

fn process_data(data: &[u8]) {
    // Log error instead of exiting when assertions disabled
    assert_or_error!(!data.is_empty(), "Cannot process empty data");
    assert_or_error!(data.len() < 1024, "Data too large: {} bytes", data.len());

    // Process data...
}

fn main() {
    env_logger::init();

    // With ENABLE_ASSERT=1: exits on failure
    // With ENABLE_ASSERT=0: logs error and continues
    std::env::set_var("ENABLE_ASSERT", "0");

    process_data(&[]);  // Will log error but not exit
}
```

### Assert with Panic

```rust
use moosicbox_assert::assert_or_panic;

fn critical_operation(input: Option<i32>) {
    // Panic with colorized output on failure
    let value = input.expect("Input required");
    assert_or_panic!(value > 0, "Value must be positive, got {}", value);

    println!("Processing value: {}", value);
}

fn main() {
    std::env::set_var("ENABLE_ASSERT", "1");

    critical_operation(Some(42));  // OK
    critical_operation(Some(-1));  // Panics with colored output
}
```

### Assert with Unimplemented

```rust
use moosicbox_assert::assert_or_unimplemented;

fn experimental_feature(enabled: bool) {
    // Mark unimplemented code paths
    assert_or_unimplemented!(enabled, "Feature not yet implemented");

    println!("Running experimental feature");
}

fn main() {
    std::env::set_var("ENABLE_ASSERT", "1");

    experimental_feature(true);   // OK
    experimental_feature(false);  // Calls unimplemented!() with colors
}
```

### Environment Control

```rust
use moosicbox_assert::assert;

fn debug_mode_example() {
    let data = vec![1, 2, 3];

    // Only checked when ENABLE_ASSERT=1
    assert!(!data.is_empty());
    assert!(data.len() == 3, "Expected 3 elements");
}

fn main() {
    // Disable assertions for production
    std::env::set_var("ENABLE_ASSERT", "0");
    debug_mode_example();  // Assertions are no-ops

    // Enable for debugging
    std::env::set_var("ENABLE_ASSERT", "1");
    debug_mode_example();  // Assertions are active
}
```

## Assertion Types

### `assert!(condition [, message])`

Exits the process with colored output when condition fails and assertions are enabled.

### `assert_or_err!(condition, error [, message])`

Returns the error when condition fails, or exits with assertion if `ENABLE_ASSERT=1`.

### `assert_or_error!(condition, message)`

Logs an error when condition fails, or exits with assertion if `ENABLE_ASSERT=1`.

### `assert_or_panic!(condition [, message])`

Panics with colored output when condition fails, or exits with assertion if `ENABLE_ASSERT=1`.

### `assert_or_unimplemented!(condition [, message])`

Calls `unimplemented!()` when condition fails, or exits with assertion if `ENABLE_ASSERT=1`.

### `die!([message])`

Unconditionally exits with colored output when assertions are enabled.

## Additional Macros

The library also provides additional utility macros for specialized use cases:

- `die_or_warn!(message)`: Exits with colored output if `ENABLE_ASSERT=1`, otherwise logs a warning
- `die_or_err!(error, message)`: Exits if `ENABLE_ASSERT=1`, otherwise returns the error
- `die_or_error!(message)`: Exits if `ENABLE_ASSERT=1`, otherwise logs an error
- `die_or_propagate!(result [, message])`: Exits if `ENABLE_ASSERT=1` and result is error, otherwise propagates error with `?`
- `die_or_panic!(message)`: Exits if `ENABLE_ASSERT=1`, otherwise panics
- `die_or_unimplemented!(message)`: Exits if `ENABLE_ASSERT=1`, otherwise calls `unimplemented!()`

## Environment Variables

- `ENABLE_ASSERT`: Set to "1" to enable assertions, any other value disables them

## Output Format

When assertions fail, the output includes:

- **Red background** with **white text** for visibility
- **Bold** and **underlined** formatting
- **Full stack trace** showing the failure location
- **Custom messages** with formatting support

## Dependencies

- `colored`: For colorized terminal output
- `moosicbox_env_utils`: For environment variable handling
- `log`: For logging warnings and errors
- Standard library backtrace support

## Use Cases

- **Development**: Enable detailed assertion checking
- **Testing**: Verify preconditions and postconditions
- **Production**: Disable assertions for performance
- **Debugging**: Get detailed failure information with stack traces
