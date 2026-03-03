---
# Partial: Switchy Package Usage
# Expected variables: none required
---

### Use Switchy Packages for Simulator Compatibility

**CRITICAL: Code MUST use `switchy_*` packages instead of direct `std`/`tokio` equivalents for operations that need to work in simulator mode.**

This ensures deterministic behavior and compatibility with both real and simulated environments.

**Common Replacements:**

| Instead of                     | Use                           |
| ------------------------------ | ----------------------------- |
| `std::fs::*`                   | `switchy_fs::sync::*`         |
| `tokio::fs::*`                 | `switchy_fs::unsync::*`       |
| `std::time::SystemTime::now()` | `switchy_time::now()`         |
| `std::time::Instant::now()`    | `switchy_time::instant_now()` |
| `rand::thread_rng()`           | `switchy_random::Rng::new()`  |
| `uuid::Uuid::new_v4()`         | `switchy_uuid::new_v4()`      |
| `tokio::net::TcpStream`        | `switchy_tcp::TcpStream`      |
| `tokio::net::TcpListener`      | `switchy_tcp::TcpListener`    |
| `tokio::spawn`                 | `switchy_async::spawn`        |
| `tokio::time::sleep`           | `switchy_async::time::sleep`  |

**Note**: `std::time::Duration` is fine - it's a data type, not a source of non-determinism.

**When to opt out of simulator mode (tests only):**

Use these test attributes when tests MUST use real system resources:

```rust
// Test requires real filesystem (e.g., testing actual disk I/O)
#[test_log::test(switchy_async::test(real_fs))]
async fn test_disk_operations() { ... }

// Test requires real time (e.g., testing actual timeouts)
#[test_log::test(switchy_async::test(real_time))]
async fn test_timeout_behavior() { ... }

// Test cannot run in simulator at all
#[test_log::test(switchy_async::test(no_simulator))]
async fn test_external_service() { ... }
```

**Temporary real filesystem access within simulator:**

```rust
use switchy_fs::with_real_fs;

fn cleanup_test_artifacts() {
    with_real_fs(|| {
        std::fs::remove_dir_all("/tmp/test_artifacts").ok();
    });
}
```

**Dev Dependencies for Async Tests:**

When writing async tests using `#[test_log::test(switchy_async::test)]`, you need `tokio` as a dev dependency for tests to work in non-simulator mode. However, `cargo-machete` will flag this as unused since it's only used through macro expansion.

Add to your `Cargo.toml`:

```toml
[dev-dependencies]
switchy_async = { workspace = true, features = ["macros", "tokio"] }
test-log      = { workspace = true }
tokio         = { workspace = true }

[package.metadata.cargo-machete]
ignored = ["tokio"]
```

The `tokio` dependency is required for the `switchy_async::test` macro to expand to a working test when not in simulator mode.
