---
# Template: Unit Test Coverage Enhancement
# Default variables

project_name: '${repository_name}'
repository: '${repository}'
package_path: '.'
package_name: '${derive_package_name(package_path)}'
target_path: 'src/**/*.rs'
branch_name: 'test/coverage-${package_name}-${run_id}'
custom_guidelines: ''
commit_message: 'test(${package_name}): add unit tests to increase coverage'
---

You are helping increase unit test coverage for ${project_name}.

IMPORTANT: Follow the repository's AGENTS.md for guidance on code standards and test conventions.

Context:

- REPO: ${repository}
- PACKAGE: ${package_name}
- TARGET: ${target_path}
- BRANCH: ${branch_name}

## Task

Add meaningful unit tests to ${package_name} to increase test coverage for untested or undertested code.

## CRITICAL: Test Selection Criteria

You must ONLY add tests that meet ALL of the following criteria:

1. **Clear Scope**: You must VERY CLEARLY understand the exact scope and behavior of the code being tested
2. **No Duplication**: You must be ABSOLUTELY SURE there are no other tests that test the same thing
3. **Meaningful Value**: The test must provide USEFUL coverage, not just test trivial or obvious behavior
4. **Real Gaps**: The test must fill an actual gap in test coverage, not redundantly test well-covered code

**DO NOT** add tests for:

- Simple getters/setters with no logic
- Trivial constructors that just assign values
- Code that is already well-tested through integration tests
- Obvious behavior that doesn't need verification
- Simple forwarding functions with no logic

**DO** add tests for:

- Complex business logic
- Edge cases and error conditions
- State transitions and validation logic
- Data transformations and calculations
- Concurrent operations and race conditions
- Error handling paths

## MoosicBox Testing Conventions

### Test Attribute Usage

**Async Tests:**

- Use `#[test_log::test(switchy_async::test)]` for async tests (NEVER use `tokio::test`)
- Use `#[test_log::test(switchy_async::test(no_simulator))]` for tests that must NOT run in simulator mode
- Use `#[test_log::test(switchy_async::test(real_time))]` for tests requiring real (not simulated) time
- Use `#[switchy_async::test]` when test_log is not needed

**Synchronous Tests:**

- Use `#[test]` for basic synchronous tests
- Use `#[test_log::test]` for synchronous tests that need logging

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

// Simple synchronous test
#[test]
fn test_sync_function() {
    // Test code
}

// Synchronous test with logging
#[test_log::test]
fn test_sync_with_logging() {
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

## Dependency Management

**CRITICAL**: Follow these strict dependency management rules when adding tests.

### Adding Dev Dependencies

1. **Only add when necessary**: Only add dependencies to `[dev-dependencies]` if they are:
    - Truly needed for the new tests
    - NOT already available in the package's existing dependencies or dev-dependencies
    - NOT available through existing workspace dependencies
2. **Always use workspace dependencies**: Use `workspace = true`, NEVER inline versions in package Cargo.toml

### Adding New Workspace Dependencies

If you need a brand new dependency that doesn't exist in the workspace:

1. **Add to workspace `Cargo.toml`** in the `[workspace.dependencies]` section with:
    - `default-features = false` (ALWAYS, no exceptions)
    - Full explicit version number including patch (e.g., `"0.4.28"` NOT `"0.4"`)
    - Only the specific features needed
2. **Verify latest stable version**:
    - Check crates.io for the LATEST stable version
    - Use the full semantic version (major.minor.patch)
3. **Package Cargo.toml**: Add to `[dev-dependencies]` with:
    - `workspace = true`
    - Only opt-in to specific features needed: `features = ["feature1", "feature2"]`

**Example workflow for adding a new test dependency:**

```toml
# In workspace Cargo.toml [workspace.dependencies]:
mockall = { version = "0.13.2", default-features = false }

# In package Cargo.toml [dev-dependencies]:
mockall = { workspace = true, features = ["mock-trait-default"] }
```

**Common test dependencies already in workspace:**

- `pretty_assertions = "1.4.1"`
- `test-log = "0.2.18"`
- `switchy_async` (with features like `["macros", "sync", "time", "tokio"]`)

Check the workspace `Cargo.toml` before adding any new dependencies!

## Verification (MANDATORY)

Before creating ANY commit, you MUST run:

1. Run `cargo fmt`
2. Run `cargo test -p ${package_name}` to verify tests pass
3. If package uses switchy packages, run `cargo test -p ${package_name} -p simvar`
4. Run `cargo clippy --all-targets -- -D warnings`
5. Run `~/.cargo/bin/cargo-machete --with-metadata` from workspace root
6. Run `npx prettier --write "**/*.{md,yaml,yml}"` from workspace root
7. Run `~/.cargo/bin/taplo format` from workspace root

If ANY check fails, fix the issues before committing.
NEVER commit code that doesn't pass all checks.

## 📝 Commit Message Instructions

If you add tests, you MUST provide a commit message description.

At the END of your response, include a section formatted EXACTLY as follows:

```
COMMIT_MESSAGE_START
- Brief description of tests added (1-2 sentences per major area)
- Focus on what functionality is now tested and why it was important
COMMIT_MESSAGE_END
```

Example:

```
COMMIT_MESSAGE_START
- Added tests for connection pool error handling to verify proper cleanup on connection failures
- Added edge case tests for empty input validation in parse_config function
- Added concurrent access tests for cache operations to verify thread safety
COMMIT_MESSAGE_END
```

Requirements:

- Keep each bullet point concise (1-2 sentences max)
- Focus on WHAT is tested and WHY it's important (what gap in coverage it fills)
- Use bullet points with dashes (-)
- Do not include code snippets or line numbers
- If no tests were added (because none met the criteria), output "No tests added - existing coverage is adequate or no clear test opportunities identified"
- DO NOT push

## Response Guidelines

When responding to users:

- NEVER reference files in /tmp or other temporary directories - users cannot access these
- Always include plans, summaries, and important information directly in your comment response
- If you create a plan or analysis, paste the full content in your response, not just a file path
- Remember: you run on an ephemeral server - any files you create are only accessible during your execution

${custom_guidelines}
