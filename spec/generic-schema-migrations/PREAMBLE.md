# Generic Schema Migrations - Preamble

The MoosicBox project currently has a working database migration system in `packages/schema` that handles schema evolution for SQLite, PostgreSQL, and MySQL databases. However, this system is tightly coupled to MoosicBox-specific schemas and migration files, making it unsuitable for other projects like HyperChad that need their own independent schema management.

## Context

- Specs use checkboxes (`- [ ]`) to track progress
- Four-phase workflow: preliminary check → deep analysis → execution → verification
- NO COMPROMISES - halt on any deviation from spec
    - Includes comprehensive test coverage for all business logic
    - Tests must be written alongside implementation, not deferred
    - Both success and failure paths must be tested
- Living documents that evolve during implementation
- After having completed a checkbox, 'check' it and add details under it regarding the file/location updated as PROOF

See `generic-schema-migrations/plan.md` for the current status of the generic-schema-migrations and what's next to be done.
