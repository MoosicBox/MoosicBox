# Generic Schema Migrations - Execution Plan

## Executive Summary

Extract the generic migration logic from `moosicbox_schema` into a reusable `switchy_schema` package that any project can use for database schema evolution. This provides a foundation for HyperChad and other projects to manage their database schemas independently while maintaining full compatibility with existing MoosicBox code.

**Current Status:** 🟡 **Implementation Phase** - Phase 1 complete, Phase 2 mostly complete

**Completion Estimate:** ~15% complete - Core foundation and traits implemented

## Status Legend

- 🔴 **Critical** - Blocks core functionality
- 🟡 **Important** - Affects user experience or API design
- 🟢 **Minor** - Nice-to-have or polish items
- ✅ **Complete** - Fully implemented and validated
- 🟡 **In Progress** - Currently being worked on
- ❌ **Blocked** - Waiting on dependencies or design decisions

## Open Questions

These items need further investigation or decision during implementation:

### Database-Specific SQL Handling
- How should we handle SQL differences between databases (postgres/sqlite/mysql)?
- Should we enforce database-specific subdirectories like moosicbox does?

### Migration Ordering
- Migration ordering for identical timestamps (currently undefined behavior)

### Error Recovery & Partial Migration State
- What happens if a migration fails halfway through?
- How to handle partially applied migrations?
- Should we support "dirty" state detection?
- Recovery mechanisms for corrupted migration state

### Concurrent Migration Protection
- How to prevent multiple processes from running migrations simultaneously?
- Lock mechanism (database locks, file locks, etc.)?
- Timeout handling for stuck migrations?

## Phase 1: Package Creation and Setup ✅ **COMPLETED**

**Goal:** Create the switchy_schema package and integrate it into the workspace

**Status:** All tasks completed successfully. Package builds and integrates with workspace.

### 1.1 Package Creation

- [x] Create package directory structure ✅ **CRITICAL**
  - [x] Create `packages/switchy/schema/` directory
    - ✓ Created at packages/switchy/schema/
  - [x] Create `packages/switchy/schema/src/` directory
    - ✓ Created at packages/switchy/schema/src/
  - [x] Create `packages/switchy/schema/src/lib.rs` with initial module structure
    - ✓ Created with modules, error types, and re-exports (37 lines)
  - [x] Create `packages/switchy/schema/Cargo.toml` with package metadata
    - ✓ Created with name="switchy_schema", dependencies, and features

### 1.2 Workspace Integration

- [x] Update root `Cargo.toml` ✅ **CRITICAL**
  - [x] Add `packages/switchy/schema` to workspace members
    - ✓ Added at line 115 in root Cargo.toml
  - [x] Add `switchy_schema` to workspace dependencies section
    - ✓ Added at line 270 in root Cargo.toml
  - [x] Define version as `{ path = "packages/switchy/schema" }`
    - ✓ Defined with version 0.1.0 and correct path

### 1.3 Initial Module Structure

- [x] Create placeholder module files ✅ **CRITICAL**
  - [x] Create empty `src/migration.rs`
    - ✓ Created with Migration and MigrationSource traits (31 lines)
  - [x] Create empty `src/runner.rs`
    - ✓ Created with MigrationRunner struct (16 lines)
  - [x] Create `src/discovery/mod.rs`
    - ✓ Created at src/discovery/mod.rs (3 lines)
  - [x] Create empty `src/version.rs`
    - ✓ Created with VersionTracker struct (25 lines)
  - [x] Wire up modules in `src/lib.rs`
    - ✓ All modules declared and public in lib.rs

### 1.4 Build Verification

- [x] Verify package builds ✅ **CRITICAL**
  - [x] Run `cargo build -p switchy_schema`
    - ✓ Builds successfully with nix-shell
  - [x] Ensure no compilation errors
    - ✓ Only 1 warning for unused field
  - [x] Verify workspace recognizes the new package
    - ✓ Appears in cargo metadata and cargo tree

## Phase 2: Core Migration Types ✅ **MOSTLY COMPLETED**

**Goal:** Define fundamental types and traits for the migration system

**Status:** ~95% complete. All core traits and error types implemented. Only missing detailed error context.

### 2.1 Migration Trait Definition

- [x] `packages/switchy/schema/src/migration.rs` - Core migration trait ✅ **CRITICAL**
  - [x] Define `Migration` trait with `id()`, `up()`, `down()` methods
    - ✓ Defined in src/migration.rs lines 6-26
  - [x] down() has default empty Ok(()) implementation
    - ✓ Lines 11-13: returns Ok(())
  - [x] Add optional `description()`, `depends_on()`, `supported_databases()`
    - ✓ Lines 15-25 with default implementations
  - [x] Use async-trait for database operations
    - ✓ Line 5: #[async_trait] on trait
  - [x] Support both SQL and code-based migrations
    - ✓ Trait-based design allows any implementation

### 2.2 Error Types

- [x] `packages/switchy/schema/src/lib.rs` - Error handling ✅ **CRITICAL**
  - [x] Define `MigrationError` enum with database, validation, dependency errors
    - ✓ Lines 19-35 in lib.rs with 5 error variants
  - [x] Use thiserror for comprehensive error messages
    - ✓ Line 19: #[derive(Debug, Error)] with error messages
  - [ ] Include context for debugging (migration ID, SQL, etc.)
    - ✗ Not implemented - only basic error messages

### 2.3 Migration Source Trait

- [x] `packages/switchy/schema/src/migration.rs` - Source trait ✅ **CRITICAL**
  - [x] Define `MigrationSource` trait
    - ✓ Lines 28-31 in src/migration.rs
  - [x] async fn migrations() -> Result<Vec<Box<dyn Migration>>, MigrationError>
    - ✓ Line 30: exact signature implemented
  - [x] Return migration collections
    - ✓ Returns Vec<Box<dyn Migration>>
  - [x] Handle source-specific errors
    - ✓ Returns Result type for error handling

### 2.4 Migration Error Types

- [x] `packages/switchy/schema/src/lib.rs` - Unified error handling ✅ **CRITICAL**
  - [x] Define `MigrationError` with thiserror
    - ✓ Same as 2.2 - lines 19-35 in lib.rs
  - [x] Cases for database errors (#[from] DatabaseError)
    - ✓ Line 22: Database(#[from] DatabaseError)
  - [x] Cases for discovery errors
    - ✓ Line 25: Discovery(String)
  - [x] Cases for validation errors
    - ✓ Line 28: Validation(String)
  - [x] Use async-trait for Migration trait
    - ✓ Applied in src/migration.rs line 5

### 2.5 Package Configuration

- [x] `packages/switchy/schema/Cargo.toml` - Package setup ✅ **CRITICAL**
  - [x] Package name: `switchy_schema`
    - ✓ Line 8 in Cargo.toml: name = "switchy_schema"
  - [x] Dependencies: switchy_database, async-trait, thiserror, include_dir (optional)
    - ✓ Lines 17-21: all required dependencies present
  - [x] Features: embedded, directory, code, validation, test-utils
    - ✓ Lines 26-32: all features defined
  - [x] Default features: embedded
    - ✓ Line 24: default = ["embedded"]

## Phase 3: Migration Discovery

**Goal:** Implement migration discovery from various sources with feature-gated modules

**Status:** 0% complete. Only empty trait placeholder exists.

### 3.1 Common Discovery Interface

- [ ] `packages/switchy/schema/src/discovery/mod.rs` - Common types ❌ **CRITICAL**
  - [ ] Define `DiscoverySource` trait
    - ✗ Only empty trait placeholder - no methods defined (lines 1-3)
  - [ ] Common discovery errors
    - ✗ Not implemented
  - [ ] Migration collection types
    - ✗ Not implemented
  - [ ] Shared utility functions
    - ✗ Not implemented

### 3.2 File-Based Discovery (feature = "directory")

- [ ] `packages/switchy/schema/src/discovery/directory.rs` - Directory discovery ❌ **CRITICAL**
  - [ ] Feature-gated with `#[cfg(feature = "directory")]`
  - [ ] Implements `MigrationSource` trait with async migrations() method
  - [ ] Provide `DirectoryMigrations::from_path()` or similar explicit API
  - [ ] Scan directories for migration files in format: `YYYY-MM-DD-HHMMSS_name/up.sql`
  - [ ] down.sql is optional, metadata.toml is allowed
  - [ ] Empty migration files are treated as successful no-ops
  - [ ] Handle database-specific subdirectories

### 3.3 Embedded Discovery (feature = "embedded")

- [ ] `packages/switchy/schema/src/discovery/embedded.rs` - Embedded discovery ❌ **CRITICAL**
  - [ ] Feature-gated with `#[cfg(feature = "embedded")]`
  - [ ] Implements `MigrationSource` trait with async migrations() method
  - [ ] Provide `EmbeddedMigrations::new()` or similar explicit API
  - [ ] Extract migrations from include_dir structures
  - [ ] Maintain compatibility with existing moosicbox patterns
  - [ ] Support nested directory structures
  - [ ] Parse migration names and ordering

### 3.4 Code-Based Discovery (feature = "code")

- [ ] `packages/switchy/schema/src/discovery/code.rs` - Code discovery ❌ **IMPORTANT**
  - [ ] Feature-gated with `#[cfg(feature = "code")]`
  - [ ] Implements `MigrationSource` trait with async migrations() method
  - [ ] Provide explicit API for code-based migrations
  - [ ] Registry for programmatically defined migrations
  - [ ] Type-safe migration definitions
  - [ ] Integration with trait-based migrations

### 3.5 Package Compilation

- [ ] Ensure clean compilation ❌ **CRITICAL**
  - [ ] Package must compile without warnings when no discovery features are enabled
  - [ ] Core types and traits are always available
  - [ ] Discovery implementations are feature-gated additions

## Phase 4: Migration Runner

**Goal:** Core execution engine for running migrations

**Status:** 0% complete. Only empty struct placeholders exist.

### 4.1 Runner Implementation

- [ ] `packages/switchy/schema/src/runner.rs` - Migration runner ❌ **CRITICAL**
  - [ ] Create `MigrationRunner` struct with configurable options
    - ✗ Only empty struct - no fields or configuration (lines 1-16)
  - [ ] Provide specific constructors: new_embedded(), new_directory(), new_code()
    - ✗ Only basic new() constructor - no specialized constructors
  - [ ] Support different execution strategies (All, UpTo, Steps, DryRun)
    - ✗ Not implemented
  - [ ] Use BTreeMap for deterministic ordering (like moosicbox_schema)
    - ✗ Not implemented
  - [ ] Follow moosicbox pattern: query tracking table for each migration individually
    - ✗ Not implemented
  - [ ] If migration not found in table → execute and record it
    - ✗ Not implemented
  - [ ] If migration found in table → skip (already ran)
    - ✗ Not implemented
  - [ ] SQL execution via `exec_raw` - no validation or parsing needed
    - ✗ Not implemented
  - [ ] Implement transaction management (per-migration or batch)
    - ✗ Not implemented
  - [ ] NOTE: Verify switchy_database transaction support at implementation time
    - ✗ Not verified
  - [ ] Add migration hooks (before/after/error callbacks)
    - ✗ Not implemented

### 4.2 Version Tracking

- [ ] `packages/switchy/schema/src/version.rs` - Version management ❌ **CRITICAL**
  - [ ] Create standard migrations tracking table (default: `__switchy_migrations`)
    - ✗ Only constant defined - no table creation logic (line 1)
  - [ ] Exact schema matching moosicbox: name (Text, NOT NULL), run_on (DateTime, NOT NULL, DEFAULT NOW)
    - ✗ No schema definition - only struct with table_name field
  - [ ] Support configurable table names
    - ✗ Struct has table_name field but no functionality (lines 3-25)
  - [ ] Handle rollback tracking
    - ✗ Not implemented

### 4.3 Dependency Resolution

- [ ] `packages/switchy/schema/src/runner.rs` - Dependency handling ❌ **IMPORTANT**
  - [ ] Topological sort for migration ordering
  - [ ] Validate dependency cycles
  - [ ] Support conditional dependencies
  - [ ] Clear error messages for missing dependencies

## Phase 5: Rollback Support

**Goal:** Safe rollback functionality with comprehensive validation

### 5.1 Rollback Engine

- [ ] `packages/switchy/schema/src/rollback.rs` - Rollback implementation ❌ **IMPORTANT**
  - [ ] Implement rollback by N steps
  - [ ] Validate down() methods exist before rollback
  - [ ] Update tracking table with rollback status
  - [ ] Support dry-run rollback validation

### 5.2 Rollback Validation

- [ ] `packages/switchy/schema/src/rollback.rs` - Rollback safety ❌ **IMPORTANT**
  - [ ] Verify rollback path exists for all migrations
  - [ ] Check for data loss warnings
  - [ ] Validate rollback order and dependencies
  - [ ] Provide rollback impact analysis

## Phase 6: Validation & Safety

**Goal:** Comprehensive validation to prevent migration issues

### 6.1 Migration Validator

- [ ] `packages/switchy/schema/src/validation.rs` - Validation engine ❌ **IMPORTANT**
  - [ ] Checksum validation for applied migrations
  - [ ] Dependency cycle detection
  - [ ] Migration naming convention validation
  - [ ] Validate migration sources are accessible

### 6.2 Dry Run Support

- [ ] `packages/switchy/schema/src/validation.rs` - Dry run ❌ **IMPORTANT**
  - [ ] Generate execution plan showing which migrations would run
  - [ ] Show migration order and dependencies
  - [ ] Display migration metadata (ID, description, etc.)
  - [ ] Validate migration sources are accessible

### 6.3 Safety Checks

- [ ] `packages/switchy/schema/src/validation.rs` - Safety features ❌ **IMPORTANT**
  - [ ] Prevent running migrations on production without confirmation
  - [ ] Backup recommendations before destructive operations
  - [ ] Lock file support to prevent concurrent migrations
  - [ ] Environment-specific migration controls

## Phase 7: moosicbox_schema Migration

**Goal:** Update existing moosicbox_schema to use switchy_schema

### 7.1 Wrapper Implementation

- [ ] `packages/schema/src/lib.rs` - Update moosicbox_schema ❌ **CRITICAL**
  - [ ] Replace direct migration logic with switchy_schema calls
  - [ ] Maintain existing public API unchanged
  - [ ] Use MigrationRunner with embedded sources
  - [ ] Keep existing function signatures and behavior

### 7.2 Migration Compatibility

- [ ] `packages/schema/src/lib.rs` - Ensure compatibility ❌ **CRITICAL**
  - [ ] Verify all existing migrations continue to work
  - [ ] Maintain migration table name compatibility
  - [ ] Preserve migration ordering and checksums
  - [ ] Test against existing databases
  - [ ] Add unit tests using in-memory SQLite similar to existing tests
  - [ ] Verify migrations run without clippy warnings

### 7.3 Feature Propagation

- [ ] `packages/schema/Cargo.toml` - Update dependencies ❌ **CRITICAL**
  - [ ] Add switchy_schema dependency
  - [ ] Propagate feature flags appropriately
  - [ ] Maintain existing feature compatibility
  - [ ] Update documentation

## Phase 8: Testing Infrastructure

**Goal:** Comprehensive testing utilities and coverage

### 8.1 Test Utilities

- [ ] `packages/switchy/schema/src/test_utils.rs` - Test helpers ❌ **IMPORTANT**
  - [ ] `TestDatabase` using switchy_database simulated/in-memory SQLite
  - [ ] `TestMigrationBuilder` for creating test migrations
  - [ ] Migration assertion helpers
  - [ ] Complex migration verification utilities (like test_api_sources_table_migration)
  - [ ] Support for testing data transformations during migrations

### 8.2 Integration Tests

- [ ] `packages/switchy/schema/tests/` - Integration tests ❌ **CRITICAL**
  - [ ] Test migration execution across all database types
  - [ ] Test rollback functionality
  - [ ] Test dependency resolution
  - [ ] Test error handling and recovery

### 8.3 Compatibility Tests

- [ ] `packages/schema/tests/` - Compatibility tests ❌ **CRITICAL**
  - [ ] Verify moosicbox_schema continues to work unchanged
  - [ ] Test migration state preservation
  - [ ] Test feature flag combinations
  - [ ] Performance regression tests

## Phase 9: Migration Listing

**Goal:** Provide ability to list available migrations

### 9.1 List Implementation

- [ ] Add `list()` method to migration sources ❌ **MINOR**
  - [ ] Returns list of available migrations
  - [ ] Include migration ID, description if available
  - [ ] Indicate which migrations have been applied
  - [ ] Sort by migration order

## Phase 10: Documentation & Examples

**Goal:** Comprehensive documentation and usage examples

### 10.1 API Documentation

- [ ] `packages/switchy/schema/src/lib.rs` - API docs ❌ **IMPORTANT**
  - [ ] Basic rustdoc for public APIs
  - [ ] Simple usage examples

### 10.2 Usage Examples

- [ ] `packages/switchy/schema/examples/` - Example applications ❌ **MINOR**
  - [ ] `basic_usage.rs` - Simple migration example
  - [ ] `hyperchad_integration.rs` - HyperChad-specific example

## Phase 11: Future Enhancements

**Goal:** Consider advanced features after core functionality is complete

### 11.1 CLI Integration

- [ ] CLI implementation ❌ **MINOR**
  - [ ] `create` - Generate new migration files
  - [ ] `status` - Show migration status and pending migrations
  - [ ] `migrate` - Run pending migrations
  - [ ] `rollback` - Rollback N migrations
  - [ ] Basic environment variable configuration
  - [ ] Database connection string handling

### 11.2 Error Recovery Investigation

- [ ] Research error recovery patterns ❌ **MINOR**
  - [ ] Investigate partial migration recovery strategies
  - [ ] Design "dirty" state detection
  - [ ] Document recovery best practices

### 11.3 Checksum Implementation

- [ ] Add checksum validation ❌ **MINOR**
  - [ ] Choose checksum algorithm (SHA256 recommended)
  - [ ] Implement checksum calculation for migrations
  - [ ] Add checksum verification before execution
  - [ ] Handle checksum mismatches gracefully

### 11.4 Remote Discovery Implementation

- [ ] Remote migration source ❌ **MINOR**
  - [ ] Implement `MigrationSource` trait for remote sources
  - [ ] Feature-gated with `#[cfg(feature = "remote")]`
  - [ ] Fetch migrations from remote sources
  - [ ] Authentication and caching support
  - [ ] Network error handling

### 11.5 Migration State Query API

- [ ] Query API for migration state ❌ **MINOR**
  - [ ] Check if specific migration is applied
  - [ ] Get list of pending migrations
  - [ ] Get migration history
  - [ ] Separate from MigrationRunner for focused API

## Success Metrics

- **Zero Breaking Changes**: moosicbox_schema continues to work unchanged
- **Database Agnostic**: Works with SQLite, PostgreSQL, MySQL via switchy_database
- **Type Safe**: Leverages Rust's type system for compile-time safety
- **Extensible**: Easy to add new migration sources and strategies
- **Well Tested**: >90% test coverage with integration tests
- **Functional**: Core functionality works correctly with basic tooling

## Technical Decisions

### Why Extract from moosicbox_schema?
- Enables independent schema management for HyperChad
- Creates reusable component for other projects
- Maintains existing functionality while adding flexibility
- Follows single responsibility principle

### Why Use Trait-Based Design?
- Enables both SQL and code-based migrations
- Provides type safety and compile-time validation
- Allows for extensible migration sources
- Integrates well with Rust's async ecosystem

### Why Support Multiple Sources?
- Embedded: Zero-config deployment with compile-time validation
- Directory: Development flexibility and runtime migration loading
- Code: Type-safe migrations with complex logic
- Remote: Future extensibility for distributed systems

### Migration Ordering Strategy
- Timestamp-based naming for deterministic ordering
- Dependency system for complex migration relationships
- Topological sort for dependency resolution
- Clear error messages for ordering conflicts

## Package Structure

```
packages/switchy/schema/
├── Cargo.toml
├── src/
│   ├── lib.rs           # Public API and re-exports
│   ├── migration.rs     # Migration trait and types
│   ├── runner.rs        # Migration runner and builder
│   ├── discovery/       # Migration discovery utilities
│   │   ├── mod.rs       # Common discovery traits and types
│   │   ├── embedded.rs  # Embedded discovery (feature = "embedded")
│   │   ├── directory.rs # Directory discovery (feature = "directory")
│   │   └── code.rs      # Code-based discovery (feature = "code")
│   ├── version.rs       # Version tracking and management
│   ├── rollback.rs      # Rollback functionality
│   ├── validation.rs    # Validation and safety checks
│   ├── cli.rs           # Optional CLI utilities
│   └── test_utils.rs    # Test helpers and utilities
├── tests/
│   ├── integration.rs   # Integration tests
│   ├── rollback.rs      # Rollback tests
│   └── compatibility.rs # Compatibility tests
├── examples/
│   ├── basic_usage.rs
│   └── hyperchad_integration.rs
└── migrations/          # Test migrations
    └── test_migrations/
```

## Dependencies

### Core Dependencies
```toml
[dependencies]
switchy_database = { workspace = true }
async-trait = { workspace = true }
thiserror = { workspace = true }
include_dir = { workspace = true, optional = true }
chrono = { workspace = true }


[features]
default = ["embedded"]
embedded = ["include_dir"]
directory = []
code = []
all-discovery = ["embedded", "directory", "code"]
validation = []
test-utils = []
```

## Execution Order

### Prerequisites
- switchy_database package stable and available
- Understanding of existing moosicbox_schema patterns
- Test database infrastructure available

### Phase Dependencies
1. **Phase 1** (Package Creation) - No dependencies, must be done first
2. **Phase 2** (Core Types) - Requires Phase 1 complete
3. **Phase 3** (Discovery) - Requires Phase 2 complete
4. **Phase 4** (Runner) - Requires Phases 2-3 complete
   - NOTE: Verify switchy_database transaction support
5. **Phase 5** (Rollback) - Requires Phase 4 complete
6. **Phase 6** (Validation) - Requires Phase 4 complete
7. **Phase 7** (moosicbox Migration) - Requires Phases 2-6 complete
8. **Phase 8** (Testing) - Can parallel with development phases
9. **Phase 9** (Migration Listing) - Requires Phases 2-3 complete
10. **Phase 10** (Documentation) - Can parallel with all phases
11. **Phase 11** (Future Enhancements) - After all core phases complete

### Parallel Work Opportunities
- Core types and discovery can be developed simultaneously
- Validation can proceed in parallel with rollback development
- Migration listing can be developed alongside other phases
- Documentation can be written as features are implemented
- Testing can be developed incrementally with each phase

## Risks & Mitigations

### Risk: Breaking existing moosicbox_schema functionality
**Mitigation:** Maintain moosicbox_schema as thin wrapper, comprehensive compatibility tests

### Risk: Complex dependency resolution
**Mitigation:** Start with simple timestamp ordering, add dependencies incrementally

### Risk: Database-specific migration differences
**Mitigation:** Leverage switchy_database abstractions, test across all database types

### Risk: Performance impact of new abstraction layer
**Mitigation:** Benchmark against existing implementation, optimize hot paths

### Risk: Migration state corruption
**Mitigation:** Comprehensive validation, atomic operations, backup recommendations

## Next Steps

1. Create `packages/switchy/schema/` package directory and workspace integration
2. Implement core types and traits for migration system
3. Add feature-gated discovery modules for different migration sources
4. Create migration runner with transaction support
5. Add rollback support and validation features
6. Update `moosicbox_schema` to use switchy_schema internally
7. Add comprehensive testing with robust test utilities
8. Implement migration listing functionality
9. Validate HyperChad integration and provide usage examples
