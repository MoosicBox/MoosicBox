---
# Partial: Rust Dependency Management Guidelines
# Expected variables: none required
---

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
