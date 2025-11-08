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
