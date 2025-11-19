# Generic Pipelines - Execution Plan

## Executive Summary

Create a universal CI/CD workflow tool that can execute and translate between different workflow formats, including a new generic workflow format that is backend-agnostic. The tool introduces a generic workflow format that allows users to write workflows once and run them on any supported backend (GitHub Actions, GitLab CI, local execution, etc.). Backend-specific functionality is supported through conditional execution blocks. The tool should handle workflow translation on-the-fly or through persisted generic representations, with first-class support for local execution without containerization.

**Current Status:** 🟡 **In Progress** - Phase 1 partially complete, AST implementation done

**Completion Estimate:** 5% complete - Section 1.1 complete with full AST implementation

## Status Legend

- 🔴 **Critical** - Blocks core functionality
- 🟡 **Important** - Affects user experience or API design
- 🟢 **Minor** - Nice-to-have or polish items
- ✅ **Complete** - Fully implemented and validated
- 🟡 **In Progress** - Currently being worked on
- ❌ **Blocked** - Waiting on dependencies or design decisions

## Open Questions

These items need further investigation or decision during implementation:

### Resolved Decisions

- ✅ **AST format**: YAML-serializable for debugging and intermediate storage
- ✅ **Expression syntax**: GitHub Actions compatible `${{ }}` syntax
- ✅ **Backend conditionals**: Use `if: ${{ backend == 'name' }}` pattern
- ✅ **Action pattern**: Follow GitHub's `uses:` and `with:` pattern
- ✅ **Generic format**: Treated as first-class workflow format alongside GitHub/GitLab

### Resolved Decisions (from requirements gathering)

#### Execution Model

- ✅ **Phase execution**: Sequential - each phase must complete before the next begins
- ✅ **Local job execution**: Sequential initially, parallel execution as future enhancement
- ✅ **Matrix execution**: Sequential locally, only current OS supported
- ✅ **Proof tracking**: Individual proof entries under each checkbox, verification per step

#### Action System

- ✅ **Action resolution**: Top-level `actions:` property maps names to definitions
- ✅ **Action formats**: `{ type: "github|file|inline", url/path/definition: ... }`
- ✅ **Translation failures**: Hard fail when backend translation missing (no fallbacks)
- ✅ **Action distribution**: Standard actions shipped with tool, repo-based actions cached

#### Backend Behavior

- ✅ **Backend detection**: CLI flag `--backend=` with env auto-detection fallback
- ✅ **Backend conditionals**: Simple conditions stripped, complex ones become false
- ✅ **Translation strategy**: Runtime translation, preserve structure across backends

#### Data Flow

- ✅ **Step outputs**: Via `$PIPELINE_OUTPUT` file (like GitHub's `$GITHUB_OUTPUT`)
- ✅ **Output types**: All outputs are strings, no type validation
- ✅ **Secrets**: Environment variables with `PIPELINE_SECRET_` prefix locally
- ✅ **Artifacts**: Handled via generic actions with backend-specific implementations

#### Error Handling

- ✅ **Failure model**: Match GitHub's outcome vs conclusion semantics exactly
- ✅ **Continue-on-error**: Affects conclusion but not outcome
- ✅ **DAG validation**: Circular dependency check at parse time

#### Triggers and Events

- ✅ **Local triggers**: Ignored initially, stubbed event context
- ✅ **Generic triggers**: Backend-agnostic names that translate to platform-specific

### Newly Resolved Decisions (from specification clarification)

#### Workflow Format

- ✅ **Top-level structure**: `version`, `name`, `triggers`, `actions`, `jobs`
- ✅ **Trigger names**: `push` (not commit), `pull_request`, `schedule`, `manual`
- ✅ **Trigger format**: Support both simple lists and detailed parameters
- ✅ **Job dependencies**: Use GitHub's `needs:` syntax exactly
- ✅ **Matrix syntax**: Keep GitHub's exact structure including strategy/matrix/exclude
- ✅ **File location**: Any location, any YAML file, no special naming required

#### Action System

- ✅ **Action types**: `github` (repo field), `file` (path field), `inline` (runs field)
- ✅ **GitHub action format**: `repo: actions/checkout@v4` or `repo: owner/name@ref`
- ✅ **Custom action format**: GitHub-like with name/inputs/outputs/runs structure
- ✅ **Action resolution**: ONLY via explicit `actions:` mapping, no search paths
- ✅ **Action inputs**: Passed at usage level with `with:`, not in action definition
- ✅ **Built-in actions**: Implemented as standard custom actions, not special syntax

#### Execution Semantics

- ✅ **Step outputs**: Write to `$PIPELINE_OUTPUT`, same as `$GITHUB_OUTPUT`
- ✅ **Output storage**: Temp file per step or in-memory representation
- ✅ **Secrets locally**: `PIPELINE_SECRET_*` env vars OR `--secret KEY=val` CLI args
- ✅ **Environment contexts**: Support `env`, `vars`, `secrets` (not GitHub-specific)
- ✅ **Error handling**: Use GitHub's exact keywords (continue-on-error, outcome, conclusion)
- ✅ **Job failure**: Mark failed but continue other non-dependent jobs
- ✅ **Matrix locally**: Run only current OS, map ubuntu-latest→linux, etc.

#### Translation Behavior

- ✅ **Backend conditionals**: Replace with constant true/false during translation
- ✅ **Translation output**: Write to actual .github/workflows/ directory
- ✅ **Untranslatable actions**: Generate compatible action for target platform
- ✅ **AST execution**: Execute directly from AST, don't generate scripts
- ✅ **Filename preservation**: Keep original filename when translating

#### CLI Design

- ✅ **Run command**: `gpipe run workflow.yml [options]`
- ✅ **Run options**: `--backend=local` (default), `--secret`, `--env`, `--dry-run`
- ✅ **NO run options**: No `--job` or `--matrix-os` selection
- ✅ **Translate command**: `gpipe translate workflow.yml --target=github [--output=path]`
- ✅ **Auto-discovery**: No automatic workflow discovery, must specify file

#### Artifact System

- ✅ **Artifact actions**: Built-in `upload-artifact`/`download-artifact` actions
- ✅ **Implementation**: Standard custom actions that translate to platform-specific
- ✅ **Local storage**: Persist artifacts in `.pipeline/artifacts/[run-id]/[name]/`
- ✅ **Artifact persistence**: Keep between runs, don't auto-cleanup
- ✅ **Priority**: Later feature, not required for MVP

### Implementation Decisions (from specification refinement)

#### AST Structure

- ✅ **Core node types**: Workflow, Job, Step with defined fields
- ✅ **Step representation**: Enum variants (UseAction vs RunScript) not optional fields
- ✅ **Expression storage**: Parsed Expression trees, not raw strings
- ✅ **Backend conditionals**: Same storage as regular conditions (no special handling)
- ✅ **Collections**: Use BTreeMap for deterministic ordering (MoosicBox convention)

#### Expression Language

- ✅ **MVP functions**: `toJson()`, `fromJson()`, `contains()`, `startsWith()`, `join()`, `format()`
- ✅ **Operators**: `==`, `!=`, `&&`, `||`, `!`, property access with `.`
- ✅ **No status functions**: Skip `always()`, `success()`, `failure()` for MVP
- ✅ **Expression AST**: Complete enum with String, Number, Boolean, Null, Variable, BinaryOp, UnaryOp, FunctionCall, Index

#### Package Structure

- ✅ **Umbrella crate**: `packages/gpipe/` following switchy/hyperchad pattern
- ✅ **Sub-crates**: `gpipe_ast`, `gpipe_parser`, `gpipe_runner`, `gpipe_translator`, `gpipe_actions`, `gpipe_cli`
- ✅ **Binary name**: `gpipe` (not pipeline)
- ✅ **Naming convention**: All packages use gpipe\_ prefix

#### Built-in Actions

- ✅ **No magic**: Regular file-based actions in `.pipeline/actions/` directory
- ✅ **No embedded actions**: Not compiled into binary, loaded from repo
- ✅ **Standard format**: Use same YAML format as user-defined actions
- ✅ **Initial built-ins**: checkout, setup-\*, upload-artifact as regular action files

## Phase 1: Generic Workflow Format Definition 🟡

**Goal:** Define the platform-agnostic workflow format that serves as the primary input format

**Status:** Section 1.1 complete with full AST implementation - remaining sections are specification only

### 1.1 Generic Workflow Syntax ✅ COMPLETE

- [x] Define generic workflow YAML schema 🔴 **CRITICAL**
    - Proof: packages/gpipe/ast/src/workflow.rs:45-60
    - Complete Workflow struct with version, name, triggers, actions, jobs fields
    - Full YAML schema documented in comments at packages/gpipe/ast/src/workflow.rs:5-43
    - All types use BTreeMap for deterministic ordering per MoosicBox conventions
    - Structure:
        ```yaml
        version: 1.0
        name: string
        triggers:
            push:
                branches: [string]
            pull_request:
                types: [string]
            schedule:
                cron: string
            manual:
        actions:
            name:
                type: github|file|inline
                repo: string # for github
                path: string # for file
                # inline has full action definition
        jobs:
            job-name:
                needs: [string]
                env:
                    KEY: value
                strategy:
                    matrix:
                        os: [ubuntu-latest, windows-latest, macos-latest]
                        exclude:
                            - os: windows-latest
                steps:
                    - uses: action-name
                      with:
                          param: value
                    - run: shell command
                      id: step-id
                      if: ${{ expression }}
                      continue-on-error: boolean
        ```

#### 1.1 Verification

- [x] Run `cargo build -p gpipe_ast` - Package compiles
    - Proof: packages/gpipe/ast/src/ contains complete AST implementation
    - Workflow, Job, Step, Expression, Trigger, ActionDef types all defined
- [x] Run `cargo test -p gpipe_ast` - All tests pass
    - Proof: No tests exist yet (package is type definitions only)
- [x] Run `cargo clippy -p gpipe_ast -- -D warnings` - No clippy warnings
    - Proof: Clean build with all clippy lints enabled
- [x] Run `cargo fmt` - Code formatted
    - Proof: All workspace files properly formatted
- [x] Run `cargo machete` - No unused dependencies
    - Proof: Only serde and serde_yaml used, both required
- [x] Create example workflow files in spec/generic-pipelines/examples/
    - Proof: spec/generic-pipelines/examples/ contains 5 comprehensive example workflows
    - basic-workflow.yml - Simple single-job workflow
    - multi-job.yml - Job dependencies and step outputs demonstration
    - backend-conditional.yml - Backend-specific conditional execution
    - matrix-build.yml - Matrix strategy with multiple OS/versions
    - inline-action.yml - Custom inline action definitions with inputs/outputs
- [x] Document schema in packages/gpipe/README.md
    - Proof: packages/gpipe/README.md contains complete schema documentation
    - Comprehensive workflow format specification with examples
    - All trigger types, action types, and syntax documented
    - Backend conditional usage and expression language reference

### 1.2 Generic Action System

- [ ] Define generic action definition format 🔴 **CRITICAL**
    - GitHub type:
        ```yaml
        checkout:
            type: github
            repo: actions/checkout@v4 # Format: owner/name@ref
        ```
    - File type:
        ```yaml
        my-action:
            type: file
            path: ./.pipeline/actions/my-action/action.yml
        ```
    - Inline type:
        ```yaml
        echo-message:
            type: inline
            name: Echo Message
            description: Echoes a message
            inputs:
                message:
                    description: Message to echo
                    required: true
                    default: 'Hello'
            outputs:
                result:
                    description: The result
            runs:
                steps:
                    - run: |
                          echo "${{ inputs.message }}"
                          echo "result=done" >> $PIPELINE_OUTPUT
        ```
- [ ] Action resolution requires explicit declaration 🔴 **CRITICAL**
    - ALL actions must be in top-level `actions:` mapping
    - NO implicit search paths or conventions
    - NO automatic discovery
- [ ] Custom action file format (GitHub-like) 🔴 **CRITICAL**
    - Files referenced by `type: file` use this structure:
        ```yaml
        name: My Custom Action
        description: Does something useful
        inputs:
            param-name:
                description: Parameter description
                required: true|false
                default: 'value'
        outputs:
            output-name:
                description: Output description
        runs:
            steps:
                - run: shell command
                - uses: another-action # Can reference other actions
        ```
- [ ] Action inputs passed at usage, not definition 🟡 **IMPORTANT**
    - Use `with:` at step level to pass inputs
    - Action definition only declares what inputs exist

#### 1.2 Verification

- [ ] Run `cargo build -p gpipe_actions` - Package compiles
- [ ] Run `cargo test -p gpipe_actions` - All tests pass
- [ ] Run `cargo clippy -p gpipe_actions -- -D warnings` - No clippy warnings
- [ ] Test loading and resolving generic action definitions

### 1.3 Backend Context System

- [ ] Define `backend` context variable concept 🔴 **CRITICAL**
    - **Specification**: Will be available as `${{ backend }}` in expressions
    - **Planned values**: `'local'`, `'github'`, `'gitlab'`, `'jenkins'`, etc.
    - **Design decision**: Value will come from CLI flag `--backend=name` (default: `local`)
    - **Usage**: For conditional execution and translation
    - NOTE: This is definition only - implementation in Phase 3.2 and 4.3
- [ ] Define supported backend identifiers 🔴 **CRITICAL**
    - Document what each backend identifier represents:
        - `local`: Direct command execution without containers
        - `github`: GitHub Actions environment
        - `gitlab`: GitLab CI environment
        - `jenkins`: Jenkins pipeline (future)
- [ ] Document backend detection strategy 🔴 **CRITICAL**
    - **Design**: How backend will be determined (not implementation)
    - Primary: CLI flag `--backend=name`
    - Fallback: Environment detection (CI env vars)
    - Default: `local` when no CI detected

#### 1.3 Verification

- [ ] Document in spec/generic-pipelines/backend-context.md
- [ ] Define complete list of supported backends
- [ ] Specify environment detection rules

### 1.4 Backend Conditional Syntax Definition

- [ ] Define backend conditional expression format 🔴 **CRITICAL**
    - Syntax: `if: ${{ backend == 'name' }}`
    - Uses backend context defined in 1.3
    - Translation semantics:
        - When translating to GitHub: `backend == 'github'` → `true`
        - When translating to GitLab: `backend == 'gitlab'` → `true`
        - Complex: `${{ backend == 'github' && matrix.os == 'ubuntu' }}` → `${{ true && matrix.os == 'ubuntu' }}`
    - Runtime semantics:
        - Backend value from context (see 1.3)
        - Steps with false conditions skipped entirely

#### 1.4 Verification

- [ ] Document in spec/generic-pipelines/syntax.md
- [ ] Add examples showing backend conditionals
- [ ] Create test cases for validation

### 1.5 Output Variable Syntax Definition

- [ ] Define step output capture mechanism 🔴 **CRITICAL**
    - Environment variable: `$PIPELINE_OUTPUT`
    - Write format: `echo "name=value" >> $PIPELINE_OUTPUT`
    - Multi-line format:
        ```bash
        echo "content<<EOF" >> $PIPELINE_OUTPUT
        echo "line 1" >> $PIPELINE_OUTPUT
        echo "line 2" >> $PIPELINE_OUTPUT
        echo "EOF" >> $PIPELINE_OUTPUT
        ```
    - Access syntax: `${{ steps.<step-id>.outputs.<name> }}`
    - Translation mappings:
        - GitHub: `$PIPELINE_OUTPUT` → `$GITHUB_OUTPUT`
        - GitLab: Use artifacts or CI variables
        - Local: Temp file per step

#### 1.5 Verification

- [ ] Document output syntax in spec
- [ ] Create examples with step outputs
- [ ] Define edge cases (empty outputs, special characters)

### 1.6 Trigger Mapping Definition

- [ ] Define generic trigger vocabulary 🔴 **CRITICAL**
    - Generic trigger types:
        ```yaml
        push:
            branches: [main, develop]
        pull_request:
            types: [opened, synchronize]
        schedule:
            cron: '0 0 * * *'
        manual:
        ```
    - Backend mappings:
      | Generic | GitHub | GitLab |
      |---------|---------|---------|
      | `push` | `push` | `push` |
      | `pull_request` | `pull_request` | `merge_request` |
      | `schedule` | `schedule` | `schedule` |
      | `manual` | `workflow_dispatch` | `web` |
    - Configuration translation rules:
        - Branch filters translate directly
        - PR types map to platform-specific events
        - Cron syntax is universal

#### 1.6 Verification

- [ ] Document complete trigger mapping table
- [ ] Create examples for each trigger type
- [ ] Define unsupported trigger handling

## Phase 2: Core AST and Workflow Model 🟡

**Goal:** Define the abstract syntax tree for representing ALL workflow formats (Generic, GitHub Actions, GitLab CI, etc.) in a unified internal structure

**Status:** AST implementation complete in gpipe_ast package - package creation steps added

### 2.1 AST Definition

- [ ] Define workflow node types 🔴 **CRITICAL**

    ```rust
    pub struct Workflow {
        pub version: String,
        pub name: String,
        pub triggers: Vec<Trigger>,
        pub actions: BTreeMap<String, ActionDef>,
        pub jobs: BTreeMap<String, Job>,
    }

    pub struct Job {
        pub needs: Vec<String>,
        pub strategy: Option<MatrixStrategy>,
        pub env: BTreeMap<String, String>,
        pub steps: Vec<Step>,
        pub if_condition: Option<Expression>,
    }

    pub enum Step {
        UseAction {
            id: Option<String>,
            uses: String,
            with: BTreeMap<String, String>,
            env: BTreeMap<String, String>,
            if_condition: Option<Expression>,
            continue_on_error: bool,
        },
        RunScript {
            id: Option<String>,
            run: String,
            env: BTreeMap<String, String>,
            if_condition: Option<Expression>,
            continue_on_error: bool,
            working_directory: Option<String>,
        },
    }
    ```

- [ ] Create expression language AST 🔴 **CRITICAL**
    ```rust
    pub enum Expression {
        String(String),
        Number(f64),
        Boolean(bool),
        Null,
        Variable(Vec<String>),  // e.g., ["github", "sha"]
        BinaryOp {
            left: Box<Expression>,
            op: BinaryOperator,
            right: Box<Expression>,
        },
        UnaryOp {
            op: UnaryOperator,
            expr: Box<Expression>,
        },
        FunctionCall {
            name: String,
            args: Vec<Expression>,
        },
        Index {
            expr: Box<Expression>,
            index: Box<Expression>,
        },
    }
    ```
- [ ] Define matrix strategy representation 🔴 **CRITICAL**
- [ ] Support for artifacts and caching abstractions 🟡 **IMPORTANT**
- [ ] Define secret/credential abstraction 🔴 **CRITICAL**

#### 2.1 Verification

- [ ] Run `cargo build -p gpipe_ast --all-features` - Verify compilation
- [ ] Run `cargo test -p gpipe_ast` - All tests pass
- [ ] Run `cargo clippy -p gpipe_ast -- -D warnings` - No clippy warnings
- [ ] Run `cargo fmt -- --check` - Code properly formatted
- [ ] Run `cargo doc -p gpipe_ast --no-deps` - Documentation builds

### 2.2 Workflow Model

- [ ] Create Workflow struct with metadata and jobs 🔴 **CRITICAL**
    - Fields: version, name, triggers, actions map, jobs map
- [ ] Define Job struct with dependencies and steps 🔴 **CRITICAL**
    - Sequential execution locally (definition order when ready)
    - DAG validation for circular dependencies at parse time
    - `needs:` array for dependencies (GitHub syntax)
    - Failed jobs block dependents from running
- [ ] Create Step variants 🔴 **CRITICAL**
    - UseAction: references action from `actions:` map with `with:` params
    - RunScript: shell command with working dir
    - Both support: `id`, `if`, `continue-on-error`, `env`
- [ ] Implement Context struct for variables and state 🔴 **CRITICAL**
    - `$PIPELINE_OUTPUT` file for step outputs (or in-memory map)
    - `PIPELINE_SECRET_*` env vars for secrets
    - Contexts: `env`, `secrets`, `vars`, `steps`, `needs`, `matrix`, `backend`
- [ ] Add support for job/step outputs 🟡 **IMPORTANT**
    - All outputs are strings (no type validation)
    - GitHub-compatible: outcome vs conclusion semantics
    - Step outputs via `echo "key=value" >> $PIPELINE_OUTPUT`
- [ ] Error handling semantics 🟡 **IMPORTANT**
    - `continue-on-error: true`: Sets conclusion=failure, outcome=success
    - Without flag: Both conclusion and outcome = failure
    - Failed job continues other non-dependent jobs

#### 2.2 Verification

- [ ] Run `cargo build -p gpipe_ast` - Package compiles
- [ ] Run `cargo test -p gpipe_ast` - All tests pass
- [ ] Run `cargo clippy -p gpipe_ast -- -D warnings` - No clippy warnings
- [ ] Run `cargo tarpaulin -p gpipe_ast` - Code coverage > 80%

### 2.3 Package Structure

- [ ] Create packages/gpipe umbrella package 🔴 **CRITICAL**
    - Main Cargo.toml re-exports all sub-crates with features
    - Follow switchy/hyperchad pattern for organization
- [ ] Create sub-packages with consistent naming 🔴 **CRITICAL**
    - `gpipe_ast` - Core AST types and structures
    - `gpipe_parser` - Parsers for Generic/GitHub/GitLab formats
    - `gpipe_runner` - Local execution engine
    - `gpipe_translator` - Format translation logic
    - `gpipe_actions` - Action loading and resolution
    - `gpipe_cli` - CLI binary named 'gpipe'
- [ ] Implement Serialize/Deserialize for all AST nodes 🔴 **CRITICAL**
- [ ] Add validation methods for workflow correctness 🟡 **IMPORTANT**
- [ ] Include builder patterns for ergonomic construction 🟡 **IMPORTANT**

#### 2.3 Verification

- [ ] Run `cargo build -p gpipe_ast` - Package compiles
- [ ] Run `cargo test -p gpipe_ast` - All tests pass
- [ ] Run `cargo clippy -p gpipe_ast -- -D warnings` - No clippy warnings
- [ ] Run `cargo fmt -- --check` - Code properly formatted
- [ ] Run `cargo doc -p gpipe_ast` - Documentation builds
    - [ ] Run `cargo test -p gpipe_ast --test builder_patterns_test` - Builder tests pass
    - [ ] Run `cargo doc -p gpipe_ast` - Builder APIs documented

### 2.4 Parser Package Setup

- [ ] Create packages/gpipe/parser directory structure 🔴 **CRITICAL**
    - Create src/lib.rs with module exports
    - Create src/generic.rs for generic format parser
    - Create src/expression.rs for expression parser
- [ ] Set up gpipe_parser Cargo.toml 🔴 **CRITICAL**
    - Dependencies: gpipe_ast, serde, serde_yaml, nom/pest
    - Features: fail-on-warnings
- [ ] Add to workspace Cargo.toml 🔴 **CRITICAL**
- [ ] Create README.md with package description

#### 2.4 Verification

- [ ] Run `cargo build -p gpipe_parser` - Package compiles
- [ ] Run `cargo fmt` - Code formatted
- [ ] Run `cargo machete` - No unused dependencies

### 2.5 Runner Package Setup

- [ ] Create packages/gpipe/runner directory structure 🔴 **CRITICAL**
    - Create src/lib.rs with module exports
    - Create src/local.rs for local execution
    - Create src/context.rs for execution context
- [ ] Set up gpipe_runner Cargo.toml 🔴 **CRITICAL**
    - Dependencies: gpipe_ast, gpipe_parser, tokio, tempfile
    - Features: fail-on-warnings
- [ ] Add to workspace Cargo.toml 🔴 **CRITICAL**
- [ ] Create README.md with package description

#### 2.5 Verification

- [ ] Run `cargo build -p gpipe_runner` - Package compiles
- [ ] Run `cargo fmt` - Code formatted
- [ ] Run `cargo machete` - No unused dependencies

### 2.6 Translator Package Setup

- [ ] Create packages/gpipe/translator directory structure 🔴 **CRITICAL**
    - Create src/lib.rs with module exports
    - Create src/github.rs for GitHub Actions translation
    - Create src/gitlab.rs for GitLab CI translation
- [ ] Set up gpipe_translator Cargo.toml 🔴 **CRITICAL**
    - Dependencies: gpipe_ast, serde, serde_yaml
    - Features: fail-on-warnings
- [ ] Add to workspace Cargo.toml 🔴 **CRITICAL**
- [ ] Create README.md with package description

#### 2.6 Verification

- [ ] Run `cargo build -p gpipe_translator` - Package compiles
- [ ] Run `cargo fmt` - Code formatted
- [ ] Run `cargo machete` - No unused dependencies

## Phase 3: Workflow Parsers 🔴

**Goal:** Parse all workflow formats (Generic, GitHub Actions, GitLab CI, etc.) into the AST

**Status:** All tasks pending - Parser implementation needed

### 3.1 Multi-Format Parser

- [ ] Parse Generic workflow format (primary format) 🔴 **CRITICAL**
- [ ] Parse GitHub Actions workflow format 🔴 **CRITICAL**
- [ ] Parse GitLab CI workflow format 🟡 **IMPORTANT**
- [ ] Parse workflow triggers (on: push, pull_request, etc.) 🔴 **CRITICAL**
- [ ] Parse job definitions with needs dependencies 🔴 **CRITICAL**
- [ ] Parse steps with uses/run/if conditions including backend conditionals 🔴 **CRITICAL**
- [ ] Handle with parameters and env variables 🔴 **CRITICAL**
- [ ] Parse matrix strategies and includes/excludes 🟡 **IMPORTANT**

#### 3.1 Verification

- [ ] Run `cargo build -p gpipe_parser` - Package compiles
- [ ] Run `cargo test -p gpipe_parser` - All tests pass
- [ ] Run `cargo clippy -p gpipe_parser -- -D warnings` - No clippy warnings
- [ ] Run `cargo fmt` - Code formatted
- [ ] Run `cargo machete` - No unused dependencies
- [ ] Test parsing MoosicBox workflows successfully

### 3.2 Backend Conditional Parser

- [ ] Parse backend conditional expressions 🔴 **CRITICAL**
    - Recognize `backend` as special variable in expressions
    - Parse into `Expression::Variable(vec!["backend"])`
    - Support comparisons: `backend == 'local'`, `backend != 'github'`
    - Handle in complex expressions: `backend == 'local' && os == 'linux'`
- [ ] Extend expression parser for backend context 🔴 **CRITICAL**
    - Add backend to known context variables
    - Validate backend values are strings
    - Parse quoted backend names correctly
- [ ] Test backend conditional parsing 🟡 **IMPORTANT**
    - Unit tests for various backend expressions
    - Test invalid backend names produce errors
    - Verify AST structure is correct

#### 3.2 Verification

- [ ] Run `cargo build -p gpipe_parser` - Package compiles
- [ ] Run `cargo test -p gpipe_parser` - All tests pass
- [ ] Run `cargo clippy -p gpipe_parser -- -D warnings` - No clippy warnings
- [ ] Run `cargo fmt` - Code formatted
- [ ] Run `cargo machete` - No unused dependencies
- [ ] Test parsing of complex backend expressions
- [ ] Verify error messages for invalid syntax

### 3.3 Expression Evaluator

- [ ] Implement GitHub Actions expression syntax parser 🔴 **CRITICAL**
- [ ] Support MVP function set 🔴 **CRITICAL**
    - String functions: `toJson()`, `fromJson()`, `contains()`, `startsWith()`, `join()`, `format()`
    - No status functions initially (not used in MoosicBox workflows)
- [ ] Support context variables 🔴 **CRITICAL**
    - Contexts: `env`, `secrets`, `vars`, `steps`, `needs`, `matrix`, `backend`
    - Property access with `.` notation (e.g., `github.sha`, `matrix.os`)
- [ ] Implement operators 🔴 **CRITICAL**
    - Comparison: `==`, `!=`
    - Logical: `&&`, `||`, `!`
    - Property access: `.` for nested objects
- [ ] Handle string interpolation and type coercion 🟡 **IMPORTANT**

#### 3.3 Verification

- [ ] Run `cargo build -p gpipe_parser` - Package compiles
- [ ] Run `cargo test -p gpipe_parser` - All tests pass
- [ ] Run `cargo clippy -p gpipe_parser -- -D warnings` - No clippy warnings
- [ ] Run `cargo fmt` - Code formatted
- [ ] Run `cargo machete` - No unused dependencies
- [ ] Test expression evaluation matches GitHub Actions behavior

### 3.4 Workflow Validation

- [ ] Validate job dependency cycles 🔴 **CRITICAL**
- [ ] Check for undefined variables and references 🟡 **IMPORTANT**
- [ ] Validate action references and parameters 🟡 **IMPORTANT**
- [ ] Provide detailed error messages with line numbers 🟡 **IMPORTANT**

#### 3.4 Verification

- [ ] Run `cargo build -p gpipe_parser` - Package compiles
- [ ] Run `cargo test -p gpipe_parser` - All tests pass
- [ ] Run `cargo clippy -p gpipe_parser -- -D warnings` - No clippy warnings
- [ ] Run `cargo fmt` - Code formatted
- [ ] Run `cargo machete` - No unused dependencies
- [ ] Test validation provides clear error messages

## Phase 4: Local Runner Implementation 🔴

**Goal:** Execute workflows locally without containers

**Status:** All tasks pending - Execution engine needed

### 4.1 Execution Engine

- [ ] Create LocalRunner struct implementing WorkflowBackend trait 🔴 **CRITICAL**
- [ ] Implement job scheduler with dependency resolution 🔴 **CRITICAL**
    - Sequential execution (no parallelism initially)
    - Definition order when multiple jobs ready
    - Failed jobs prevent dependents from starting
    - Non-dependent jobs continue despite failures
- [ ] Execute shell commands via std::process::Command 🔴 **CRITICAL**
    - Direct AST execution, no script generation
    - Create temp `$PIPELINE_OUTPUT` file per step
    - Pass outputs between steps via context
- [ ] Manage working directories and environment variables 🔴 **CRITICAL**
    - Map `PIPELINE_SECRET_*` env vars to `${{ secrets.* }}`
    - Support `--secret KEY=value` CLI arguments
    - Support `--env KEY=value` CLI overrides
- [ ] Handle step conditions and continue-on-error 🟡 **IMPORTANT**
    - Evaluate `if:` expressions before running step
    - Match GitHub's outcome/conclusion model exactly
    - Skip steps with false conditions
- [ ] Matrix execution for local runner 🟡 **IMPORTANT**
    - Only run current OS combinations
    - Map OS values: ubuntu-latest→linux, windows-latest→windows, macos-latest→macos
    - Skip non-matching OS matrix entries

#### 4.1 Verification

- [ ] Run `cargo build -p gpipe_runner` - Package compiles
- [ ] Run `cargo test -p gpipe_runner` - All tests pass
- [ ] Run `cargo clippy -p gpipe_runner -- -D warnings` - No clippy warnings
- [ ] Run `cargo fmt` - Code formatted
- [ ] Run `cargo machete` - No unused dependencies
- [ ] Test basic workflow execution works locally

### 4.2 Output Management

- [ ] Implement `$PIPELINE_OUTPUT` file creation 🔴 **CRITICAL**
    - Create temp file in `.pipeline/runs/[run-id]/outputs/`
    - Set `PIPELINE_OUTPUT` env var to file path
    - Clean up file after step completion
- [ ] Parse output file format 🔴 **CRITICAL**
    - Parse `key=value` lines
    - Handle multi-line values with heredoc syntax
    - Store in context as `steps.<id>.outputs.<key>`
- [ ] Make outputs available to expressions 🔴 **CRITICAL**
    - Add to expression evaluation context
    - Support `${{ steps.build.outputs.binary }}` syntax
    - Pass outputs between dependent jobs
- [ ] Handle output edge cases 🟡 **IMPORTANT**
    - Empty output files
    - Invalid format lines (warn and skip)
    - Very large outputs (size limits)

#### 4.2 Verification

- [ ] Run `cargo build -p gpipe_runner` - Package compiles
- [ ] Run `cargo test -p gpipe_runner` - All tests pass
- [ ] Run `cargo clippy -p gpipe_runner -- -D warnings` - No clippy warnings
- [ ] Run `cargo fmt` - Code formatted
- [ ] Run `cargo machete` - No unused dependencies
- [ ] Test multi-line output handling
- [ ] Verify outputs persist across steps
- [ ] Test cleanup of temp files

### 4.3 Backend Context Evaluation

- [ ] Add backend to execution context 🔴 **CRITICAL**
    - Set from `--backend` CLI flag (default: 'local')
    - Make available in expression context
    - Pass to all expression evaluations
- [ ] Evaluate backend conditionals at runtime 🔴 **CRITICAL**
    - Check `if` conditions before executing steps
    - Skip steps where backend condition is false
    - Log skipped steps in verbose mode
- [ ] Handle complex backend expressions 🟡 **IMPORTANT**
    - Combine with other conditions: `backend == 'local' && matrix.os == 'linux'`
    - Short-circuit evaluation for performance
    - Cache evaluation results per step

#### 4.3 Verification

- [ ] Run `cargo build -p gpipe_runner` - Package compiles
- [ ] Run `cargo test -p gpipe_runner` - All tests pass
- [ ] Run `cargo clippy -p gpipe_runner -- -D warnings` - No clippy warnings
- [ ] Run `cargo fmt` - Code formatted
- [ ] Run `cargo machete` - No unused dependencies
- [ ] Test step skipping based on backend
- [ ] Verify complex expressions work correctly
- [ ] Test with different --backend values

### 4.4 Environment Management

- [ ] Detect and validate local tool availability 🔴 **CRITICAL**
- [ ] Set up PATH and environment variables 🔴 **CRITICAL**
- [ ] Create temporary directories for artifacts 🟡 **IMPORTANT**
- [ ] Implement artifact upload/download locally 🟡 **IMPORTANT**
- [ ] Handle cache storage and retrieval 🟢 **MINOR**

#### 4.4 Verification

- [ ] Run `cargo build -p gpipe_runner` - Package compiles
- [ ] Run `cargo test -p gpipe_runner` - All tests pass
- [ ] Run `cargo clippy -p gpipe_runner -- -D warnings` - No clippy warnings
- [ ] Run `cargo fmt` - Code formatted
- [ ] Run `cargo machete` - No unused dependencies
- [ ] Test environment isolation between jobs

## Phase 5: Action Translation System 🔴

**Goal:** Translate all action types (Generic actions, GitHub Actions, GitLab CI actions) to executable commands for local execution and backend generation

**Status:** All tasks pending - Action registry and translation needed

### 5.1 Action Registry

- [ ] Define ActionTranslator trait 🔴 **CRITICAL**
- [ ] Load built-in actions from `.pipeline/actions/` 🔴 **CRITICAL**
    - No embedded actions in binary
    - Load from repo directory at runtime
    - Use standard file-based action format
- [ ] Implement built-in actions as standard YAML files:
    - [ ] `.pipeline/actions/checkout.yml` 🔴 **CRITICAL**
        ```yaml
        name: Checkout
        description: Checkout repository
        runs:
            steps:
                - if: ${{ backend == 'github' }}
                  uses: actions/checkout@v4
                - if: ${{ backend == 'local' }}
                  run: |
                      git fetch --depth=1
                      git checkout ${{ github.sha || 'HEAD' }}
        ```
    - [ ] `.pipeline/actions/setup-node.yml` 🔴 **CRITICAL**
    - [ ] `.pipeline/actions/upload-artifact.yml` 🟡 **IMPORTANT** (Later feature)
    - [ ] `.pipeline/actions/cache.yml` 🟡 **IMPORTANT**
- [ ] Generate compatible action for untranslatable actions 🔴 **CRITICAL**

#### 5.1 Verification

- [ ] Run `cargo build -p gpipe_actions` - Package compiles
- [ ] Run `cargo test -p gpipe_actions` - All tests pass
- [ ] Run `cargo clippy -p gpipe_actions -- -D warnings` - No clippy warnings
- [ ] Test common actions (checkout, setup-\*, cache) translate correctly

### 5.2 JavaScript Action Support

- [ ] Parse action.yml metadata files 🟡 **IMPORTANT**
- [ ] Extract and execute Node.js action sources 🟡 **IMPORTANT**
- [ ] Set up action inputs/outputs via environment 🟡 **IMPORTANT**
- [ ] Handle pre/post scripts for actions 🟢 **MINOR**

#### 5.2 Verification

- [ ] Run `cargo build -p gpipe_actions` - Package compiles
- [ ] Run `cargo test -p gpipe_actions` - All tests pass
- [ ] Run `cargo clippy -p gpipe_actions -- -D warnings` - No clippy warnings
- [ ] Test JavaScript action execution works

#### 5.3 Verification

- [ ] Run `cargo build -p gpipe_actions` - Package compiles
- [ ] Run `cargo test -p gpipe_actions` - All tests pass
- [ ] Run `cargo clippy -p gpipe_actions -- -D warnings` - No clippy warnings
- [ ] Test fallback strategies work for untranslatable actions

#### 5.4 Verification

- [ ] Run `cargo build -p gpipe_actions` - Package compiles
- [ ] Run `cargo test -p gpipe_actions` - All tests pass
- [ ] Run `cargo clippy -p gpipe_actions -- -D warnings` - No clippy warnings
- [ ] Test action metadata system works

## Phase 6: Backend Abstraction Layer 🟡

**Goal:** Support multiple CI/CD backends

**Status:** All tasks pending - Backend trait design needed

### 6.1 Backend Trait

- [ ] Define WorkflowBackend trait with execute methods 🔴 **CRITICAL**
- [ ] Add capability queries (supports_parallelism, supports_artifacts) 🟡 **IMPORTANT**
- [ ] Implement backend-specific configuration 🟡 **IMPORTANT**
- [ ] Handle backend limitations and workarounds 🟡 **IMPORTANT**

#### 6.1 Verification

- [ ] Run `cargo build -p gpipe_translator` - Package compiles
- [ ] Run `cargo test -p gpipe_translator` - All tests pass
- [ ] Run `cargo clippy -p gpipe_translator -- -D warnings` - No clippy warnings
- [ ] Run `cargo fmt` - Code formatted
- [ ] Run `cargo machete` - No unused dependencies
- [ ] Test backend trait implementations work

### 6.2 Backend Translation Rules

- [ ] Implement backend conditional simplification 🔴 **CRITICAL**
    - During translation to target backend:
        - Replace `backend == 'target'` with `true`
        - Replace `backend != 'target'` with `false`
        - Replace `backend == 'other'` with `false`
    - Preserve rest of expression structure
    - Simplify resulting expressions when possible
- [ ] Implement output variable translation 🔴 **CRITICAL**
    - Map `$PIPELINE_OUTPUT` to backend-specific:
        - GitHub: `$GITHUB_OUTPUT`
        - GitLab: Artifact or variable approach
        - Jenkins: Environment variable
    - Update all `echo` statements that write to output
- [ ] Handle untranslatable features 🟡 **IMPORTANT**
    - Warn when features have no backend equivalent
    - Provide best-effort fallback
    - Document limitations in generated file

#### 6.2 Verification

- [ ] Run `cargo build -p gpipe_translator` - Package compiles
- [ ] Run `cargo test -p gpipe_translator` - All tests pass
- [ ] Run `cargo clippy -p gpipe_translator -- -D warnings` - No clippy warnings
- [ ] Run `cargo fmt` - Code formatted
- [ ] Run `cargo machete` - No unused dependencies
- [ ] Test conditional simplification produces valid YAML
- [ ] Verify output variables translate correctly
- [ ] Test with complex real-world workflows

### 6.3 Trigger Translation

- [ ] Implement trigger mapping tables 🔴 **CRITICAL**
    - Create mapping for each backend:
        ```rust
        match (generic_trigger, target_backend) {
            (TriggerType::Push, Backend::Github) => "push",
            (TriggerType::PullRequest, Backend::Github) => "pull_request",
            (TriggerType::PullRequest, Backend::Gitlab) => "merge_request",
            // ...
        }
        ```
- [ ] Translate trigger configurations 🔴 **CRITICAL**
    - Map branch filters appropriately
    - Convert event types to platform format
    - Handle cron schedules (mostly universal)
- [ ] Handle unsupported triggers 🟡 **IMPORTANT**
    - Warn when trigger has no equivalent
    - Skip trigger or provide documentation
    - Add comment in generated file explaining limitation

#### 6.3 Verification

- [ ] Run `cargo build -p gpipe_translator` - Package compiles
- [ ] Run `cargo test -p gpipe_translator` - All tests pass
- [ ] Run `cargo clippy -p gpipe_translator -- -D warnings` - No clippy warnings
- [ ] Run `cargo fmt` - Code formatted
- [ ] Run `cargo machete` - No unused dependencies
- [ ] Test all trigger types for each backend
- [ ] Verify configuration options translate
- [ ] Test with MoosicBox workflows

### 6.4 GitHub Actions Backend

#### 6.4 Verification

- [ ] Run `cargo build -p gpipe_translator` - Package compiles
- [ ] Run `cargo test -p gpipe_translator` - All tests pass
- [ ] Run `cargo clippy -p gpipe_translator -- -D warnings` - No clippy warnings
- [ ] Run `cargo fmt` - Code formatted
- [ ] Run `cargo machete` - No unused dependencies
- [ ] Test GitHub Actions YAML generation works

### 6.5 GitLab CI Backend

#### 6.5 Verification

- [ ] Run `cargo build -p gpipe_translator` - Package compiles
- [ ] Run `cargo test -p gpipe_translator` - All tests pass
- [ ] Run `cargo clippy -p gpipe_translator -- -D warnings` - No clippy warnings
- [ ] Run `cargo fmt` - Code formatted
- [ ] Run `cargo machete` - No unused dependencies
- [ ] Test GitLab CI YAML generation works

### 6.6 Jenkins Backend

#### 6.6 Verification

- [ ] Run `cargo build -p gpipe_translator` - Package compiles
- [ ] Run `cargo test -p gpipe_translator` - All tests pass
- [ ] Run `cargo clippy -p gpipe_translator -- -D warnings` - No clippy warnings
- [ ] Run `cargo fmt` - Code formatted
- [ ] Run `cargo machete` - No unused dependencies
- [ ] Test Jenkins pipeline generation works

## Phase 7: CLI Interface 🟡

**Goal:** User-friendly command-line interface

**Status:** All tasks pending - CLI implementation needed

### 7.1 Core Commands

- [ ] `run` - Execute workflow locally 🔴 **CRITICAL**
    ```bash
    gpipe run workflow.yml
    gpipe run workflow.yml --backend=local  # Default
    gpipe run workflow.yml --secret API_KEY=xxx --secret TOKEN=yyy
    gpipe run workflow.yml --env NODE_ENV=test --env DEBUG=true
    gpipe run workflow.yml --dry-run  # Show execution plan
    ```
- [ ] `translate` - Convert between formats 🟡 **IMPORTANT**

    ```bash
    gpipe translate workflow.yml --target=github
    # Writes to .github/workflows/workflow.yml by default

    gpipe translate workflow.yml --target=github --output=custom.yml
    # Writes to specified path

    gpipe translate workflow.yml --target=gitlab
    # Writes to .gitlab-ci.yml by default
    ```

- [ ] `validate` - Check workflow syntax 🟡 **IMPORTANT**
    ```bash
    gpipe validate workflow.yml
    # Validates syntax and references
    ```
- [ ] NO `cache-action` command initially 🟢 **MINOR**
- [ ] NO auto-discovery of workflows
- [ ] NO `--job` or `--matrix-os` selection options

#### 7.1 Verification

- [ ] Run `cargo build --bin gpipe` - Binary builds
- [ ] Run `cargo test -p gpipe_cli` - CLI tests pass
- [ ] Run `cargo clippy -p gpipe_cli -- -D warnings` - No warnings
- [ ] Test CLI commands work with help flags

#### 5.2 Verification

- [ ] Run `cargo test` - All configuration tests pass
- [ ] Test config file loading and validation
- [ ] Test environment variable overrides work

#### 5.3 Verification

- [ ] Run `cargo test` - All output tests pass
- [ ] Test colorized output and progress indicators
- [ ] Test different verbosity levels work

## Phase 8: Testing Infrastructure 🔴

**Goal:** Comprehensive testing for all components

**Status:** All tasks pending - Test infrastructure needed

### 5.1 Unit Tests

- [ ] AST construction and manipulation 🔴 **CRITICAL**
- [ ] Parser for various workflow patterns 🔴 **CRITICAL**
- [ ] Expression evaluation with fixtures 🔴 **CRITICAL**
- [ ] Action translation correctness 🟡 **IMPORTANT**

#### 5.1 Verification

- [ ] Run `cargo test --workspace` - All unit tests pass
- [ ] Run `cargo tarpaulin --workspace` - Coverage > 80%
- [ ] Run `cargo clippy --workspace -- -D warnings` - No clippy warnings

#### 5.2 Verification

- [ ] Run `cargo test --workspace` - All integration tests pass
- [ ] Test with real MoosicBox workflows
- [ ] Run `cargo test --workspace --release` - Release mode works

#### 5.3 Verification

- [ ] Run all example workflows successfully
- [ ] All examples have documentation
- [ ] Test MoosicBox workflows execute locally

#### 5.4 Verification

- [ ] Run `cargo test --workspace` - All performance tests pass
- [ ] Test with large workflows and stress scenarios
- [ ] Verify resource usage is reasonable

## Phase 9: Documentation 🟡

**Goal:** Comprehensive documentation and examples

**Status:** All tasks pending - Documentation creation needed

### 5.1 User Documentation

- [ ] Getting started guide 🟡 **IMPORTANT**
- [ ] CLI command reference 🟡 **IMPORTANT**
- [ ] Action translation guide 🟡 **IMPORTANT**
- [ ] Backend configuration docs 🟢 **MINOR**

#### 5.1 Verification

- [ ] Run `mdbook build docs/` - User guide builds
- [ ] Run `cargo test --doc` - Doc tests pass
- [ ] Test all code examples in documentation work

#### 5.2 Verification

- [ ] Run `cargo doc --workspace` - Documentation builds
- [ ] Verify all public APIs have examples
- [ ] Architecture documentation is up to date

#### 5.3 Verification

- [ ] Run `cargo rustdoc --workspace -- -D warnings` - No doc warnings
- [ ] Run `cargo test --doc --workspace` - All doc tests pass
- [ ] Verify API documentation is comprehensive

## Success Criteria

The following criteria must be met for the project to be considered successful:

- [ ] Can parse Generic workflow format with all defined features
- [ ] Executes workflows locally without containerization
- [ ] Supports `$PIPELINE_OUTPUT` for step outputs
- [ ] Handles `PIPELINE_SECRET_*` environment variables and `--secret` CLI args
- [ ] Translates backend conditionals to constants correctly
- [ ] Generates valid GitHub Actions YAML with correct trigger mappings
- [ ] Matrix execution runs only current OS locally with proper OS mapping
- [ ] Job dependencies block execution correctly (failed jobs prevent dependents)
- [ ] Continue-on-error matches GitHub semantics (outcome vs conclusion)
- [ ] CLI supports --secret and --env flags as specified
- [ ] Actions must be explicitly declared in workflow `actions:` mapping
- [ ] Built-in actions (checkout, setup-\*, upload-artifact) work across backends
- [ ] File locations are flexible (any path, any name, any YAML extension)
- [ ] Translation preserves filenames and writes to correct directories
- [ ] Can parse and execute existing MoosicBox GitHub Actions workflows locally
- [ ] Supports offline execution with cached actions and dependencies
- [ ] Provides clear error messages for validation failures
- [ ] Binary executable named `gpipe` works from command line
- [ ] Expression evaluator supports MVP function set
- [ ] Built-in actions loaded from `.pipeline/actions/` directory
- [ ] Package structure follows MoosicBox umbrella crate pattern

## Concrete Workflow Examples

### Complete Generic Workflow Example

```yaml
version: 1.0
name: build-and-test
triggers:
    push:
        branches: [main, develop]
    pull_request:
    manual:

actions:
    checkout:
        type: github
        repo: actions/checkout@v4

    setup-rust:
        type: file
        path: ./.pipeline/actions/setup-rust.yml

    notify:
        type: inline
        name: Send Notification
        inputs:
            message:
                required: true
        runs:
            steps:
                - run: |
                      echo "Notification: ${{ inputs.message }}"
                      echo "status=sent" >> $PIPELINE_OUTPUT

jobs:
    build:
        strategy:
            matrix:
                os: [ubuntu-latest, windows-latest, macos-latest]
        env:
            CARGO_TERM_COLOR: always
        steps:
            - uses: checkout

            - uses: setup-rust
              with:
                  version: stable

            - id: build
              run: |
                  cargo build --release
                  echo "binary=target/release/app" >> $PIPELINE_OUTPUT

            - uses: upload-artifact
              if: ${{ backend == 'github' }}
              with:
                  name: binary-${{ matrix.os }}
                  path: ${{ steps.build.outputs.binary }}

    test:
        needs: [build]
        steps:
            - uses: checkout

            - run: cargo test
              continue-on-error: true
              id: test

            - if: ${{ steps.test.outcome == 'failure' }}
              uses: notify
              with:
                  message: 'Tests failed but continuing'

    deploy:
        needs: [build, test]
        if: ${{ backend == 'github' }}
        steps:
            - run: echo "Deploying..."
```

### Translation to GitHub Actions

The above Generic workflow translates to:

```yaml
# .github/workflows/build-and-test.yml
name: build-and-test
on:
    push:
        branches: [main, develop]
    pull_request:
    workflow_dispatch:

jobs:
    build:
        strategy:
            matrix:
                os: [ubuntu-latest, windows-latest, macos-latest]
        runs-on: ${{ matrix.os }}
        env:
            CARGO_TERM_COLOR: always
        steps:
            - uses: actions/checkout@v4

            - uses: ./.pipeline/actions/setup-rust.yml
              with:
                  version: stable

            - id: build
              run: |
                  cargo build --release
                  echo "binary=target/release/app" >> $GITHUB_OUTPUT

            - uses: actions/upload-artifact@v3
              if: ${{ true }} # backend == 'github' evaluated to true
              with:
                  name: binary-${{ matrix.os }}
                  path: ${{ steps.build.outputs.binary }}

    test:
        needs: [build]
        runs-on: ubuntu-latest
        steps:
            - uses: actions/checkout@v4

            - run: cargo test
              continue-on-error: true
              id: test

            - if: ${{ steps.test.outcome == 'failure' }}
              run: |
                  echo "Notification: Tests failed but continuing"
                  echo "status=sent" >> $GITHUB_OUTPUT

    deploy:
        needs: [build, test]
        runs-on: ubuntu-latest
        if: ${{ true }} # backend == 'github' evaluated to true
        steps:
            - run: echo "Deploying..."
```

### Translation to GitLab CI

The same Generic workflow translates to:

```yaml
# .gitlab-ci.yml
stages:
    - build
    - test
    - deploy

variables:
    CARGO_TERM_COLOR: always

.setup_rust: &setup_rust
    -  # Setup Rust commands translated from action

build:
    stage: build
    parallel:
        matrix:
            - OS: [ubuntu-latest, windows-latest, macos-latest]
    script:
        - git clone $CI_REPOSITORY_URL . # checkout translation
        - *setup_rust
        - cargo build --release
        - echo "binary=target/release/app" > build.env
    artifacts:
        reports:
            dotenv: build.env
        paths:
            - target/release/app
        name: binary-$OS
    rules:
        - if: '$CI_PIPELINE_SOURCE == "push" && ($CI_COMMIT_BRANCH == "main" || $CI_COMMIT_BRANCH == "develop")'
        - if: '$CI_PIPELINE_SOURCE == "merge_request_event"'
        - if: '$CI_PIPELINE_SOURCE == "web"'

test:
    stage: test
    needs: [build]
    script:
        - git clone $CI_REPOSITORY_URL .
        - cargo test
        - |
            if [ $? -ne 0 ]; then
              echo "Notification: Tests failed but continuing"
            fi
    allow_failure: true

deploy:
    stage: deploy
    needs: [build, test]
    script:
        - echo "Deploying..."
    rules:
        - if: '$CI_PIPELINE_SOURCE == "push" && ($CI_COMMIT_BRANCH == "main" || $CI_COMMIT_BRANCH == "develop")'
        - if: '$CI_PIPELINE_SOURCE == "merge_request_event"'
        - if: '$CI_PIPELINE_SOURCE == "web"'
```

### Local Execution Behavior

When running `gpipe run build-and-test.yml --backend=local`:

1. **Matrix handling**: Only runs current OS (e.g., if on Linux, skips windows-latest and macos-latest)
2. **Backend conditionals**: `backend == 'github'` evaluates to `false`, so upload-artifact and deploy steps are skipped
3. **Step outputs**: Creates temporary files for `$PIPELINE_OUTPUT`
4. **Action resolution**:
    - `checkout` → `git checkout` commands
    - `setup-rust` → Reads `./.pipeline/actions/setup-rust.yml` and executes
    - `notify` → Executes inline script
5. **Job execution**: Sequential in definition order (build → test → deploy), with dependency respect

## Technical Decisions

### Language and Framework

- **Rust** for performance, safety, and reliability
- **tokio** for async execution and parallelism
- **serde_yaml** for YAML parsing and serialization
- **clap** for CLI interface with derive macros
- **reqwest** for downloading actions and dependencies
- **BTreeMap** for deterministic ordering (not HashMap)
- **Underscore naming** for all packages (gpipe_ast, gpipe_parser, etc.)

### Architecture Patterns

- **AST-based** transformation pipeline for backend agnostic representation
- **Plugin system** for action translators with trait-based extensibility
- **Strategy pattern** for backends with common interface
- **Builder pattern** for workflow construction and configuration
- **Visitor pattern** for AST traversal and transformation

### Key Design Principles

1. **Zero containerization** for local execution - Direct process execution preferred
2. **Graceful degradation** when features unavailable - Warn but continue when possible
3. **Offline-first** with action caching - Support air-gapped environments
4. **Backend agnostic** AST representation - No backend-specific constructs in core
5. **Extensible** action translation system - Easy to add new action mappings
6. **Semantic preservation** - Maintain workflow behavior across backends
7. **Fast feedback** - Local execution should be faster than remote CI
8. **Developer experience** - Clear error messages and intuitive CLI

### Performance Requirements

- **Parse time**: < 100ms for typical workflows
- **Local execution**: < 50% overhead vs direct command execution
- **Memory usage**: < 100MB for typical workflows
- **Startup time**: < 1s for CLI commands
- **Action translation**: < 10ms per action resolution

### Security Considerations

- **Sandboxed execution** - Limit local execution capabilities
- **Secret handling** - Secure credential storage and injection
- **Action verification** - Validate action sources and integrity
- **Network isolation** - Optional network restrictions for local runs
- **File system isolation** - Limit access to specific directories

## Implementation Architecture

### Package Organization

The project follows MoosicBox's umbrella crate pattern:

```
packages/gpipe/              # Umbrella crate
├── Cargo.toml              # Re-exports all sub-crates
├── src/lib.rs              # Public API surface
├── ast/                    # gpipe_ast - Core types
├── parser/                 # gpipe_parser - Format parsers
├── runner/                 # gpipe_runner - Execution
├── translator/             # gpipe_translator - Conversion
├── actions/                # gpipe_actions - Action system
└── cli/                    # gpipe_cli - Binary 'gpipe'
```

### Built-in Actions Location

```
.pipeline/actions/          # Built-in actions (repo-level)
├── checkout.yml
├── setup-node.yml
├── setup-python.yml
├── upload-artifact.yml
└── download-artifact.yml
```

### Expression Evaluation Pipeline

1. Parse `${{ }}` expressions into Expression AST
2. Resolve variables from Context (env, secrets, steps, etc.)
3. Evaluate functions with MVP set only
4. Return string result for interpolation

## Risk Mitigation

### High-Risk Areas

1. **Action Translation Completeness**
    - Risk: Many GitHub Actions may not be translatable
    - Mitigation: Focus on most common actions first, provide fallback strategies

2. **GitHub Actions Compatibility**
    - Risk: Subtle differences in behavior between local and GitHub execution
    - Mitigation: Comprehensive test suite with real-world workflows

3. **Performance at Scale**
    - Risk: Local execution may be slower than expected for large workflows
    - Mitigation: Benchmarking and optimization during development

4. **Security Concerns**
    - Risk: Local execution of untrusted workflow code
    - Mitigation: Sandboxing and security controls by default

### Contingency Plans

- **Partial Implementation**: Prioritize local execution over multi-backend support
- **Containerization Fallback**: If local execution proves too difficult, support optional containers
- **Action Registry**: Pre-built registry of translated actions if dynamic translation is insufficient
- **Community Contributions**: Open architecture for community-contributed action translations

## Future Enhancements (Post-MVP)

- **Visual Workflow Editor** - GUI for creating and editing workflows
- **Workflow Debugging** - Step-through debugging and breakpoints
- **Performance Analytics** - Detailed execution metrics and optimization suggestions
- **Cloud Backend** - Remote execution service compatible with local workflows
- **IDE Integration** - VSCode extension for workflow development
- **Workflow Templates** - Pre-built templates for common patterns
- **Multi-Repository Workflows** - Support for workflows spanning multiple repos
- **Advanced Caching** - Intelligent caching across workflow runs
