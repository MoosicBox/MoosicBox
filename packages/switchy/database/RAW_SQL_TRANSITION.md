# Raw SQL transition (local implementation status)

**Incomplete boundary: do not treat a build without `raw-sql` as SQL-fragment-safe yet.**

## Integration

* `switchy_database` defaults include `raw-sql`. Consumers disabling defaults must explicitly request it to retain existing arbitrary SQL calls.
* `switchy_database_connection/raw-sql` forwards to the database feature and is default-enabled. Backend features do not enable it.
* The umbrella defaults include `database-raw-sql` and `database-connection-raw-sql`. Both use weak dependency forwarding: they do not independently activate a database dependency. Enable `database` / `database-connection` (or an appropriate backend feature) separately.
* Without `raw-sql`, `Database::{exec_raw,query_raw,exec_raw_params,query_raw_params}`, including access through transactions/savepoints, are absent. Decorator implementations must conditionally compile those methods with a matching consumer feature, or remove them in a no-raw-only consumer. Ordinary structured methods are unchanged.
* `query::Literal`, `literal`, literal conversions and `ExpressionType::Literal`, plus `Executable for String/&str`, are absent without the feature.
* SQLx SQLite/Postgres `get_connection` is only public with `raw-sql`. Backend-internal access and generated SQL execution remain private and available when needed.
* `query::identifier` expressions in no-raw builds denote **one exact identifier**, quoted for the backend. A dot is part of the name, not qualification. Existing raw-enabled expression rendering is preserved. Do not move projections, aliases or SQL functions into identifier strings to migrate callers.
* `postgres-raw` continues to name a backend, not raw-SQL permission. `schema` and `cascade` do not imply `raw-sql`.
* Eventual removal from defaults requires a compatibility release; not performed here.

## Blocking gaps

This is a partial implementation, not an authorization/security boundary ready for Bcode's final no-raw migration. Table names, column projections, value keys, unique keys, join conditions, aliases and schema names/fragments still require comprehensive safe rendering or typed replacement. `Identifier` expression quoting alone does not cover those routes. Public backend helper APIs still require complete audit. `SqlInterval` is already numeric structured data, not a free-form string.

No workflow connection API was added: existing Bcode needs immediate transactions, connection affinity across synchronous reads/writes, read-only opens, configurable busy timeouts, busy/locked error classification, affected-row counts and complete lease release. Do not replace its driver persistence with the existing five-connection/10ms initializer. No typed CHECK-constraint/schema migration capability was added for the usage singleton table. These missing capabilities remain integration blockers, not permission to add SQL fragments or generic pragma APIs.

## Observed validation

Commands run from the MoosicBox root:

* `cargo fmt --all`: passed.
* `cargo check -p switchy_database --no-default-features --features sqlite-rusqlite,schema,cascade` and same with `,raw-sql`: passed.
* `cargo clippy -p switchy_database --lib --no-default-features --features sqlite-rusqlite,schema,cascade -- -D warnings` and same with `,raw-sql`: passed after fixing explicit statement/connection release in moved raw helpers.
* `cargo check -p switchy_database --no-default-features --features sqlite-sqlx,postgres-sqlx,mysql-sqlx,postgres-raw,schema,cascade`: passed with dead-code warnings; not strict-clean.
* `cargo check -p switchy_database --no-default-features --features turso,schema,cascade`: passed with warnings.
* `cargo check -p switchy_database_connection --no-default-features --features sqlite-rusqlite`: passed (a then-observed unused helper was subsequently gated).
* `cargo check -p switchy --no-default-features --features database-sqlite-rusqlite,database-schema`: passed.
* `cargo test -p switchy_database --test raw_boundary --no-default-features --features sqlite-rusqlite,schema,cascade`: one SQL-looking identifier behavior test passed. Dev dependencies activate additional backends but not raw-sql.
* `cargo test -p switchy_database --doc --no-default-features --features sqlite-rusqlite,schema,cascade -- 'lib.rs'`: 8 ordinary tests and 4 compile-fail exclusion tests passed; 26 ignored and 33 filtered.
* `cargo test -p switchy_database --lib --no-default-features --features sqlite-rusqlite,schema,cascade,raw-sql rusqlite::tests`: 17 passed, 325 filtered.
* Strict `--all-targets` clippy, both feature-on/off: **failed**. Dev dependencies expand the backend graph; moved backend methods expose significant-drop-tightening lints and no-raw builds retain unused helpers. No-raw existing raw-based test suites still need feature gating or typed fixtures. These are blocking, not waived.
* DuckDB build, server-backed runtime tests, complete workspace build/test/clippy, and downstream integration were not verified.

Logs from this execution are `/tmp/switchy-{check,on,off,matrix,turso,connection,umbrella,test,doc,tests-on,clippy-on,clippy-off,all-clippy,all-clippy-off}.log`; temporary logs are evidence pointers, not durable CI artifacts.
