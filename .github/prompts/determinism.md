---
# Template: Determinism Checker
# Scans workspace for non-deterministic patterns and replaces with switchy_* equivalents

project_name: '${repository_name}'
repository: '${repository}'
branch_name: 'refactor/determinism-${run_id}'
custom_guidelines: ''
commit_message: 'refactor: replace non-deterministic patterns with switchy equivalents'
---

You are helping ensure deterministic behavior in ${project*name} by replacing non-deterministic patterns with their switchy*\* equivalents.

IMPORTANT: Follow the repository's AGENTS.md for guidance on code standards.

Context:

- REPO: ${repository}
- BRANCH: ${branch_name}
- SCOPE: Entire workspace (all packages under `packages/`)

## Task

Find and replace non-deterministic patterns across the workspace with deterministic alternatives using the switchy\_\* packages. This enables reproducible behavior in simulator mode for testing.

## Non-Deterministic Patterns to Fix

### 1. Collection Types (CRITICAL - per AGENTS.md)

| Find                            | Replace With                      |
| ------------------------------- | --------------------------------- |
| `HashMap`                       | `BTreeMap`                        |
| `HashSet`                       | `BTreeSet`                        |
| `use std::collections::HashMap` | `use std::collections::BTreeMap`  |
| `use std::collections::HashSet` | `use std::collections::BTreeSet`  |
| `indexmap::IndexMap`            | `BTreeMap` (unless order matters) |
| `indexmap::IndexSet`            | `BTreeSet` (unless order matters) |

### 2. Filesystem Operations

| Find                      | Replace With                       |
| ------------------------- | ---------------------------------- |
| `use std::fs`             | `use switchy_fs::sync`             |
| `use tokio::fs`           | `use switchy_fs::unsync`           |
| `std::fs::read_to_string` | `switchy_fs::sync::read_to_string` |
| `std::fs::write`          | `switchy_fs::sync::write`          |
| `std::fs::create_dir_all` | `switchy_fs::sync::create_dir_all` |
| `std::fs::remove_dir_all` | `switchy_fs::sync::remove_dir_all` |
| `std::fs::File`           | `switchy_fs::sync::File`           |
| `tokio::fs::*`            | `switchy_fs::unsync::*`            |

### 3. Time Operations

| Find                           | Replace With                  |
| ------------------------------ | ----------------------------- |
| `std::time::SystemTime::now()` | `switchy_time::now()`         |
| `std::time::Instant::now()`    | `switchy_time::instant_now()` |
| `use std::time::SystemTime`    | `use switchy_time`            |
| `use std::time::Instant`       | `use switchy_time`            |

**Note**: `std::time::Duration` is fine - it's just a data type, not a source of non-determinism.

### 4. Random Number Generation

| Find                 | Replace With                          |
| -------------------- | ------------------------------------- |
| `use rand::`         | `use switchy_random`                  |
| `rand::thread_rng()` | `switchy_random::Rng::new()`          |
| `rand::Rng`          | `switchy_random::Rng`                 |
| `rand::random()`     | `switchy_random::Rng::new().random()` |

### 5. TCP Networking

| Find                      | Replace With               |
| ------------------------- | -------------------------- |
| `tokio::net::TcpStream`   | `switchy_tcp::TcpStream`   |
| `tokio::net::TcpListener` | `switchy_tcp::TcpListener` |
| `use tokio::net::`        | `use switchy_tcp`          |

### 6. Async Runtime

| Find                   | Replace With                   |
| ---------------------- | ------------------------------ |
| `tokio::spawn`         | `switchy_async::spawn`         |
| `tokio::time::sleep`   | `switchy_async::time::sleep`   |
| `tokio::time::timeout` | `switchy_async::time::timeout` |

## Switchy Package Reference

**Legacy locations (still valid, use these):**

- `switchy_fs` (`packages/fs/`) - Filesystem operations (sync and async)
- `switchy_time` (`packages/time/`) - Time operations (`now()`, `instant_now()`)
- `switchy_random` (`packages/random/`) - Random number generation
- `switchy_tcp` (`packages/tcp/`) - TCP networking
- `switchy_async` (`packages/async/`) - Async runtime primitives

**New packages should go under `packages/switchy/<domain>/`**

## What NOT to Change

1. **External dependency internals** - Don't modify code in external crates
2. **Build/tooling packages** - Skip `clippier`, `bloaty`, `gpipe` packages
3. **Tests requiring real behavior** - Don't change tests marked with `no_simulator` or `real_time`
4. **Already-correct code** - Don't change code already using switchy\_\* packages correctly
5. **Duration types** - `std::time::Duration` is fine (it's just a data type)
6. **The switchy\_\* packages' internal implementations** - Don't change the `standard.rs`/`tokio.rs` modules that wrap the real implementations (those need to use the real APIs)

## Extending Switchy Packages

If you find non-deterministic code with no switchy equivalent:

### Option A: Add to Existing Switchy Package

1. Identify the appropriate switchy package
2. Add the function/type to BOTH:
    - `src/simulator.rs` (or `src/simulator/*.rs`) - deterministic implementation
    - `src/standard.rs` or `src/tokio.rs` - real implementation wrapper
3. Ensure the public API signature is IDENTICAL between both
4. Update the `impl_*!` macro in `lib.rs` to export the new function
5. Add tests that work in both modes

### Option B: Create New Switchy Package

If the domain doesn't fit existing packages:

1. Create `packages/switchy/<domain>/` (e.g., `packages/switchy/http/`)
2. Include:
    - `src/lib.rs` with feature-gated module exports and `impl_*!` macros
    - `src/simulator.rs` for deterministic implementation
    - `src/standard.rs` or `src/tokio.rs` for real implementation
    - `Cargo.toml` with `simulator` feature flag and `fail-on-warnings` feature
    - `README.md` documenting usage
3. Package name should be `switchy_<domain>` (e.g., `switchy_http`)
4. Add to workspace `Cargo.toml` in `[workspace.dependencies]`

### API Parity Rules (CRITICAL)

Both feature paths MUST have identical public APIs:

```rust
// In lib.rs - the macro ensures both paths export the same symbols
#[cfg(feature = "simulator")]
impl_fs!(simulator);

#[cfg(all(not(feature = "simulator"), feature = "std"))]
impl_fs!(standard);
```

If you add `pub fn foo()` to simulator, you MUST add `pub fn foo()` with the same signature to standard/tokio.

## Dependency Management

When adding switchy\_\* dependencies to a package:

1. Add to package's `Cargo.toml` `[dependencies]`:
    ```toml
    switchy_fs = { workspace = true }
    ```
2. Enable only the features needed:
    ```toml
    switchy_fs = { workspace = true, features = ["sync"] }
    ```
3. If the package has a `simulator` feature, forward it:
    ```toml
    [features]
    simulator = ["switchy_fs/simulator"]
    ```

## Verification (MANDATORY)

Before creating ANY commit, you MUST run:

1. Run `cargo fmt`
2. Run `cargo test` to verify all tests pass
3. Run `cargo test -p simvar` to verify simulator compatibility
4. Run `cargo clippy --all-targets -- -D warnings`
5. Run `~/.cargo/bin/cargo-machete --with-metadata` from workspace root
6. Run `npx prettier --write "**/*.{md,yaml,yml}"` from workspace root
7. Run `~/.cargo/bin/taplo format` from workspace root

If ANY check fails, fix the issues before committing.
NEVER commit code that doesn't pass all checks.

## Commit Message Instructions

If you make changes, you MUST provide a commit message description.

At the END of your response, include a section formatted EXACTLY as follows:

```
COMMIT_MESSAGE_START
- Brief description of changes (1-2 sentences per major area)
- Focus on what non-deterministic patterns were replaced and why
COMMIT_MESSAGE_END
```

Example:

```
COMMIT_MESSAGE_START
- Replaced HashMap with BTreeMap in config and session modules for deterministic iteration order
- Switched from std::fs to switchy_fs in library package for filesystem operations
- Added missing `metadata()` function to switchy_fs for parity with std::fs
COMMIT_MESSAGE_END
```

Requirements:

- Keep each bullet point concise (1-2 sentences max)
- Focus on WHAT was changed and WHY (what non-determinism was eliminated)
- Use bullet points with dashes (-)
- Do not include code snippets or line numbers
- If no changes needed, output "No changes required - workspace already uses deterministic patterns"
- DO NOT push

## Response Guidelines

When responding to users:

- NEVER reference files in /tmp or other temporary directories - users cannot access these
- Always include plans, summaries, and important information directly in your comment response
- If you create a plan or analysis, paste the full content in your response, not just a file path
- Remember: you run on an ephemeral server - any files you create are only accessible during your execution

${custom_guidelines}
