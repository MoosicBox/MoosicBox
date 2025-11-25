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

### Test Organization

- Group tests in `#[cfg(test)]` modules
- For packages with multiple backend implementations, create separate test modules for each
- Use descriptive test names that explain what is being tested
- Include setup helpers when multiple tests share setup logic
