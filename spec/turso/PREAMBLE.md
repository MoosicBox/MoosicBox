# Turso Database Backend - Preamble

The MoosicBox project currently uses SQLite-based database backends (rusqlite and sqlx-sqlite) for local data storage. While SQLite is proven and reliable, these implementations have several limitations: SQLite's single-writer architecture limits concurrent write throughput, rusqlite provides only synchronous I/O which blocks async runtimes, and SQLite was not designed for modern distributed systems or edge computing scenarios. These constraints become particularly problematic as MoosicBox scales and requires better concurrency for multi-tenant scenarios, AI workloads, and distributed architectures.

This specification outlines the integration of **Turso Database** as a new database backend option. Turso is a ground-up Rust rewrite of SQLite (not a fork like libSQL), providing native async I/O with Linux io_uring support, experimental concurrent writes through `BEGIN CONCURRENT`, and built-in features like vector search for AI/RAG workloads. The solution maintains SQLite compatibility for SQL dialect and file formats while delivering modern async-first architecture and preparing MoosicBox for distributed scenarios through Turso's DST (Distributed SQLite) capabilities.

The Turso backend will be implemented as a new feature in the `switchy_database` package, following the same abstraction patterns as existing backends (rusqlite, postgres, mysql). This approach allows for drop-in replacement of SQLite backends while maintaining flexibility to use traditional SQLite where appropriate. The implementation focuses initially on local file-based databases with future extensibility to Turso Cloud sync and embedded replicas.

The implementation will run alongside existing database backends, allowing gradual migration and testing. Turso Database is designed to be SQLite-compatible, enabling existing schemas and queries to work without modification while providing performance and concurrency improvements where the experimental features are enabled.

## Prerequisites

- Follow MoosicBox coding conventions (BTreeMap/BTreeSet, workspace dependencies)

## Context

- Specs use checkboxes (`- [ ]`) to track progress
- Four-phase workflow: preliminary check → deep analysis → execution → verification
- NO COMPROMISES - halt on any deviation from spec
    - Includes comprehensive test coverage for all business logic
    - Tests must be written alongside implementation, not deferred
    - Both success and failure paths must be tested
- Living documents that evolve during implementation
- After completing a checkbox, 'check' it and add details under it regarding the file/location updated as PROOF

See `turso/plan.md` for the current status and what's next to be done.
