//! Database backends using the `SQLx` library
//!
//! This module provides database implementations using the async-native `SQLx` library,
//! supporting multiple database backends through a unified interface.
//!
//! # Supported Backends
//!
//! * **MySQL** (feature: `mysql-sqlx`) - MySQL database support via SQLx
//! * **PostgreSQL** (feature: `postgres-sqlx`) - PostgreSQL database support via SQLx
//! * **SQLite** (feature: `sqlite-sqlx`) - SQLite database support via SQLx
//!
//! # SQLx Architecture
//!
//! The SQLx backends use:
//! * **Native async I/O** - Built on tokio for true async database operations
//! * **Connection pooling** - Built-in connection pool management
//! * **Compile-time verification** - SQL queries can be checked at compile time (not used here)
//! * **Type safety** - Strong typing for database values
//!
//! # Schema Introspection
//!
//! Each SQLx backend implements schema introspection using database-specific system catalogs:
//! * **PostgreSQL**: Uses `information_schema` tables
//! * **MySQL**: Uses `information_schema` tables
//! * **SQLite**: Uses PRAGMA commands
//!
//! See individual backend modules for implementation details.

#[cfg(feature = "mysql-sqlx")]
/// `MySQL` database backend using `SQLx`
pub mod mysql;
#[cfg(feature = "postgres-sqlx")]
/// `PostgreSQL` database backend using `SQLx`
pub mod postgres;
#[cfg(feature = "sqlite-sqlx")]
/// `SQLite` database backend using `SQLx`
pub mod sqlite;

#[cfg(all(feature = "postgres-sqlx", feature = "schema"))]
pub(crate) mod postgres_introspection;

#[cfg(all(feature = "mysql-sqlx", feature = "schema"))]
pub(crate) mod mysql_introspection;
