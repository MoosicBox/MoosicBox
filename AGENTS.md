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
- **Dependencies**: Use `workspace = true`, never path dependencies
- **Clippy**: Required in every package:
    ```rust
    #![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
    #![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
    #![allow(clippy::multiple_crate_versions)]
    ```
- **Rustdoc Error docs**: Use asterisks (\*) for bullet points, document all error conditions
- **Must use**: Add `#[must_use]` to constructors and getters (including those that return Option/Result types). Note: While the Option and Result types themselves are marked `#[must_use]`, this does NOT automatically apply to functions returning those types - you must explicitly add `#[must_use]` to the function. Clippy's `must_use_candidate` lint will suggest where to add it.

### Package Organization

- **Naming**: All packages use underscore naming (`moosicbox_audio_decoder`)
- **Features**: Always include `fail-on-warnings = []` feature
- **Serde**: Use `SCREAMING_SNAKE_CASE` for rename attributes

### Documentation

- Document all public APIs with comprehensive error information
- Include examples for complex functions
- **Version numbers**: Always specify full version numbers including patch version (e.g., `0.1.4` not `0.1`) in README installation examples
