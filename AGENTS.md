# MoosicBox Agent Guidelines

## Build/Test Commands

- **Rust build**: `cargo build`
- **Rust test**: `cargo test` (all packages), `cargo test -p <package>` (single package)
- **Rust lint**: `cargo clippy --all-targets`
- **Rust lint enforce no warnings**: `cargo clippy --all-targets -- -D warnings`
- **Format**: `cargo fmt` (Rust) for ALL packages in the workspace

## Code Style Guidelines

### Rust Patterns

- **Collections**: Always use `BTreeMap`/`BTreeSet`, never `HashMap`/`HashSet`
- **Dependencies**: Use `workspace = true`, never path dependencies or inline versions
- **New Dependencies**: When adding a new dependency:
    - Add to workspace `Cargo.toml` with `default-features = false`
    - Specify full version including patch (e.g., `"0.4.28"` not `"0.4"`)
    - Verify you're using the LATEST stable version from crates.io
    - In package `Cargo.toml`, use `workspace = true` and opt-in to specific features only
- **Clippy**: Required in every package:
    ```rust
    #![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
    #![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
    #![allow(clippy::multiple_crate_versions)]
    ```
- **Rustdoc Error docs**: Use asterisks (\*) for bullet points, document all error conditions
- **Must use**: Add `#[must_use]` to constructors and getters that return types OTHER THAN Result or Option. **CRITICAL**: Do NOT add `#[must_use]` to functions returning Result or Option types - these types are already marked `#[must_use]` and adding the attribute to the function is redundant and will trigger clippy warnings (e.g., "this function has a `#[must_use]` attribute with no message, but returns a type already marked as `#[must_use]`"). Only add `#[must_use]` to functions that return other types where ignoring the return value would be a mistake.

### Package Organization

- **Naming**: All packages use underscore naming (`moosicbox_audio_decoder`)
- **Features**: Always include `fail-on-warnings = []` feature
- **Serde**: Use `SCREAMING_SNAKE_CASE` for rename attributes
- **`_models` Pattern**: When a package defines types that other packages need
  to depend on without pulling in the full package's functionality:
    - Extract shared types into a sibling `models/` subdirectory as a separate
      crate (e.g., `moosicbox_session` has `moosicbox_session_models`)
    - `_models` crates contain ONLY: structs, enums, type aliases, `From`/`Into`
      impls, serialization derives, and simple utility/parsing functions on those
      types
    - `_models` crates must NOT contain: business logic, database queries, HTTP
      handlers, service orchestration, or heavy dependencies
    - This prevents circular dependencies: two crates that need each other's types
      can both depend on a shared `_models` crate instead of depending on each other
    - `_models` crates should be leaves in the dependency graph (they may depend on
      other `_models` crates but never on their parent implementation crate)
    - Never create generic "shared" or "common" crates -- types belong in the
      domain-specific `_models` crate for the package that owns them

### Documentation

- Document all public APIs with comprehensive error information
- Include examples for complex functions
- **Version numbers**: Always specify full version numbers including patch version (e.g., `0.1.4` not `0.1`) in README installation examples
