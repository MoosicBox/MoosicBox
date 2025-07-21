# MoosicBox Agent Guidelines

## NixOS Environment

- **Always use shell.nix**: Run commands with `nix-shell --run "command"` on NixOS systems
- **Example**: `nix-shell --run "cargo build"` or `nix-shell --run "pnpm lint"`
- **Shell provides**: Rust toolchain, Node.js, audio libraries, databases, system dependencies

## Git Usage Restrictions

- **READ-ONLY git operations**: Only use `git status`, `git log`, `git diff`, `git show`, `git branch -v`
- **NO modifications**: Never use `git add`, `git commit`, `git push`, `git pull`, `git rebase`, `git reset`, `git merge`, `git stash`
- **NO history changes**: Never modify git history or working tree state
- **View only**: Agents should only inspect git state, never change it

## Build/Test Commands

- **Rust build**: `cargo build` (fastest), `cargo build --profile fast` (optimized for speed)
- **Rust test**: `cargo test` (all packages), `cargo test -p <package>` (single package)
- **Rust lint**: `cargo clippy --all-targets --all-features`
- **TypeScript lint**: `pnpm lint` (or `npm run lint`)
- **TypeScript typecheck**: `pnpm typecheck` (or `npm run typecheck`)
- **Format**: `cargo fmt` (Rust), `pnpm pretty:write` (TypeScript)
- **Validate all**: `pnpm validate` (runs typecheck, lint, format check)

## Code Style Guidelines

### Rust Patterns

- **Collections**: Always use `BTreeMap`/`BTreeSet`, never `HashMap`/`HashSet`
- **Dependencies**: Use `workspace = true`, never path dependencies
- **Clippy**: Required in every package: `#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]`
- **Error docs**: Use asterisks (\*) for bullet points, document all error conditions
- **Must use**: Add `#[must_use]` to constructors and getters

### Package Organization

- **Naming**: All packages use underscore naming (`moosicbox_audio_decoder`)
- **Features**: Always include `fail-on-warnings = []` feature
- **Serde**: Use `SCREAMING_SNAKE_CASE` for rename attributes

### Documentation

- Use asterisks (\*) not dashes (-) for bullet points
- Document all public APIs with comprehensive error information
- Include examples for complex functions

## Cursor Rules Integration

This codebase follows domain-driven design with 120+ packages organized by functionality. Key architectural patterns include deterministic collections, strong ID typing, and comprehensive error handling. See `.cursor/rules/` for detailed patterns.
