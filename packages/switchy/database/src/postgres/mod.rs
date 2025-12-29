//! PostgreSQL database backend implementation
//!
//! This module provides native PostgreSQL database support using the `postgres` crate.
//! It includes connection management, query execution, and schema introspection for PostgreSQL.

#[allow(clippy::module_inception)]
#[cfg(feature = "postgres-raw")]
/// `PostgreSQL` database backend implementation using native `tokio-postgres`
pub mod postgres;

#[cfg(all(feature = "postgres-raw", feature = "schema"))]
pub(crate) mod introspection;
