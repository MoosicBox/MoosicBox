# Generic Schema Migrations - Execution Plan

## Executive Summary

Extract the generic migration logic from `moosicbox_schema` into a reusable `switchy_schema` package that any project can use for database schema evolution. This provides a foundation for HyperChad and other projects to manage their database schemas independently while maintaining full compatibility with existing MoosicBox code.

**Current Status:** ✅ **Phase 10.1 Complete + 10.2.1.1 + 10.2.1.3 Complete** - Phases 1-5, 7 (all sub-phases), 8.1-8.6, 9.1, 10.1, 10.2.1.1, and 10.2.1.3 complete. Migration listing, comprehensive API documentation, database transaction trait architecture, and SQLite transaction serialization now available. Ready for Phase 10.2.1.4 (SQLite sqlx implementation).

**Completion Estimate:** ~90% complete - Core foundation, traits, discovery methods, migration runner, rollback, Arc migration, comprehensive test utilities, moosicbox_schema wrapper, test migration, new feature demonstrations, complete documentation, migration listing, full API documentation, and database transaction trait architecture all finished. Phase 10.2 backend implementations will add actual transaction support and schema builder extensions. Production-ready for HyperChad integration with excellent developer experience.

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

3. **Transaction Support** → Moved to Phase 10.2.1
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
  - [~] Transaction management - MOVED to Phase 10.2.1
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

## Phase 7: Testing Infrastructure ✅ **COMPLETED** (All sub-phases 7.1-7.6 finished 2025-01-14)

**Goal:** Provide comprehensive test utilities for verifying migration correctness and behavior

**Status:** ✅ **COMPLETED** - All test utilities implemented with comprehensive examples

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

### 7.5 Test Assertion Helpers ✅ **COMPLETED**

- [x] `packages/switchy/schema/test_utils/src/assertions.rs` - Test assertions ✅ **IMPORTANT**
  - [x] Table existence verification using `switchy_database::query::select()`
  - [x] Column presence/type verification with ORM-style queries
  - [x] Row count assertions with `i64::try_from().expect()` conversions
  - [x] Data integrity checks using query builder and PRAGMA commands
  - [x] Migration state verification via `__switchy_migrations` table queries
  - [x] Schema comparison utilities with query builder validation
  - [x] INSERT operations using `db.insert().value().execute()` pattern
  - [x] All functions return `Result<(), DatabaseError>` with proper error propagation
  - [x] Comprehensive unit tests (23) and doc tests (17) all passing
  - [x] Zero clippy warnings with full pedantic linting

### 7.6 Documentation and Examples ✅ **COMPLETED**

- [x] Add comprehensive documentation ✅ **MINOR**
  - [x] Usage examples in module docs (basic module docs exist)
  - [x] Doc examples for all assertion functions (comprehensive examples)
  - [x] Example test cases showing all three verification methods (verify_migrations_full_cycle, verify_migrations_with_state, verify_migrations_with_mutations)
    - ✓ Created `basic_migration_test` example demonstrating `verify_migrations_full_cycle`
    - ✓ Created `state_migration_test` example demonstrating `verify_migrations_with_state`
    - ✓ Created `mutation_migration_test` example demonstrating `verify_migrations_with_mutations`
    - ✓ All examples include comprehensive Cargo.toml files and runnable code
    - ✓ Examples show realistic migration scenarios with proper error handling
  - [x] Document feature flags and when to use them (sqlite feature documented)

**Implementation Details (Added 2025-01-14):**

✅ **Phase 7.6 Completed Successfully - Comprehensive Examples Created:**

**Three Full-Featured Migration Test Examples:**
1. **`basic_migration_test`** (`packages/switchy/schema/examples/basic_migration_test/`)
   - Demonstrates `verify_migrations_full_cycle` for simple up/down testing
   - Shows basic table creation with schema query builder
   - Includes proper workspace metadata and README documentation

2. **`state_migration_test`** (`packages/switchy/schema/examples/state_migration_test/`)
   - Demonstrates `verify_migrations_with_state` for data preservation testing
   - Shows adding columns with default values to existing tables
   - Validates data integrity through migration cycles
   - Includes test module with `FilterableQuery` trait usage

3. **`mutation_migration_test`** (`packages/switchy/schema/examples/mutation_migration_test/`)
   - Demonstrates `verify_migrations_with_mutations` for comprehensive testing
   - Implements custom `MutationProvider` for dynamic test scenarios
   - Tests migrations against various database states
   - Includes test module with `MutationProvider` trait usage

4. **`borrowed_migrations`** (`packages/switchy/schema/examples/borrowed_migrations/`)
   - Demonstrates lifetime management patterns for migrations
   - Shows how to work with borrowed data in migration contexts
   - Illustrates proper lifetime annotations for complex scenarios

5. **`static_migrations`** (`packages/switchy/schema/examples/static_migrations/`)
   - Demonstrates 'static lifetime migrations
   - Shows embedded migration patterns
   - Illustrates compile-time migration validation

**Key Implementation Decisions:**
- **Schema Query Builder Usage**: All examples use modern `switchy_database` schema builder API where supported
- **Hybrid Approach**: Raw SQL used only for ALTER TABLE and CREATE INDEX (not yet supported by builder)
- **Workspace Consistency**: All examples use `edition = { workspace = true }` for 2024 edition
- **Documentation**: Each example includes comprehensive README.md with usage instructions
- **Test Coverage**: Examples include test modules demonstrating real-world usage patterns
- **Trait Imports**: Test modules properly import required traits (`FilterableQuery`, `MutationProvider`)

**Workspace Improvements:**
- Updated all example packages to use workspace inheritance for edition
- Added `readme = "README.md"` field to all example Cargo.toml files
- Created comprehensive README documentation for all examples and test utilities
- Fixed clippy warnings including collapsible if statements
- Ensured all packages follow consistent metadata patterns

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

✅ **Phase 7.3, 7.4, and 7.5 Completed Successfully (2025-01-14):**
- **Complete Query Builder Integration**:
  - Used ORM-style query builder for all SELECT operations
  - Used insert builder for all INSERT operations
  - Only `exec_raw` for PRAGMA commands (no query builder support available)
- **Schema Builder Integration**:
  - Enabled `schema` feature for `switchy_database`
  - All CREATE TABLE operations use `db.create_table().column().execute()` pattern
  - Updated all doc examples to showcase modern schema builder API
- **Test Assertion Helpers**:
  - 10 assertion functions covering tables, columns, rows, migrations, and schema
  - All functions return `Result<(), DatabaseError>` for clean error propagation
  - Comprehensive doc examples with schema builder usage
- **Error Handling**:
  - Used `i64::try_from().expect()` with proper panic documentation
  - Clean `TestError` propagation throughout test utilities
  - No incorrect error type mappings
- **Test Coverage**:
  - 23 unit tests passing (table existence, column validation, row counts, etc.)
  - 17 doc tests passing (all assertion function examples)
  - Zero clippy warnings with full pedantic linting
- **Zero Compromises**: Achieved all requirements using modern APIs where available

**Key Technical Decisions for Phase 7.5:**
1. **Complete Query Builder Integration**:
   - Used ORM-style query builder for all SELECT operations
   - Used insert builder for all INSERT operations
   - Only `exec_raw` for PRAGMA commands (no query builder support available)
2. **Schema Builder Integration**: Added `schema` feature to leverage modern table creation API
3. **Error Conversion Strategy**: Used `try_from().expect()` for integer conversions with documented panic conditions
4. **Intentional `exec_raw` Usage**:
   - PRAGMA commands: SQLite-specific, no query builder support exists
   - Migration test utilities: Testing raw SQL migrations requires raw SQL execution (by design)
5. **Test Organization**: Kept all assertions in single module for simplicity
6. **Feature Gating**: All assertions require `sqlite` feature to ensure database availability

**Out of Scope for Phase 7:**
- Schema diffing tools
- Testing against different database types (PostgreSQL, MySQL) - user provides the database
- Production database testing utilities

### Technical Implementation Notes

**Clippy Compliance:**
- All packages pass `cargo clippy --all-targets` with zero warnings
- Full pedantic linting enabled across all packages
- Workspace-wide consistency for metadata inheritance

**Schema Query Builder Integration:**
- Examples demonstrate best practices using `switchy_database` schema builder
- Raw SQL only used where builder doesn't yet support operations (ALTER TABLE, CREATE INDEX)
- All data operations use modern query builder syntax

**Test Infrastructure:**
- Test modules require proper trait imports for extension methods
- `FilterableQuery` trait needed for `where_eq` and similar methods
- `MutationProvider` trait needed for mutation testing functionality
- All examples include runnable test code with proper async/await patterns

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

## Phase 8: moosicbox_schema Migration ✅ **COMPLETED**

**Prerequisites:** ✅ All Phase 7 sub-phases complete with comprehensive test coverage and examples

**Status:** ✅ **FULLY COMPLETE** - All sub-phases (8.1-8.6) successfully implemented

**Goal:** Transform `moosicbox_schema` from a custom migration implementation (~260 lines) to a thin wrapper around `switchy_schema` (~150 lines), while maintaining 100% backward compatibility and gaining new features like rollback support.

**Achievements:**
- ✅ 42% code reduction achieved (260 → 150 lines)
- ✅ Zero breaking changes - all existing code works unchanged
- ✅ New capabilities: rollback support, test utilities, better error handling
- ✅ Comprehensive documentation and migration guide created
- ✅ All tests successfully migrated to MigrationTestBuilder pattern

### 8.1 Enable Custom Table Names in switchy_schema

**Goal:** Remove the artificial limitation preventing custom migration table names

- [x] Update VersionTracker Methods ✅ **CRITICAL**
  - [x] Update `packages/switchy/schema/src/version.rs`:
    - [x] Remove limitation check from `ensure_table_exists()` - use `&self.table_name`
    - [x] Remove limitation check from `is_migration_applied()` - use `&self.table_name`
    - [x] Remove limitation check from `record_migration()` - use `&self.table_name`
    - [x] Remove limitation check from `get_applied_migrations()` - use `&self.table_name`
    - [x] Remove limitation check from `remove_migration()` - use `&self.table_name`
    - [x] Update all documentation to remove "Limitations" sections
    - [x] Remove TODO comments about switchy_database limitations

- [x] Add Convenience Method to MigrationRunner ✅ **CRITICAL**
  - [x] Update `packages/switchy/schema/src/runner.rs`:
    - [x] Add `with_table_name(impl Into<String>)` method for easy configuration
    - [x] Update documentation to show custom table name usage

- [x] Test Custom Table Names ✅ **IMPORTANT**
  - [x] Add test case using custom table name
  - [x] Verify migrations work with non-default table names
  - [x] Ensure backward compatibility with default table name

### Phase 8.1 Implementation Notes (Completed)

**Key Implementation Details:**
- ✅ Removed limitation checks from all 5 methods (`ensure_table_exists`, `is_migration_applied`, `record_migration`, `get_applied_migrations`, `remove_migration`)
- ✅ Now uses `&self.table_name` instead of `DEFAULT_MIGRATIONS_TABLE`
- ✅ Removed all "Limitations" documentation sections
- ✅ Removed TODO comments about switchy_database limitations
- ✅ Added `with_table_name(impl Into<String>)` method
- ✅ Updated module documentation with custom table name usage example
- ✅ Method integrates cleanly with existing builder pattern
- ✅ Added `test_custom_table_name()` unit test
- ✅ Added `test_custom_table_name_integration()` integration test with actual database
- ✅ Added `switchy_database_connection` as dev dependency
- ✅ All 23 tests pass including 2 new tests
- ✅ Verified backward compatibility with default table name

**Testing Approach:**
- Unit tests verify the API works correctly
- Integration test creates actual SQLite database and runs migrations with custom table name
- Test verifies both the custom migration tracking table and the actual migrated tables exist

**No Compromises Made:**
- Every requirement was implemented exactly as specified
- No workarounds or hacks needed
- Clean, maintainable code that follows existing patterns

### 8.2 Core moosicbox_schema Implementation ✅ **COMPLETED**

**Prerequisites:** ✅ Phase 8.1 complete - custom table names fully supported

**Goal:** Replace custom migration logic with switchy_schema while keeping the same API

**Completion Notes:**
- ✅ Successfully reduced from ~260 lines to ~150 lines (42% reduction)
- ✅ All migrations embedded using `include_str!` (43 SQLite, 38 PostgreSQL)
- ✅ Maintains custom table name `__moosicbox_schema_migrations`
- ✅ Environment variable support preserved (`MOOSICBOX_SKIP_MIGRATION_EXECUTION`)
- ✅ Maintains exact same public API: `migrate_config()`, `migrate_library()`, `migrate_library_until()`
- ✅ Test-only migration collection functions implemented and working
- ✅ All tests passing (6 tests including comprehensive migration validation)
- ✅ One compromise: PostgreSQL migrations on SQLite log warnings instead of failing (improves robustness)

**Important Design Note**: The implementation intentionally runs both PostgreSQL and SQLite migrations when both features are enabled. This is not a bug - it's designed for development/testing scenarios. In production, only one database feature is ever enabled, so only one set of migrations runs. This behavior must be preserved for compatibility.

- [x] Implement Unified Migration Functions ✅ **CRITICAL**
  - [x] Rewrite `packages/schema/src/lib.rs` with unified functions:
    - [x] Add `switchy_schema` dependency with `embedded` feature to Cargo.toml
    - [x] Add `switchy_env` dependency for environment variable support
    - [x] Keep existing dependencies that are still needed (include_dir, log, thiserror)
    - [x] Define core types and constants (`MIGRATIONS_TABLE_NAME`)
    - [x] Implement single `migrate_config()` function with internal feature-gated blocks for both databases
    - [x] Implement single `migrate_library()` function with internal feature-gated blocks for both databases
    - [x] Implement single `migrate_library_until()` function with internal feature-gated blocks for both databases

- [x] Implement Database Migration Logic ✅ **CRITICAL**
  - [x] Within each unified function:
    - [x] Use `include_str!` to embed migration directories for both databases
    - [x] Add `#[cfg(feature = "postgres")]` block using `MigrationRunner::new_embedded()` with PostgreSQL directories
    - [x] Add `#[cfg(feature = "sqlite")]` block using `MigrationRunner::new_embedded()` with SQLite directories
    - [x] Implement `ExecutionStrategy::UpTo` support for `migrate_library_until()`
    - [x] Implement `MOOSICBOX_SKIP_MIGRATION_EXECUTION` environment variable support
    - [x] Use custom table name: `__moosicbox_schema_migrations` for all migrations

### 8.3 Test Utilities Enhancement ✅ **COMPLETED**

**Prerequisites:** ✅ Phase 8.2 complete - moosicbox_schema using switchy_schema

**Goal:** Add advanced testing capabilities to `switchy_schema_test_utils` for complex migration scenarios

**Motivation:** The `scan` package tests need to run migrations up to a specific point, insert test data, then run remaining migrations. This pattern tests data migration scenarios and should be supported by our test utilities rather than requiring direct access to migration constants.

- [x] Add MigrationTestBuilder to switchy_schema_test_utils ✅ **COMPLETE**
  - [x] Create `packages/switchy/schema/test_utils/src/builder.rs`
  - [x] Implement `MigrationTestBuilder` struct with:
    - [x] Support for multiple breakpoints in migration sequence
    - [x] Data setup callbacks before/after specific migrations
    - [x] Initial setup before any migrations
    - [x] Optional rollback skipping for debugging
    - [x] Custom migration table name support
  - [x] Builder methods:
    - [x] `new(migrations: Vec<Arc<dyn Migration>>)` - Create builder
    - [x] `with_initial_setup(F)` - Setup before any migrations run
    - [x] `with_data_before(migration_id, F)` - Insert data BEFORE specified migration runs
    - [x] `with_data_after(migration_id, F)` - Insert data AFTER specified migration runs
    - [x] `skip_rollback()` - Skip rollback for debugging
    - [x] `with_table_name(String)` - Custom migration table
    - [x] `run(db)` - Execute the test scenario
  - [x] Implementation details:
    - [x] Sort breakpoints by migration order automatically
    - [x] Execute migrations in chunks between breakpoints
    - [x] Support multiple data insertions at different points
    - [x] Maintain exact migration ordering (alphabetical by ID)
    - [x] Full rollback at end unless `skip_rollback()` called

- [x] Export Migration Collections from moosicbox_schema ✅ **COMPLETE**
  - [x] Add function `get_sqlite_library_migrations() -> Vec<Arc<dyn Migration>>`
  - [x] Add function `get_sqlite_config_migrations() -> Vec<Arc<dyn Migration>>`
  - [x] Add function `get_postgres_library_migrations() -> Vec<Arc<dyn Migration>>`
  - [x] Add function `get_postgres_config_migrations() -> Vec<Arc<dyn Migration>>`
  - [x] Mark as `#[cfg(test)]` for test usage only
  - [x] Functions should call the internal migration source functions and extract migrations

- [x] Update Documentation ✅ **COMPLETE**
  - [x] Add builder pattern examples to test_utils documentation
  - [x] Document migration testing best practices
  - [x] Add example for data migration testing pattern
  - [x] Show difference between `with_data_before` and `with_data_after`

### API Design Rationale for Phase 8.3

**Why `with_data_before` and `with_data_after`?**
- **Clear timing**: Explicitly states when data insertion happens relative to migration
- **Flexible**: Supports both "insert old format data to be migrated" and "insert data for later migrations to use"
- **Intuitive**: Follows natural language patterns

**Why no `with_final_verification`?**
- **Simpler**: Tests can just assert after `run()` completes - database is in final state
- **Standard pattern**: Follows normal testing conventions
- **More readable**: Assertions visible in test function, not hidden in callbacks

**Example Usage:**
```rust
// Test data migration scenario
MigrationTestBuilder::new(get_sqlite_library_migrations())
    .with_table_name("__moosicbox_schema_migrations")
    .with_data_before(
        "2025-06-03-211603_cache_api_sources_on_tables",
        |db| Box::pin(async move {
            // Insert old format data that migration will transform
            db.exec_raw("INSERT INTO api_sources (entity_type, entity_id, source, source_id) VALUES ('artists', 1, 'Tidal', 'art123')").await
        })
    )
    .run(&db)
    .await?;

// Verify migration transformed data correctly
let artist = query::select("artists")
    .columns(&["id", "api_sources"])
    .where_eq("id", 1)
    .execute(&*db)
    .await?;
assert_eq!(artist[0].get("api_sources").unwrap().as_str().unwrap(), "[{\"id\":\"art123\",\"source\":\"Tidal\"}]");
```

### Success Criteria for Phase 8.3

- [x] MigrationTestBuilder supports the exact testing pattern used in `scan/src/output.rs` ✅
- [x] Builder provides ergonomic API with clear timing semantics ✅
- [x] Type-safe builder pattern prevents misuse ✅
- [x] Supports multiple data insertion points in single test ✅
- [x] Documentation includes clear examples showing before/after usage ✅
- [x] All existing test utility functions remain unchanged ✅
- [x] Integration with moosicbox_schema migration collections works seamlessly ✅

### Phase 8.3 Implementation Notes (Completed)

**Key Implementation Details:**
- ✅ MigrationTestBuilder successfully implemented with all required features
- ✅ Breakpoint system allows data insertion at any point in migration sequence
- ✅ Migration tracking table manually updated for breakpoint migrations
- ✅ Comprehensive test coverage with 6 test cases
- ✅ All clippy warnings resolved

**Key Implementation Decisions:**

1. **Breakpoint Grouping**: Multiple breakpoints on the same migration are grouped and executed in sequence:
   - All `with_data_before` actions for a migration run first
   - Then the migration runs
   - Then all `with_data_after` actions run
   - This prevents duplicate migration execution

2. **Migration Tracking**: Manual migration table updates are performed when running migrations directly to maintain consistency with `MigrationRunner`

3. **Error Handling**:
   - Clear error messages for non-existent migration IDs
   - Proper error propagation from breakpoint actions
   - All errors wrapped in `TestError` enum

4. **Test Coverage**: Comprehensive test suite verifying:
   - `test_with_data_before_breakpoint` - Data inserted before migration gets NULL for new columns
   - `test_with_data_after_breakpoint` - Data inserted after migration can use new columns
   - `test_multiple_breakpoints_in_sequence` - Multiple breakpoints including same migration
   - `test_initial_setup_functionality` - Initial setup runs before any migrations
   - `test_breakpoint_with_nonexistent_migration_id` - Proper error handling
   - `test_rollback_works_with_breakpoints` - Rollback functionality preserved

5. **Clippy Compliance**: All clippy warnings addressed:
   - Added `# Errors` documentation for public async functions
   - Moved `use` statements to top of functions
   - No items after statements

**Issue Discovered:**
- Default rollback behavior causes all existing tests to fail
- Tests expect migrations to persist for integration testing
- Rollback should be opt-in, not default
- Fix tracked in Phase 8.3.5

### Implementation Notes for Phase 8.3

**Breakpoint Execution Model:**
- Breakpoints are grouped by migration to prevent duplicate execution
- Execution order for each migration with breakpoints:
  1. Run all migrations before this one (if any)
  2. Execute all `with_data_before` actions for this migration
  3. Run the migration
  4. Execute all `with_data_after` actions for this migration
  5. Continue to next migration with breakpoints

**Migration Table Management:**
- When migrations are run directly (not through `MigrationRunner`), the builder manually updates the migration tracking table
- This ensures consistency with normal migration execution
- The table is created if it doesn't exist using the same schema as `MigrationRunner`

**Error Handling Strategy:**
- All errors are wrapped in `TestError` enum for consistent handling
- Migration not found errors use `MigrationError::Validation` variant
- Database errors are passed through transparently
- Clear error messages help with debugging test failures

### 8.3.5 Fix Default Rollback Behavior ⚠️ **CRITICAL**

**Issue Discovered:** During testing, all existing tests expect migrations to persist after execution, but MigrationTestBuilder defaults to rolling back migrations. This causes all tests to fail as they can't work with the schema.

**Status:** ✅ **COMPLETED**

- [x] **Update MigrationTestBuilder Default Behavior**
  - [x] Change `skip_rollback` field to `with_rollback` (defaults to false)
  - [x] Rename `skip_rollback()` method to `with_rollback()`
  - [x] Update constructor to set `with_rollback: false` by default
  - [x] Update `run()` method to check `if with_rollback` instead of `if !skip_rollback`

- [x] **Update Test Examples**
  - [x] Remove `.skip_rollback()` calls from existing tests (no longer needed)
  - [x] Update tests that need rollback to use `.with_rollback()`
  - [x] Update documentation examples

**Rationale:**
- All existing tests expect persistent schema for integration testing
- Rollback should be opt-in for migration reversibility testing
- Default behavior should match common use case (integration tests)

### Phase 8.3.5 Implementation Notes (Completed)

**Key Implementation Details:**
- ✅ Successfully inverted default rollback behavior
- ✅ Changed field from `skip_rollback: bool` to `with_rollback: bool`
- ✅ Renamed method from `skip_rollback()` to `with_rollback()`
- ✅ Updated all 32 tests to use new API
- ✅ Zero breaking changes for external users (unreleased API)

**Behavior Change:**
- **Old Default**: Migrations rolled back after execution (broke integration tests)
- **New Default**: Migrations persist after execution (supports integration testing)
- **Opt-in**: Use `.with_rollback()` for migration reversibility testing

**Test Updates:**
- Removed all `.skip_rollback()` calls (7 instances) - persistence is now default
- Added `.with_rollback()` to rollback test (1 instance)
- Updated test assertions to match new default behavior
- All 32 unit tests and 23 doc tests passing

### 8.4 Update Tests to Use New Utilities ✅ **COMPLETED**

**Prerequisites:** ✅ Phase 8.3.5 complete - MigrationTestBuilder rollback behavior fixed

**Goal:** Update existing tests to use the new test utilities instead of direct migration constants

**Status:** ✅ **COMPLETED** - All sub-phases were found to be already complete during implementation review.

#### 8.4.0 Migration Table Schema Alignment ✅ **ALREADY CORRECT**

**Discovery:** The assumed issue never existed - `switchy_schema` was implemented correctly from the start using the `id` column, matching `moosicbox_schema` expectations.

- [x] **switchy_schema already uses `id` column correctly** ✅ **COMPLETE**
  - [x] `ensure_table_exists()` creates table with `id` column
  - [x] `is_migration_applied()` uses WHERE clause with `id`
  - [x] `record_migration()` inserts into `id` column
  - [x] `get_applied_migrations()` selects `id` column
  - [x] `remove_migration()` uses WHERE clause with `id`

**Implementation Note:** No changes were needed - the original implementation was correct.

#### 8.4.1 Fix Migration Collection Accessibility ✅ **ALREADY CORRECT**

**Discovery:** Migration collection functions were never restricted to test-only usage.

- [x] **Migration collections already accessible to other packages** ✅ **COMPLETE**
  - [x] Functions only gated by database features (`#[cfg(feature = "sqlite")]`)
  - [x] Never marked with `#[cfg(test)]` restriction
  - [x] Available to any package that depends on `moosicbox_schema` with appropriate features

**Implementation Note:** No changes were needed - functions were already properly accessible.

#### 8.4.2 Update scan/src/output.rs Tests ✅ **COMPLETED**

**Discovery:** All scan tests had already been updated to use `MigrationTestBuilder` during earlier work.

- [x] **All 6 test locations successfully updated** ✅ **COMPLETE**
  - [x] Line 934: `test_update_api_sources` macro test (uses `with_data_before`)
  - [x] Line 1196: `can_scan_single_artist_with_single_album_with_single_track`
  - [x] Line 1338: `can_scan_multiple_artists_with_multiple_albums_with_multiple_tracks`
  - [x] Line 1537: `can_scan_multiple_artists_with_shared_albums`
  - [x] Line 1743: `can_scan_multiple_artists_with_shared_albums_without_api_source`
  - [x] Line 1930: `can_scan_multiple_artists_with_shared_albums_and_tracks`

- [x] **Implementation Pattern Used** ✅ **COMPLETE**
  - [x] Only 1 test uses `with_data_before` (the macro test that needs specific timing)
  - [x] Other 5 tests simply run all migrations without breakpoints (simpler approach)
  - [x] All tests use `get_sqlite_library_migrations().await.unwrap()`
  - [x] All tests specify custom table name `"__moosicbox_schema_migrations"`

**Implementation Note:** The actual implementation was simpler than planned - most tests don't need complex breakpoint patterns.

#### 8.4.3 Verification ✅ **COMPLETED**

- [x] **Test Compilation and Functionality** ✅ **COMPLETE**
  - [x] All packages compile without errors (`cargo clippy --all-targets`)
  - [x] All 7 scan tests pass successfully
  - [x] No "column named id" errors (issue never existed)
  - [x] Migration behavior identical to before:
    - [x] Same migration order (alphabetical by ID)
    - [x] Same table name (`__moosicbox_schema_migrations`)
    - [x] Same test data insertion timing
    - [x] Migration tracking table uses `id` column consistently

### Phase 8.4 Implementation Notes (Completed)

**Key Discoveries:**
- **Pre-completed Work**: Phase 8.4 was essentially already done during earlier phases
- **Correct Initial Implementation**: `switchy_schema` was implemented correctly from the start
- **Simpler Test Patterns**: Most tests don't need complex breakpoint patterns - just running all migrations works fine
- **Already Accessible APIs**: Migration collection functions were never test-restricted

**Actual Implementation Pattern:**
```rust
// Most tests use this simple pattern:
MigrationTestBuilder::new(get_sqlite_library_migrations().await.unwrap())
    .with_table_name("__moosicbox_schema_migrations")
    .run(&*db)
    .await
    .unwrap();

// Only 1 test needs breakpoint timing:
MigrationTestBuilder::new(get_sqlite_library_migrations().await.unwrap())
    .with_table_name("__moosicbox_schema_migrations")
    .with_data_before("2025-06-03-211603_cache_api_sources_on_tables", |db| Box::pin(async move {
        // Insert test data before specific migration
        Ok(())
    }))
    .run(&*db)
    .await
    .unwrap();
```

**Benefits Achieved:**
- ✅ Clean, maintainable test code
- ✅ No exposure of migration implementation details
- ✅ All existing functionality preserved
- ✅ Zero breaking changes to test behavior

### Migration Pattern Comparison

**Before (direct migration constants):**
```rust
// Complex, exposes implementation details
moosicbox_schema::sqlite::SQLITE_LIBRARY_MIGRATIONS.run_until(&*db, Some("migration_id")).await.unwrap();
db.exec_raw("INSERT INTO old_format_table ...").await.unwrap();
moosicbox_schema::sqlite::SQLITE_LIBRARY_MIGRATIONS.run(&*db).await.unwrap();
```

**After (test builder):**
```rust
// Clean, expressive, hides implementation details
MigrationTestBuilder::new(get_sqlite_library_migrations().await.unwrap())
    .with_table_name("__moosicbox_schema_migrations")
    .with_data_after("migration_id", |db| Box::pin(async move {
        db.exec_raw("INSERT INTO old_format_table ...").await?;
        Ok(())
    }))
    .run(&*db).await.unwrap();
```

### 8.5 Testing & Validation

**Prerequisites:** ✅ Phase 8.4 complete - All tests updated to use new utilities

**Goal:** Ensure all existing functionality works correctly with the new architecture

**Current Status:** ✅ **COMPLETE** - All functionality verified and new features demonstrated

- [x] Verify Existing Tests ✅ **COMPLETE**
  - [x] All existing tests pass without modification:
    - ✅ All 7 `scan/src/output.rs` tests using new MigrationTestBuilder (passing)
    - ✅ All `moosicbox_schema` tests continue to work (6 tests passing)
    - ✅ All `switchy_schema` core tests (23 tests passing)
    - ✅ All `switchy_schema_test_utils` tests (32 + 23 tests passing)
  - [x] Migration behavior verified identical to before:
    - ✅ Same migration order (alphabetical by ID)
    - ✅ Same table name (`__moosicbox_schema_migrations`)
    - ✅ Same environment variable support (preserved in wrapper)
    - ✅ Same error handling patterns

- [x] Test New Features ✅ **COMPLETE**
  - [x] Add test demonstrating rollback functionality (new capability!)
  - [x] Add test for `.with_rollback()` functionality in real scenarios
  - [x] Add test with multiple `with_data_before` and `with_data_after` calls
  - [x] Verify environment variable support still works in integration scenarios

- [x] Migration Order Verification ✅ **COMPLETE**
  - ✅ Migrations run in same order as before (alphabetical by ID)
  - ✅ `with_data_before` stops at correct migration (verified in scan tests)
  - ✅ Already-applied migrations are skipped (core functionality working)
  - ✅ Rollback works correctly (comprehensive test coverage in switchy_schema)

### Phase 8.5 Implementation Notes (Completed)

**New Integration Tests Added:**

1. **Rollback Demonstration** (`demonstrate_rollback_functionality`)
   - Creates a table migration with rollback capability
   - Verifies table is created and then properly removed after rollback
   - Demonstrates the `.with_rollback()` functionality in action

2. **Complex Breakpoint Patterns** (`demonstrate_complex_breakpoint_patterns`)
   - Tests multiple `with_data_before` and `with_data_after` calls in single test
   - Demonstrates data insertion at different migration points
   - Verifies data state changes correctly (NULL vs populated columns)
   - Shows realistic data migration testing scenarios

3. **Environment Variable Integration** (`demonstrate_environment_variable_integration`)
   - Tests `MOOSICBOX_SKIP_MIGRATION_EXECUTION=1` functionality
   - Verifies migrations are skipped but function calls succeed
   - Confirms no migration tracking table is created when skipped
   - Demonstrates end-to-end environment variable support

**Key Achievements:**
- ✅ All new features properly demonstrated with integration tests
- ✅ 35 total tests passing in `switchy_schema_test_utils`
- ✅ All existing functionality continues to work (scan tests: 7 passing)
- ✅ Environment variable behavior correctly validated
- ✅ Rollback capability proven in realistic scenarios
- ✅ Complex breakpoint patterns working as designed

**Technical Implementation:**
- Used proper `#[cfg(all(test, feature = "sqlite"))]` gating for moosicbox_schema integration
- Correctly handled environment variable values ("1" not "true")
- Proper trait imports for database query operations
- Clean error handling and test isolation

### 8.6 Documentation & Cleanup ✅ **COMPLETED**

**Prerequisites:** ✅ Phase 8.5 complete - All testing validated

**Goal:** Document changes and remove obsolete code

**Status:** ✅ **COMPLETED** - All documentation updated and code already clean

- [x] Code Cleanup ✅ **MINOR**
  - [x] Remove old migration constant exports from moosicbox_schema - ✅ None existed (already clean from Phase 8.2)
  - [x] Remove `sqlite` and `postgres` modules from public API - ✅ None existed (already clean from Phase 8.2)
  - [x] Clean up unused imports - ✅ Verified clean (no warnings)
  - [x] Remove any remaining references to old migration constants - ✅ None found

- [x] Documentation Updates ✅ **MINOR**
  - [x] Update moosicbox_schema package README with new architecture - ✅ Completely rewritten
  - [x] Document that tests should use MigrationTestBuilder instead of direct constants - ✅ Included in README
  - [x] Add examples showing migration testing best practices - ✅ Comprehensive examples added
  - [x] Document the new test-only migration collection functions - ✅ All 4 functions documented
  - [x] Add migration guide for updating existing tests - ✅ Created MIGRATION_GUIDE.md

**Implementation Notes (Added 2025-01-15):**

✅ **Phase 8.6 Completed Successfully:**
- The code cleanup items were discovered to already be complete from Phase 8.2's proper implementation
- No old migration constants or modules were publicly exported (implementation was already clean)
- Documentation comprehensively updated with modern architecture description
- Created detailed MIGRATION_GUIDE.md for developers migrating existing tests
- Zero compromises made - all requirements fully satisfied

### Success Criteria

- [x] Custom table names work in switchy_schema (Phase 8.1) ✅
- [x] moosicbox_schema successfully uses switchy_schema (Phase 8.2) ✅
- [x] Test utilities support complex migration testing patterns with clear timing semantics (Phase 8.3) ✅
- [x] MigrationTestBuilder defaults to persistent migrations (no rollback) ✅
- [x] Rollback is opt-in via `.with_rollback()` method ✅
- [x] All existing tests updated to use MigrationTestBuilder (Phase 8.4) ✅
- [x] All existing tests pass without behavioral changes (Phase 8.5) ✅
- [x] Documentation fully updated with new architecture (Phase 8.6) ✅
- [x] Migration guide created for updating existing tests (Phase 8.6) ✅
- [x] Migration table remains `__moosicbox_schema_migrations` ✅
- [x] Migration order is preserved (alphabetical by ID) ✅
- [x] Environment variable support maintained ✅
- [x] No changes required to calling code (server/src/lib.rs, events/profiles_event.rs) ✅
- [x] **build.rs remains unchanged and continues to trigger recompilation on migration changes** ✅
- [x] Functions compile without warnings when all features are enabled ✅
- [x] Single unified API regardless of feature combination ✅
- [x] Migration constants no longer exposed in public API ✅

### Benefits of This Migration

1. **Code Reduction**: ~260 lines → ~150 lines (42% reduction) ✅ ACHIEVED
2. **New Features**:
   - ✅ Rollback support (available through test utilities)
   - ✅ Dry-run mode (available but not yet exposed)
   - ✅ Migration hooks (available but not yet exposed)
   - ✅ Better error handling
   - ✅ Comprehensive test utilities
     - ✅ Advanced test builder pattern with clear timing semantics (Phase 8.3)
     - ✅ **Intuitive test defaults**: Migrations persist by default for integration testing
3. **Improved Maintainability**: Single migration system to maintain ✅
4. **Zero Breaking Changes**: All existing code continues to work ✅
5. **Better Testing**: Ergonomic test utilities replace direct constant access ✅ COMPLETE
6. **Cleaner API**: Migration implementation details no longer exposed ✅ COMPLETE

### Risk Mitigation

1. **Risk**: Different migration ordering
   - **Mitigation**: Both use BTreeMap with alphabetical sorting

2. **Risk**: Table name incompatibility
   - **Mitigation**: ~~Phase 8.1 enables custom table names~~ ✅ RESOLVED - Custom table names fully working

3. **Risk**: Test failures
   - **Mitigation**: Compatibility layer maintains exact same API

4. **Risk**: Missing environment variable support
   - **Mitigation**: Explicitly handle in wrapper implementation

5. **Risk**: Accidentally "fixing" the dual-migration behavior
   - **Mitigation**: Document that running both migrations when both features are enabled is intentional for development/testing


### Phase 8 Lessons Learned

1. **Default Behavior Matters**: The initial implementation defaulted to rollback, which broke all existing tests. The default should match the most common use case (integration testing with persistent schema).

2. **Test Builder Ergonomics**: The `MigrationTestBuilder` provides a much cleaner API than direct migration constant access, hiding implementation details while providing more flexibility.

3. **Incremental Migration**: Successfully migrating from a custom implementation to a generic one requires maintaining 100% backward compatibility while gradually introducing new features.

4. **Debug Logging**: Temporary debug logging was crucial for understanding the issue with migration execution in the test builder.

### Note on Callers
No changes needed! The two places that use moosicbox_schema will continue to work exactly as before:
- `packages/server/src/lib.rs` - calls `migrate_config()`
- `packages/server/src/events/profiles_event.rs` - calls `migrate_library()`

### Key Discoveries During Phase 8 Implementation

**Discoveries that differed from expectations:**

1. **Code Already Clean (Phase 8.6)**: The implementation from Phase 8.2 was so thorough that no old migration constants or modules existed to remove. The wrapper was already properly abstracted with no cleanup needed.

2. **Simpler Test Patterns**: Most tests don't need complex breakpoint patterns - simple `MigrationTestBuilder::new().run()` suffices for integration testing. Only 1 out of 6 scan tests needed the complex `with_data_before` pattern.

3. **Default Behavior Importance**: Initial rollback-by-default broke all tests because they expect persistent schema for integration testing. Changing to persist-by-default matched actual usage patterns.

4. **Documentation Gap**: The original README was completely outdated, showing the importance of keeping documentation synchronized with implementation changes.

**Implementation insights:**
- Zero compromises were needed - all requirements were achievable
- The generic architecture proved robust and extensible
- Test migration patterns are simpler than initially expected
- Documentation quality significantly impacts developer experience

## Phase 9: Migration Listing

**Goal:** Provide ability to list available migrations

### 9.1 List Implementation ✅ **COMPLETED**

- [x] Add `list()` method to migration sources ✅ **MINOR**
  - `packages/switchy/schema/src/migration.rs:145-155` - Default `list()` method implementation in `MigrationSource` trait
  - [x] Returns list of available migrations
    - `packages/switchy/schema/src/migration.rs:146` - `let migrations = self.migrations().await?;`
  - [x] Include migration ID, description if available
    - `packages/switchy/schema/src/migration.rs:149-152` - Maps `migration.id()` and `migration.description()` to `MigrationInfo` fields
  - [x] Indicate which migrations have been applied
    - `packages/switchy/schema/src/runner.rs:489-493` - `MigrationRunner::list_migrations()` updates `applied` field using `VersionTracker::get_applied_migrations()`
  - [x] Sort by migration order
    - `packages/switchy/schema/src/runner.rs:496` - `migrations.sort_by(|a, b| a.id.cmp(&b.id));`

### Phase 9.1 Implementation Notes (Completed)

**Key Implementation Details:**
- ✅ **MigrationInfo Struct**: Added `MigrationInfo` struct to `migration.rs` with `id`, `description`, and `applied` fields
  - `packages/switchy/schema/src/migration.rs:103-111` - `MigrationInfo` struct definition with all required fields
- ✅ **Default list() Implementation**: Added default `list()` method to `MigrationSource` trait that calls `migrations()` and extracts metadata
  - `packages/switchy/schema/src/migration.rs:145-155` - Default `list()` implementation in `MigrationSource` trait
- ✅ **MigrationRunner Integration**: Added `list_migrations()` method to `MigrationRunner` that combines source list with database applied status
  - `packages/switchy/schema/src/runner.rs:476-499` - `list_migrations()` method implementation
- ✅ **Applied Status Detection**: Uses `VersionTracker::get_applied_migrations()` to determine which migrations have been applied
  - `packages/switchy/schema/src/runner.rs:486` - `let applied_migrations = self.version_tracker.get_applied_migrations(db).await?;`
- ✅ **Consistent Sorting**: All migrations sorted by ID for deterministic ordering across all sources
  - `packages/switchy/schema/src/runner.rs:496` - `migrations.sort_by(|a, b| a.id.cmp(&b.id));`
- ✅ **All Sources Supported**: Default implementation works for all existing sources (embedded, directory, code)
  - Default trait implementation automatically applies to all sources without requiring individual implementations

**Technical Architecture:**
- **Two-Level API**: `MigrationSource::list()` provides base listing, `MigrationRunner::list_migrations()` adds database status
- **Zero Breaking Changes**: All existing code continues to work, new functionality is purely additive
- **Efficient Implementation**: Queries database once and uses HashSet for O(1) applied status lookup
- **Consistent Behavior**: Same sorting and metadata extraction across all migration sources

**Test Coverage:**
- ✅ Unit tests for `MigrationInfo` struct creation and manipulation
  - `packages/switchy/schema/src/migration.rs:185-194` - `test_migration_info_creation()` test function
- ✅ Tests for default `list()` implementation with mock migration source
  - `packages/switchy/schema/src/migration.rs:196-224` - `test_default_list_implementation()` test function
- ✅ Integration tests for `MigrationRunner::list_migrations()` with actual database
  - `packages/switchy/schema/src/runner.rs:830-842` - `test_list_migrations_empty_source()` test
  - `packages/switchy/schema/src/runner.rs:844-902` - `test_list_migrations_with_applied_status()` test
- ✅ Tests for applied/unapplied status detection with partial migration runs
  - `packages/switchy/schema/src/runner.rs:884-901` - Applied status validation in `test_list_migrations_with_applied_status()`
- ✅ Tests for migration ordering and sorting behavior
  - `packages/switchy/schema/src/runner.rs:862-871` - Non-alphabetical insertion with alphabetical verification
- ✅ Tests for CodeMigrationSource list() method
  - `packages/switchy/schema/src/discovery/code.rs:291-318` - `test_code_migration_source_list()` test function
- ✅ All existing tests continue to pass (28 unit tests + 12 doc tests)
  - Command `cargo test -p switchy_schema` output shows "28 passed; 0 failed" and "12 passed; 0 failed"

**Design Decisions:**
1. **Applied Status Default**: `MigrationSource::list()` defaults `applied` to `false` since it doesn't have database access
2. **Database Required for Status**: Real applied status requires database connection through `MigrationRunner::list_migrations()`
3. **Reuse Existing APIs**: Leverages existing `migrations()` method for consistency and maintenance
4. **Sort by ID**: Uses migration ID for sorting to match existing behavior in the runner
5. **Description Support**: Properly handles optional descriptions from `Migration::description()` method

**Benefits Achieved:**
- ✅ **Developer Visibility**: Developers can now list and inspect available migrations
- ✅ **Status Awareness**: Clear indication of which migrations have been applied
- ✅ **Tooling Foundation**: Provides foundation for CLI tools and migration status commands
- ✅ **Debugging Support**: Helps with migration debugging and troubleshooting
- ✅ **Zero Overhead**: No performance impact on existing migration execution

## Phase 10: Documentation & Examples

**Goal:** Comprehensive documentation and usage examples

### 10.1 API Documentation ✅ **COMPLETED**

- [x] `packages/switchy/schema/src/lib.rs` - API docs ✅ **IMPORTANT**
  - `packages/switchy/schema/src/lib.rs:1-143` - Comprehensive module documentation with architecture overview and usage examples
  - [x] Basic rustdoc for public APIs
    - `packages/switchy/schema/src/lib.rs:153-179` - MigrationError enum and Result type documentation
    - `packages/switchy/schema/src/discovery/mod.rs:1-39` - Discovery module overview with method comparison
    - `packages/switchy/schema/src/discovery/embedded.rs:1-68` - Embedded migrations documentation with examples
    - `packages/switchy/schema/src/discovery/directory.rs:1-56` - Directory migrations documentation with examples
    - `packages/switchy/schema/src/version.rs:1-57` - Version tracking documentation with usage examples
  - [x] Simple usage examples
    - `packages/switchy/schema/src/lib.rs:20-51` - Quick start with embedded migrations
    - `packages/switchy/schema/src/lib.rs:56-68` - Migration listing example
    - `packages/switchy/schema/src/lib.rs:75-86` - Custom configuration example
    - `packages/switchy/schema/src/version.rs:25-44` - Version tracker usage examples

### Phase 10.1 Implementation Notes (Completed)

**Key Documentation Added:**

- ✅ **Comprehensive lib.rs Documentation**: Added 143 lines of module-level documentation covering:
  - Core features and capabilities overview
  - Quick start guide with multiple examples
  - Architecture explanation linking to all modules
  - Migration source comparison and recommendations
  - Testing support overview
- ✅ **All Public APIs Documented**: Every public struct, enum, trait, and function now has rustdoc:
  - MigrationError enum with detailed error descriptions
  - Result type alias explanation
  - VersionTracker with usage examples and schema details
  - All discovery modules with feature comparisons
- ✅ **Discovery Module Documentation**: Each discovery method now has comprehensive docs:
  - Feature comparison table in discovery/mod.rs
  - Embedded migrations with compile-time benefits
  - Directory migrations with development workflow
  - Code migrations with programmatic examples
- ✅ **Working Code Examples**: All documentation includes practical examples:
  - Multiple quick-start scenarios
  - Configuration customization examples
  - Migration listing and status checking
  - Version tracker direct usage
- ✅ **Error-Free Doc Tests**: 24 documentation tests all pass:
  - 18 compiled and executed successfully
  - 6 properly ignored (require external migration directories)
  - Zero test failures or compilation errors

**Documentation Architecture:**

- **Hierarchical Information**: Overview in lib.rs, details in module docs
- **Multiple Entry Points**: Quick start, detailed examples, and API reference
- **Feature-Aware**: Documentation respects feature gates and optional functionality
- **Error Handling**: Comprehensive error documentation with usage guidance
- **Real-World Examples**: Practical scenarios matching actual use cases

**Technical Quality:**

- **28 unit tests + 18 doc tests** all passing
- **Zero compromises**: All requirements fully satisfied
- **Consistent Style**: Follows rust documentation best practices
- **Cross-Referenced**: Liberal use of doc links between modules
- **Accessibility**: Clear explanations for both beginners and advanced users

### 10.2 Usage Examples

**Goal:** Create clean examples that demonstrate schema migrations using type-safe query builders rather than raw SQL

#### 10.2.1 Database Transaction Support ❌ **CRITICAL**

**Goal:** Add transaction support to switchy_database to enable safe schema operations, particularly for SQLite workarounds

**Background:** Transaction support is fundamental for safe database operations. The schema builder extensions (10.2.2) require proper transaction handling, especially for SQLite table recreation workarounds.

**Key Design Decisions Made:**
- ✅ **Trait-based approach**: `DatabaseTransaction: Database` provides uniform API across backends
- ✅ **Manual rollback required**: No auto-rollback on drop, users must explicitly commit or rollback
- ✅ **Backend-specific implementations**: Each backend uses optimal internal approach (Arc<Mutex>, pool connections, etc.)
- ✅ **Type-erased transactions**: Return `Box<dyn DatabaseTransaction>` for ergonomic generic usage
- ✅ **Full Database trait support**: All CRUD and schema operations work within transactions
- ✅ **Connection pool aware**: Properly handles sqlx pools and connection lifecycle

##### 10.2.1.1 Add Transaction Types and Traits ✅ **COMPLETED**

**Design Decision:** Use trait-based type erasure to provide uniform transaction API across all database backends while allowing each backend to use optimal internal implementation.

**Implementation Notes (Completed):**
- Removed `exec_in_transaction` convenience method to maintain dyn-compatibility
- Removed `database_type()` method - each backend handles its own limitations internally
- Transaction state tracking deferred to individual backend implementations
- Prioritized backward compatibility and clean abstractions over convenience features

- [x] Create `DatabaseTransaction` trait in `packages/database/src/lib.rs`
  - [x] Extend Database trait: `trait DatabaseTransaction: Database + Send + Sync`
  - [x] Add methods: `async fn commit(self: Box<Self>) -> Result<(), DatabaseError>`
  - [x] Add methods: `async fn rollback(self: Box<Self>) -> Result<(), DatabaseError>`
  - [x] Note: `self: Box<Self>` works automatically with boxed trait objects - users call `tx.commit()` directly
  - [x] Add manual rollback requirement - no auto-rollback on drop
  - [x] Add comprehensive documentation with usage patterns and error handling
- [x] Add transaction methods to Database trait:
  - [x] Add `async fn begin_transaction(&self) -> Result<Box<dyn DatabaseTransaction>, DatabaseError>`

- [x] **Transaction usage patterns and ergonomics:**
  - [x] Ensure `&*tx` dereferences to `&dyn Database` for execute() calls
  - [x] Document pattern: `tx.insert(...).execute(&*tx).await?` then `tx.commit().await?`
  - [x] Alternative: implement `Deref` for transaction types to auto-deref to `&dyn Database`
  - [x] Ensure transaction can be used multiple times before commit/rollback
- [x] **Error handling semantics:**
  - [x] No "poisoned" state tracking - transactions remain usable after failed operations
  - [x] Users decide whether to continue operations or rollback after errors
  - [x] Document that commit() may fail if previous operations corrupted state
- [x] **Recursive transaction prevention:**
  - [x] `begin_transaction()` called on a `DatabaseTransaction` returns `Err(DatabaseError::AlreadyInTransaction)`
  - [x] Add `AlreadyInTransaction` variant to `DatabaseError` enum
  - [x] Document that nested transactions require savepoints (Phase 13)
- [x] Update `DatabaseError` enum in `packages/database/src/lib.rs`:
  - [x] Add `AlreadyInTransaction` variant for nested transaction attempts
  - [x] Add `TransactionCommitted` variant if operations attempted after commit
  - [x] Add `TransactionRolledBack` variant if operations attempted after rollback

**Actual Implementation (Phase 10.2.2):**
- [x] All 6 database backends have stub `begin_transaction()` implementations
- [x] Each returns appropriate error indicating transaction support not yet implemented
- [x] Ready for actual implementation in phases 10.2.1.3-10.2.1.11
- [x] Test databases in other packages updated with stub implementations
- [x] Database trait remains dyn-compatible - no breaking changes
- [x] All existing code continues to compile and work

##### 10.2.1.2 Transaction Isolation Architecture ❌ **CRITICAL DESIGN PHASE**

**Problem Identified:** The naive approach of sharing connections between Database and DatabaseTransaction instances causes transaction poisoning - operations on the original database during a transaction execute within that transaction, breaking isolation guarantees.

**Solution:** Hybrid connection management approach that provides true transaction isolation while maintaining backward compatibility with the Database trait.

**Core Requirements:**
- **No Breaking Changes**: Existing Database trait and usage patterns must work unchanged
- **True Isolation**: Transactions must use separate connections to prevent poisoning
- **Resource Efficiency**: Lazy secondary connection creation to minimize overhead
- **Thread Safety**: Proper atomic state management for concurrent access

**Architecture Decision: Hybrid Connection Management**

Each database backend will implement:
1. **Primary Connection**: Used for normal Database operations when no transaction is active
2. **Separate Transaction Connections**: Each transaction gets its own dedicated connection
3. **Secondary Connection**: Created lazily for non-transactional operations during active transactions
4. **Atomic State Tracking**: `transaction_active` flag prevents connection conflicts

**Backend-Specific Strategies:**
- **SQLite (file-based)**: Create new Connection per transaction from file path
- **PostgreSQL**: Use Client cloning or connection pool checkout
- **MySQL**: Use connection pool checkout
- **SqlX (all)**: Use PoolConnection checkout for transactions

**Backward Compatibility Guarantee:**
- All existing code using `&dyn Database` continues to work unchanged
- Transaction API remains identical: `tx.commit()`, `tx.rollback()`
- Query execution patterns unchanged: `stmt.execute(&*tx)`
- Same error types and handling

**Performance Considerations:**
- Connection creation overhead acceptable for transaction correctness
- Lazy secondary connection creation minimizes resource usage
- Prepared statement caching per connection maintains query performance
- Connection pooling strategies per backend for optimal resource utilization

## Implementation Trade-offs

### Rusqlite Transaction Serialization
**Decision**: Use serialized locking for all rusqlite transactions
**Rationale**:
- SQLite in-memory databases cannot share data between connections
- Creating separate connections would result in empty databases for transactions
- Serialization ensures consistency between in-memory and file-based databases

**Trade-off**:
- Non-transactional operations block during active transactions
- Acceptable because SQLite already serializes write operations internally
- Read concurrency optimization deferred to Phase 14

**Impact**: Correct isolation semantics at cost of some read concurrency

##### 10.2.1.3 Implement for SQLite (rusqlite) ✅ **COMPLETED**

**Prerequisites:**
- ✅ Phase 10.2.1.1 complete - DatabaseTransaction trait and stub implementations ready

**Status**: ✅ **COMPLETE** - Semaphore-based serialization successfully implemented

**Implementation Notes**:
- ✅ Full Database trait implementation for RusqliteTransaction
- ✅ **FIXED**: Removed separate connection logic, uses same connection for all databases
- ✅ **IMPLEMENTED**: Serialized locking via Arc<tokio::sync::Semaphore> with 1 permit
- ✅ **CONSISTENT**: File-based and in-memory databases have identical isolation semantics
- ✅ 5 comprehensive tests including concurrency and isolation verification

**Architecture Decision: Serialized Locking Approach**

After analysis, we chose serialized access over separate connections:

**Problem**: SQLite in-memory databases are connection-scoped
- Each connection has its own separate in-memory database
- Creating separate connections would result in empty databases for transactions
- File-based and in-memory databases would behave inconsistently

**Solution**: Serialize all database access during transactions using Semaphore
- Use `transaction_lock: Arc<tokio::sync::Semaphore>` with 1 permit for exclusive access
- Acquire `OwnedSemaphorePermit` in transaction - automatically released on drop
- Both file-based and in-memory databases get identical isolation semantics
- Clean ownership model without lifetime complications

**Technical Implementation with Semaphore:**
```rust
// In RusqliteDatabase struct:
transaction_lock: Arc<tokio::sync::Semaphore>, // Initialized with Semaphore::new(1)

// In begin_transaction():
let permit = self.transaction_lock.acquire_owned().await
    .map_err(|_| DatabaseError::LockAcquisitionFailed)?;

// In RusqliteTransaction struct:
_permit: tokio::sync::OwnedSemaphorePermit, // Automatically released on drop
```

**Why Semaphore over Mutex<()>:**
- `OwnedSemaphorePermit` can be stored in the transaction struct (no lifetime issues)
- Automatic release on drop ensures cleanup even on panic
- Clean ownership model - the transaction "owns" exclusive access

**Implementation Requirements:**
- **NO separate connections** - Both in-memory and file-based databases MUST use the same connection
- **Semaphore-based serialization** - Use `Arc<tokio::sync::Semaphore>` with 1 permit
- **Owned permit storage** - Transaction holds `OwnedSemaphorePermit` for its lifetime
- **Automatic cleanup** - Permit automatically released when transaction is dropped
- **Defer optimizations** - File-based optimization belongs in Phase 14, NOT here
- **Test concurrency** - Must have tests that verify transactions are serialized, not parallel

**Implementation Status:**

- [x] **Database Struct Updates**: Added `transaction_lock: Arc<tokio::sync::Semaphore>` (1 permit) to RusqliteDatabase
- [x] **Permit Acquisition**: `begin_transaction()` acquires `OwnedSemaphorePermit` before BEGIN
- [x] **Removed Separate Connections**: Fixed lines 562-563, now uses same connection for all databases
- [x] **Transaction Struct**: RusqliteTransaction stores `_permit: OwnedSemaphorePermit`
- [x] **Serialized Access**: Second transaction waits for first to complete (permit release)
- [x] **Full Database Trait**: All methods implemented in RusqliteTransaction with proper semaphore usage
- [x] **State Management**: Proper commit/rollback with atomic flags
- [x] **Concurrent Transaction Tests**: Tests verify serialization behavior and isolation

**Trade-offs Accepted**:
- ✅ **Correctness Over Performance**: Serialized access ensures perfect isolation
- ✅ **Consistent Behavior**: All database types work identically
- ⚠️ **Reduced Concurrency**: Non-transactional operations block during transactions
- ✅ **Acceptable Impact**: SQLite already serializes writes internally

**Testing Status**:
- [x] Transaction commit/rollback functionality
- [x] Multiple operations within single transaction
- [x] State tracking prevents double commit/rollback
- [x] Nested transaction rejection (unsupported)
- [x] All CRUD operations within transactions
- [x] **Transaction isolation test**: Perfect isolation achieved through semaphore blocking
- [x] Resource cleanup and error handling
- [x] Backward compatibility verification
- [x] **Concurrent transaction serialization**: Verified only one transaction can be active at a time
- [x] **SimulationDatabase integration**: Fixed delegation to inner RusqliteDatabase transactions
- [x] **Cross-feature compatibility**: Tests work with switchy_async and any feature combination

**Phase Relationship:**
- **Phase 10.2.1.3** (this phase): Implements correctness through serialization for ALL database types
- **Phase 14**: Will optimize performance through parallelization for file-based databases only
- Current implementation must use identical serialized locking for all databases
- Performance optimizations intentionally deferred to maintain code clarity and correctness

##### 10.2.1.4 Implement for SQLite (sqlx)

**Prerequisites:** ✅ Phase 10.2.1.3 complete - Semaphore-based serialization proven with rusqlite

**Challenge:** sqlx::Transaction has lifetime limitations. Apply hybrid connection architecture with sqlx Pool.

**Hybrid Connection Architecture for SqlX:**

- [ ] Update `SqliteSqlxDatabase` struct for isolation:
  - [ ] Keep existing `pool: Arc<Pool<Sqlite>>` for backward compatibility
  - [ ] Add `transaction_active: Arc<AtomicBool>` for atomic state tracking
  - [ ] Add `secondary_conn: Arc<Mutex<Option<PoolConnection<Sqlite>>>>` for operations during transactions

- [ ] Create `SqliteSqlxTransaction` struct with owned connection:
  - [ ] Store `connection: Option<PoolConnection<Sqlite>>` (OWNED pool connection)
  - [ ] Store `parent_flag: Arc<AtomicBool>` reference to parent's transaction_active flag
  - [ ] Store `committed: AtomicBool` and `rolled_back: AtomicBool` for state tracking
  - [ ] Implement `Drop` trait to return connection to pool and clear parent flag

- [ ] Update `begin_transaction()` in SqliteSqlxDatabase:
  - [ ] Atomically check and set `transaction_active` flag
  - [ ] Acquire dedicated connection from pool for transaction
  - [ ] Execute "BEGIN IMMEDIATE" command on acquired connection
  - [ ] Return `SqliteSqlxTransaction` with owned pool connection

- [ ] Update Database trait implementation in SqliteSqlxDatabase:
  - [ ] Modify write operations to check transaction state
  - [ ] Use secondary pool connection when transaction is active (lazy acquisition)
  - [ ] Ensure proper connection return to pool

- [ ] Implement `DatabaseTransaction` trait for SqliteSqlxTransaction:
  - [ ] Implement all Database trait methods using owned pool connection
  - [ ] Implement `commit()` and `rollback()` using sqlx query execution
  - [ ] Ensure connection returns to pool on drop

- [ ] Add comprehensive tests including pool behavior and isolation guarantees

##### 10.2.1.5 Implement for PostgreSQL (postgres)

**Prerequisites:** ✅ Phase 10.2.1.4 complete - Hybrid connection approach proven with pool-based backends

**Challenge:** tokio-postgres::Transaction has lifetime limitations. Apply hybrid connection architecture with Client cloning.

**Hybrid Connection Architecture for tokio-postgres:**

- [ ] Update `PostgresDatabase` struct for isolation:
  - [ ] Keep existing `client: Arc<Mutex<Client>>` for backward compatibility
  - [ ] Add `transaction_active: Arc<AtomicBool>` for atomic state tracking
  - [ ] Add `secondary_client: Arc<Mutex<Option<Client>>>` for operations during transactions
  - [ ] Store connection parameters for creating new clients

- [ ] Create `PostgresTransaction` struct with owned client:
  - [ ] Store `client: Option<Client>` (OWNED client, not shared)
  - [ ] Store `parent_flag: Arc<AtomicBool>` reference to parent's transaction_active flag
  - [ ] Store `committed: AtomicBool` and `rolled_back: AtomicBool` for state tracking
  - [ ] Implement `Drop` trait to clear parent flag and auto-rollback if needed

- [ ] Update `begin_transaction()` in PostgresDatabase:
  - [ ] Atomically check and set `transaction_active` flag
  - [ ] Create new Client connection for transaction (clone connection params)
  - [ ] Execute "BEGIN" command on new client
  - [ ] Return `PostgresTransaction` with owned client

- [ ] Update Database trait implementation in PostgresDatabase:
  - [ ] Modify write operations to check transaction state
  - [ ] Use secondary client when transaction is active (lazy creation)
  - [ ] PostgreSQL supports concurrent reads and writes better than SQLite

- [ ] Implement `DatabaseTransaction` trait for PostgresTransaction:
  - [ ] Implement all Database trait methods using owned client
  - [ ] Implement `commit()` and `rollback()` using client transaction commands
  - [ ] Proper state validation and cleanup

- [ ] Add comprehensive tests including client management and isolation guarantees

##### 10.2.1.6 Implement for PostgreSQL (sqlx)

**Prerequisites:** ✅ Phase 10.2.1.5 complete - PostgreSQL-specific isolation patterns established

**Hybrid Connection Architecture for PostgreSQL sqlx:**

- [ ] Update `PostgresSqlxDatabase` struct following established pattern:
  - [ ] Keep existing `pool: Arc<Pool<Postgres>>` for backward compatibility
  - [ ] Add `transaction_active: Arc<AtomicBool>` for atomic state tracking
  - [ ] Add `secondary_conn: Arc<Mutex<Option<PoolConnection<Postgres>>>>` for operations during transactions

- [ ] Create `PostgresSqlxTransaction` struct with owned connection:
  - [ ] Store `connection: Option<PoolConnection<Postgres>>` (OWNED pool connection)
  - [ ] Store `parent_flag: Arc<AtomicBool>` reference to parent's transaction_active flag
  - [ ] Store `committed: AtomicBool` and `rolled_back: AtomicBool` for state tracking
  - [ ] Implement `Drop` trait for proper resource cleanup

- [ ] Apply hybrid connection management pattern established in previous phases
- [ ] Implement transaction isolation using dedicated pool connections
- [ ] Add comprehensive tests including PostgreSQL-specific concurrency scenarios

##### 10.2.1.7 Implement for MySQL (sqlx)

**Prerequisites:** ✅ Phase 10.2.1.6 complete - All major database patterns established

**Hybrid Connection Architecture for MySQL sqlx:**

- [ ] Update `MysqlSqlxDatabase` struct following established sqlx pattern:
  - [ ] Keep existing `pool: Arc<Pool<MySql>>` for backward compatibility
  - [ ] Add `transaction_active: Arc<AtomicBool>` for atomic state tracking
  - [ ] Add `secondary_conn: Arc<Mutex<Option<PoolConnection<MySql>>>>` for operations during transactions

- [ ] Create `MysqlSqlxTransaction` struct with owned connection:
  - [ ] Store `connection: Option<PoolConnection<MySql>>` (OWNED pool connection)
  - [ ] Store `parent_flag: Arc<AtomicBool>` reference to parent's transaction_active flag
  - [ ] Store `committed: AtomicBool` and `rolled_back: AtomicBool` for state tracking
  - [ ] Implement `Drop` trait for proper resource cleanup

- [ ] Apply hybrid connection management pattern consistent with other sqlx backends
- [ ] Implement transaction isolation using dedicated pool connections
- [ ] Add comprehensive tests including MySQL-specific isolation behavior
  - [ ] Verify transaction remains valid for multiple operations
- [ ] Add comprehensive unit tests

##### 10.2.1.8 Implement for Database Simulator

**Prerequisites:** ✅ Phase 10.2.1.7 complete - All production database backends support isolation

**Hybrid Architecture for Simulator (Test Environment):**

- [ ] Update `SimulatorDatabase` struct for isolation testing:
  - [ ] Add `transaction_active: Arc<AtomicBool>` consistent with other backends
  - [ ] Add state tracking for operations during transactions
  - [ ] Support testing isolation scenarios

- [ ] Create `SimulatorTransaction` struct following established pattern:
  - [ ] Store `parent_flag: Arc<AtomicBool>` reference for consistency
  - [ ] Store `committed: AtomicBool` and `rolled_back: AtomicBool` for state tracking
  - [ ] Track all operations performed within transaction for test verification
  - [ ] Support operation rollback by reverting stored operations

- [ ] Implement mock transaction isolation:
  - [ ] Separate transaction operations from main database operations
  - [ ] Support testing transaction poisoning prevention
  - [ ] Enable verification of isolation guarantees in tests

- [ ] Add extensive testing capabilities for migration and isolation verification

##### 10.2.1.9 Add Comprehensive Transaction and Isolation Tests

- [ ] **Backend-specific functionality tests** for each database implementation:
  - [ ] Test commit flow for all backends (rusqlite, sqlx sqlite/postgres/mysql, simulator)
  - [ ] Test rollback flow for all backends
  - [ ] Test manual rollback requirement (no auto-rollback on drop)
  - [ ] Test state tracking (prevent double-commit/rollback)
  - [ ] Test error handling during commit/rollback operations

- [ ] **CRITICAL: Transaction Isolation Tests** (NEW REQUIREMENT):
  - [ ] **Poison Prevention Tests**: Verify operations on original database during transaction don't affect transaction
  - [ ] **Isolation Verification**: Verify transaction operations don't affect original database until commit
  - [ ] **Concurrent Access Tests**: Test multiple threads accessing database during active transaction
  - [ ] **State Synchronization**: Test `transaction_active` flag prevents conflicts
  - [ ] **Resource Management**: Verify connections are properly isolated and cleaned up

- [ ] **Connection management tests** (UPDATED):
  - [ ] Test hybrid connection architecture for all backends
  - [ ] Test secondary connection creation (lazy initialization)
  - [ ] Test pool connection behavior (sqlx backends) with dedicated transaction connections
  - [ ] Test connection cleanup on transaction completion
  - [ ] Test proper connection ownership (transactions own their connections)

- [ ] **Integration tests with Database operations**:
  - [ ] Test all CRUD operations within transactions (unchanged)
  - [ ] Test schema operations (CREATE TABLE, etc.) within transactions (unchanged)
  - [ ] Test transaction rollback preserves pre-transaction state (unchanged)
  - [ ] **NEW**: Test operations on database continue working during active transactions
  - [ ] **NEW**: Test concurrent read operations during active transactions

- [ ] **Transaction ergonomics tests** (unchanged):
  - [ ] Test `&*tx` dereference pattern with all statement types
  - [ ] Test multiple operations on single transaction
  - [ ] Test error in middle of transaction doesn't prevent further operations
  - [ ] Verify begin_transaction() on transaction returns appropriate error
  - [ ] Test transaction consumption on commit/rollback (compile-time safety)

**UPDATED: Hybrid Connection Architecture Summary**

Each backend implements the following isolation pattern:

**SQLite (rusqlite)**:
- Primary: `Arc<Mutex<Connection>>` for normal operations
- Transaction: `Option<Connection>` (owned) created from database path
- Secondary: `Arc<Mutex<Option<Connection>>>` for operations during transactions

**SqlX Backends (sqlite, postgres, mysql)**:
- Primary: `Arc<Pool<DatabaseType>>` for normal operations
- Transaction: `Option<PoolConnection<DatabaseType>>` (owned) acquired from pool
- Secondary: `Arc<Mutex<Option<PoolConnection<DatabaseType>>>>` for operations during transactions

**PostgreSQL (tokio-postgres)**:
- Primary: `Arc<Mutex<Client>>` for normal operations
- Transaction: `Option<Client>` (owned) created with connection parameters
- Secondary: `Arc<Mutex<Option<Client>>>` for operations during transactions

**All Backends Share:**
- `transaction_active: Arc<AtomicBool>` for atomic state tracking
- `parent_flag` reference in transactions to clear parent state
- Lazy secondary connection creation to minimize resource usage
- `Drop` trait implementation for proper cleanup

##### 10.2.1.10 Validate Backward Compatibility and Performance

**CRITICAL VALIDATION PHASE**: Ensure hybrid architecture maintains all promises

- [ ] **Zero Breaking Changes Verification**:
  - [ ] Verify all existing Database trait usage patterns continue working unchanged
  - [ ] Test existing migration code works without modification
  - [ ] Confirm query execution patterns (`stmt.execute(&dyn Database)`) unchanged
  - [ ] Validate error types and handling remain consistent

- [ ] **Performance Impact Assessment**:
  - [ ] Measure connection creation overhead for transactions
  - [ ] Compare performance with/without secondary connections
  - [ ] Evaluate lazy secondary connection creation benefits
  - [ ] Test connection pool resource utilization

- [ ] **Resource Management Verification**:
  - [ ] Verify no connection leaks under normal operation
  - [ ] Test proper cleanup on transaction Drop
  - [ ] Validate secondary connections are cleaned up when not needed
  - [ ] Ensure transaction_active flag is always properly cleared

- [ ] **Stress Testing**:
  - [ ] High-frequency transaction creation/completion cycles
  - [ ] Concurrent operations on database during active transactions
  - [ ] Connection pool exhaustion scenarios (sqlx backends)
  - [ ] Memory usage patterns with long-running transactions

##### 10.2.1.11 Document Transaction Architecture and Usage Patterns

- [ ] Create transaction usage documentation in `packages/database/src/lib.rs`:
  - [ ] Document the execute pattern: `stmt.execute(&*tx).await?`
  - [ ] Show complete transaction lifecycle example
  - [ ] Explain commit consumes transaction (prevents use-after-commit)
  - [ ] Document error handling best practices within transactions
- [ ] Add usage examples showing:
  ```rust
  // Example pattern to document
  let tx = db.begin_transaction().await?;

  // Multiple operations on same transaction
  tx.insert("users").values(...).execute(&*tx).await?;
  tx.update("posts").set(...).execute(&*tx).await?;

  // Handle errors without poisoning
  if let Err(e) = tx.delete("temp").execute(&*tx).await {
      // User chooses: continue or rollback
      return tx.rollback().await;
  }

  // Commit consumes transaction
  tx.commit().await?;
  // tx no longer usable here - compile error!
  ```
- [ ] Document common pitfalls:
  - [ ] Forgetting to commit or rollback (leaks connection)
  - [ ] Trying to use transaction after commit
  - [ ] Nested begin_transaction() calls

#### 10.2.2 Extend Schema Builder Functionality ❌ **IMPORTANT**

**Prerequisites:** 10.2.1 (Database Transaction Support) must be complete before this step

**Background:** Current `switchy_database::schema` module only supports `CreateTableStatement`. For clean migration examples, we need all DDL operations available through type-safe builders.

##### 10.2.2.1 Add DropTableStatement

- [ ] Create `DropTableStatement` struct in `packages/database/src/schema.rs`
  - [ ] Add fields: `table_name: &'a str`, `if_exists: bool`, `cascade: bool`
  - [ ] Add builder methods: `if_exists()`, `cascade()`
  - [ ] Implement `execute()` method calling `db.exec_drop_table()`
- [ ] Add to `packages/database/src/lib.rs` Database trait:
  - [ ] Add `fn drop_table<'a>(&self, table_name: &'a str) -> schema::DropTableStatement<'a>`
  - [ ] Add `async fn exec_drop_table(&self, statement: &DropTableStatement<'_>) -> Result<(), DatabaseError>`
- [ ] Implement `exec_drop_table` for each backend:
  - [ ] SQLite in `packages/database/src/rusqlite/mod.rs`
  - [ ] SQLite in `packages/database/src/sqlx/sqlite.rs`
  - [ ] PostgreSQL in `packages/database/src/postgres/postgres.rs`
  - [ ] PostgreSQL in `packages/database/src/sqlx/postgres.rs`
  - [ ] MySQL in `packages/database/src/sqlx/mysql.rs`
- [ ] Implement `Executable` trait for `DropTableStatement` in `packages/database/src/executable.rs`
- [ ] Add unit tests for DropTableStatement builder
- [ ] Add integration tests for each database backend

##### 10.2.2.2 Add CreateIndexStatement

- [ ] Create `CreateIndexStatement` struct in `packages/database/src/schema.rs`
  - [ ] Add fields: `index_name: &'a str`, `table_name: &'a str`, `columns: Vec<&'a str>`, `unique: bool`, `if_not_exists: bool`
  - [ ] Add builder methods: `table()`, `column()`, `columns()`, `unique()`, `if_not_exists()`
  - [ ] Implement `execute()` method calling `db.exec_create_index()`
- [ ] Add to Database trait:
  - [ ] Add `fn create_index<'a>(&self, index_name: &'a str) -> schema::CreateIndexStatement<'a>`
  - [ ] Add `async fn exec_create_index(&self, statement: &CreateIndexStatement<'_>) -> Result<(), DatabaseError>`
- [ ] Implement `exec_create_index` for each backend:
  - [ ] SQLite (rusqlite)
  - [ ] SQLite (sqlx)
  - [ ] PostgreSQL (postgres)
  - [ ] PostgreSQL (sqlx)
  - [ ] MySQL (sqlx)
- [ ] Implement `Executable` trait for `CreateIndexStatement`
- [ ] Add unit tests for CreateIndexStatement builder
- [ ] Add integration tests for each database backend

##### 10.2.2.3 Add DropIndexStatement

- [ ] Create `DropIndexStatement` struct in `packages/database/src/schema.rs`
  - [ ] Add fields: `index_name: &'a str`, `if_exists: bool`
  - [ ] Add builder method: `if_exists()`
  - [ ] Implement `execute()` method calling `db.exec_drop_index()`
- [ ] Add to Database trait:
  - [ ] Add `fn drop_index<'a>(&self, index_name: &'a str) -> schema::DropIndexStatement<'a>`
  - [ ] Add `async fn exec_drop_index(&self, statement: &DropIndexStatement<'_>) -> Result<(), DatabaseError>`
- [ ] Implement `exec_drop_index` for each backend:
  - [ ] SQLite (rusqlite)
  - [ ] SQLite (sqlx)
  - [ ] PostgreSQL (postgres)
  - [ ] PostgreSQL (sqlx)
  - [ ] MySQL (sqlx)
- [ ] Implement `Executable` trait for `DropIndexStatement`
- [ ] Add unit tests for DropIndexStatement builder
- [ ] Add integration tests for each database backend

##### 10.2.2.4 Add AlterTableStatement with SQLite Workarounds

**SQLite Limitation Handling:** SQLite has limited ALTER TABLE support. We implement transparent workarounds using table recreation for unsupported operations, now using proper transaction support from 10.2.

- [ ] Create `AlterTableStatement` struct in `packages/database/src/schema.rs`
  - [ ] Add field: `table_name: &'a str`, `operations: Vec<AlterOperation>`
  - [ ] Create `AlterOperation` enum with variants:
    - [ ] `AddColumn(Column)`
    - [ ] `DropColumn(String)`
    - [ ] `RenameColumn { old: String, new: String }`
    - [ ] `ModifyColumn(Column)`
  - [ ] Add builder methods: `add_column()`, `drop_column()`, `rename_column()`, `modify_column()`
  - [ ] Implement `execute()` method calling `db.exec_alter_table()`
- [ ] Add to Database trait:
  - [ ] Add `fn alter_table<'a>(&self, table_name: &'a str) -> schema::AlterTableStatement<'a>`
  - [ ] Add `async fn exec_alter_table(&self, statement: &AlterTableStatement<'_>) -> Result<(), DatabaseError>`

- [ ] **Note:** SQLite workarounds are implemented in each backend's `exec_alter_table()` method:
  - [ ] SQLite backends detect their own limitations and use table recreation internally
  - [ ] PostgreSQL/MySQL backends use standard ALTER TABLE SQL directly
  - [ ] No database type detection needed - each backend knows its own capabilities
- [ ] Implement `exec_alter_table` for each backend:
  - [ ] SQLite (rusqlite) - with transaction-safe workarounds
  - [ ] SQLite (sqlx) - with transaction-safe workarounds
  - [ ] PostgreSQL (postgres) - standard ALTER TABLE
  - [ ] PostgreSQL (sqlx) - standard ALTER TABLE
  - [ ] MySQL (sqlx) - standard ALTER TABLE
- [ ] Implement `Executable` trait for `AlterTableStatement`
- [ ] Add unit tests for AlterTableStatement builder
- [ ] Add SQLite-specific tests for workarounds:
  - [ ] Test DROP COLUMN with table recreation and transactions
  - [ ] Test RENAME COLUMN with version detection and transactions
  - [ ] Test MODIFY COLUMN with table recreation and transactions
  - [ ] Test index preservation during recreation
  - [ ] Test data integrity during recreation
  - [ ] Test rollback behavior on failure
- [ ] Add integration tests for each database backend

##### 10.2.2.5 Update Database Simulator

- [ ] Add mock implementations in `packages/database/src/simulator/mod.rs`:
  - [ ] `exec_drop_table()`
  - [ ] `exec_create_index()`
  - [ ] `exec_drop_index()`
  - [ ] `exec_alter_table()`

#### 10.2.3 Create Basic Usage Example ❌ **MINOR**

**Prerequisites:** 10.2.1 and 10.2.2 must be complete before this step

- [ ] Create `packages/switchy/schema/examples/basic_usage.rs`:
  - [ ] Import necessary types (no test_utils)
  - [ ] Create `CreateUsersTable` migration using `db.create_table()`
  - [ ] Create `AddEmailIndex` migration using `db.create_index()`
  - [ ] Create `AddCreatedAtColumn` migration using `db.alter_table().add_column()`
  - [ ] Implement proper `down()` methods using:
    - [ ] `db.drop_table()` for cleanup
    - [ ] `db.drop_index()` for index removal
    - [ ] `db.alter_table().drop_column()` for column removal
  - [ ] Add main() function demonstrating:
    - [ ] Database connection setup
    - [ ] EmbeddedMigrationSource creation
    - [ ] MigrationRunner initialization
    - [ ] Migration status checking with `list_migrations()`
    - [ ] Running migrations
    - [ ] Verifying schema with test data
    - [ ] Optional rollback demonstration (commented)
- [ ] Test the example:
  - [ ] Verify it compiles without warnings
  - [ ] Run with SQLite to test workarounds and transactions
  - [ ] Verify no `exec_raw` calls in the code
  - [ ] Ensure clean, readable migration code

**Success Criteria for Phase 10.2:**

**Phase 10.2.1.1 Completed ✅:**
- [x] Database transaction trait architecture established
- [x] Database trait remains dyn-compatible
- [x] Transaction execute() pattern `&*tx` architecture ready
- [x] No poisoned state design - transactions remain usable after errors
- [x] Recursive begin_transaction() properly prevented with clear error variants
- [x] Transaction consumption on commit/rollback design prevents use-after-finish bugs
- [x] Clear documentation and examples for transaction usage patterns
- [x] All existing tests continue passing
- [x] Backend-specific transaction implementations ready for 10.2.1.3-10.2.1.11

**Phase 10.2.1 Completed (NEW REQUIREMENT):**
- [ ] Hybrid connection architecture documented and understood
- [ ] Transaction isolation requirements clearly defined
- [ ] Backward compatibility guarantees established
- [ ] Performance implications of separate connections understood

**Updated Requirements for Phase 10.2 (Transaction Isolation):**
- [ ] **Full transaction isolation** across all database backends (10.2.1.3-10.2.1.8):
  - [ ] **Zero transaction poisoning** - operations on database during transactions don't affect transactions
  - [ ] **True isolation** - transaction operations don't affect database until commit
  - [ ] **Consistent isolation semantics** - in-memory and file-based databases must behave identically
  - [ ] **Acceptable serialization** - may use serialized access to achieve consistency if parallel isolation not feasible
  - [ ] **Resource management** - proper connection cleanup and transaction lifecycle management
- [ ] **Backward compatibility maintained** - no breaking changes to Database trait or usage
- [ ] **Comprehensive isolation testing** - verify poisoning prevention and concurrent access
  - [ ] **Isolation consistency test**: Verify identical behavior between in-memory and file-based databases
  - [ ] **Serialization verification**: Test that uncommitted changes are not visible to other operations
  - [ ] **Concurrent operation blocking**: Confirm operations wait during active transactions (serialized implementations)
  - [ ] **Resource cleanup**: Verify proper transaction and connection lifecycle management
- [ ] All schema operations available through type-safe builders (10.2.2)
- [ ] SQLite workarounds use proper transactions (not exec_raw) (10.2.2)
- [ ] Example uses zero `exec_raw` calls (10.2.3)
- [ ] Same migration code works on all databases with automatic transaction handling (10.2.3)

**CRITICAL SUCCESS CRITERIA (NEW):**
- [ ] **No Transaction Poisoning**: Database operations during active transactions remain isolated
- [ ] **Performance Acceptable**: Connection creation overhead doesn't significantly impact performance
- [ ] **Resource Efficient**: Secondary connections created only when needed, properly cleaned up
- [ ] **Production Ready**: All backends handle concurrent access and edge cases correctly

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

## Phase 13: Advanced Transaction Features

**Goal:** Add advanced transaction capabilities after core transaction support is complete

**Prerequisites:** Phase 10.2.1 (Database Transaction Support) must be complete

### 13.1 Nested Transaction Support (Savepoints)

**Background:** Savepoints allow nested transactions within a main transaction, enabling partial rollback without losing the entire transaction.

- [ ] Add savepoint support to `DatabaseTransaction` trait ❌ **MINOR**
  - [ ] Add `async fn savepoint(&self, name: &str) -> Result<Box<dyn Savepoint>, DatabaseError>`
  - [ ] Add `Savepoint` trait with `release()` and `rollback_to()` methods
- [ ] Implement for SQLite backends:
  - [ ] Use `SAVEPOINT name` / `RELEASE name` / `ROLLBACK TO name` commands
  - [ ] Track savepoint hierarchy for proper cleanup
- [ ] Implement for PostgreSQL backends:
  - [ ] Use PostgreSQL savepoint syntax (same as SQLite)
  - [ ] Handle PostgreSQL-specific savepoint behavior
- [ ] Implement for MySQL backends:
  - [ ] Use MySQL savepoint syntax
  - [ ] Handle InnoDB engine requirements
- [ ] Add comprehensive testing:
  - [ ] Test nested savepoint creation and rollback
  - [ ] Test savepoint hierarchy management
  - [ ] Test error handling with failed savepoints

### 13.2 Transaction Isolation Levels

**Background:** Allow configuring transaction isolation for specific use cases.

- [ ] Add isolation level support ❌ **MINOR**
  - [ ] Define `TransactionIsolation` enum (ReadUncommitted, ReadCommitted, RepeatableRead, Serializable)
  - [ ] Add `begin_transaction_with_isolation()` method to Database trait
  - [ ] Add `set_isolation_level()` method to existing transactions
- [ ] Implement for all database backends:
  - [ ] Map enum values to database-specific isolation levels
  - [ ] Handle database-specific limitations (e.g., SQLite limited isolation)
  - [ ] Provide sensible defaults for each backend
- [ ] Add testing for isolation behavior:
  - [ ] Test concurrent transaction scenarios
  - [ ] Verify isolation level enforcement
  - [ ] Test database-specific isolation behaviors

### 13.3 Transaction Timeout and Resource Management

**Background:** Prevent long-running transactions from holding resources indefinitely.

- [ ] Add transaction timeout support ❌ **MINOR**
  - [ ] Add `begin_transaction_with_timeout()` method
  - [ ] Implement timeout enforcement per backend
  - [ ] Automatic rollback on timeout expiration
- [ ] Improve connection pool handling:
  - [ ] Configurable transaction timeout for pool connections
  - [ ] Connection health checks for long-running transactions
  - [ ] Pool monitoring and metrics for transaction resource usage
- [ ] Add resource management utilities:
  - [ ] Transaction monitoring and logging
  - [ ] Resource leak detection for unreleased transactions
  - [ ] Performance metrics collection

**Success Criteria for Phase 13:**
- Nested transactions work correctly on all supported databases
- Isolation levels properly enforced with database-appropriate behavior
- Transaction resource management prevents connection pool exhaustion
- Comprehensive testing covers edge cases and concurrent scenarios

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
5. **Phase 5** (Rollback) - ✅ Complete
6. ~~**Phase 6** (Validation)~~ - ❌ Removed (unnecessary)
7. **Phase 7** (Testing Infrastructure) - ✅ Complete (all sub-phases)
8. **Phase 8** (moosicbox Migration) - ✅ Complete (all sub-phases)
9. **Phase 9** (Migration Listing) - Can proceed now (optional)
10. **Phase 10** (Documentation) - Can proceed now (optional)
11. **Phase 11** (Future Enhancements) - After core phases (optional)
12. **Phase 12** (Dynamic Table Names) - Requires switchy_database enhancement
13. **Phase 10.2.1** (Transaction Support) - Now prioritized for clean schema builder examples

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

**Completed Core Implementation:**
1. ✅ Create `packages/switchy/schema/` package directory and workspace integration
2. ✅ Implement core types and traits for migration system
3. ✅ Add feature-gated discovery modules for different migration sources
   - ✅ Embedded discovery (Phase 3.3) - Complete
   - ✅ Directory discovery (Phase 3.5) - Complete
   - ✅ Code discovery (Phase 3.6) - Complete with Executable integration
4. ✅ Create migration runner with configurable strategies (Phase 4)
5. ✅ Add rollback support and validation features (Phase 5)
6. ✅ Update `moosicbox_schema` to use switchy_schema internally (Phase 8.2)
7. ✅ Add comprehensive testing with robust test utilities (Phase 7)
8. ✅ Migrate all existing tests to use new utilities (Phase 8.4)

## Phase 14: Concurrent Transaction Optimization

### Overview
Optimize transaction implementations for maximum concurrency while maintaining correctness. This phase addresses performance optimizations that were intentionally deferred during initial implementation to prioritize correctness and consistency.

### Phase 14.1: Rusqlite Concurrent Transactions

**Goal**: Enable parallel read operations during transactions for file-based SQLite databases while maintaining serialized access for in-memory databases

**Current State** (from Phase 10.2.1.3):
- All rusqlite transactions will use serialized locking via `Arc<Mutex<()>>`
- Both in-memory and file-based databases will use same connection with serialization
- Correct but suboptimal for file-based database read concurrency
- Ready for optimization without breaking existing functionality

**Implementation Strategy**:
```rust
enum IsolationStrategy {
    Serialized,    // Required for in-memory databases
    Concurrent,    // Optimization for file-based databases
}

impl RusqliteDatabase {
    fn get_isolation_strategy(&self) -> IsolationStrategy {
        match self.path {
            Some(ref p) if !p.to_string_lossy().is_empty()
                && !p.to_string_lossy().contains(":memory:") => {
                IsolationStrategy::Concurrent
            }
            _ => IsolationStrategy::Serialized
        }
    }
}
```

**Adaptive Approach**:
1. Runtime detection of database type (check path for ":memory:" or empty)
2. Strategy selection:
   - **In-memory databases**: Maintain serialized locking (required for correctness)
   - **File-based databases**: Create separate connections for true parallelism
3. Zero changes to public Database/DatabaseTransaction traits
4. Maintain identical transaction isolation semantics

**Expected Benefits**:
- **File-based databases**: Concurrent read operations during transactions
- **In-memory databases**: Unchanged behavior (correctness preserved)
- **Performance improvement**: 2-10x throughput for read-heavy workloads with active transactions
- **Write operations**: Remain serialized (SQLite internal limitation)

### Phase 14.2: Additional Optimizations

**Potential Areas**:
- Connection pool tuning and adaptive sizing
- Prepared statement caching across transactions
- Batch operation optimization
- Query plan caching for repeated operations

**Implementation Priority**:
- Phase 14.1 is high priority for production file-based database workloads
- Phase 14.2 optimizations are lower priority, measurable performance improvements required

**Success Criteria for Phase 14:**
- File-based databases support concurrent reads during transactions
- In-memory databases maintain serialized correctness and identical behavior
- No API breaking changes or interface modifications
- Performance benchmarks show measurable improvements for target workloads
- All existing tests continue passing without modification
- Zero regression in correctness or isolation guarantees

**Remaining Work:**
1. **Phase 9**: Implement migration listing functionality (optional, nice-to-have)
2. **Phase 10**: Complete additional documentation and usage examples (optional)
3. **Phase 11+**: Future enhancements (CLI, checksum validation, etc.) (optional)
4. **Phase 14**: Concurrent transaction optimization (performance enhancement)

**Production Readiness:** ✅ The migration system is fully functional and production-ready for HyperChad and other projects. All core functionality complete.
