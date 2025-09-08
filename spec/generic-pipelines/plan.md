# Generic Pipelines - Execution Plan

## Executive Summary

Create a universal CI/CD workflow tool that can execute and translate between different workflow formats, including a new generic workflow format that is backend-agnostic. The tool introduces a generic workflow format that allows users to write workflows once and run them on any supported backend (GitHub Actions, GitLab CI, local execution, etc.). Backend-specific functionality is supported through conditional execution blocks. The tool should handle workflow translation on-the-fly or through persisted generic representations, with first-class support for local execution without containerization.

**Current Status:** 🔴 **Not Started** - Specification phase

**Completion Estimate:** 0% complete - Architecture and design pending

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

### Generic Format Questions

- How should generic action definitions be distributed and versioned?
- Should we support a central registry for generic actions?
- How do we handle generic action dependencies?
- Where should action definition search paths be configured?

### Local Execution

- How to handle environment setup without containers?
- Should we support parallel job execution locally?
- How to manage artifact storage for local runs?
- What's the strategy for network-dependent actions?

### Backend Translation

- Should actions be translated at runtime or pre-processed?
- How to handle composite actions vs JavaScript/Docker actions?
- What's the fallback strategy when translation isn't possible?

## Phase 1: Generic Workflow Format Definition 🔴

**Goal:** Define the platform-agnostic workflow format that serves as the primary input format

**Status:** All tasks pending - Core format design needed

### 1.1 Generic Workflow Syntax

- [ ] Define generic workflow YAML schema 🔴 **CRITICAL**
- [ ] Support backend conditional blocks using `if: ${{ backend == 'name' }}` 🔴 **CRITICAL**
- [ ] Define standard workflow structure (name, triggers, jobs, steps) 🔴 **CRITICAL**
- [ ] Implement GitHub Actions compatible expression syntax 🔴 **CRITICAL**
- [ ] Support environment variables and contexts 🟡 **IMPORTANT**

#### 1.1 Verification

- [ ] Run `cargo build -p pipeline_parser` - Package compiles
- [ ] Run `cargo test -p pipeline_parser` - All tests pass
- [ ] Run `cargo clippy -p pipeline_parser -- -D warnings` - No clippy warnings
- [ ] Test parsing generic workflow format with backend conditionals

### 1.2 Generic Action System

- [ ] Define generic action definition format (YAML-based) 🔴 **CRITICAL**
- [ ] Create action resolution system with search paths 🔴 **CRITICAL**
- [ ] Support custom user-defined actions 🔴 **CRITICAL**
- [ ] Implement action input/output specifications 🟡 **IMPORTANT**
- [ ] Define action translation format for each backend 🔴 **CRITICAL**

#### 1.2 Verification

- [ ] Run `cargo build -p pipeline_actions` - Package compiles
- [ ] Run `cargo test -p pipeline_actions` - All tests pass
- [ ] Run `cargo clippy -p pipeline_actions -- -D warnings` - No clippy warnings
- [ ] Test loading and resolving generic action definitions

### 1.3 Backend Context System

- [ ] Define `backend` context variable 🔴 **CRITICAL**
- [ ] Implement backend-specific step skipping logic 🔴 **CRITICAL**
- [ ] Create clear error messages for incompatible backends 🟡 **IMPORTANT**
- [ ] Support backend detection for conditional execution 🔴 **CRITICAL**
- [ ] Define supported backend identifiers (github, gitlab, local, etc.) 🔴 **CRITICAL**

#### 1.3 Verification

- [ ] Run `cargo build -p pipeline_runner` - Package compiles
- [ ] Run `cargo test -p pipeline_runner` - All tests pass
- [ ] Run `cargo clippy -p pipeline_runner -- -D warnings` - No clippy warnings
- [ ] Test backend conditional execution and skipping

## Phase 2: Core AST and Workflow Model 🔴

**Goal:** Define the abstract syntax tree for representing ALL workflow formats (Generic, GitHub Actions, GitLab CI, etc.) in a unified internal structure

**Status:** All tasks pending - Core workflow model design needed

### 2.1 AST Definition

- [ ] Define workflow node types (Job, Step, Action, Script, Conditional) 🔴 **CRITICAL**
- [ ] Create expression language for conditions and variables 🔴 **CRITICAL**
- [ ] Define matrix strategy representation 🔴 **CRITICAL**
- [ ] Support for artifacts and caching abstractions 🟡 **IMPORTANT**
- [ ] Define secret/credential abstraction 🔴 **CRITICAL**

#### 2.1 Verification

- [ ] Run `cargo build -p pipeline_ast --all-features` - Verify compilation
- [ ] Run `cargo test -p pipeline_ast` - All tests pass
- [ ] Run `cargo clippy -p pipeline_ast -- -D warnings` - No clippy warnings
- [ ] Run `cargo fmt -- --check` - Code properly formatted
- [ ] Run `cargo doc -p pipeline_ast --no-deps` - Documentation builds

### 2.2 Workflow Model

- [ ] Create Workflow struct with metadata and jobs 🔴 **CRITICAL**
- [ ] Define Job struct with dependencies and steps 🔴 **CRITICAL**
- [ ] Create Step variants (uses action, run script, conditional) 🔴 **CRITICAL**
- [ ] Implement Context struct for variables and state 🔴 **CRITICAL**
- [ ] Add support for job outputs and step outputs 🟡 **IMPORTANT**

#### 2.2 Verification

- [ ] Run `cargo build -p pipeline_ast` - Package compiles
- [ ] Run `cargo test -p pipeline_ast` - All tests pass
- [ ] Run `cargo clippy -p pipeline_ast -- -D warnings` - No clippy warnings
- [ ] Run `cargo tarpaulin -p pipeline_ast` - Code coverage > 80%

### 2.3 Package Structure

- [ ] Create packages/pipeline_ast package with Rust structures 🔴 **CRITICAL**
- [ ] Implement Serialize/Deserialize for all AST nodes 🔴 **CRITICAL**
- [ ] Add validation methods for workflow correctness 🟡 **IMPORTANT**
- [ ] Include builder patterns for ergonomic construction 🟡 **IMPORTANT**

#### 2.3 Verification

- [ ] Run `cargo build -p pipeline_ast` - Package compiles
- [ ] Run `cargo test -p pipeline_ast` - All tests pass
- [ ] Run `cargo clippy -p pipeline_ast -- -D warnings` - No clippy warnings
- [ ] Run `cargo fmt -- --check` - Code properly formatted
- [ ] Run `cargo doc -p pipeline_ast` - Documentation builds
    - [ ] Run `cargo test -p pipeline_ast --test builder_patterns_test` - Builder tests pass
    - [ ] Run `cargo doc -p pipeline_ast` - Builder APIs documented

## Phase 3: Workflow Parsers 🔴

**Goal:** Parse all workflow formats (Generic, GitHub Actions, GitLab CI, etc.) into the AST

**Status:** All tasks pending - Parser implementation needed

### 5.1 Multi-Format Parser

- [ ] Parse Generic workflow format (primary format) 🔴 **CRITICAL**
- [ ] Parse GitHub Actions workflow format 🔴 **CRITICAL**
- [ ] Parse GitLab CI workflow format 🟡 **IMPORTANT**
- [ ] Parse workflow triggers (on: push, pull_request, etc.) 🔴 **CRITICAL**
- [ ] Parse job definitions with needs dependencies 🔴 **CRITICAL**
- [ ] Parse steps with uses/run/if conditions including backend conditionals 🔴 **CRITICAL**
- [ ] Handle with parameters and env variables 🔴 **CRITICAL**
- [ ] Parse matrix strategies and includes/excludes 🟡 **IMPORTANT**

#### 5.1 Verification

- [ ] Run `cargo build -p pipeline_parser` - Package compiles
- [ ] Run `cargo test -p pipeline_parser` - All tests pass
- [ ] Run `cargo clippy -p pipeline_parser -- -D warnings` - No clippy warnings
- [ ] Test parsing MoosicBox workflows successfully

### 5.2 Expression Evaluator

- [ ] Implement GitHub Actions expression syntax parser 🔴 **CRITICAL**
- [ ] Support context variables (github, env, secrets, etc.) 🔴 **CRITICAL**
- [ ] Implement built-in functions (contains, startsWith, etc.) 🔴 **CRITICAL**
- [ ] Handle string interpolation and type coercion 🟡 **IMPORTANT**

#### 5.2 Verification

- [ ] Run `cargo build -p pipeline_parser` - Package compiles
- [ ] Run `cargo test -p pipeline_parser` - All tests pass
- [ ] Run `cargo clippy -p pipeline_parser -- -D warnings` - No clippy warnings
- [ ] Test expression evaluation matches GitHub Actions behavior

### 5.3 Workflow Validation

- [ ] Validate job dependency cycles 🔴 **CRITICAL**
- [ ] Check for undefined variables and references 🟡 **IMPORTANT**
- [ ] Validate action references and parameters 🟡 **IMPORTANT**
- [ ] Provide detailed error messages with line numbers 🟡 **IMPORTANT**

#### 5.3 Verification

- [ ] Run `cargo build -p pipeline_parser` - Package compiles
- [ ] Run `cargo test -p pipeline_parser` - All tests pass
- [ ] Run `cargo clippy -p pipeline_parser -- -D warnings` - No clippy warnings
- [ ] Test validation provides clear error messages

## Phase 4: Local Runner Implementation 🔴

**Goal:** Execute workflows locally without containers

**Status:** All tasks pending - Execution engine needed

### 5.1 Execution Engine

- [ ] Create LocalRunner struct implementing WorkflowBackend trait 🔴 **CRITICAL**
- [ ] Implement job scheduler with dependency resolution 🔴 **CRITICAL**
- [ ] Execute shell commands via std::process::Command 🔴 **CRITICAL**
- [ ] Manage working directories and environment variables 🔴 **CRITICAL**
- [ ] Handle step conditions and continue-on-error 🟡 **IMPORTANT**

#### 5.1 Verification

- [ ] Run `cargo build -p pipeline_runner` - Package compiles
- [ ] Run `cargo test -p pipeline_runner` - All tests pass
- [ ] Run `cargo clippy -p pipeline_runner -- -D warnings` - No clippy warnings
- [ ] Test basic workflow execution works locally

### 5.2 Environment Management

- [ ] Detect and validate local tool availability 🔴 **CRITICAL**
- [ ] Set up PATH and environment variables 🔴 **CRITICAL**
- [ ] Create temporary directories for artifacts 🟡 **IMPORTANT**
- [ ] Implement artifact upload/download locally 🟡 **IMPORTANT**
- [ ] Handle cache storage and retrieval 🟢 **MINOR**

#### 5.2 Verification

- [ ] Run `cargo build -p pipeline_runner` - Package compiles
- [ ] Run `cargo test -p pipeline_runner` - All tests pass
- [ ] Run `cargo clippy -p pipeline_runner -- -D warnings` - No clippy warnings
- [ ] Test environment isolation between jobs

#### 5.3 Verification

- [ ] Run `cargo build -p pipeline_runner` - Package compiles
- [ ] Run `cargo test -p pipeline_runner` - All tests pass
- [ ] Run `cargo clippy -p pipeline_runner -- -D warnings` - No clippy warnings
- [ ] Test parallel job execution works

#### 5.4 Verification

- [ ] Run `cargo build -p pipeline_runner` - Package compiles
- [ ] Run `cargo test -p pipeline_runner` - All tests pass
- [ ] Run `cargo clippy -p pipeline_runner -- -D warnings` - No clippy warnings
- [ ] Test error handling and recovery mechanisms

## Phase 5: Action Translation System 🔴

**Goal:** Translate all action types (Generic actions, GitHub Actions, GitLab CI actions) to executable commands for local execution and backend generation

**Status:** All tasks pending - Action registry and translation needed

### 5.1 Action Registry

- [ ] Define ActionTranslator trait 🔴 **CRITICAL**
- [ ] Create registry for known action translations 🔴 **CRITICAL**
- [ ] Implement translations for common actions:
    - [ ] actions/checkout → git commands 🔴 **CRITICAL**
    - [ ] actions/setup-\* → tool installation commands 🔴 **CRITICAL**
    - [ ] actions/cache → local cache operations 🟡 **IMPORTANT**
    - [ ] actions/upload-artifact → local file operations 🟡 **IMPORTANT**

#### 5.1 Verification

- [ ] Run `cargo build -p pipeline_actions` - Package compiles
- [ ] Run `cargo test -p pipeline_actions` - All tests pass
- [ ] Run `cargo clippy -p pipeline_actions -- -D warnings` - No clippy warnings
- [ ] Test common actions (checkout, setup-\*, cache) translate correctly

### 5.2 JavaScript Action Support

- [ ] Parse action.yml metadata files 🟡 **IMPORTANT**
- [ ] Extract and execute Node.js action sources 🟡 **IMPORTANT**
- [ ] Set up action inputs/outputs via environment 🟡 **IMPORTANT**
- [ ] Handle pre/post scripts for actions 🟢 **MINOR**

#### 5.2 Verification

- [ ] Run `cargo build -p pipeline_actions` - Package compiles
- [ ] Run `cargo test -p pipeline_actions` - All tests pass
- [ ] Run `cargo clippy -p pipeline_actions -- -D warnings` - No clippy warnings
- [ ] Test JavaScript action execution works

#### 5.3 Verification

- [ ] Run `cargo build -p pipeline_actions` - Package compiles
- [ ] Run `cargo test -p pipeline_actions` - All tests pass
- [ ] Run `cargo clippy -p pipeline_actions -- -D warnings` - No clippy warnings
- [ ] Test fallback strategies work for untranslatable actions

#### 5.4 Verification

- [ ] Run `cargo build -p pipeline_actions` - Package compiles
- [ ] Run `cargo test -p pipeline_actions` - All tests pass
- [ ] Run `cargo clippy -p pipeline_actions -- -D warnings` - No clippy warnings
- [ ] Test action metadata system works

## Phase 6: Backend Abstraction Layer 🟡

**Goal:** Support multiple CI/CD backends

**Status:** All tasks pending - Backend trait design needed

### 5.1 Backend Trait

- [ ] Define WorkflowBackend trait with execute methods 🔴 **CRITICAL**
- [ ] Add capability queries (supports_parallelism, supports_artifacts) 🟡 **IMPORTANT**
- [ ] Implement backend-specific configuration 🟡 **IMPORTANT**
- [ ] Handle backend limitations and workarounds 🟡 **IMPORTANT**

#### 5.1 Verification

- [ ] Run `cargo build -p pipeline_backends` - Package compiles
- [ ] Run `cargo test -p pipeline_backends` - All tests pass
- [ ] Run `cargo clippy -p pipeline_backends -- -D warnings` - No clippy warnings
- [ ] Test backend trait implementations work

#### 5.2 Verification

- [ ] Run `cargo build -p pipeline_backends` - Package compiles
- [ ] Run `cargo test -p pipeline_backends` - All tests pass
- [ ] Run `cargo clippy -p pipeline_backends -- -D warnings` - No clippy warnings
- [ ] Test GitHub Actions YAML generation works

#### 5.3 Verification

- [ ] Run `cargo build -p pipeline_backends` - Package compiles
- [ ] Run `cargo test -p pipeline_backends` - All tests pass
- [ ] Run `cargo clippy -p pipeline_backends -- -D warnings` - No clippy warnings
- [ ] Test GitLab CI YAML generation works

#### 5.4 Verification

- [ ] Run `cargo build -p pipeline_backends` - Package compiles
- [ ] Run `cargo test -p pipeline_backends` - All tests pass
- [ ] Run `cargo clippy -p pipeline_backends -- -D warnings` - No clippy warnings
- [ ] Test Jenkins pipeline generation works

## Phase 7: CLI Interface 🟡

**Goal:** User-friendly command-line interface

**Status:** All tasks pending - CLI implementation needed

### 5.1 Core Commands

- [ ] `run` - Execute workflow locally 🔴 **CRITICAL**
- [ ] `translate` - Convert between formats 🟡 **IMPORTANT**
- [ ] `validate` - Check workflow syntax 🟡 **IMPORTANT**
- [ ] `dry-run` - Show execution plan 🟡 **IMPORTANT**
- [ ] `cache-action` - Pre-download action for offline use 🟢 **MINOR**

#### 5.1 Verification

- [ ] Run `cargo build --bin ci-runner` - Binary builds
- [ ] Run `cargo test --bin ci-runner` - CLI tests pass
- [ ] Run `cargo clippy --bin ci-runner -- -D warnings` - No warnings
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

- [ ] Can parse and execute MoosicBox GitHub Actions workflows locally
- [ ] No containerization required for basic workflows (checkout, build, test)
- [ ] Supports common GitHub Actions (checkout, setup-\*, cache, upload-artifact)
- [ ] Can translate workflows to GitLab CI format with functional equivalence
- [ ] Provides clear error messages for unsupported features
- [ ] Executes faster locally than GitHub Actions for simple workflows
- [ ] Supports offline execution with cached actions and dependencies
- [ ] Handles matrix strategies and job dependencies correctly
- [ ] Maintains workflow semantics across different backends
- [ ] Provides comprehensive CLI with intuitive commands

## Technical Decisions

### Language and Framework

- **Rust** for performance, safety, and reliability
- **tokio** for async execution and parallelism
- **serde_yaml** for YAML parsing and serialization
- **clap** for CLI interface with derive macros
- **reqwest** for downloading actions and dependencies

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
