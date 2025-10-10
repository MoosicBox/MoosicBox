# Specification Template

## Purpose

This directory contains template files for creating new MoosicBox specifications. These templates ensure consistency across all specs and enforce critical patterns that prevent common implementation issues.

**When to use these templates:**

1. Creating a new feature specification
2. Adding a new phase to an existing spec
3. Ensuring an existing spec follows proper patterns
4. Teaching LLMs how to structure MoosicBox specs

## Template Files

### PREAMBLE.md

**Purpose:** Short, high-level overview of the feature

- Problem statement (2-3 sentences)
- Solution approach
- Implementation strategy
- Prerequisites and context

**Key sections:**

- Context (checkbox tracking, proof requirements, no-compromises philosophy)

### architecture.md

**Purpose:** HIGH-LEVEL technical design and component structure

- System overview with diagrams
- Design goals (primary and secondary)
- Component architecture
- Testing strategy
- Integration approach

**Not included:** Implementation details, step-by-step instructions (those go in plan.md)

### plan.md

**Purpose:** Detailed execution plan with phase-by-phase breakdown

- Executive summary with status tracking
- Design decisions (resolved)
- Phases with tasks and sub-tasks
- **Verification checklists after EVERY step**
- Success criteria
- Risk mitigation

## Critical Verification Checklist Commands

Every verification checklist in plan.md **MUST** include these commands. They are not optional.

### 1. `cargo fmt` (NOT `cargo fmt --check`)

**What it does:** Formats all Rust code in the workspace according to rustfmt rules

**Why it's critical:**

- Ensures consistent code style across the entire codebase
- Prevents formatting-related merge conflicts
- Must be run in root directory to format entire workspace
- **NEVER use `cargo fmt --check`** - we format, not just check

**Usage in verification:**

```
- [ ] Run `cargo fmt` (format code)
```

### 2. `cargo clippy --all-targets -- -D warnings`

**What it does:** Runs Clippy linter and treats all warnings as errors

**Why it's critical:**

- Catches common Rust mistakes and anti-patterns
- Enforces MoosicBox code quality standards
- The `-D warnings` flag ensures zero warnings allowed
- Can be scoped to specific packages with `-p [package-name]`

**Usage patterns:**

```
- [ ] Run `cargo clippy --all-targets -p [package-name] -- -D warnings` (zero warnings)
- [ ] Run `cargo clippy --all-targets -- -D warnings` (workspace-wide)
```

**Optional flags:**

- `-p [package-name]`: Scope to specific package
- `--no-default-features`: Check without default features
- `--features feature-name`: Check with specific features

### 3. `cargo machete`

**What it does:** Detects unused dependencies in Cargo.toml files

**Why it's critical:**

- Prevents dependency bloat
- Catches dependencies that were used but are no longer needed
- Ensures clean dependency trees
- Improves compile times by removing unused deps

**Usage in verification:**

```
- [ ] Run `cargo machete` (no unused dependencies)
```

**When to run:**

- After adding any dependency
- After refactoring code
- At the end of every phase
- Before marking phase as complete

## Additional Standard Verification Commands

### Build Verification

```
- [ ] Run `cargo build -p [package-name]` (compiles with default features)
- [ ] Run `cargo build -p [package-name] --no-default-features` (compiles without features)
```

### Test Verification

```
- [ ] Run `cargo test -p [package-name]` (all tests pass)
- [ ] Run `cargo test -p [package-name] --no-default-features` (tests pass without features)
```

### Dependency Verification

```
- [ ] Run `cargo tree -p [package-name]` (verify dependency tree)
- [ ] Run `cargo tree -p [package-name] --no-default-features` (zero dependencies initially)
```

### Workspace Verification

```
- [ ] Run `cargo metadata | grep [crate_name]` (package appears in workspace)
```

## MoosicBox Code Conventions

These conventions **MUST** be followed in all specs and implementations:

### 1. Collections: Always BTreeMap/BTreeSet

**NEVER use HashMap or HashSet**

```rust
// ✅ CORRECT
use std::collections::BTreeMap;
let map: BTreeMap<String, Value> = BTreeMap::new();

// ❌ WRONG
use std::collections::HashMap;
let map: HashMap<String, Value> = HashMap::new();
```

**Why:** Deterministic ordering for reproducible behavior and testing

### 2. Workspace Dependencies

**Always use `{ workspace = true }`**

```toml
# ✅ CORRECT
[dependencies]
serde = { workspace = true }
tokio = { workspace = true, features = ["rt"] }

# ❌ WRONG
[dependencies]
serde = "1.0"
tokio = { version = "1.0", features = ["rt"] }
```

ALWAYS use `{ workspace = true }` syntax, even if you can do the shorthand `blah.workspace = true` syntax.

**Why:** Centralized version management, consistent dependency versions

### 3. Package Naming

**Always use underscore naming**

```toml
# ✅ CORRECT
name = "moosicbox_audio_decoder"
name = "switchy_p2p"

# ❌ WRONG
name = "moosicbox-audio-decoder"
name = "switchy-p2p"
```

**Why:** Rust crate naming convention

### 4. Clippy Configuration

**Every lib.rs must include:**

```rust
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
```

**Why:** Enforces maximum code quality standards

### 5. Features

**Every package must include `fail-on-warnings` feature:**

```toml
[features]
default = []

fail-on-warnings = []
```

**Why:** CI/CD can enforce zero-warning policy

When enabled, it should propagate to all moosicbox dependencies that have the same feature.

## Proof Tracking Pattern

After completing a checkbox, add proof as indented details:

```markdown
- [x] Create `packages/p2p/` directory
      Created at packages/p2p/ with src/ subdirectory
- [x] Add `pub mod simulator;` to `lib.rs`
      Added to lib.rs at line 5, feature-gated with #[cfg(feature = "simulator")]
```

**Why proof is required:**

- Provides audit trail
- Helps reviewers verify work
- Documents exact file locations
- Makes specs self-documenting

## Phase Structure Best Practices

### Phase Goals

Each phase should have:

1. **Clear objective:** What this phase accomplishes
2. **Status indicator:** 🔴/🟡/✅ for quick visibility
3. **Task breakdown:** Numbered sections (1.1, 1.2, etc.)
4. **Verification checklist:** After EVERY task section

### Task Priority Indicators

- 🔴 **CRITICAL**: Blocks core functionality, must be done
- 🟡 **IMPORTANT**: Affects UX or API design, should be done
- 🟢 **MINOR**: Nice-to-have, can be deferred

### Self-Contained Phases

Each phase should:

- Compile independently without forward dependencies
- Have working tests
- Pass all verification checks
- Be demonstrable/testable

## Common Pitfalls to Avoid

### 1. ❌ Using `cargo fmt --check` instead of `cargo fmt`

**Correct:** `cargo fmt` (formats code)
**Wrong:** `cargo fmt --check` (only checks, doesn't format)

### 2. ❌ Forgetting `-D warnings` flag on clippy

**Correct:** `cargo clippy -- -D warnings`
**Wrong:** `cargo clippy` (allows warnings)

### 3. ❌ Using HashMap/HashSet

**Correct:** `BTreeMap`, `BTreeSet`
**Wrong:** `HashMap`, `HashSet`

### 4. ❌ Hardcoding dependency versions

**Correct:** `{ workspace = true }`
**Wrong:** `"1.0.5"`

### 5. ❌ Skipping `cargo machete`

**Always run** after adding/removing dependencies

### 6. ❌ Not providing proof after checking boxes

**Always add** indented details with file locations

## LLM Usage Guidelines

When using these templates with LLMs:

1. **Reference all three files:** PREAMBLE.md, architecture.md, and plan.md
2. **Emphasize verification checklists:** These are non-negotiable
3. **Explain the "why":** Help LLM understand reasoning behind patterns
4. **Use examples:** Point to spec/p2p/ and spec/generic-pipelines/ as reference implementations
5. **Enforce proof tracking:** Remind LLM to add proof under completed checkboxes
6. **Phase independence:** Each phase should be self-contained and functional

## Example Workflow

1. Copy template files to new spec directory: `spec/[feature-name]/`
2. Fill in placeholders in PREAMBLE.md with feature overview
3. Design high-level architecture in architecture.md
4. Break down implementation into phases in plan.md
5. For each task, include verification checklist with required commands
6. Execute phase by phase, checking boxes and adding proof
7. Ensure all verification commands pass before moving to next phase

## Questions?

See existing specs for reference:

- `spec/p2p/` - Complete P2P integration spec (good example)
- `spec/generic-pipelines/` - Generic workflow tool spec (comprehensive)

For MoosicBox-specific patterns, see:

- `.cursor/rules/` - Detailed coding conventions
- `AGENTS.md` - Development environment setup
- `DEVELOPMENT.md` - General development guidelines
