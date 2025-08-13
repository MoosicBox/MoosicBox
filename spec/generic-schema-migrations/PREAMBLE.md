# Generic Schema Migrations - Preamble

## Context

The MoosicBox project currently has a working database migration system in `packages/schema` that handles schema evolution for SQLite, PostgreSQL, and MySQL databases. However, this system is tightly coupled to MoosicBox-specific schemas and migration files, making it unsuitable for other projects like HyperChad that need their own independent schema management.

## Problem Statement

Projects using the `switchy_database` abstraction need a reusable, generic migration system that can:

1. **Manage schema evolution** - Handle forward and backward migrations
2. **Support multiple databases** - Work with SQLite, PostgreSQL, MySQL via switchy_database
3. **Track migration state** - Know which migrations have been applied
4. **Provide safety guarantees** - Prevent duplicate runs, validate checksums
5. **Enable independent schemas** - Allow each project to manage its own migrations

The current `moosicbox_schema` package contains excellent migration logic but is not reusable because:
- Migration files are embedded at compile time for MoosicBox schemas
- Table names and constants are MoosicBox-specific
- No API for external projects to define their own migrations

## Requirements

### Functional Requirements

- Extract generic migration engine from `moosicbox_schema`
- Trait-based migration sources with feature-gated implementations
- Support embedded files, runtime directories, and code-based migrations
- Provide transaction support (dependent on switchy_database capabilities)
- Enable optional rollback functionality with down migrations
- Support deterministic migration ordering
- Offer programmatic API

### Non-Functional Requirements

- Zero breaking changes to existing `moosicbox_schema` users
- Database-agnostic through `switchy_database` abstraction
- Type-safe with comprehensive error handling
- Basic documentation with usage examples
- Extensible trait-based architecture with feature-gated discovery methods
- Minimal runtime overhead

### Technical Constraints

- Must work with existing `switchy_database` trait and exec_raw
- Follow Rust best practices (traits, builders, async/await)
- Support feature flags for discovery methods
- Maintain compatibility with include_dir for embedded migrations
- Use deterministic migration ordering (timestamp-based)
- No SQL validation - pass through to database as-is
- Migration IDs can be any valid string
- Provide comprehensive test utilities

## Success Criteria

- HyperChad can manage its own schema independently
- `moosicbox_schema` continues to work unchanged as a thin wrapper
- Other projects can easily adopt the migration system
- Migration state is tracked reliably across database types
- Rollback operations work safely and predictably
- Feature-gated discovery methods work independently
- Test utilities enable easy testing of schema changes

## Out of Scope

- Database connection management (handled by switchy_database)
- SQL validation or parsing
- Migration ID format validation
- Schema diffing or automatic migration generation
- Cross-database schema translation
- Real-time schema synchronization
- Complex migration state queries (future enhancement)
- Remote migration sources (future enhancement)
- Checksum validation (future enhancement)
- CLI tools (future enhancement)
- GUI tools for migration management

## Dependencies

### Required Packages
- `switchy_database` - Database abstraction layer
- `include_dir` - Compile-time directory embedding
- `async-trait` - Async trait support
- `thiserror` - Error handling
- `chrono` - Timestamp handling

### Optional Dependencies
- `clap` - CLI interface (future enhancement)
- `tokio` - Async runtime for CLI (future enhancement)

## Architecture Principles

1. **Separation of Concerns** - Migration discovery, validation, and execution are separate
2. **Trait-Based Extensibility** - New migration sources implement MigrationSource trait
3. **Simplicity First** - No SQL validation, pass through to database as-is
4. **Feature-Gated Functionality** - Only include what's needed via feature flags
5. **Performance** - Efficient migration tracking and minimal overhead
