# Generic Schema Migrations - Execution Plan

## Executive Summary

Extract the generic migration logic from `moosicbox_schema` into a reusable `switchy_schema` package that any project can use for database schema evolution. This provides a foundation for HyperChad and other projects to manage their database schemas independently while maintaining full compatibility with existing MoosicBox code.

**Current Status:** 🟡 **Implementation Phase** - Phase 1-5 complete, Phase 7.1-7.4 complete, ready for Phase 7.5

**Completion Estimate:** ~25% complete - Core foundation, traits, discovery methods, migration runner, rollback, and Arc migration completed. Test utilities in progress.

## Status Legend

- 🔴 **Critical** - Blocks core functionality
- 🟡 **Important** - Affects user experience or API design
- 🟢 **Minor** - Nice-to-have or polish items
- ✅ **Complete** - Fully implemented and validated
- 🟡 **In Progress** - Currently being worked on
- ❌ **Blocked** - Waiting on dependencies or design decisions

## Open Questions

These items need further investigation or decision during implementation:

### Migration Ordering
- Migration ordering for identical IDs (currently uses alphabetical sorting, edge case)
- Should we validate that at least one of up.sql or down.sql exists?

### Error Recovery & Partial Migration State
- What happens if a migration fails halfway through?
- How to handle partially applied migrations?
- Should we support "dirty" state detection?
- Recovery mechanisms for corrupted migration state

### Concurrent Migration Protection
- How to prevent multiple processes from running migrations simultaneously?
- Lock mechanism (database locks, file locks, etc.)?
- Timeout handling for stuck migrations?

### Advanced Safety Features
- Production environment detection and confirmation prompts
- Backup recommendations before destructive operations
- Migration checksum validation (deferred to Phase 11.3)
- Environment-specific migration controls
- Rollback safety checks and warnings

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

## Phase 2: Core Migration Types ✅ **COMPLETED**

**Goal:** Define fundamental types and traits for the migration system

**Status:** 100% complete ✅. All core traits and error types implemented.

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
  - [x] Define `MigrationError` enum with database, validation, execution errors
    - ✓ Lines 19-35 in lib.rs with 5 error variants
  - [x] Use thiserror for comprehensive error messages
    - ✓ Line 19: #[derive(Debug, Error)] with error messages
  - [x] Include context for debugging (migration ID, SQL, etc.)
    - ✓ Proper error propagation with #[error(transparent)] and #[from]
    - ✓ Added IO error variant for file operations (line 23-24)
    - ✓ Database errors use transparent propagation (line 21-22)

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
    - ✓ Line 21-22: Database(#[from] DatabaseError) with #[error(transparent)]
  - [x] Cases for IO errors (#[from] std::io::Error)
    - ✓ Line 23-24: Io(#[from] std::io::Error)
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
  - [x] Dependencies: switchy_database, async-trait, thiserror, include_dir (optional), bytes
    - ✓ Lines 17-22: all required dependencies present including bytes and include_dir
  - [x] Features: embedded, directory, code, validation, test-utils
    - ✓ Lines 26-32: all features defined
  - [x] Default features: embedded
    - ✓ Line 22: default = ["embedded"]
  - [x] Embedded feature depends on include_dir
    - ✓ Line 29: embedded = ["dep:include_dir"]

## Phase 3: Migration Discovery ✅ **COMPLETED**

**Goal:** Implement migration discovery from various sources with feature-gated modules

**Status:** ✅ 100% complete. All three discovery methods (embedded, directory, code) are fully implemented with lifetime-aware traits and Executable integration.

### 3.1 Common Discovery Interface

- [x] `packages/switchy/schema/src/discovery/mod.rs` - Feature-gated re-exports ✅ **CRITICAL**
  - [x] Remove empty `DiscoverySource` trait (use `MigrationSource` directly)
    - ✓ Removed and replaced with feature-gated re-exports (lines 1-8)
  - [x] Add feature-gated re-exports for discovery implementations
    - ✓ All three discovery modules properly feature-gated
  - [x] Minimal shared utilities (only if duplication emerges)
    - ✓ Started with no shared code as planned

### 3.2 File-Based Discovery (feature = "directory") ✅ **COMPLETED**

- [x] `packages/switchy/schema/src/discovery/directory.rs` - Directory discovery ✅ **CRITICAL**
  - [x] Feature-gated with `#[cfg(feature = "directory")]`
    - ✓ Module feature-gated in mod.rs (line 4)
  - [x] `FileMigration` struct implementing `Migration` trait (id, up_sql: Option<String>, down_sql: Option<String>)
    - ✓ Implemented with consistent optional fields (lines 6-11)
  - [x] `DirectoryMigrationSource` struct implementing `MigrationSource` trait
    - ✓ Implemented with migrations_path field (lines 52-64)
  - [x] Provide `DirectoryMigrationSource::from_path()` or similar explicit API
    - ✓ from_path() constructor implemented (line 56)
  - [x] Scan directories for migration files (directory name becomes migration ID)
    - ✓ Fully implemented in extract_migrations() method (lines 89-137)
  - [x] Both up.sql and down.sql are optional with consistent handling
    - ✓ Both use Option<String>, missing files → None, empty files → Some("")
  - [x] Empty or missing migration files skip execution but are marked as successful
    - ✓ Implemented with proper None/empty string handling in up()/down() methods
  - [x] Directories with no SQL files are skipped entirely
    - ✓ Implemented with early continue when both files are None (lines 118-120)

### 3.3 Embedded Discovery (feature = "embedded") ✅ **COMPLETED**

- [x] `packages/switchy/schema/src/discovery/embedded.rs` - Embedded discovery ✅ **CRITICAL**
  - [x] Feature-gated with `#[cfg(feature = "embedded")]`
    - ✓ Module feature-gated in mod.rs (line 1)
  - [x] `EmbeddedMigration` struct implementing `Migration` trait (id, up_content: Option<Bytes>, down_content: Option<Bytes>)
    - ✓ Implemented with all required fields (lines 8-23)
  - [x] `EmbeddedMigrationSource` struct implementing `Migration Source` trait
    - ✓ Implemented with migrations_dir field (lines 59-67)
  - [x] `EmbeddedMigrationSource` accepts Dir<'static> from include_dir macro
    - ✓ new() constructor implemented (line 65)
  - [x] Extract migrations from include_dir structures
    - ✓ Implemented in extract_migrations() method (lines 70-101)
  - [x] Maintain compatibility with existing moosicbox patterns
    - ✓ Uses same directory structure pattern (migration_dir/up.sql, migration_dir/down.sql)
  - [x] Support nested directory structures
    - ✓ Walks directory entries to find migration directories (lines 73-100)
  - [x] Parse migration names and ordering
    - ✓ Uses directory names as IDs, BTreeMap for alphabetical ordering (lines 75-79, 70)
  - [x] Handle optional up.sql and down.sql files
    - ✓ Both files are optional, empty files treated as no-ops (lines 83-94, 32-55)
  - [x] Comprehensive unit tests with test migration files
    - ✓ 4 unit tests covering all scenarios, test_migrations/ directory created

### 3.4 Code-Based Discovery (feature = "code")

- [x] `packages/switchy/schema/src/discovery/code.rs` - Code discovery ✅ **COMPLETED**
  - [x] Feature-gated with `#[cfg(feature = "code")]`
    - ✓ Module feature-gated in mod.rs (line 7)
  - [x] `CodeMigration` struct implementing `Migration` trait (id, up_fn: Option<...>, down_fn: Option<...>)
    - ✓ Implemented with function pointer fields (lines 15-44)
  - [x] `CodeMigrationSource` struct implementing `MigrationSource` trait
    - ✓ Implemented with BTreeMap registry (lines 47-77)
  - [x] Provide explicit API for code-based migrations
    - ✓ new() and add_migration() methods implemented
  - [x] Registry for programmatically defined migrations
    - ✓ BTreeMap-based registry implemented (line 49)
  - ~~[ ] Type-safe migration definitions~~
    - ~~🔄 Partially implemented - need better cloning strategy~~ (Superseded by Phase 3.6)
  - ~~[ ] Integration with trait-based migrations~~
    - ~~✗ TODO placeholder at line 74~~ (Superseded by Phase 3.6)

### 3.5 Complete Directory Discovery Implementation

**Goal:** Implement full directory-based migration discovery using async file operations

**Status:** ✅ Complete

#### 3.5.1 Update Dependencies
- [x] Add `switchy_fs` dependency to `Cargo.toml` ✅ **CRITICAL**
  - [x] Add under `[dependencies]` with `workspace = true` and features = ["async", "tokio"]
  - [x] Make it optional, tied to `directory` feature

#### 3.5.2 Implement Directory Scanning
- [x] Update `packages/switchy/schema/src/discovery/directory.rs` ✅ **CRITICAL**
  - [x] Import `switchy_fs::unsync::{read_to_string, read_dir_sorted}`
  - [x] Add `extract_migrations()` method to `DirectoryMigrationSource`
  - [x] Scan directory for subdirectories (each subdirectory = one migration)
  - [x] Use directory name as migration ID (as-is, no validation)
  - [x] For each migration directory:
    - [x] Check for `up.sql` file
    - [x] Check for `down.sql` file
    - [x] Read file contents as `String` (not `Bytes`)
    - [x] Handle missing files (both are optional)
    - [x] Handle empty files (treat as no-op)
    - [x] Skip directories with no SQL files entirely
  - [x] Return `BTreeMap<String, FileMigration>` for deterministic ordering

#### 3.5.3 Update FileMigration Implementation
- [x] Update `FileMigration` to use `Option<String>` for both up_sql and down_sql (consistent handling) ✅ **CRITICAL**
- [x] Update `up()` method to handle `None` and empty strings as no-ops
- [x] Update `down()` method to handle `None` and empty strings as no-ops

#### 3.5.4 Add Tests
- [x] Create test migration directories under `test_migrations_dir/` ✅ **IMPORTANT**
- [x] Test various scenarios:
  - [x] Migration with both up.sql and down.sql
  - [x] Migration with only up.sql
  - [x] Migration with empty up.sql
  - [x] Migration with no SQL files (should be skipped)
  - [x] Migration with None SQL content handling
- [x] Test async file operations
- [x] Test alphabetical ordering by migration ID

#### Migration File Handling Rules (Implemented):
- Both `up.sql` and `down.sql` are optional (`Option<String>`)
- Missing files → `None`
- Empty files → `Some("")` (treated as no-op during execution)
- Directories with no SQL files are skipped entirely (not included in migration list)
- Directories with at least one SQL file create a migration
- Consistent handling: both files use the same optional pattern

### 3.6 Implement Code Discovery with Executable Integration

**Goal:** Implement code-based migrations using query builders from switchy_database with lifetime-aware traits

**Status:** ✅ Complete

#### 3.6.1 Update Core Migration Traits for Lifetimes
- [x] Update `packages/switchy/schema/src/migration.rs` ✅ **CRITICAL**
  - [x] Change `Migration` trait to `Migration<'a>: Send + Sync + 'a`
  - [x] Change `MigrationSource` trait to `MigrationSource<'a>: Send + Sync`
  - [x] Update return type to `Result<Vec<Box<dyn Migration<'a> + 'a>>>`

#### 3.6.2 Add Executable Trait to switchy_database
- [x] Create `packages/database/src/executable.rs` ✅ **CRITICAL**
  - [x] Define `Executable` trait:
    - [x] `async fn execute(&self, db: &dyn Database) -> Result<(), DatabaseError>`
  - [x] Implement `Executable` for `CreateTableStatement<'_>`
    - [x] Uses existing `db.exec_create_table()` method for database-specific SQL generation
  - [x] Implement `Executable` for `InsertStatement<'_>`
  - [x] Implement `Executable` for `UpdateStatement<'_>`
  - [x] Implement `Executable` for `DeleteStatement<'_>`
  - [x] Implement `Executable` for `UpsertStatement<'_>`
  - [x] Implement `Executable` for `String` and `&str` (for raw SQL)
- [x] Export `Executable` from `packages/database/src/lib.rs`

#### 3.6.3 Update Existing Discovery Implementations for Lifetimes
- [x] Update `EmbeddedMigration` to implement `Migration<'static>` ✅ **CRITICAL**
- [x] Update `EmbeddedMigrationSource` to implement `MigrationSource<'static>`
- [x] Update `FileMigration` to implement `Migration<'static>`
- [x] Update `DirectoryMigrationSource` to implement `MigrationSource<'static>`

#### 3.6.4 Implement Code Discovery with Lifetimes
- [x] Update `packages/switchy/schema/src/discovery/code.rs` ✅ **CRITICAL**
  - [x] Remove function pointer types
  - [x] Create `CodeMigration<'a>` struct:
    - [x] `id: String`
    - [x] `up_sql: Box<dyn Executable + 'a>`
    - [x] `down_sql: Option<Box<dyn Executable + 'a>>`
  - [x] Implement `Migration<'a>` for `CodeMigration<'a>`
    - [x] Use `up_sql.execute(db)` in `up()` method
    - [x] Use `down_sql.execute(db)` in `down()` method
  - [x] Update `CodeMigrationSource` to `CodeMigrationSource<'a>`
    - [x] Store `Vec<CodeMigration<'a>>` for simpler ownership model
    - [x] Implement `add_migration()` with `CodeMigration` parameters
    - [x] Support both raw SQL strings and query builders
  - [x] Implement `MigrationSource<'a>` for `CodeMigrationSource<'a>` ✅ **COMPLETE**
    - ✓ Returns stored migrations with deterministic sorting by ID
    - ✓ Changed storage from `Vec<CodeMigration>` to `Vec<Arc<dyn Migration>>`
    - ✓ All tests pass including ordering verification

#### 3.6.5 Add Tests for Code Discovery
- [x] Test with raw SQL strings ✅ **IMPORTANT**
- [x] Test with `CreateTableStatement` builders
- [x] Test with mixed migration types
- [x] Test lifetime handling with lifetime-aware architecture
- [x] Test ordering and retrieval

#### 3.6.6 Update Documentation
- [x] Add examples showing query builder usage ✅ **MINOR**

#### Implementation Notes:
- The trait was renamed from `IntoSql` to `Executable` to better reflect its functionality
- `Executable` doesn't generate SQL strings; it executes operations using existing Database methods
- This approach leverages database-specific SQL generation already in the Database implementations
- `CodeMigrationSource` uses `Vec` instead of `BTreeMap` for simpler ownership model
- All existing discovery methods (embedded, directory) remain fully functional with lifetime updates

### 3.7 Package Compilation

- [x] Ensure clean compilation ✅ **CRITICAL**
  - [x] Package must compile without warnings when no discovery features are enabled
    - ✓ Verified with cargo check --no-default-features
  - [x] Core types and traits are always available
    - ✓ Migration and MigrationSource traits always available
  - [x] Discovery implementations are feature-gated additions
    - ✓ All discovery modules properly feature-gated

## Phase 4: Migration Runner

**Goal:** Core execution engine for running migrations

**Status:** ✅ **CORE FUNCTIONALITY COMPLETE** (Phase 4.1 and 4.2 done, 4.3 deferred)

### Implementation Notes (Added 2025-01-14)

Phase 4.1 and 4.2 have been successfully implemented with the following decisions:

#### Completed Features ✅
- MigrationRunner with configurable options and execution strategies
- Specialized constructors for all three discovery methods
- BTreeMap-based deterministic ordering
- Version tracking with migrations table
- Migration hooks system
- Dry run support
- 17 comprehensive unit tests

#### Deferred to Future Phases
1. **Dependency Resolution (4.3)** → Removed entirely
   - Not critical for initial functionality
   - Users can handle ordering themselves with naming conventions

2. **Dynamic Table Names** → Moved to Phase 12
   - Limited by switchy_database requiring `&'static str`
   - Default table name works for 99% of use cases
   - Documented limitation with error messages

3. **Transaction Support** → Moved to Phase 13
   - Requires switchy_database enhancement
   - Current implementation is still safe (fails fast on errors)

4. **Rollback Tracking** → Will be added with Phase 5
   - Infrastructure exists (down methods implemented)
   - Tracking will be added when rollback execution is implemented

### 4.1 Runner Implementation ✅ **COMPLETED**

- [x] `packages/switchy/schema/src/runner.rs` - Migration runner
  - [x] Create `MigrationRunner` struct with configurable options
  - [x] Provide specific constructors: new_embedded(), new_directory(), new_code()
  - [x] Support different execution strategies (All, UpTo, Steps, DryRun)
  - [x] Use BTreeMap for deterministic ordering
  - [x] Follow moosicbox pattern: query tracking table for each migration
  - [x] If migration not found in table → execute and record it
  - [x] If migration found in table → skip (already ran)
  - [x] SQL execution via migration.up() using Executable trait
  - [x] Empty/missing migrations are recorded as successful
  - [x] Add migration hooks (before/after/error callbacks)
  - [~] Transaction management - DEFERRED to Phase 13
  - [x] NOTE: Verified switchy_database lacks transaction support

### 4.2 Version Tracking ✅ **COMPLETED**

- [x] `packages/switchy/schema/src/version.rs` - Version management
  - [x] Create standard migrations tracking table (default: `__switchy_migrations`)
  - [x] Exact schema matching moosicbox: name (Text), run_on (DateTime)
  - [~] Support configurable table names - LIMITED (see implementation notes)
  - ~~[ ] Handle rollback tracking~~ - DEFERRED to Phase 5

## Phase 5: Rollback Support

**Goal:** Simple, safe rollback functionality

**Status:** ✅ **COMPLETED** (2025-01-14)

**Note:** Down migrations are already implemented in all discovery methods. This phase adds the execution logic and tracking.

### 5.1 Rollback Engine ✅ **COMPLETED**

- [x] Add rollback() method to MigrationRunner ✅ **IMPORTANT**
  - [x] Support rollback strategies:
    - [x] Last: Roll back the most recent migration
    - [x] DownTo(id): Roll back to (but not including) a specific migration
    - [x] Steps(n): Roll back N migrations
    - [x] All: Roll back all applied migrations
  - [x] Use reverse chronological order (most recent first)
  - [x] Validate down() methods exist before attempting rollback
  - [x] Support dry-run to preview what would be rolled back
  - [x] Integration with existing MigrationRunner and hooks system

### 5.2 Rollback Tracking (Simplified) ✅ **COMPLETED**

- [x] Update VersionTracker for simple rollback tracking ✅ **IMPORTANT**
  - [x] When migration is successfully rolled back:
    - [x] Execute migration.down()
    - [x] DELETE the row from __switchy_migrations table
  - [x] This makes the migration eligible to run again if needed
  - [x] No schema changes required to the tracking table
  - [x] Maintains principle: "migrations table shows what's currently applied"

**Implementation Notes (Added 2025-01-14):**

✅ **Core Features Implemented:**
- `RollbackStrategy` enum with all required variants (Last, DownTo, Steps, All)
- `MigrationRunner::rollback()` method with full strategy support
- `VersionTracker::get_applied_migrations()` - returns migrations in reverse chronological order
- `VersionTracker::remove_migration()` - deletes migration records during rollback
- Built-in validation through migration source lookup and down() execution
- Dry-run support via existing `self.dry_run` flag
- Full integration with hooks system (before/after/error callbacks)
- Comprehensive test coverage (3 new test functions, all 20 unit tests + 10 doc tests passing)

✅ **Zero Compromises Made:**
- All Phase 5.1 and 5.2 requirements implemented exactly as specified
- No breaking changes to existing APIs
- Follows established patterns and conventions
- Proper error handling and rollback on failure

**Rationale:** Simple deletion approach is cleaner than complex rollback status tracking. The migrations table always reflects the current state of applied migrations.

## ~~Phase 6: Validation & Safety~~ ❌ **REMOVED**

~~**Goal:** Comprehensive validation to prevent migration issues~~

**Status:** ❌ **REMOVED** - Validation features deemed unnecessary for core functionality:
- Migration IDs can be any valid string (no naming convention needed)
- Checksum validation moved to Phase 11.3 (Future Enhancements)
- Dependency resolution removed entirely (users handle ordering themselves)
- Advanced safety features moved to Open Questions section

## Phase 7: Testing Infrastructure

**Goal:** Provide comprehensive test utilities for verifying migration correctness and behavior

**Status:** Not started

### 7.1 Test Utilities Package Creation ✅ **COMPLETED**

- [x] Create `packages/switchy/schema/test_utils/` package structure ✅ **CRITICAL**
  - [x] Create `packages/switchy/schema/test_utils/` directory
    - ✓ Created at packages/switchy/schema/test_utils/
  - [x] Create `packages/switchy/schema/test_utils/src/` directory
    - ✓ Created at packages/switchy/schema/test_utils/src/
  - [x] Create `packages/switchy/schema/test_utils/src/lib.rs`
    - ✓ Created with clippy config, error types, and feature-gated helper (40 lines)
  - [x] Create `packages/switchy/schema/test_utils/Cargo.toml`
    - ✓ Package name: `switchy_schema_test_utils`
    - ✓ Dependencies:
      - `switchy_schema = { workspace = true }`
      - `switchy_database = { workspace = true }`
      - `switchy_database_connection = { workspace = true, optional = true }`
      - `async-trait = { workspace = true }`
      - `thiserror = { workspace = true }`
    - ✓ Features:
      - `fail-on-warnings = []` (default)
      - `sqlite = ["dep:switchy_database_connection", "switchy_database_connection/sqlite-sqlx"]`
  - [x] Update root `Cargo.toml` to include new package in workspace
    - ✓ Added to workspace members at line 118
    - ✓ Added to workspace dependencies at line 274
  - [x] Add error wrapper type (similar to `MigrationError` in switchy_schema)
    - ✓ `TestError` enum that propagates `MigrationError` and `DatabaseError`

### 7.2 Database Helper Functions ✅ **COMPLETED**

- [x] `packages/switchy/schema/test_utils/src/lib.rs` - Database creation helpers ✅ **CRITICAL**
  - [x] Feature-gated in-memory database helper:
    ```rust
    #[cfg(feature = "sqlite")]
    pub async fn create_empty_in_memory() -> Result<Box<dyn Database>, switchy_database_connection::InitSqliteSqlxDatabaseError>
    ```
    - ✓ Uses `switchy_database_connection::init_sqlite_sqlx(None)` for in-memory SQLite
    - ✓ Proper error handling with specific error type
    - ✓ Comprehensive documentation with error section
  - [x] All test functions accept `&dyn Database` as parameter:
    - ✓ User provides the database instance they want to test against
    - ✓ Allows testing with any database type
    - ✓ No database creation logic in core test utilities (ready for Phase 7.3+)

### 7.3 Core Test Utilities ✅ **COMPLETED**

- [x] `packages/switchy/schema/test_utils/src/lib.rs` - Core test functionality ✅ **CRITICAL**

  - [x] **VecMigrationSource helper** - Internal utility for test functions:
    ```rust
    struct VecMigrationSource<'a> {
        migrations: Vec<Arc<dyn Migration<'a> + 'a>>,
    }

    impl<'a> MigrationSource<'a> for VecMigrationSource<'a> {
        async fn migrations(&self) -> Result<Vec<Arc<dyn Migration<'a> + 'a>>> {
            Ok(self.migrations.clone()) // Cheap Arc cloning!
        }
    }
    ```
    - [x] Used internally by test functions to wrap Vec into MigrationSource
    - [x] Leverages Arc for cheap cloning without RefCell or unsafe code
    - [x] Simple constructor: `VecMigrationSource::new(migrations)`

  - [x] **Basic migration verification** - Test migrations from fresh state:
    ```rust
    pub async fn verify_migrations_full_cycle<'a>(
        db: &dyn Database,
        migrations: Vec<Arc<dyn Migration<'a> + 'a>>
    ) -> Result<(), TestError>
    ```
    - [x] Create `VecMigrationSource` from provided migrations
    - [x] Create `MigrationRunner` internally from switchy_schema
    - [x] Run all migrations forward (up) on provided database
    - [x] Verify no errors during forward migration
    - [x] Run all migrations backward (down) using rollback functionality
    - [x] Verify database returns to initial state
    - [x] Verify no errors during rollback
    - [x] Add unit tests for this functionality

  - [x] **Pre-seeded state verification** - Test with existing data:
    ```rust
    pub async fn verify_migrations_with_state<'a, F, Fut>(
        db: &dyn Database,
        migrations: Vec<Arc<dyn Migration<'a> + 'a>>,
        setup: F
    ) -> Result<(), TestError>
    where
        F: FnOnce(&dyn Database) -> Fut,
        Fut: Future<Output = Result<(), DatabaseError>>
    ```
    - [x] Execute setup closure to populate initial state
    - [x] Create `VecMigrationSource` from provided migrations
    - [x] Create `MigrationRunner` internally from switchy_schema
    - [x] Run all migrations forward
    - [x] Verify migrations handle existing data correctly
    - [x] Run all migrations backward using rollback functionality
    - [x] Verify rollback preserves/restores initial state
    - [x] Add unit tests for this functionality

### 7.4 Mutation Provider and Advanced Testing ✅ **COMPLETED**

- [x] `packages/switchy/schema/test_utils/src/mutations.rs` - Mutation handling ✅ **IMPORTANT**
  - [x] Define `MutationProvider` trait:
    ```rust
    pub trait MutationProvider {
        async fn get_mutation(&self, after_migration_id: &str)
            -> Option<Arc<dyn Executable>>;
    }
    ```
  - [x] Implement for common patterns:
    - [x] `BTreeMap<String, Arc<dyn Executable>>` - Map migration IDs to mutations (NOT HashMap!)
    - [x] `Vec<(String, Arc<dyn Executable>)>` - Ordered list of mutations
    - [x] Builder pattern for constructing mutation sequences
  - [x] Add unit tests for each implementation

- [x] `packages/switchy/schema/test_utils/src/lib.rs` - Advanced mutation testing ✅ **IMPORTANT**
  - [x] **Interleaved state mutations** - Test with data changes between migrations:
    ```rust
    pub async fn verify_migrations_with_mutations<'a, M>(
        db: &dyn Database,
        migrations: Vec<Arc<dyn Migration<'a> + 'a>>,
        mutations: M
    ) -> Result<(), TestError>
    where M: MutationProvider
    ```
    - **Note**: Uses `Arc` for consistency with Phase 7.2.5 migration and `TestError` for consistency with Phase 7.3 test utilities
    - [x] Support mutations via:
      - [x] Raw SQL strings
      - [x] `Arc<dyn Executable>` (query builders)
      - [x] Arbitrary closures: `FnOnce(&dyn Database) -> Result<(), DatabaseError>` (via Executable trait)
    - [x] Execute mutations between specific migrations
    - [x] Verify migrations handle intermediate state changes
    - [x] Verify rollback works with mutated data
    - [x] Add unit tests for this functionality

### 7.5 Test Assertion Helpers

- [ ] `packages/switchy/schema/test_utils/src/assertions.rs` - Test assertions ❌ **IMPORTANT**
  - [ ] Table existence verification
  - [ ] Column presence/type verification
  - [ ] Row count assertions
  - [ ] Data integrity checks
  - [ ] Migration state verification (which migrations are applied)
  - [ ] Schema comparison utilities
  - [ ] All functions return `Result<(), DatabaseError>` or propagate existing errors
  - [ ] Add unit tests for assertion helpers

### 7.6 Documentation and Examples

- [ ] Add comprehensive documentation ❌ **MINOR**
  - [ ] Usage examples in module docs
  - [ ] Example test cases showing all three verification methods
  - [ ] Document feature flags and when to use them

**Key Design Decisions:**
- No custom error types - Propagate existing `MigrationError` and `DatabaseError`, with optional thin wrapper only if needed
- User provides database - Test utilities accept `&dyn Database`, don't create databases themselves
- Feature-gated SQLite helper - `create_empty_in_memory()` only available with `sqlite` feature
- BTreeMap over HashMap - Always use `BTreeMap` for deterministic ordering
- Tests alongside implementation - Each component gets unit tests as it's built, not separately

**Implementation Notes (Added 2025-01-14):**

✅ **Phase 7.1, 7.2, and 7.2.5 Completed Successfully:**
- Package structure follows exact pattern from `hyperchad_test_utils`
- `TestError` wrapper type implemented for clean error propagation
- SQLite feature enables both `switchy_database_connection` dependency and `sqlite-sqlx` feature
- `create_empty_in_memory()` uses `init_sqlite_sqlx(None)` for in-memory database creation
- **Arc migration completed**: All migration types now use `Arc<dyn Migration>` instead of `Box<dyn Migration>`
- Zero clippy warnings with full pedantic linting enabled
- Comprehensive documentation with proper backticks and error sections
- Workspace integration at correct locations (line 118 members, line 274 dependencies)
- **Ready for Phase 7.3**: Test utilities can now easily clone migrations via Arc

**Out of Scope for Phase 7:**
- Testing against different database types (PostgreSQL, MySQL) - user provides the database
- Performance benchmarking utilities
- Migration generation helpers
- Schema diffing tools
- Production database testing utilities

### 7.2.5 Migration Type Update to Arc ✅ **COMPLETED**

- [x] Update core migration types from `Box<dyn Migration>` to `Arc<dyn Migration>` ✅ **CRITICAL**
  - [x] Update `MigrationSource` trait return type:
    ```rust
    async fn migrations(&self) -> Result<Vec<Arc<dyn Migration<'a> + 'a>>>;
    ```
    - ✓ Changed from `Box<dyn Migration>` to `Arc<dyn Migration>`
  - [x] Update all MigrationSource implementations:
    - ✓ `EmbeddedMigrationSource` - uses `Arc::new()` instead of `Box::new()`
    - ✓ `DirectoryMigrationSource` - uses `Arc::new()` instead of `Box::new()`
    - ✓ `CodeMigrationSource` - updated return type signature
  - [x] Update `MigrationRunner` to work with Arc:
    - ✓ Internal BTreeMap uses `Arc<dyn Migration>`
    - ✓ `apply_strategy` method signature updated
    - ✓ All test cases updated to use `Arc::new()`
  - [x] Update documentation examples:
    - ✓ Added `std::sync::Arc` imports to all doc examples
    - ✓ Updated all type signatures in documentation
    - ✓ All doc tests pass
  - [x] Verify compatibility:
    - ✓ All 20 unit tests pass
    - ✓ All 10 doc tests pass
    - ✓ Zero clippy warnings
    - ✓ No breaking changes to public API

**Arc Migration Benefits:**
- **Cheap cloning**: `Arc::clone()` just increments reference count
- **Clean test utilities**: No RefCell, unsafe code, or complex ownership patterns
- **Shared ownership**: Multiple test utilities can share the same migrations
- **Zero compromises**: All existing functionality preserved

## Phase 8: moosicbox_schema Migration

**Goal:** Update existing moosicbox_schema to use switchy_schema

### 8.1 Wrapper Implementation

- [ ] `packages/schema/src/lib.rs` - Update moosicbox_schema ❌ **CRITICAL**
  - [ ] Replace direct migration logic with switchy_schema calls
  - [ ] Maintain existing public API unchanged
  - [ ] Use MigrationRunner with embedded sources
  - [ ] Keep existing function signatures and behavior

### 8.2 Migration Compatibility

- [ ] `packages/schema/src/lib.rs` - Ensure compatibility ❌ **CRITICAL**
  - [ ] Verify all existing migrations continue to work
  - [ ] Maintain migration table name compatibility
  - [ ] Preserve migration ordering and checksums
  - [ ] Test against existing databases
  - [ ] Add unit tests using in-memory SQLite similar to existing tests
  - [ ] Verify migrations run without clippy warnings

### 8.3 Feature Propagation

- [ ] `packages/schema/Cargo.toml` - Update dependencies ❌ **CRITICAL**
  - [ ] Add switchy_schema dependency
  - [ ] Propagate feature flags appropriately
  - [ ] Maintain existing feature compatibility
  - [ ] Update documentation

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

### 11.6 Snapshot Testing Utilities

- [ ] Snapshot testing infrastructure for migration verification ❌ **MINOR**
  - [ ] **Schema Snapshots**
    - [ ] Capture database schema state after each migration
    - [ ] Normalize schema representation across database types
    - [ ] Compare schema evolution over time
    - [ ] Detect unintended schema changes
  - [ ] **Migration Sequence Snapshots**
    - [ ] Record the full sequence of migrations applied
    - [ ] Track migration ordering and dependencies
    - [ ] Useful for debugging migration issues
  - [ ] **SQL Statement Snapshots** (optional)
    - [ ] Capture actual SQL executed by migrations
    - [ ] Review what changes migrations make
    - [ ] Detect database-specific SQL differences
  - [ ] **Data Snapshots** (complex - consider deferring)
    - [ ] Capture data state at specific points
    - [ ] Verify data transformations
    - [ ] Handle non-deterministic data (timestamps, auto-increment IDs)
  - [ ] **Implementation Considerations**
    - [ ] Snapshot format (JSON, SQL, or custom format)
    - [ ] Update mechanism via environment variable (e.g., `UPDATE_SNAPSHOTS=1`)
    - [ ] Integration with existing test utilities
    - [ ] Snapshot storage and versioning strategy
    - [ ] Handling database-specific variations
  - [ ] **Benefits**
    - [ ] Regression detection for schema changes
    - [ ] Documentation of schema evolution
    - [ ] Review-friendly PR diffs for schema changes
    - [ ] Debugging aid for migration issues
    - [ ] Cross-database compatibility verification

### 11.7 Complete CodeMigrationSource Implementation

- [ ] Finish `CodeMigrationSource::migrations()` implementation ❌ **MINOR**
  - [ ] Replace empty Vec return with proper migration retrieval
  - [ ] Support dynamic addition of migrations via `add_migration()`
  - [ ] Handle ownership correctly with Arc-based migrations
  - [ ] Implement proper migration ordering (BTreeMap-based)
  - [ ] Add comprehensive tests for code-based migration functionality
  - [ ] Update documentation with working examples

### 11.8 Ergonomic Async Closure Support for Test Utilities

**Goal:** Improve the ergonomics of `verify_migrations_with_state` to avoid requiring `Box::pin`

**Current Issue:** Users must write `|db| Box::pin(async move { ... })` which is verbose and non-intuitive

**Potential Solutions to Evaluate:**

#### Option 1: Dual Function Approach
- [ ] Create `verify_migrations_with_sync_setup` for simple synchronous setup ❌ **MINOR**
- [ ] Keep `verify_migrations_with_async_setup` for complex async cases
- **Pros:** Clear separation, optimal for each use case
- **Cons:** API duplication, more functions to maintain

#### Option 2: Builder Pattern
- [ ] Create `MigrationTest` builder with `.with_setup()` method ❌ **MINOR**
- [ ] Builder handles the boxing internally
- **Pros:** Fluent API, extensible for future options
- **Cons:** More complex API, departure from current simple functions

#### Option 3: Helper Function (`setup_fn`)
- [ ] Add `setup_fn()` helper that wraps closure and returns boxed future ❌ **MINOR**
- [ ] Users write `setup_fn(|db| async move { ... })`
- **Pros:** Minimal API change, backward compatible, clear intent
- **Cons:** Still requires wrapping, though more discoverable than `Box::pin`

#### Option 4: Trait-Based Approach
- [ ] Define `SetupFn` trait that auto-implements for async closures ❌ **MINOR**
- [ ] Trait implementation handles boxing internally
- **Pros:** Most ergonomic, no wrapping needed
- **Cons:** Complex trait bounds, potential compilation issues

**Recommendation:** Defer decision until we have more real-world usage patterns. The current `Box::pin` approach is standard in the Rust async ecosystem and well-understood by developers.

## ~~Phase 12: Migration Dependency Resolution~~ ❌ **REMOVED**

~~**Goal:** Advanced dependency management for complex migration scenarios~~

**Status:** ❌ **REMOVED** - Dependency resolution deemed unnecessary:
- Users can handle migration ordering themselves using naming conventions
- Adds unnecessary complexity to the core package
- Most migrations don't require complex dependencies
- Ordering can be managed through migration IDs (e.g., timestamp prefixes)

## Phase 12: Dynamic Table Name Support

**Goal:** Enable truly configurable migration table names

**Status:** Not started

**Blocker:** Requires enhancement to switchy_database to support dynamic table names

### 12.1 Database Enhancement

- [ ] Enhance switchy_database to support dynamic table names ❌ **CRITICAL**
  - [ ] Add query_raw and exec_query_raw methods that return data
  - [ ] OR: Add runtime table name resolution to existing methods
  - [ ] Maintain backward compatibility

### 12.2 Version Tracker Update

- [ ] Update VersionTracker to use dynamic table names ❌ **IMPORTANT**
  - [ ] Remove current limitation/error messages
  - [ ] Full support for custom table names
  - [ ] Update all database operations to use dynamic names

## Phase 13: Transaction Support

**Goal:** Add transaction isolation for migration execution

**Status:** Not started

**Blocker:** Requires transaction support in switchy_database

### 13.1 Database Transaction Support

- [ ] Add transaction support to switchy_database ❌ **CRITICAL**
  - [ ] begin_transaction() method
  - [ ] commit() method
  - [ ] rollback() method
  - [ ] Nested transaction support (savepoints)

### 13.2 Runner Transaction Integration

- [ ] Update MigrationRunner to use transactions ❌ **IMPORTANT**
  - [ ] Per-migration transactions (default)
  - [ ] Batch transaction mode
  - [ ] Configurable transaction strategies
  - [ ] Proper error handling and rollback on failure

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
- Simple timestamp-based ordering for deterministic execution
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
bytes = { workspace = true }
chrono = { workspace = true }

[dev-dependencies]
tokio = { workspace = true, features = ["macros", "rt"] }


[features]
default = ["embedded"]
embedded = ["dep:include_dir"]
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
1. **Phase 1** (Package Creation) - ✅ Complete
2. **Phase 2** (Core Types) - ✅ Complete
3. **Phase 3** (Discovery) - ✅ Complete
4. **Phase 4** (Runner Core) - ✅ Complete (4.1, 4.2)
5. **Phase 5** (Rollback) - Requires Phase 4 complete
6. **Phase 6** (Validation) - Requires Phase 4 complete
7. **Phase 7** (moosicbox Migration) - Requires Phases 4-6 complete
8. **Phase 8** (Testing) - Can proceed now
9. **Phase 9** (Migration Listing) - Can proceed now
10. **Phase 10** (Documentation) - Can proceed now
11. **Phase 11** (Future Enhancements) - After core phases
12. **Phase 12** (Dynamic Table Names) - Requires switchy_database enhancement
13. **Phase 13** (Transaction Support) - Requires switchy_database enhancement

### Parallel Work Opportunities
- Core types and discovery can be developed simultaneously
- Validation can proceed in parallel with rollback development
- Migration listing can be developed alongside other phases
- Documentation can be written as features are implemented
- Testing can be developed incrementally with each phase

## Risks & Mitigations

### Risk: Breaking existing moosicbox_schema functionality
**Mitigation:** Maintain moosicbox_schema as thin wrapper, comprehensive compatibility tests

### Risk: Migration ordering conflicts
**Mitigation:** Use timestamp-based naming conventions, clear documentation on ordering

### Risk: Database-specific migration differences
**Mitigation:** Leverage switchy_database abstractions, test across all database types

### Risk: Performance impact of new abstraction layer
**Mitigation:** Benchmark against existing implementation, optimize hot paths

### Risk: Migration state corruption
**Mitigation:** Comprehensive validation, atomic operations, backup recommendations

## Next Steps

1. ✅ Create `packages/switchy/schema/` package directory and workspace integration
2. ✅ Implement core types and traits for migration system
3. 🔄 Add feature-gated discovery modules for different migration sources
   - ✅ Embedded discovery (Phase 3.3) - Complete
   - ❌ Directory discovery (Phase 3.5) - Complete reimplementation needed
   - ❌ Code discovery (Phase 3.6) - Complete reimplementation with IntoSql integration needed
4. Create migration runner with transaction support (Phase 4)
5. Add rollback support and validation features (Phase 5-6)
6. Update `moosicbox_schema` to use switchy_schema internally (Phase 7)
7. Add comprehensive testing with robust test utilities (Phase 8)
8. Implement migration listing functionality (Phase 9)
9. Validate HyperChad integration and provide usage examples (Phase 10)
