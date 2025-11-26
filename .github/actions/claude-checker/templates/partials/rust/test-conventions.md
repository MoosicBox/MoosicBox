---
# Partial: MoosicBox Rust Test Conventions
# Expected variables (with defaults)
package_name: ''
---

## MoosicBox Testing Conventions

### Test Attribute Usage

**CRITICAL: ALL tests (both synchronous and asynchronous) MUST use `#[test_log::test]` or its variants.**

**Async Tests:**

- Use `#[test_log::test(switchy_async::test)]` for async tests (NEVER use `tokio::test`)
- Use `#[test_log::test(switchy_async::test(no_simulator))]` for tests that must NOT run in simulator mode
- Use `#[test_log::test(switchy_async::test(real_time))]` for tests requiring real (not simulated) time

**Synchronous Tests:**

- Use `#[test_log::test]` for ALL synchronous tests (NEVER use raw `#[test]`)
- This ensures consistent logging and test infrastructure across all tests

**Examples:**

```rust
// Async test with logging and simulator support
#[test_log::test(switchy_async::test)]
async fn test_async_operation() {
    // Test code
}

// Async test that cannot run in simulator mode
#[test_log::test(switchy_async::test(no_simulator))]
async fn test_real_database_operation() {
    // Test code that needs real database
}

// Async test requiring real time (not simulated)
#[test_log::test(switchy_async::test(real_time))]
async fn test_timeout_behavior() {
    // Test code that relies on actual time passage
}

// Synchronous test with test_log (REQUIRED - do NOT use raw #[test])
#[test_log::test]
fn test_sync_function() {
    // Test code
}
```

### Simulator Compatibility

**CRITICAL**: If the package has a `simulator` feature OR uses any switchy packages:

1. Tests must be compatible with simulator mode by default
2. Use `#[test_log::test(switchy_async::test(no_simulator))]` ONLY when absolutely necessary
3. To verify simulator compatibility for packages without a direct `simulator` feature:
    ```bash
    cargo test -p ${package_name} -p simvar
    ```
4. If tests fail with simvar, investigate and fix simulator compatibility issues

${include('rust/switchy-packages')}

### Test Organization

- Group tests in `#[cfg(test)]` modules
- For packages with multiple backend implementations, create separate test modules for each
- Use descriptive test names that explain what is being tested
- Include setup helpers when multiple tests share setup logic

### Global State Isolation

**CRITICAL: Tests that access shared global state MUST be serialized to prevent race conditions and test flakiness.**

**When to use `#[serial]`:**

Tests MUST use `#[serial]` from `serial_test` when they:

- Read or modify `static` variables with interior mutability (`LazyLock<Mutex<...>>`, `LazyLock<RwLock<...>>`, `OnceLock`, etc.)
- Modify environment variables or process-wide state
- Access shared filesystem paths that aren't unique per test
- Interact with singleton resources (global search indices, connection pools, etc.)

**Usage:**

```rust
#[cfg(test)]
mod tests {
    use serial_test::serial;

    // Synchronous test with global state
    #[test_log::test]
    #[serial]
    fn test_modifies_global_config() {
        // Test that modifies a global LazyLock<Mutex<...>>
    }

    // Async test with global state
    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_global_search_index() {
        // Test that modifies a global search index
    }

    // Use named groups when only specific tests share state
    #[test_log::test]
    #[serial(config_state)]
    fn test_config_setting_a() {
        // Only serialized with other tests in "config_state" group
    }
}
```

**State Cleanup Patterns:**

Always ensure global state is properly reset after tests to avoid polluting other tests:

```rust
struct TestSetup;

impl TestSetup {
    pub fn new() -> Self {
        // Initialize test state
        Self
    }
}

impl Drop for TestSetup {
    fn drop(&mut self) {
        // Clean up / reset global state
    }
}

fn before_each() {
    // Reset state to known good configuration
}

#[test_log::test]
#[serial]
fn test_with_cleanup() {
    let _setup = TestSetup::new();
    before_each();
    // Test runs, then TestSetup::drop() cleans up
}
```

**Prefer Test Isolation Over Serialization:**

When possible, design code to avoid global state:

- Use dependency injection instead of global singletons
- Create unique test directories using `moosicbox_config::get_tests_dir_path()`
- Pass configuration as parameters rather than reading from global state
