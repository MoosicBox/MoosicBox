# MoosicBox Schema Migrations

Owns the embedded historical SQL migrations and their `raw-sql` dependency.
The SQL files, migration names, ordering, and checksums are unchanged.
`moosicbox_schema` owns migration orchestration and delegates source construction here.

Application crates must not depend on this crate (including transitively in normal
library builds). Cargo unifies features in the server binary, so enforce typed-only
application APIs through isolated per-package checks, not combined workspace checks.
The standard build/test matrix checks application packages independently.
