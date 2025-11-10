# Generic Pipelines (gpipe)

Universal CI/CD workflow orchestration tool. Currently provides type definitions and AST. Planned: execution and translation across multiple backends.

## Overview

Generic Pipelines defines a unified workflow format that will support:

- Local execution without containers (planned)
- Translation to GitHub Actions YAML (planned)
- Translation to GitLab CI YAML (planned)
- Extension to other CI/CD platforms (planned)

Currently, this package provides the complete AST (Abstract Syntax Tree) types for representing generic workflow definitions in Rust. The AST supports a workflow format designed to be written once and executed anywhere, with backend-specific functionality through conditional execution blocks.

## Features

### Implemented

- ✅ **Workflow AST** - Complete abstract syntax tree types for workflow definitions
- ✅ **Type Safety** - Fully typed Rust data structures with serde support

### Planned

- 🔮 **Local Execution** - Run workflows directly on your machine without Docker
- 🔮 **Multi-Backend Support** - Target GitHub Actions, GitLab CI, and more
- 🔮 **Backend Conditionals** - Execute steps only on specific platforms
- 🔮 **Custom Actions** - Define reusable inline actions or reference external ones
- 🔮 **Matrix Builds** - Run jobs with multiple configurations
- 🔮 **Job Dependencies** - Define complex workflows with job orchestration
- 🔮 **Step Outputs** - Pass data between steps and jobs

## Workflow Schema Specification

The AST types support the following workflow schema. This section documents the intended workflow format.

### Top-Level Structure

```yaml
version: 1.0 # Schema version (required)
name: workflow-name # Human-readable name (required)
triggers: { ... } # When to run the workflow (required)
actions: { ... } # Action definitions (required)
jobs: { ... } # Job definitions with steps (required)
```

| Field      | Type   | Required | Description                      |
| ---------- | ------ | -------- | -------------------------------- |
| `version`  | string | ✓        | Schema version (currently "1.0") |
| `name`     | string | ✓        | Workflow name                    |
| `triggers` | object | ✓        | When to run the workflow         |
| `actions`  | object | ✓        | Action definitions used in steps |
| `jobs`     | object | ✓        | Job definitions with steps       |

### Triggers

Trigger types defined in the AST with planned backend mappings:

| Generic        | GitHub Actions      | GitLab CI       | Description               |
| -------------- | ------------------- | --------------- | ------------------------- |
| `push`         | `push`              | `push`          | Git push events           |
| `pull_request` | `pull_request`      | `merge_request` | Pull/merge request events |
| `schedule`     | `schedule`          | `schedule`      | Cron-based scheduling     |
| `manual`       | `workflow_dispatch` | `web`           | Manual execution          |

Example:

```yaml
triggers:
    push:
        branches: [main, develop]
    pull_request:
        types: [opened, synchronize]
    schedule:
        cron: '0 0 * * *'
    manual:
```

### Actions

Three types of actions are defined in the schema:

#### 1. GitHub Actions

Reference existing GitHub Actions by repository:

```yaml
actions:
    checkout:
        type: github
        repo: actions/checkout@v4

    setup-node:
        type: github
        repo: actions/setup-node@v3
```

#### 2. File-based Actions

Reference local action files:

```yaml
actions:
    custom-build:
        type: file
        path: ./.pipeline/actions/build-action.yml
```

#### 3. Inline Actions

Define actions directly in the workflow:

```yaml
actions:
    notify:
        type: inline
        name: Send Notification
        description: Sends a custom notification
        inputs:
            message:
                description: Message to send
                required: true
                default: 'Hello'
            channel:
                description: Notification channel
                required: false
                default: 'general'
        outputs:
            status:
                description: Delivery status
        runs:
            steps:
                - run: |
                      echo "Sending: ${{ inputs.message }} to ${{ inputs.channel }}"
                      echo "status=sent" >> $PIPELINE_OUTPUT
```

### Jobs

Jobs contain steps and can depend on other jobs:

```yaml
jobs:
    build:
        needs: [] # Job dependencies (optional)
        env: # Environment variables (optional)
            CARGO_TERM_COLOR: always
        strategy: # Matrix strategy (optional)
            matrix:
                os: [ubuntu-latest, windows-latest]
        steps: # Steps to execute (required)
            - uses: action-name # Use an action
              with: # Action parameters
                  param: value
            - run: shell command # Run shell command
              id: step-id # Step identifier (optional)
              if: ${{ expression }} # Conditional execution (optional)
              continue-on-error: false # Continue on failure (optional)
              env: # Step-level environment (optional)
                  KEY: value
```

### Step Outputs

Steps can produce outputs using the `$PIPELINE_OUTPUT` environment variable:

```yaml
steps:
    - id: build
      run: |
          cargo build --release
          echo "binary=target/release/app" >> $PIPELINE_OUTPUT
          echo "version=$(cargo pkgid | cut -d# -f2)" >> $PIPELINE_OUTPUT

    - run: |
          echo "Built binary: ${{ steps.build.outputs.binary }}"
          echo "Version: ${{ steps.build.outputs.version }}"
```

**Output Format:**

- Simple: `echo "key=value" >> $PIPELINE_OUTPUT`
- Multi-line: Use heredoc syntax with EOF delimiter

**Planned translation:**

- GitHub Actions: `$PIPELINE_OUTPUT` → `$GITHUB_OUTPUT`
- GitLab CI: Uses artifacts or CI variables
- Local: Temporary file per step

### Backend Conditionals

Execute steps conditionally based on the execution backend:

```yaml
steps:
    # Only run on GitHub Actions
    - if: ${{ backend == 'github' }}
      uses: actions/cache@v3
      with:
          path: target
          key: cargo-cache

    # Only run locally
    - if: ${{ backend == 'local' }}
      run: echo "No caching available locally"

    # Complex conditions
    - if: ${{ backend == 'github' && github.event_name == 'push' }}
      run: echo "GitHub push event"
```

**Planned backend support:**

- `'local'` - Direct command execution
- `'github'` - GitHub Actions
- `'gitlab'` - GitLab CI
- `'jenkins'` - Jenkins

### Matrix Strategies

Run jobs with different configurations:

```yaml
jobs:
    test:
        strategy:
            matrix:
                os: [ubuntu-latest, windows-latest, macos-latest]
                rust-version: [stable, nightly]
                exclude:
                    # Skip expensive combinations
                    - os: macos-latest
                      rust-version: nightly
        steps:
            - run: |
                  echo "Testing on ${{ matrix.os }} with Rust ${{ matrix.rust-version }}"
```

**Planned local execution behavior:** Only runs combinations matching the current OS.

### Expression Language

GitHub Actions compatible expressions using `${{ }}` syntax:

#### Contexts Available:

- `env` - Environment variables
- `secrets` - Secret values (local: `PIPELINE_SECRET_*` env vars)
- `vars` - Repository variables
- `steps` - Step outputs
- `needs` - Job outputs from dependencies
- `matrix` - Matrix strategy variables
- `backend` - Current execution backend
- `github` - GitHub-specific context (when applicable)

#### Operators:

- Comparison: `==`, `!=`
- Logical: `&&`, `||`, `!`
- Property access: `.` for nested objects

#### Functions (MVP set):

- `toJson()` - Convert to JSON string
- `fromJson()` - Parse JSON string
- `contains()` - Check substring/array membership
- `startsWith()` - Check string prefix
- `join()` - Join array elements
- `format()` - String formatting

## Examples

See the `spec/generic-pipelines/examples/` directory for complete workflow examples:

- **`basic-workflow.yml`** - Simple single-job workflow
- **`multi-job.yml`** - Job dependencies and step outputs
- **`backend-conditional.yml`** - Backend-specific behavior
- **`matrix-build.yml`** - Matrix strategy with multiple OS/versions
- **`inline-action.yml`** - Custom inline action definitions

## Package Structure

This is an umbrella crate that provides access to workflow AST types.

### Current Structure

- **`packages/gpipe/`** - Umbrella crate with re-exports
- **`packages/gpipe/ast/`** - AST types and structures (✅ complete)
    - `Workflow`, `Job`, `Step`, `Action` types
    - `Expression` AST for conditionals
    - Full serde support for serialization/deserialization

### Planned Sub-Packages

- **`gpipe_parser`** 🔮 - YAML workflow parsers
- **`gpipe_runner`** 🔮 - Local execution engine
- **`gpipe_translator`** 🔮 - Format translation (to GitHub Actions, GitLab CI, etc.)
- **`gpipe_actions`** 🔮 - Action loading and resolution
- **`gpipe_cli`** 🔮 - Command-line interface

## Usage (Planned)

```bash
# Execute workflow locally
gpipe run workflow.yml

# Execute with specific backend context
gpipe run workflow.yml --backend=local

# Translate to GitHub Actions
gpipe translate workflow.yml --target=github

# Translate to GitLab CI
gpipe translate workflow.yml --target=gitlab

# Validate workflow syntax
gpipe validate workflow.yml
```

## Rust API

This is an umbrella crate that re-exports the core AST types via the `ast` module:

```rust
use gpipe::ast::*;
use gpipe::ast::serde_yaml;
use std::collections::BTreeMap;

// Create a basic workflow using the AST types
let workflow = Workflow {
    version: "1.0".to_string(),
    name: "example".to_string(),
    triggers: vec![],
    actions: BTreeMap::new(),
    jobs: BTreeMap::new(),
};

// Serialize to YAML
let yaml = serde_yaml::to_string(&workflow).unwrap();
```

## Architecture

```
┌─────────────────────┐    ┌─────────────────────┐    ┌─────────────────────┐
│   Generic YAML      │    │   GitHub Actions    │    │    GitLab CI        │
│   (Primary Format)  │    │       YAML          │    │      YAML           │
└──────────┬──────────┘    └──────────┬──────────┘    └──────────┬──────────┘
           │                          │                          │
           ▼                          ▼                          ▼
    ┌─────────────────────────────────────────────────────────────────────┐
    │                        Generic AST                                  │
    │                        (gpipe_ast)                                  │
    └─────────────────────┬─────────────────────┬─────────────────────────┘
                          │                     │
                          ▼                     ▼
              ┌─────────────────────┐  ┌─────────────────────┐
              │   Local Runner      │  │  Translators        │
              │   (gpipe_runner)    │  │  (gpipe_translator) │
              └─────────────────────┘  └─────────────────────┘
```

## Contributing

This project follows MoosicBox conventions:

- Use `BTreeMap` for deterministic ordering (not `HashMap`)
- Package naming: `gpipe_*` (underscores)
- Include `#[must_use]` on constructors and getters
- Comprehensive error documentation with asterisks (\*) for bullet points
- All packages include `fail-on-warnings = []` feature

## License

See the repository root for license information.
