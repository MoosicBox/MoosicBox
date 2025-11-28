//! Turso Database backend implementation
//!
//! **⚠️ BETA**: Turso Database is currently in BETA (v0.2.2).
//! Use caution with production data.
//!
//! This module provides a [`Database`](crate::Database) implementation using
//! [Turso Database](https://github.com/tursodatabase/turso), a ground-up Rust
//! rewrite of `SQLite` with modern async architecture.
//!
//! # Important Limitations
//!
//! * **Local databases only**: Supports file-based and in-memory (`:memory:`) databases
//! * **No remote connections**: Cannot connect to Turso Cloud (use libSQL client for that)
//! * **Blob types not supported**: Reading `BLOB` columns will panic with `unimplemented!()`.
//!   This matches the rusqlite backend limitation exactly. Workaround: encode binary data as
//!   base64 TEXT or store file paths instead of binary content.
//! * **BETA status**: API may change, bugs may exist, not recommended for production use
//!
//! See [Appendix B in the spec](https://github.com/tursodatabase/turso) for details on
//! the distinction between Turso Database (this implementation) and Turso Cloud (libSQL).
//!
//! # Features
//!
//! * **Native async I/O** with `io_uring` support (Linux)
//! * **SQLite-compatible** file format and SQL dialect
//! * **Full transaction support** (begin, commit, rollback, savepoints)
//! * **Schema introspection** including bulletproof foreign key parsing
//! * **Experimental features** (not exposed):
//!   - Concurrent writes (MVCC via `BEGIN CONCURRENT`)
//!   - Encryption at rest
//!   - Vector search capabilities
//!
//! # Examples
//!
//! ## Basic Usage
//!
//! ```ignore
//! use switchy_database::{Database, DatabaseValue};
//! use switchy_database::turso::TursoDatabase;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a new database file
//!     let db = TursoDatabase::new("database.db").await?;
//!     let db: &dyn Database = &db;
//!
//!     // Create a table
//!     db.exec_raw("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)").await?;
//!
//!     // Insert data using parameterized queries
//!     db.exec_raw_params(
//!         "INSERT INTO users (name) VALUES (?1)",
//!         &[DatabaseValue::String("Alice".to_string())]
//!     ).await?;
//!
//!     // Query data
//!     let rows = db.query_raw("SELECT id, name FROM users").await?;
//!     println!("Found {} users", rows.len());
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Transactions
//!
//! ```ignore
//! use switchy_database::{Database, DatabaseValue};
//! use switchy_database::turso::TursoDatabase;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let db = TursoDatabase::new("database.db").await?;
//!     let db: &dyn Database = &db;
//!
//!     // Begin a transaction
//!     let tx = db.begin_transaction().await?;
//!
//!     // Execute operations within the transaction
//!     tx.exec_raw_params(
//!         "INSERT INTO users (name) VALUES (?1)",
//!         &[DatabaseValue::String("Bob".to_string())]
//!     ).await?;
//!
//!     // Commit the transaction
//!     tx.commit().await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## In-Memory Database
//!
//! ```ignore
//! use switchy_database::Database;
//! use switchy_database::turso::TursoDatabase;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Use `:memory:` for an in-memory database
//!     let db = TursoDatabase::new(":memory:").await?;
//!     let db: &dyn Database = &db;
//!
//!     // Database is fully functional but not persisted to disk
//!     db.exec_raw("CREATE TABLE temp (value INTEGER)").await?;
//!
//!     Ok(())
//! }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

/// Transaction support for Turso database
pub mod transaction;

use thiserror::Error;
use turso::{Builder, Value as TursoValue};

use crate::{
    DatabaseValue,
    query_transform::{QuestionMarkHandler, transform_query_for_params},
    sql_interval::SqlInterval,
};

pub use transaction::TursoTransaction;

#[cfg(feature = "schema")]
use std::sync::LazyLock;

#[cfg(feature = "schema")]
pub(crate) static FK_PATTERN: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r#"(?i)FOREIGN\s+KEY\s*\(([^)]+)\)\s*REFERENCES\s+((?:[^\s(,\[\]"'`]+|"(?:[^"]|"")*"|`(?:[^`]|``)*`|\[(?:[^\]])*\]|'(?:[^']|'')*'))\s*\(([^)]+)\)([^,)]*)"#
    ).expect("FK regex pattern should compile")
});

#[cfg(feature = "schema")]
static ON_UPDATE_PATTERN: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?i)ON\s+UPDATE\s+(CASCADE|SET\s+NULL|SET\s+DEFAULT|RESTRICT|NO\s+ACTION)")
        .expect("ON UPDATE regex pattern should compile")
});

#[cfg(feature = "schema")]
static ON_DELETE_PATTERN: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?i)ON\s+DELETE\s+(CASCADE|SET\s+NULL|SET\s+DEFAULT|RESTRICT|NO\s+ACTION)")
        .expect("ON DELETE regex pattern should compile")
});

/// Errors that can occur during Turso database operations
#[derive(Debug, Error)]
pub enum TursoDatabaseError {
    /// Underlying Turso library error
    #[error(transparent)]
    Turso(#[from] turso::Error),
    /// Database connection error
    #[error("Connection error: {0}")]
    Connection(String),
    /// SQL query execution error
    #[error("Query error: {0}")]
    Query(String),
    /// Transaction operation error
    #[error("Transaction error: {0}")]
    Transaction(String),
    /// Unsupported data type conversion
    #[error("Unsupported type conversion: {0}")]
    UnsupportedType(String),
}

impl From<turso::Error> for crate::DatabaseError {
    fn from(value: turso::Error) -> Self {
        Self::Turso(value.into())
    }
}

/// Turso Database implementation providing `SQLite`-compatible API
#[derive(Debug)]
pub struct TursoDatabase {
    database: turso::Database,
    connection: turso::Connection,
}

impl TursoDatabase {
    /// Create a new Turso database instance
    ///
    /// Creates a single shared connection for regular database operations. Transactions
    /// will create separate connections as needed.
    ///
    /// # Errors
    ///
    /// * Returns `TursoDatabaseError::Connection` if the database cannot be opened or
    ///   the initial connection cannot be established
    pub async fn new(path: &str) -> Result<Self, TursoDatabaseError> {
        log::debug!("Creating Turso database: path={path}");
        let builder = Builder::new_local(path);
        let database = builder
            .build()
            .await
            .map_err(|e| TursoDatabaseError::Connection(e.to_string()))?;

        log::debug!("Opening Turso connection: path={path}");
        let connection = database.connect().map_err(TursoDatabaseError::Turso)?;

        log::debug!("Turso database initialized: path={path}");
        Ok(Self {
            database,
            connection,
        })
    }
}

pub(crate) fn format_sqlite_interval(interval: &SqlInterval) -> Vec<String> {
    let mut modifiers = Vec::new();

    if interval.years != 0 {
        let sign = if interval.years >= 0 { "+" } else { "" };
        modifiers.push(format!(
            "{}{} year{}",
            sign,
            interval.years,
            if interval.years.abs() == 1 { "" } else { "s" }
        ));
    }
    if interval.months != 0 {
        let sign = if interval.months >= 0 { "+" } else { "" };
        modifiers.push(format!(
            "{}{} month{}",
            sign,
            interval.months,
            if interval.months.abs() == 1 { "" } else { "s" }
        ));
    }
    if interval.days != 0 {
        let sign = if interval.days >= 0 { "+" } else { "" };
        modifiers.push(format!(
            "{}{} day{}",
            sign,
            interval.days,
            if interval.days.abs() == 1 { "" } else { "s" }
        ));
    }
    if interval.hours != 0 {
        let sign = if interval.hours >= 0 { "+" } else { "" };
        modifiers.push(format!(
            "{}{} hour{}",
            sign,
            interval.hours,
            if interval.hours.abs() == 1 { "" } else { "s" }
        ));
    }
    if interval.minutes != 0 {
        let sign = if interval.minutes >= 0 { "+" } else { "" };
        modifiers.push(format!(
            "{}{} minute{}",
            sign,
            interval.minutes,
            if interval.minutes.abs() == 1 { "" } else { "s" }
        ));
    }

    if interval.seconds != 0 || interval.nanos != 0 {
        let sign = if interval.seconds >= 0 && interval.nanos == 0 {
            "+"
        } else if interval.seconds < 0 {
            ""
        } else {
            "+"
        };
        if interval.nanos == 0 {
            modifiers.push(format!(
                "{}{} second{}",
                sign,
                interval.seconds,
                if interval.seconds.abs() == 1 { "" } else { "s" }
            ));
        } else {
            #[allow(clippy::cast_precision_loss)]
            let fractional =
                interval.seconds as f64 + (f64::from(interval.nanos) / 1_000_000_000.0);
            modifiers.push(format!("{sign}{fractional} seconds"));
        }
    }

    if modifiers.is_empty() {
        vec!["0 seconds".to_string()]
    } else {
        modifiers
    }
}

pub(crate) fn turso_transform_query_for_params(
    query: &str,
    params: &[DatabaseValue],
) -> Result<(String, Vec<DatabaseValue>), crate::DatabaseError> {
    transform_query_for_params(query, params, &QuestionMarkHandler, |param| match param {
        DatabaseValue::Now => Some("datetime('now')".to_string()),
        DatabaseValue::NowPlus(interval) => {
            let modifiers = format_sqlite_interval(interval);
            if modifiers.is_empty() {
                Some("datetime('now')".to_string())
            } else {
                Some(format!(
                    "datetime('now', {})",
                    modifiers
                        .iter()
                        .map(|m| format!("'{m}'"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }
        }
        _ => None,
    })
    .map_err(crate::DatabaseError::QueryFailed)
}

impl From<TursoValue> for DatabaseValue {
    fn from(value: TursoValue) -> Self {
        match value {
            TursoValue::Null => Self::Null,
            TursoValue::Integer(i) => Self::Int64(i),
            TursoValue::Real(f) => Self::Real64(f),
            TursoValue::Text(s) => Self::String(s),
            TursoValue::Blob(_) => unimplemented!("Blob types are not supported yet"),
        }
    }
}

pub(crate) fn database_value_to_turso_value(
    value: &DatabaseValue,
) -> Result<TursoValue, TursoDatabaseError> {
    match value {
        DatabaseValue::Null => Ok(TursoValue::Null),
        DatabaseValue::String(s) | DatabaseValue::StringOpt(Some(s)) => {
            Ok(TursoValue::Text(s.clone()))
        }
        DatabaseValue::StringOpt(None) => Ok(TursoValue::Null),
        DatabaseValue::Bool(b) | DatabaseValue::BoolOpt(Some(b)) => {
            Ok(TursoValue::Integer(i64::from(*b)))
        }
        DatabaseValue::BoolOpt(None) => Ok(TursoValue::Null),
        DatabaseValue::Int8(i) | DatabaseValue::Int8Opt(Some(i)) => {
            Ok(TursoValue::Integer(i64::from(*i)))
        }
        DatabaseValue::Int8Opt(None) => Ok(TursoValue::Null),
        DatabaseValue::Int16(i) | DatabaseValue::Int16Opt(Some(i)) => {
            Ok(TursoValue::Integer(i64::from(*i)))
        }
        DatabaseValue::Int16Opt(None) => Ok(TursoValue::Null),
        DatabaseValue::Int32(i) | DatabaseValue::Int32Opt(Some(i)) => {
            Ok(TursoValue::Integer(i64::from(*i)))
        }
        DatabaseValue::Int32Opt(None) => Ok(TursoValue::Null),
        DatabaseValue::Int64(i) | DatabaseValue::Int64Opt(Some(i)) => Ok(TursoValue::Integer(*i)),
        DatabaseValue::Int64Opt(None) => Ok(TursoValue::Null),
        DatabaseValue::UInt8(u) | DatabaseValue::UInt8Opt(Some(u)) => {
            Ok(TursoValue::Integer(i64::from(*u)))
        }
        DatabaseValue::UInt8Opt(None) => Ok(TursoValue::Null),
        DatabaseValue::UInt16(u) | DatabaseValue::UInt16Opt(Some(u)) => {
            Ok(TursoValue::Integer(i64::from(*u)))
        }
        DatabaseValue::UInt16Opt(None) => Ok(TursoValue::Null),
        DatabaseValue::UInt32(u) | DatabaseValue::UInt32Opt(Some(u)) => {
            Ok(TursoValue::Integer(i64::from(*u)))
        }
        DatabaseValue::UInt32Opt(None) => Ok(TursoValue::Null),
        DatabaseValue::UInt64(u) | DatabaseValue::UInt64Opt(Some(u)) => i64::try_from(*u)
            .map(TursoValue::Integer)
            .map_err(|e| TursoDatabaseError::UnsupportedType(format!("u64 too large: {e}"))),
        DatabaseValue::UInt64Opt(None) => Ok(TursoValue::Null),
        DatabaseValue::Real32(f) | DatabaseValue::Real32Opt(Some(f)) => {
            Ok(TursoValue::Real(f64::from(*f)))
        }
        DatabaseValue::Real32Opt(None) => Ok(TursoValue::Null),
        DatabaseValue::Real64(f) | DatabaseValue::Real64Opt(Some(f)) => Ok(TursoValue::Real(*f)),
        DatabaseValue::Real64Opt(None) => Ok(TursoValue::Null),
        #[cfg(feature = "decimal")]
        DatabaseValue::Decimal(d) | DatabaseValue::DecimalOpt(Some(d)) => {
            Ok(TursoValue::Text(d.to_string()))
        }
        #[cfg(feature = "decimal")]
        DatabaseValue::DecimalOpt(None) => Ok(TursoValue::Null),
        #[cfg(feature = "uuid")]
        DatabaseValue::Uuid(u) | DatabaseValue::UuidOpt(Some(u)) => {
            Ok(TursoValue::Text(u.to_string()))
        }
        #[cfg(feature = "uuid")]
        DatabaseValue::UuidOpt(None) => Ok(TursoValue::Null),
        DatabaseValue::NowPlus(_) | DatabaseValue::Now => Err(TursoDatabaseError::UnsupportedType(
            "Now/NowPlus should be transformed before parameter binding".to_string(),
        )),
        DatabaseValue::DateTime(dt) => {
            Ok(TursoValue::Text(dt.format("%Y-%m-%d %H:%M:%S").to_string()))
        }
    }
}

pub(crate) fn to_turso_params(
    params: &[DatabaseValue],
) -> Result<Vec<TursoValue>, TursoDatabaseError> {
    params.iter().map(database_value_to_turso_value).collect()
}

pub(crate) fn from_turso_row(
    column_names: &[String],
    row: &turso::Row,
) -> Result<crate::Row, TursoDatabaseError> {
    let mut columns = Vec::with_capacity(column_names.len());

    for (index, column_name) in column_names.iter().enumerate() {
        let value = row
            .get_value(index)
            .map_err(|e| TursoDatabaseError::Query(format!("Failed to get column {index}: {e}")))?;

        columns.push((column_name.clone(), DatabaseValue::from(value)));
    }

    Ok(crate::Row { columns })
}

trait ToSql {
    fn to_sql(&self) -> String;
}

impl<T: crate::query::Expression + ?Sized> ToSql for T {
    #[allow(clippy::too_many_lines)]
    fn to_sql(&self) -> String {
        use crate::query::{ExpressionType, SortDirection};
        match self.expression_type() {
            ExpressionType::Eq(value) => {
                if value.right.is_null() {
                    format!("({} IS {})", value.left.to_sql(), value.right.to_sql())
                } else {
                    format!("({} = {})", value.left.to_sql(), value.right.to_sql())
                }
            }
            ExpressionType::Gt(value) => {
                if value.right.is_null() {
                    panic!("Invalid > comparison with NULL");
                } else {
                    format!("({} > {})", value.left.to_sql(), value.right.to_sql())
                }
            }
            ExpressionType::In(value) => {
                format!("{} IN ({})", value.left.to_sql(), value.values.to_sql())
            }
            ExpressionType::NotIn(value) => {
                format!("{} NOT IN ({})", value.left.to_sql(), value.values.to_sql())
            }
            ExpressionType::Lt(value) => {
                if value.right.is_null() {
                    panic!("Invalid < comparison with NULL");
                } else {
                    format!("({} < {})", value.left.to_sql(), value.right.to_sql())
                }
            }
            ExpressionType::Or(value) => format!(
                "({})",
                value
                    .conditions
                    .iter()
                    .map(|x| x.to_sql())
                    .collect::<Vec<_>>()
                    .join(" OR ")
            ),
            ExpressionType::And(value) => format!(
                "({})",
                value
                    .conditions
                    .iter()
                    .map(|x| x.to_sql())
                    .collect::<Vec<_>>()
                    .join(" AND ")
            ),
            ExpressionType::Gte(value) => {
                if value.right.is_null() {
                    panic!("Invalid >= comparison with NULL");
                } else {
                    format!("({} >= {})", value.left.to_sql(), value.right.to_sql())
                }
            }
            ExpressionType::Lte(value) => {
                if value.right.is_null() {
                    panic!("Invalid <= comparison with NULL");
                } else {
                    format!("({} <= {})", value.left.to_sql(), value.right.to_sql())
                }
            }
            ExpressionType::Join(value) => format!(
                "{} JOIN {} ON {}",
                if value.left { "LEFT" } else { "" },
                value.table_name,
                value.on
            ),
            ExpressionType::Sort(value) => format!(
                "({}) {}",
                value.expression.to_sql(),
                match value.direction {
                    SortDirection::Asc => "ASC",
                    SortDirection::Desc => "DESC",
                }
            ),
            ExpressionType::NotEq(value) => {
                if value.right.is_null() {
                    format!("({} IS NOT {})", value.left.to_sql(), value.right.to_sql())
                } else {
                    format!("({} != {})", value.left.to_sql(), value.right.to_sql())
                }
            }
            ExpressionType::InList(value) => value
                .values
                .iter()
                .map(|value| value.to_sql())
                .collect::<Vec<_>>()
                .join(","),
            ExpressionType::Coalesce(value) => format!(
                "IFNULL({})",
                value
                    .values
                    .iter()
                    .map(|value| value.to_sql())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            ExpressionType::Literal(value) => value.value.clone(),
            ExpressionType::Identifier(value) => value.value.clone(),
            ExpressionType::SelectQuery(value) => {
                let joins = value.joins.as_ref().map_or_else(String::new, |joins| {
                    joins
                        .iter()
                        .map(ToSql::to_sql)
                        .collect::<Vec<_>>()
                        .join(" ")
                });

                let where_clause = value.filters.as_ref().map_or_else(String::new, |filters| {
                    if filters.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "WHERE {}",
                            filters
                                .iter()
                                .map(|x| format!("({})", x.to_sql()))
                                .collect::<Vec<_>>()
                                .join(" AND ")
                        )
                    }
                });

                let sort_clause = value.sorts.as_ref().map_or_else(String::new, |sorts| {
                    if sorts.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "ORDER BY {}",
                            sorts
                                .iter()
                                .map(ToSql::to_sql)
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    }
                });

                let limit = value
                    .limit
                    .map_or_else(String::new, |limit| format!("LIMIT {limit}"));

                format!(
                    "SELECT {} {} FROM {} {} {} {} {}",
                    if value.distinct { "DISTINCT" } else { "" },
                    value.columns.join(", "),
                    value.table_name,
                    joins,
                    where_clause,
                    sort_clause,
                    limit
                )
            }
            ExpressionType::DatabaseValue(value) => match value {
                DatabaseValue::Null
                | DatabaseValue::BoolOpt(None)
                | DatabaseValue::StringOpt(None)
                | DatabaseValue::Int8Opt(None)
                | DatabaseValue::Int16Opt(None)
                | DatabaseValue::Int32Opt(None)
                | DatabaseValue::Int64Opt(None)
                | DatabaseValue::UInt8Opt(None)
                | DatabaseValue::UInt16Opt(None)
                | DatabaseValue::UInt32Opt(None)
                | DatabaseValue::UInt64Opt(None)
                | DatabaseValue::Real64Opt(None)
                | DatabaseValue::Real32Opt(None) => "NULL".to_string(),
                #[cfg(feature = "decimal")]
                DatabaseValue::DecimalOpt(None) => "NULL".to_string(),
                #[cfg(feature = "uuid")]
                DatabaseValue::UuidOpt(None) => "NULL".to_string(),
                DatabaseValue::Now => "strftime('%Y-%m-%dT%H:%M:%f', 'now')".to_string(),
                DatabaseValue::NowPlus(interval) => {
                    let modifiers = format_sqlite_interval(interval);
                    let modifier_str = modifiers
                        .iter()
                        .map(|m| format!("'{m}'"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("strftime('%Y-%m-%dT%H:%M:%f', datetime('now', {modifier_str}))")
                }
                _ => "?".to_string(),
            },
        }
    }
}

fn build_join_clauses(joins: Option<&[crate::query::Join<'_>]>) -> String {
    joins.map_or_else(String::new, |joins| {
        joins
            .iter()
            .map(|join| {
                format!(
                    "{}JOIN {} ON {}",
                    if join.left { "LEFT " } else { "" },
                    join.table_name,
                    join.on
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    })
}

fn build_where_clause(filters: Option<&[Box<dyn crate::query::BooleanExpression>]>) -> String {
    filters.map_or_else(String::new, |filters| {
        if filters.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", build_where_props(filters).join(" AND "))
        }
    })
}

fn build_where_props(filters: &[Box<dyn crate::query::BooleanExpression>]) -> Vec<String> {
    use std::ops::Deref;
    filters
        .iter()
        .map(|filter| filter.deref().to_sql())
        .collect()
}

fn build_sort_clause(sorts: Option<&[crate::query::Sort]>) -> String {
    sorts.map_or_else(String::new, |sorts| {
        if sorts.is_empty() {
            String::new()
        } else {
            format!("ORDER BY {}", build_sort_props(sorts).join(", "))
        }
    })
}

fn build_sort_props(sorts: &[crate::query::Sort]) -> Vec<String> {
    sorts.iter().map(ToSql::to_sql).collect()
}

fn build_set_clause(values: &[(&str, Box<dyn crate::query::Expression>)]) -> String {
    if values.is_empty() {
        String::new()
    } else {
        format!("SET {}", build_set_props(values).join(", "))
    }
}

fn build_set_props(values: &[(&str, Box<dyn crate::query::Expression>)]) -> Vec<String> {
    use std::ops::Deref;
    values
        .iter()
        .map(|(name, value)| format!("{name}=({})", value.deref().to_sql()))
        .collect()
}

fn build_values_clause(values: &[(&str, Box<dyn crate::query::Expression>)]) -> String {
    if values.is_empty() {
        "DEFAULT VALUES".to_string()
    } else {
        format!("VALUES({})", build_values_props(values).join(", "))
    }
}

fn build_values_props(values: &[(&str, Box<dyn crate::query::Expression>)]) -> Vec<String> {
    use std::ops::Deref;
    values
        .iter()
        .map(|(_, value)| value.deref().to_sql())
        .collect()
}

fn bexprs_to_values(values: &[Box<dyn crate::query::BooleanExpression>]) -> Vec<DatabaseValue> {
    values
        .iter()
        .flat_map(|value| value.params().unwrap_or_default().into_iter().cloned())
        .collect()
}

fn bexprs_to_values_opt(
    values: Option<&[Box<dyn crate::query::BooleanExpression>]>,
) -> Option<Vec<DatabaseValue>> {
    values.map(bexprs_to_values)
}

#[allow(clippy::too_many_arguments)]
async fn select(
    connection: &turso::Connection,
    table_name: &str,
    distinct: bool,
    columns: &[&str],
    filters: Option<&[Box<dyn crate::query::BooleanExpression>]>,
    joins: Option<&[crate::query::Join<'_>]>,
    sort: Option<&[crate::query::Sort]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, TursoDatabaseError> {
    let query = format!(
        "SELECT {} {} FROM {table_name} {} {} {} {}",
        if distinct { "DISTINCT" } else { "" },
        columns.join(", "),
        build_join_clauses(joins),
        build_where_clause(filters),
        build_sort_clause(sort),
        limit.map_or_else(String::new, |limit| format!("LIMIT {limit}"))
    );

    let filter_params = bexprs_to_values_opt(filters).unwrap_or_default();
    log::trace!("Running select query: {query} with params: {filter_params:?}");

    let mut stmt = connection.prepare(&query).await?;
    let column_names: Vec<String> = stmt
        .columns()
        .iter()
        .map(|c| c.name().to_string())
        .collect();

    let params = to_turso_params(&filter_params)?;
    let mut rows = stmt.query(params).await?;

    let mut results = Vec::new();
    while let Some(row) = rows.next().await? {
        results.push(from_turso_row(&column_names, &row)?);
    }

    log::trace!("SELECT: returned {} rows", results.len());
    Ok(results)
}

async fn find_row(
    connection: &turso::Connection,
    table_name: &str,
    distinct: bool,
    columns: &[&str],
    filters: Option<&[Box<dyn crate::query::BooleanExpression>]>,
    joins: Option<&[crate::query::Join<'_>]>,
    sort: Option<&[crate::query::Sort]>,
) -> Result<Option<crate::Row>, TursoDatabaseError> {
    let query = format!(
        "SELECT {} {} FROM {table_name} {} {} {} LIMIT 1",
        if distinct { "DISTINCT" } else { "" },
        columns.join(", "),
        build_join_clauses(joins),
        build_where_clause(filters),
        build_sort_clause(sort),
    );

    let filter_params = bexprs_to_values_opt(filters).unwrap_or_default();
    log::trace!("Running find_row query: {query} with params: {filter_params:?}");

    let mut stmt = connection.prepare(&query).await?;
    let column_names: Vec<String> = stmt
        .columns()
        .iter()
        .map(|c| c.name().to_string())
        .collect();

    let params = to_turso_params(&filter_params)?;
    let mut rows = stmt.query(params).await?;

    if let Some(row) = rows.next().await? {
        log::trace!("find_row: row found");
        let result = from_turso_row(&column_names, &row)?;
        // Drain remaining rows to ensure statement executes to completion
        while rows.next().await?.is_some() {}
        Ok(Some(result))
    } else {
        log::trace!("find_row: no row found");
        Ok(None)
    }
}

async fn insert_and_get_row(
    connection: &turso::Connection,
    table_name: &str,
    values: &[(&str, Box<dyn crate::query::Expression>)],
) -> Result<crate::Row, TursoDatabaseError> {
    let query = if values.is_empty() {
        format!("INSERT INTO {table_name} DEFAULT VALUES RETURNING *")
    } else {
        let columns = values.iter().map(|(name, _)| *name).collect::<Vec<_>>();
        format!(
            "INSERT INTO {table_name} ({}) {} RETURNING *",
            columns.join(", "),
            build_values_clause(values),
        )
    };

    let all_values = values
        .iter()
        .flat_map(|(_, v)| v.params().unwrap_or_default().into_iter().cloned())
        .collect::<Vec<_>>();

    log::trace!("Running insert_and_get_row query: {query} with params: {all_values:?}");

    let mut stmt = connection.prepare(&query).await?;
    let column_names: Vec<String> = stmt
        .columns()
        .iter()
        .map(|c| c.name().to_string())
        .collect();

    let params = to_turso_params(&all_values)?;
    let mut rows = stmt.query(params).await?;

    log::trace!("Fetching first row from INSERT RETURNING");
    if let Some(row) = rows.next().await? {
        let result_row = from_turso_row(&column_names, &row)?;
        log::trace!(
            "INSERT RETURNING: row fetched successfully with columns: {:?}",
            result_row.columns
        );
        // Drain remaining rows to ensure statement executes to completion
        // This is required for turso to properly commit the transaction
        while rows.next().await?.is_some() {}
        Ok(result_row)
    } else {
        Err(TursoDatabaseError::Query(
            "INSERT did not return a row".to_string(),
        ))
    }
}

#[allow(clippy::too_many_lines)]
async fn update_and_get_rows(
    connection: &turso::Connection,
    table_name: &str,
    values: &[(&str, Box<dyn crate::query::Expression>)],
    filters: Option<&[Box<dyn crate::query::BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, TursoDatabaseError> {
    if limit.is_none() {
        let query = format!(
            "UPDATE {table_name} {} {} RETURNING *",
            build_set_clause(values),
            build_where_clause(filters),
        );

        let mut all_values = values
            .iter()
            .flat_map(|(_, v)| v.params().unwrap_or_default().into_iter().cloned())
            .collect::<Vec<_>>();

        let mut filter_values = filters
            .map(|filters| {
                filters
                    .iter()
                    .flat_map(|value| value.params().unwrap_or_default().into_iter().cloned())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        all_values.append(&mut filter_values);

        log::trace!("Running update_and_get_rows query: {query} with params: {all_values:?}");

        let mut stmt = connection.prepare(&query).await?;
        let column_names: Vec<String> = stmt
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect();

        let params = to_turso_params(&all_values)?;
        let mut rows = stmt.query(params).await?;

        let mut results = Vec::new();
        while let Some(row) = rows.next().await? {
            results.push(from_turso_row(&column_names, &row)?);
        }

        log::trace!("UPDATE RETURNING: fetched {} rows", results.len());
        return Ok(results);
    }

    let select_query = format!(
        "SELECT rowid FROM {table_name} {} LIMIT {}",
        build_where_clause(filters),
        limit.unwrap(),
    );

    let filter_values = filters
        .map(|filters| {
            filters
                .iter()
                .flat_map(|value| value.params().unwrap_or_default().into_iter().cloned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    log::trace!(
        "Running update (select phase) query: {select_query} with params: {filter_values:?}"
    );

    let rowids = {
        let mut stmt = connection.prepare(&select_query).await?;
        let params = to_turso_params(&filter_values)?;
        let mut rows = stmt.query(params).await?;

        let mut rowids = Vec::new();
        while let Some(row) = rows.next().await? {
            rowids.push(row.get::<i64>(0)?);
        }
        log::trace!(
            "UPDATE: selected {} rowids for LIMIT update: {:?}",
            rowids.len(),
            rowids
        );
        rowids
    };

    if rowids.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = (0..rowids.len())
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");

    let update_query = format!(
        "UPDATE {table_name} {} WHERE rowid IN ({placeholders}) RETURNING *",
        build_set_clause(values),
    );

    let mut update_values = values
        .iter()
        .flat_map(|(_, v)| v.params().unwrap_or_default().into_iter().cloned())
        .collect::<Vec<_>>();

    let rowid_values: Vec<DatabaseValue> = rowids.into_iter().map(DatabaseValue::Int64).collect();

    let mut rowid_params = rowid_values;
    update_values.append(&mut rowid_params);

    log::trace!("Running update query: {update_query} with params: {update_values:?}");

    let mut stmt = connection.prepare(&update_query).await?;
    let column_names: Vec<String> = stmt
        .columns()
        .iter()
        .map(|c| c.name().to_string())
        .collect();

    let params = to_turso_params(&update_values)?;
    let mut rows = stmt.query(params).await?;

    let mut results = Vec::new();
    while let Some(row) = rows.next().await? {
        results.push(from_turso_row(&column_names, &row)?);
    }

    log::trace!("UPDATE RETURNING: fetched {} rows", results.len());
    Ok(results)
}

async fn update_and_get_row(
    connection: &turso::Connection,
    table_name: &str,
    values: &[(&str, Box<dyn crate::query::Expression>)],
    filters: Option<&[Box<dyn crate::query::BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Option<crate::Row>, TursoDatabaseError> {
    let rows = update_and_get_rows(connection, table_name, values, filters, limit).await?;
    Ok(rows.into_iter().next())
}

async fn delete(
    connection: &turso::Connection,
    table_name: &str,
    filters: Option<&[Box<dyn crate::query::BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, TursoDatabaseError> {
    let select_cols = if limit.is_some() {
        "rowid, *".to_string()
    } else {
        "*".to_string()
    };

    let select_query = format!(
        "SELECT {select_cols} FROM {table_name} {} {}",
        build_where_clause(filters),
        limit.map_or_else(String::new, |limit| format!("LIMIT {limit}")),
    );

    let filter_values = filters
        .map(|filters| {
            filters
                .iter()
                .flat_map(|value| value.params().unwrap_or_default().into_iter().cloned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    log::trace!(
        "Running delete (select phase) query: {select_query} with params: {filter_values:?}"
    );

    let (results, rowids) = {
        let mut stmt = connection.prepare(&select_query).await?;
        let all_column_names: Vec<String> = stmt
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect();

        log::trace!(
            "DELETE: query returned {} columns: {:?}",
            all_column_names.len(),
            all_column_names
        );

        let mut column_names = all_column_names.clone();
        if limit.is_some() && !column_names.is_empty() {
            column_names.remove(0);
        }

        let params = to_turso_params(&filter_values)?;
        let mut rows = stmt.query(params).await?;

        let mut results = Vec::new();
        let mut rowids = Vec::new();
        while let Some(row) = rows.next().await? {
            if limit.is_some() {
                rowids.push(row.get::<i64>(0)?);
                let mut row_columns = Vec::with_capacity(column_names.len());
                for (col_idx, column_name) in column_names.iter().enumerate() {
                    let value = row.get_value(col_idx + 1).map_err(|e| {
                        TursoDatabaseError::Query(format!(
                            "Failed to get column {} (index {}): {e}",
                            column_name,
                            col_idx + 1
                        ))
                    })?;
                    row_columns.push((column_name.clone(), DatabaseValue::from(value)));
                }
                results.push(crate::Row {
                    columns: row_columns,
                });
            } else {
                results.push(from_turso_row(&column_names, &row)?);
            }
        }
        (results, rowids)
    };

    let delete_query = if let Some(_limit) = limit {
        if rowids.is_empty() {
            return Ok(results);
        }
        let placeholders = (0..rowids.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", ");
        format!("DELETE FROM {table_name} WHERE rowid IN ({placeholders})")
    } else {
        format!("DELETE FROM {table_name} {}", build_where_clause(filters),)
    };

    let delete_params = if limit.is_some() {
        to_turso_params(
            &rowids
                .into_iter()
                .map(DatabaseValue::Int64)
                .collect::<Vec<_>>(),
        )?
    } else {
        to_turso_params(&filter_values)?
    };

    log::trace!("Running delete query: {delete_query}");
    connection.execute(&delete_query, delete_params).await?;

    log::trace!("DELETE: deleted {} rows", results.len());
    Ok(results)
}

async fn upsert(
    connection: &turso::Connection,
    table_name: &str,
    values: &[(&str, Box<dyn crate::query::Expression>)],
    filters: Option<&[Box<dyn crate::query::BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, TursoDatabaseError> {
    log::trace!("Running upsert on table: {table_name}");
    let rows = update_and_get_rows(connection, table_name, values, filters, limit).await?;

    Ok(if rows.is_empty() {
        log::trace!("UPSERT: no rows updated, performing insert");
        vec![insert_and_get_row(connection, table_name, values).await?]
    } else {
        log::trace!("UPSERT: updated {} rows", rows.len());
        rows
    })
}

async fn upsert_and_get_row(
    connection: &turso::Connection,
    table_name: &str,
    values: &[(&str, Box<dyn crate::query::Expression>)],
    filters: Option<&[Box<dyn crate::query::BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<crate::Row, TursoDatabaseError> {
    match find_row(connection, table_name, false, &["*"], filters, None, None).await? {
        Some(row) => {
            let updated = update_and_get_row(connection, table_name, values, filters, limit)
                .await?
                .ok_or_else(|| {
                    TursoDatabaseError::Query("UPDATE did not return a row".to_string())
                })?;

            let str1 = format!("{row:?}");
            let str2 = format!("{updated:?}");

            if str1 == str2 {
                log::trace!("No updates to {table_name}");
            } else {
                log::debug!("Changed {table_name} from {str1} to {str2}");
            }

            Ok(updated)
        }
        None => insert_and_get_row(connection, table_name, values).await,
    }
}

#[async_trait::async_trait]
impl crate::Database for TursoDatabase {
    async fn query_raw(&self, query: &str) -> Result<Vec<crate::Row>, crate::DatabaseError> {
        log::trace!("query_raw: query:\n{query}");

        let mut stmt = self
            .connection
            .prepare(query)
            .await
            .map_err(|e| crate::DatabaseError::QueryFailed(e.to_string()))?;

        let column_info = stmt.columns();
        let column_names: Vec<String> = column_info.iter().map(|c| c.name().to_string()).collect();

        let mut rows = stmt
            .query(())
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

        let mut results = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?
        {
            results.push(from_turso_row(&column_names, &row).map_err(crate::DatabaseError::Turso)?);
        }

        log::trace!("query_raw: returned {} rows", results.len());
        Ok(results)
    }

    async fn query_raw_params(
        &self,
        query: &str,
        params: &[DatabaseValue],
    ) -> Result<Vec<crate::Row>, crate::DatabaseError> {
        log::trace!("query_raw_params: query: {query} with params: {params:?}");

        let (transformed_query, filtered_params) = turso_transform_query_for_params(query, params)?;

        let mut stmt = self
            .connection
            .prepare(&transformed_query)
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

        let column_info = stmt.columns();
        let column_names: Vec<String> = column_info.iter().map(|c| c.name().to_string()).collect();

        let turso_params =
            to_turso_params(&filtered_params).map_err(crate::DatabaseError::Turso)?;

        let mut rows = stmt
            .query(turso_params)
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

        let mut results = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?
        {
            results.push(from_turso_row(&column_names, &row).map_err(crate::DatabaseError::Turso)?);
        }

        log::trace!("query_raw_params: returned {} rows", results.len());
        Ok(results)
    }

    async fn exec_raw(&self, statement: &str) -> Result<(), crate::DatabaseError> {
        log::trace!("exec_raw: query:\n{statement}");

        self.connection
            .execute(statement, ())
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

        log::trace!("exec_raw: completed");
        Ok(())
    }

    async fn exec_raw_params(
        &self,
        query: &str,
        params: &[DatabaseValue],
    ) -> Result<u64, crate::DatabaseError> {
        log::trace!("exec_raw_params: query: {query} with params: {params:?}");

        let (transformed_query, filtered_params) = turso_transform_query_for_params(query, params)?;

        let turso_params =
            to_turso_params(&filtered_params).map_err(crate::DatabaseError::Turso)?;

        let mut stmt = self
            .connection
            .prepare(&transformed_query)
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

        let affected_rows = stmt
            .execute(turso_params)
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

        log::trace!("exec_raw_params: affected {affected_rows} rows");
        Ok(affected_rows)
    }

    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn crate::DatabaseTransaction>, crate::DatabaseError> {
        log::debug!("begin_transaction: creating new connection for transaction");

        let connection = self.database.connect()?;

        let tx = TursoTransaction::new(connection)
            .await
            .map_err(crate::DatabaseError::Turso)?;

        log::debug!("begin_transaction: transaction started");
        Ok(Box::new(tx))
    }

    async fn query(
        &self,
        query: &crate::query::SelectQuery<'_>,
    ) -> Result<Vec<crate::Row>, crate::DatabaseError> {
        Ok(select(
            &self.connection,
            query.table_name,
            query.distinct,
            query.columns,
            query.filters.as_deref(),
            query.joins.as_deref(),
            query.sorts.as_deref(),
            query.limit,
        )
        .await
        .map_err(crate::DatabaseError::Turso)?)
    }

    async fn query_first(
        &self,
        query: &crate::query::SelectQuery<'_>,
    ) -> Result<Option<crate::Row>, crate::DatabaseError> {
        Ok(find_row(
            &self.connection,
            query.table_name,
            query.distinct,
            query.columns,
            query.filters.as_deref(),
            query.joins.as_deref(),
            query.sorts.as_deref(),
        )
        .await
        .map_err(crate::DatabaseError::Turso)?)
    }

    async fn exec_update(
        &self,
        statement: &crate::query::UpdateStatement<'_>,
    ) -> Result<Vec<crate::Row>, crate::DatabaseError> {
        Ok(update_and_get_rows(
            &self.connection,
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            statement.limit,
        )
        .await
        .map_err(crate::DatabaseError::Turso)?)
    }

    async fn exec_update_first(
        &self,
        statement: &crate::query::UpdateStatement<'_>,
    ) -> Result<Option<crate::Row>, crate::DatabaseError> {
        Ok(update_and_get_row(
            &self.connection,
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            Some(1),
        )
        .await
        .map_err(crate::DatabaseError::Turso)?)
    }

    async fn exec_insert(
        &self,
        statement: &crate::query::InsertStatement<'_>,
    ) -> Result<crate::Row, crate::DatabaseError> {
        Ok(
            insert_and_get_row(&self.connection, statement.table_name, &statement.values)
                .await
                .map_err(crate::DatabaseError::Turso)?,
        )
    }

    async fn exec_upsert(
        &self,
        statement: &crate::query::UpsertStatement<'_>,
    ) -> Result<Vec<crate::Row>, crate::DatabaseError> {
        Ok(upsert(
            &self.connection,
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            statement.limit,
        )
        .await
        .map_err(crate::DatabaseError::Turso)?)
    }

    async fn exec_upsert_first(
        &self,
        statement: &crate::query::UpsertStatement<'_>,
    ) -> Result<crate::Row, crate::DatabaseError> {
        Ok(upsert_and_get_row(
            &self.connection,
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            statement.limit,
        )
        .await
        .map_err(crate::DatabaseError::Turso)?)
    }

    async fn exec_upsert_multi(
        &self,
        statement: &crate::query::UpsertMultiStatement<'_>,
    ) -> Result<Vec<crate::Row>, crate::DatabaseError> {
        let mut all_results = Vec::new();
        for values in &statement.values {
            let results = upsert(&self.connection, statement.table_name, values, None, None)
                .await
                .map_err(crate::DatabaseError::Turso)?;
            all_results.extend(results);
        }

        Ok(all_results)
    }

    async fn exec_delete(
        &self,
        statement: &crate::query::DeleteStatement<'_>,
    ) -> Result<Vec<crate::Row>, crate::DatabaseError> {
        Ok(delete(
            &self.connection,
            statement.table_name,
            statement.filters.as_deref(),
            statement.limit,
        )
        .await
        .map_err(crate::DatabaseError::Turso)?)
    }

    async fn exec_delete_first(
        &self,
        statement: &crate::query::DeleteStatement<'_>,
    ) -> Result<Option<crate::Row>, crate::DatabaseError> {
        let rows = delete(
            &self.connection,
            statement.table_name,
            statement.filters.as_deref(),
            Some(1),
        )
        .await
        .map_err(crate::DatabaseError::Turso)?;
        Ok(rows.into_iter().next())
    }

    #[cfg(feature = "schema")]
    async fn exec_create_table(
        &self,
        statement: &crate::schema::CreateTableStatement<'_>,
    ) -> Result<(), crate::DatabaseError> {
        exec_create_table(&self.connection, statement).await
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_table(
        &self,
        statement: &crate::schema::DropTableStatement<'_>,
    ) -> Result<(), crate::DatabaseError> {
        exec_drop_table(&self.connection, statement).await
    }

    #[cfg(feature = "schema")]
    async fn exec_create_index(
        &self,
        statement: &crate::schema::CreateIndexStatement<'_>,
    ) -> Result<(), crate::DatabaseError> {
        exec_create_index(&self.connection, statement).await
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_index(
        &self,
        statement: &crate::schema::DropIndexStatement<'_>,
    ) -> Result<(), crate::DatabaseError> {
        exec_drop_index(&self.connection, statement).await
    }

    #[cfg(feature = "schema")]
    async fn exec_alter_table(
        &self,
        statement: &crate::schema::AlterTableStatement<'_>,
    ) -> Result<(), crate::DatabaseError> {
        exec_alter_table(&self.connection, statement).await
    }

    #[cfg(feature = "schema")]
    async fn table_exists(&self, table: &str) -> Result<bool, crate::DatabaseError> {
        let query = "SELECT name FROM sqlite_master WHERE type='table' AND name=?";
        let rows = self
            .query_raw_params(query, &[DatabaseValue::String(table.to_string())])
            .await?;
        Ok(!rows.is_empty())
    }

    #[cfg(feature = "schema")]
    async fn list_tables(&self) -> Result<Vec<String>, crate::DatabaseError> {
        let query =
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'";
        let rows = self.query_raw(query).await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| row.get("name"))
            .filter_map(|v| match v {
                DatabaseValue::String(s) => Some(s),
                _ => None,
            })
            .collect())
    }

    #[cfg(feature = "schema")]
    async fn get_table_info(
        &self,
        table: &str,
    ) -> Result<Option<crate::schema::TableInfo>, crate::DatabaseError> {
        if !self.table_exists(table).await? {
            return Ok(None);
        }

        let columns = self.get_table_columns(table).await?;

        let columns_map = columns
            .into_iter()
            .map(|col| (col.name.clone(), col))
            .collect();

        let indexes = get_table_indexes(self, table).await?;
        let foreign_keys = get_table_foreign_keys(self, table).await?;

        Ok(Some(crate::schema::TableInfo {
            name: table.to_string(),
            columns: columns_map,
            indexes,
            foreign_keys,
        }))
    }

    #[cfg(feature = "schema")]
    async fn get_table_columns(
        &self,
        table: &str,
    ) -> Result<Vec<crate::schema::ColumnInfo>, crate::DatabaseError> {
        let query = format!("PRAGMA table_info({table})");
        let rows = self.query_raw(&query).await?;

        let create_sql_query = "SELECT sql FROM sqlite_master WHERE type='table' AND name=?";
        let create_sql_rows = self
            .query_raw_params(
                create_sql_query,
                &[DatabaseValue::String(table.to_string())],
            )
            .await?;

        let create_sql = create_sql_rows
            .into_iter()
            .find_map(|row| match row.get("sql") {
                Some(DatabaseValue::String(s)) => Some(s),
                _ => None,
            });

        let mut columns = Vec::new();

        for row in rows {
            let ordinal = match row.get("cid") {
                Some(DatabaseValue::Int64(i)) => {
                    u32::try_from(i).unwrap_or_else(|_| u32::try_from(columns.len()).unwrap_or(0))
                }
                _ => u32::try_from(columns.len()).unwrap_or(0),
            };

            let name = match row.get("name") {
                Some(DatabaseValue::String(s)) => s.clone(),
                _ => continue,
            };

            let type_str = match row.get("type") {
                Some(DatabaseValue::String(s)) => s.clone(),
                _ => String::from("TEXT"),
            };

            let not_null = match row.get("notnull") {
                Some(DatabaseValue::Int64(i)) => i != 0,
                _ => false,
            };

            let is_pk = match row.get("pk") {
                Some(DatabaseValue::Int64(i)) => i != 0,
                _ => false,
            };

            let default_value = match row.get("dflt_value") {
                Some(DatabaseValue::String(s)) => Some(s.clone()),
                _ => None,
            };

            let data_type = sqlite_type_to_data_type(&type_str);
            let default_val = parse_default_value(default_value.as_deref());

            let auto_increment = if is_pk {
                check_autoincrement_in_sql(create_sql.as_deref(), &name)
            } else {
                false
            };

            columns.push(crate::schema::ColumnInfo {
                name,
                data_type,
                nullable: !not_null,
                is_primary_key: is_pk,
                auto_increment,
                default_value: default_val,
                ordinal_position: ordinal + 1,
            });
        }

        Ok(columns)
    }

    #[cfg(feature = "schema")]
    async fn column_exists(&self, table: &str, column: &str) -> Result<bool, crate::DatabaseError> {
        let columns = self.get_table_columns(table).await?;
        Ok(columns.iter().any(|col| col.name == column))
    }
}

#[cfg(feature = "schema")]
#[allow(clippy::too_many_lines)]
async fn exec_create_table(
    conn: &turso::Connection,
    statement: &crate::schema::CreateTableStatement<'_>,
) -> Result<(), crate::DatabaseError> {
    let mut query = "CREATE TABLE ".to_string();

    if statement.if_not_exists {
        query.push_str("IF NOT EXISTS ");
    }

    query.push_str(statement.table_name);
    query.push('(');

    let mut first = true;

    for column in &statement.columns {
        if first {
            first = false;
        } else {
            query.push(',');
        }

        if column.auto_increment && statement.primary_key.is_none_or(|x| x != column.name) {
            return Err(crate::DatabaseError::InvalidSchema(format!(
                "Column '{}' must be the primary key to enable auto increment",
                &column.name
            )));
        }

        query.push_str(&column.name);
        query.push(' ');

        match column.data_type {
            crate::schema::DataType::VarChar(size) => {
                query.push_str("VARCHAR(");
                query.push_str(&size.to_string());
                query.push(')');
            }
            crate::schema::DataType::Text
            | crate::schema::DataType::Date
            | crate::schema::DataType::Time
            | crate::schema::DataType::DateTime
            | crate::schema::DataType::Timestamp
            | crate::schema::DataType::Json
            | crate::schema::DataType::Jsonb
            | crate::schema::DataType::Uuid
            | crate::schema::DataType::Xml
            | crate::schema::DataType::Array(..)
            | crate::schema::DataType::Inet
            | crate::schema::DataType::MacAddr
            | crate::schema::DataType::Decimal(..) => query.push_str("TEXT"),
            crate::schema::DataType::Char(size) => {
                query.push_str("CHAR(");
                query.push_str(&size.to_string());
                query.push(')');
            }
            crate::schema::DataType::Bool
            | crate::schema::DataType::TinyInt
            | crate::schema::DataType::SmallInt
            | crate::schema::DataType::Int
            | crate::schema::DataType::BigInt
            | crate::schema::DataType::Serial
            | crate::schema::DataType::BigSerial => query.push_str("INTEGER"),
            crate::schema::DataType::Real
            | crate::schema::DataType::Double
            | crate::schema::DataType::Money => query.push_str("REAL"),
            crate::schema::DataType::Blob | crate::schema::DataType::Binary(_) => {
                query.push_str("BLOB");
            }
            crate::schema::DataType::Custom(ref type_name) => query.push_str(type_name),
        }

        if !column.nullable {
            query.push_str(" NOT NULL");
        }

        if column.auto_increment && statement.primary_key.is_some_and(|pk| pk == column.name) {
            query.push_str(" PRIMARY KEY AUTOINCREMENT");
        }

        if let Some(default) = &column.default {
            query.push_str(" DEFAULT ");

            match default {
                crate::DatabaseValue::Null
                | crate::DatabaseValue::StringOpt(None)
                | crate::DatabaseValue::BoolOpt(None)
                | crate::DatabaseValue::Int8Opt(None)
                | crate::DatabaseValue::Int16Opt(None)
                | crate::DatabaseValue::Int32Opt(None)
                | crate::DatabaseValue::Int64Opt(None)
                | crate::DatabaseValue::UInt8Opt(None)
                | crate::DatabaseValue::UInt16Opt(None)
                | crate::DatabaseValue::UInt32Opt(None)
                | crate::DatabaseValue::UInt64Opt(None)
                | crate::DatabaseValue::Real64Opt(None)
                | crate::DatabaseValue::Real32Opt(None) => {
                    query.push_str("NULL");
                }
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::DecimalOpt(None) => {
                    query.push_str("NULL");
                }
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::UuidOpt(None) => {
                    query.push_str("NULL");
                }
                crate::DatabaseValue::StringOpt(Some(x)) | crate::DatabaseValue::String(x) => {
                    query.push('\'');
                    query.push_str(x);
                    query.push('\'');
                }
                crate::DatabaseValue::BoolOpt(Some(x)) | crate::DatabaseValue::Bool(x) => {
                    query.push_str(if *x { "1" } else { "0" });
                }
                crate::DatabaseValue::Int8Opt(Some(x)) | crate::DatabaseValue::Int8(x) => {
                    query.push_str(&x.to_string());
                }
                crate::DatabaseValue::Int16Opt(Some(x)) | crate::DatabaseValue::Int16(x) => {
                    query.push_str(&x.to_string());
                }
                crate::DatabaseValue::Int32Opt(Some(x)) | crate::DatabaseValue::Int32(x) => {
                    query.push_str(&x.to_string());
                }
                crate::DatabaseValue::Int64Opt(Some(x)) | crate::DatabaseValue::Int64(x) => {
                    query.push_str(&x.to_string());
                }
                crate::DatabaseValue::UInt8Opt(Some(x)) | crate::DatabaseValue::UInt8(x) => {
                    query.push_str(&x.to_string());
                }
                crate::DatabaseValue::UInt16Opt(Some(x)) | crate::DatabaseValue::UInt16(x) => {
                    query.push_str(&x.to_string());
                }
                crate::DatabaseValue::UInt32Opt(Some(x)) | crate::DatabaseValue::UInt32(x) => {
                    query.push_str(&x.to_string());
                }
                crate::DatabaseValue::UInt64Opt(Some(x)) | crate::DatabaseValue::UInt64(x) => {
                    query.push_str(&x.to_string());
                }
                crate::DatabaseValue::Real64Opt(Some(x)) | crate::DatabaseValue::Real64(x) => {
                    query.push_str(&x.to_string());
                }
                crate::DatabaseValue::Real32Opt(Some(x)) | crate::DatabaseValue::Real32(x) => {
                    query.push_str(&x.to_string());
                }
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::DecimalOpt(Some(x)) | crate::DatabaseValue::Decimal(x) => {
                    query.push('\'');
                    query.push_str(&x.to_string());
                    query.push('\'');
                }
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::Uuid(u) | crate::DatabaseValue::UuidOpt(Some(u)) => {
                    query.push('\'');
                    query.push_str(&u.to_string());
                    query.push('\'');
                }
                crate::DatabaseValue::NowPlus(interval) => {
                    let modifiers = format_sqlite_interval(interval);
                    let modifier_str = modifiers
                        .iter()
                        .map(|m| format!("'{m}'"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    {
                        use std::fmt::Write;
                        write!(
                            query,
                            "(strftime('%Y-%m-%dT%H:%M:%f', datetime('now', {modifier_str})))"
                        )
                        .unwrap();
                    }
                }
                crate::DatabaseValue::Now => {
                    query.push_str("(strftime('%Y-%m-%dT%H:%M:%f', 'now'))");
                }
                crate::DatabaseValue::DateTime(x) => {
                    query.push('\'');
                    query.push_str(&x.and_utc().to_rfc3339());
                    query.push('\'');
                }
            }
        }
    }

    moosicbox_assert::assert!(!first);

    let pk_in_column = statement.primary_key.is_some_and(|pk| {
        statement
            .columns
            .iter()
            .any(|col| col.name == pk && col.auto_increment)
    });

    if let Some(primary_key) = &statement.primary_key
        && !pk_in_column
    {
        query.push_str(", PRIMARY KEY (");
        query.push_str(primary_key);
        query.push(')');
    }

    for (source, target) in &statement.foreign_keys {
        query.push_str(", FOREIGN KEY (");
        query.push_str(source);
        query.push_str(") REFERENCES ");
        query.push_str(target);
    }

    query.push(')');

    conn.execute(&query, ())
        .await
        .map_err(TursoDatabaseError::Turso)?;

    Ok(())
}

#[cfg(feature = "schema")]
async fn exec_drop_table(
    conn: &turso::Connection,
    statement: &crate::schema::DropTableStatement<'_>,
) -> Result<(), crate::DatabaseError> {
    #[cfg(feature = "cascade")]
    {
        use crate::schema::DropBehavior;
        match statement.behavior {
            DropBehavior::Cascade => {
                // Get drop order (dependents first, target last)
                let drop_order = find_cascade_dependents(conn, statement.table_name).await?;

                // Drop tables in order (Turso doesn't support PRAGMA foreign_keys,
                // so we rely on the correct drop order to avoid FK violations)
                for table in &drop_order {
                    let drop_sql = if statement.if_exists {
                        format!("DROP TABLE IF EXISTS {table}")
                    } else {
                        format!("DROP TABLE {table}")
                    };

                    conn.execute(&drop_sql, ())
                        .await
                        .map_err(TursoDatabaseError::Turso)?;
                }

                return Ok(());
            }
            DropBehavior::Restrict => {
                // Check if table has dependents
                let has_deps = has_dependents(conn, statement.table_name).await?;

                if has_deps {
                    return Err(crate::DatabaseError::InvalidQuery(format!(
                        "Cannot drop table '{}' because other tables depend on it",
                        statement.table_name
                    )));
                }

                // No dependents, proceed with normal DROP
            }
            DropBehavior::Default => {}
        }
    }

    let mut query = "DROP TABLE ".to_string();

    if statement.if_exists {
        query.push_str("IF EXISTS ");
    }

    query.push_str(statement.table_name);

    conn.execute(&query, ())
        .await
        .map_err(TursoDatabaseError::Turso)?;

    Ok(())
}

#[cfg(feature = "schema")]
async fn exec_create_index(
    conn: &turso::Connection,
    statement: &crate::schema::CreateIndexStatement<'_>,
) -> Result<(), crate::DatabaseError> {
    let unique_str = if statement.unique { "UNIQUE " } else { "" };
    let if_not_exists_str = if statement.if_not_exists {
        "IF NOT EXISTS "
    } else {
        ""
    };

    let columns_str = statement
        .columns
        .iter()
        .map(|col| format!("`{col}`"))
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!(
        "CREATE {}INDEX {}{} ON {} ({})",
        unique_str, if_not_exists_str, statement.index_name, statement.table_name, columns_str
    );

    conn.execute(&sql, ())
        .await
        .map_err(TursoDatabaseError::Turso)?;

    Ok(())
}

#[cfg(feature = "schema")]
async fn exec_drop_index(
    conn: &turso::Connection,
    statement: &crate::schema::DropIndexStatement<'_>,
) -> Result<(), crate::DatabaseError> {
    let if_exists_str = if statement.if_exists {
        "IF EXISTS "
    } else {
        ""
    };

    let sql = format!("DROP INDEX {}{}", if_exists_str, statement.index_name);

    conn.execute(&sql, ())
        .await
        .map_err(TursoDatabaseError::Turso)?;

    Ok(())
}

#[cfg(feature = "schema")]
async fn column_requires_table_recreation(
    conn: &turso::Connection,
    table_name: &str,
    column_name: &str,
) -> Result<bool, crate::DatabaseError> {
    let params = to_turso_params(&[DatabaseValue::String(table_name.to_string())])
        .map_err(crate::DatabaseError::Turso)?;

    let mut stmt = conn
        .prepare("SELECT sql FROM sqlite_master WHERE type='table' AND name=?")
        .await
        .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

    let column_info = stmt.columns();
    let column_names: Vec<String> = column_info.iter().map(|c| c.name().to_string()).collect();

    let mut rows = stmt
        .query(params)
        .await
        .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

    let row = rows
        .next()
        .await
        .map_err(|e| crate::DatabaseError::Turso(e.into()))?
        .ok_or_else(|| {
            crate::DatabaseError::InvalidQuery(format!("Table '{table_name}' not found"))
        })?;

    let turso_row = from_turso_row(&column_names, &row).map_err(crate::DatabaseError::Turso)?;

    let table_sql: String = turso_row
        .get("sql")
        .and_then(|v| match v {
            DatabaseValue::String(s) => Some(s),
            _ => None,
        })
        .ok_or_else(|| {
            crate::DatabaseError::InvalidQuery(format!("Table '{table_name}' not found"))
        })?;

    let table_sql_upper = table_sql.to_uppercase();
    let column_name_upper = column_name.to_uppercase();

    if table_sql_upper.contains(&format!("{column_name_upper} "))
        && table_sql_upper.contains("PRIMARY KEY")
    {
        let column_pos = table_sql_upper.find(&column_name_upper);
        let pk_pos = table_sql_upper.find("PRIMARY KEY");
        if let (Some(col_pos), Some(pk_pos)) = (column_pos, pk_pos)
            && pk_pos > col_pos
            && (pk_pos - col_pos) < 200
        {
            return Ok(true);
        }
    }

    if table_sql_upper.contains(&format!("{column_name_upper} "))
        && table_sql_upper.contains("UNIQUE")
    {
        let column_pos = table_sql_upper.find(&column_name_upper);
        let unique_pos = table_sql_upper.find("UNIQUE");
        if let (Some(col_pos), Some(unique_pos)) = (column_pos, unique_pos)
            && unique_pos > col_pos
            && (unique_pos - col_pos) < 100
        {
            return Ok(true);
        }
    }

    if table_sql_upper.contains("CHECK") && table_sql_upper.contains(&column_name_upper) {
        return Ok(true);
    }

    if table_sql_upper.contains(&format!("{column_name_upper} "))
        && table_sql_upper.contains("GENERATED")
    {
        return Ok(true);
    }

    let params = to_turso_params(&[DatabaseValue::String(table_name.to_string())])
        .map_err(crate::DatabaseError::Turso)?;

    let mut stmt = conn
        .prepare(
            "SELECT sql FROM sqlite_master WHERE type='index' AND tbl_name=? AND sql IS NOT NULL",
        )
        .await
        .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

    let column_info = stmt.columns();
    let column_names: Vec<String> = column_info.iter().map(|c| c.name().to_string()).collect();

    let mut rows = stmt
        .query(params)
        .await
        .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

    while let Some(row) = rows
        .next()
        .await
        .map_err(|e| crate::DatabaseError::Turso(e.into()))?
    {
        let turso_row = from_turso_row(&column_names, &row).map_err(crate::DatabaseError::Turso)?;
        if let Some(DatabaseValue::String(index_sql)) = turso_row.get("sql") {
            let index_sql_upper = index_sql.to_uppercase();
            if index_sql_upper.contains("UNIQUE") && index_sql_upper.contains(&column_name_upper) {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

#[cfg(feature = "schema")]
#[allow(clippy::too_many_lines)]
fn modify_create_table_sql(
    original_sql: &str,
    original_table_name: &str,
    new_table_name: &str,
    column_name: &str,
    new_data_type: &crate::schema::DataType,
    new_nullable: Option<bool>,
    new_default: Option<&crate::DatabaseValue>,
) -> Result<String, crate::DatabaseError> {
    let data_type_str = match new_data_type {
        crate::schema::DataType::Text
        | crate::schema::DataType::VarChar(_)
        | crate::schema::DataType::Char(_)
        | crate::schema::DataType::Date
        | crate::schema::DataType::Time
        | crate::schema::DataType::DateTime
        | crate::schema::DataType::Timestamp
        | crate::schema::DataType::Json
        | crate::schema::DataType::Jsonb
        | crate::schema::DataType::Uuid
        | crate::schema::DataType::Xml
        | crate::schema::DataType::Array(..)
        | crate::schema::DataType::Inet
        | crate::schema::DataType::MacAddr
        | crate::schema::DataType::Custom(_)
        | crate::schema::DataType::Decimal(..) => "TEXT",
        crate::schema::DataType::Bool
        | crate::schema::DataType::TinyInt
        | crate::schema::DataType::SmallInt
        | crate::schema::DataType::Int
        | crate::schema::DataType::BigInt
        | crate::schema::DataType::Serial
        | crate::schema::DataType::BigSerial => "INTEGER",
        crate::schema::DataType::Real
        | crate::schema::DataType::Double
        | crate::schema::DataType::Money => "REAL",
        crate::schema::DataType::Blob | crate::schema::DataType::Binary(_) => "BLOB",
    };

    let mut new_column_def = format!("`{column_name}` {data_type_str}");

    if let Some(nullable) = new_nullable
        && !nullable
    {
        new_column_def.push_str(" NOT NULL");
    }

    if let Some(default_value) = new_default {
        use std::fmt::Write;

        let default_str = match default_value {
            crate::DatabaseValue::String(s) | crate::DatabaseValue::StringOpt(Some(s)) => {
                format!("'{}'", s.replace('\'', "''"))
            }
            crate::DatabaseValue::StringOpt(None) | crate::DatabaseValue::Null => {
                "NULL".to_string()
            }
            crate::DatabaseValue::Int8(i) | crate::DatabaseValue::Int8Opt(Some(i)) => i.to_string(),
            crate::DatabaseValue::Int16(i) | crate::DatabaseValue::Int16Opt(Some(i)) => {
                i.to_string()
            }
            crate::DatabaseValue::Int32(i) | crate::DatabaseValue::Int32Opt(Some(i)) => {
                i.to_string()
            }
            crate::DatabaseValue::Int64(i) | crate::DatabaseValue::Int64Opt(Some(i)) => {
                i.to_string()
            }
            crate::DatabaseValue::UInt8(i) | crate::DatabaseValue::UInt8Opt(Some(i)) => {
                i.to_string()
            }
            crate::DatabaseValue::UInt16(i) | crate::DatabaseValue::UInt16Opt(Some(i)) => {
                i.to_string()
            }
            crate::DatabaseValue::UInt32(i) | crate::DatabaseValue::UInt32Opt(Some(i)) => {
                i.to_string()
            }
            crate::DatabaseValue::Int8Opt(None)
            | crate::DatabaseValue::Int16Opt(None)
            | crate::DatabaseValue::Int32Opt(None)
            | crate::DatabaseValue::Int64Opt(None)
            | crate::DatabaseValue::UInt8Opt(None)
            | crate::DatabaseValue::UInt16Opt(None)
            | crate::DatabaseValue::UInt32Opt(None)
            | crate::DatabaseValue::UInt64Opt(None)
            | crate::DatabaseValue::Real64Opt(None)
            | crate::DatabaseValue::Real32Opt(None)
            | crate::DatabaseValue::BoolOpt(None) => "NULL".to_string(),
            #[cfg(feature = "decimal")]
            crate::DatabaseValue::DecimalOpt(None) => "NULL".to_string(),
            #[cfg(feature = "uuid")]
            crate::DatabaseValue::UuidOpt(None) => "NULL".to_string(),
            crate::DatabaseValue::UInt64(i) | crate::DatabaseValue::UInt64Opt(Some(i)) => {
                i.to_string()
            }
            crate::DatabaseValue::Real64(f) | crate::DatabaseValue::Real64Opt(Some(f)) => {
                f.to_string()
            }
            crate::DatabaseValue::Real32(f) | crate::DatabaseValue::Real32Opt(Some(f)) => {
                f.to_string()
            }
            #[cfg(feature = "decimal")]
            crate::DatabaseValue::Decimal(d) | crate::DatabaseValue::DecimalOpt(Some(d)) => {
                format!("'{d}'")
            }
            #[cfg(feature = "uuid")]
            crate::DatabaseValue::Uuid(u) | crate::DatabaseValue::UuidOpt(Some(u)) => {
                format!("'{u}'")
            }
            crate::DatabaseValue::Bool(b) | crate::DatabaseValue::BoolOpt(Some(b)) => {
                if *b { "1" } else { "0" }.to_string()
            }
            crate::DatabaseValue::DateTime(dt) => format!("'{}'", dt.format("%Y-%m-%d %H:%M:%S")),
            crate::DatabaseValue::Now => "CURRENT_TIMESTAMP".to_string(),
            crate::DatabaseValue::NowPlus(_) => {
                return Err(crate::DatabaseError::InvalidQuery(
                    "NowPlus not supported in ModifyColumn DEFAULT".to_string(),
                ));
            }
        };

        write!(new_column_def, " DEFAULT {default_str}").unwrap();
    }

    let column_pattern = format!(
        r"`?{}`?\s+\w+(\s+(NOT\s+NULL|PRIMARY\s+KEY|UNIQUE|CHECK\s*\([^)]+\)|DEFAULT\s+[^,\s)]+|GENERATED\s+[^,)]+))*",
        regex::escape(column_name)
    );

    let re = regex::Regex::new(&column_pattern).map_err(|_| {
        crate::DatabaseError::InvalidQuery(format!(
            "Failed to create regex for column '{column_name}'"
        ))
    })?;

    let modified_sql = re.replace(original_sql, new_column_def.as_str());
    let final_sql = modified_sql.replace(original_table_name, new_table_name);

    Ok(final_sql)
}

#[cfg(feature = "schema")]
#[allow(clippy::too_many_lines)]
async fn exec_modify_column_workaround(
    conn: &turso::Connection,
    table_name: &str,
    column_name: &str,
    new_data_type: crate::schema::DataType,
    new_nullable: Option<bool>,
    new_default: Option<&crate::DatabaseValue>,
) -> Result<(), crate::DatabaseError> {
    let type_str = match new_data_type {
        crate::schema::DataType::VarChar(len) => format!("VARCHAR({len})"),
        crate::schema::DataType::Text
        | crate::schema::DataType::Date
        | crate::schema::DataType::Time
        | crate::schema::DataType::DateTime
        | crate::schema::DataType::Timestamp
        | crate::schema::DataType::Json
        | crate::schema::DataType::Jsonb
        | crate::schema::DataType::Uuid
        | crate::schema::DataType::Xml
        | crate::schema::DataType::Array(_)
        | crate::schema::DataType::Inet
        | crate::schema::DataType::MacAddr => "TEXT".to_string(),
        crate::schema::DataType::Char(len) => format!("CHAR({len})"),
        crate::schema::DataType::Bool
        | crate::schema::DataType::TinyInt
        | crate::schema::DataType::SmallInt
        | crate::schema::DataType::Int
        | crate::schema::DataType::BigInt
        | crate::schema::DataType::Serial
        | crate::schema::DataType::BigSerial => "INTEGER".to_string(),
        crate::schema::DataType::Real
        | crate::schema::DataType::Double
        | crate::schema::DataType::Decimal(_, _)
        | crate::schema::DataType::Money => "REAL".to_string(),
        crate::schema::DataType::Blob | crate::schema::DataType::Binary(_) => "BLOB".to_string(),
        crate::schema::DataType::Custom(type_name) => type_name,
    };

    let temp_column = format!(
        "{}_temp_{}",
        column_name,
        switchy_time::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );

    conn.execute("BEGIN TRANSACTION", ())
        .await
        .map_err(TursoDatabaseError::Turso)?;

    let nullable_str = match new_nullable {
        Some(true) | None => "",
        Some(false) => " NOT NULL",
    };

    let default_str = match new_default {
        Some(val) => {
            let val_str = match val {
                crate::DatabaseValue::String(s) => format!("'{s}'"),
                crate::DatabaseValue::Int64(n) => n.to_string(),
                crate::DatabaseValue::UInt64(n) => n.to_string(),
                crate::DatabaseValue::Bool(b) => if *b { "1" } else { "0" }.to_string(),
                crate::DatabaseValue::Real64(r) => r.to_string(),
                crate::DatabaseValue::Real32(r) => r.to_string(),
                crate::DatabaseValue::Null => "NULL".to_string(),
                crate::DatabaseValue::Now => "CURRENT_TIMESTAMP".to_string(),
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::Decimal(d) | crate::DatabaseValue::DecimalOpt(Some(d)) => {
                    format!("'{d}'")
                }
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::Uuid(u) | crate::DatabaseValue::UuidOpt(Some(u)) => {
                    format!("'{u}'")
                }
                _ => {
                    return Err(crate::DatabaseError::InvalidSchema(
                        "Unsupported default value type for MODIFY COLUMN".to_string(),
                    ));
                }
            };
            format!(" DEFAULT {val_str}")
        }
        None => String::new(),
    };

    let result = async {
        conn.execute(
            &format!(
                "ALTER TABLE {table_name} ADD COLUMN `{temp_column}` {type_str}{nullable_str}{default_str}"
            ),
            (),
        )
        .await
        .map_err(TursoDatabaseError::Turso)?;

        conn.execute(
            &format!(
                "UPDATE {table_name} SET `{temp_column}` = CAST(`{column_name}` AS {type_str})"
            ),
            (),
        )
        .await
        .map_err(TursoDatabaseError::Turso)?;

        conn.execute(
            &format!("ALTER TABLE {table_name} DROP COLUMN `{column_name}`"),
            (),
        )
        .await
        .map_err(TursoDatabaseError::Turso)?;

        conn.execute(
            &format!(
                "ALTER TABLE {table_name} ADD COLUMN `{column_name}` {type_str}{nullable_str}{default_str}"
            ),
            (),
        )
        .await
        .map_err(TursoDatabaseError::Turso)?;

        conn.execute(
            &format!("UPDATE {table_name} SET `{column_name}` = `{temp_column}`"),
            (),
        )
        .await
        .map_err(TursoDatabaseError::Turso)?;

        conn.execute(
            &format!("ALTER TABLE {table_name} DROP COLUMN `{temp_column}`"),
            (),
        )
        .await
        .map_err(TursoDatabaseError::Turso)?;

        Ok::<(), crate::DatabaseError>(())
    }
    .await;

    match result {
        Ok(()) => {
            conn.execute("COMMIT", ())
                .await
                .map_err(TursoDatabaseError::Turso)?;
            Ok(())
        }
        Err(e) => {
            let _ = conn.execute("ROLLBACK", ()).await;
            Err(e)
        }
    }
}

#[cfg(feature = "schema")]
#[allow(clippy::too_many_lines)]
async fn exec_table_recreation_workaround(
    conn: &turso::Connection,
    table_name: &str,
    column_name: &str,
    new_data_type: &crate::schema::DataType,
    new_nullable: Option<bool>,
    new_default: Option<&crate::DatabaseValue>,
) -> Result<(), crate::DatabaseError> {
    conn.execute("BEGIN TRANSACTION", ())
        .await
        .map_err(TursoDatabaseError::Turso)?;

    let result = async {
        let mut stmt = conn
            .prepare("PRAGMA foreign_keys")
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

        let column_info = stmt.columns();
        let column_names: Vec<String> = column_info.iter().map(|c| c.name().to_string()).collect();

        let mut rows = stmt
            .query(())
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

        let foreign_keys_enabled = if let Some(row) = rows
            .next()
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?
        {
            let turso_row = from_turso_row(&column_names, &row).map_err(crate::DatabaseError::Turso)?;
            column_names
                .iter()
                .find_map(|col| turso_row.get(col))
                .and_then(|v| match v {
                    DatabaseValue::Int32(i) => Some(i),
                    #[allow(clippy::cast_possible_truncation)]
                    DatabaseValue::Int64(i) => Some(i as i32),
                    _ => None,
                })
                .unwrap_or(0)
        } else {
            0
        };

        if foreign_keys_enabled == 1 {
            conn.execute("PRAGMA foreign_keys=OFF", ())
                .await
                .map_err(TursoDatabaseError::Turso)?;
        }

        let params = to_turso_params(&[DatabaseValue::String(table_name.to_string())])
            .map_err(crate::DatabaseError::Turso)?;

        let mut stmt = conn
            .prepare("SELECT sql FROM sqlite_master WHERE tbl_name=? AND type IN ('index','trigger','view') AND sql IS NOT NULL")
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

        let column_info = stmt.columns();
        let column_names: Vec<String> = column_info.iter().map(|c| c.name().to_string()).collect();

        let mut rows = stmt
            .query(params)
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

        let mut schema_objects = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?
        {
            let turso_row = from_turso_row(&column_names, &row).map_err(crate::DatabaseError::Turso)?;
            if let Some(DatabaseValue::String(sql)) = turso_row.get("sql") {
                schema_objects.push(sql.clone());
            }
        }

        let params = to_turso_params(&[DatabaseValue::String(table_name.to_string())])
            .map_err(crate::DatabaseError::Turso)?;

        let mut stmt = conn
            .prepare("SELECT sql FROM sqlite_master WHERE type='table' AND name=?")
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

        let column_info = stmt.columns();
        let column_names: Vec<String> = column_info.iter().map(|c| c.name().to_string()).collect();

        let mut rows = stmt
            .query(params)
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

        let row = rows
            .next()
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?
            .ok_or_else(|| {
                crate::DatabaseError::InvalidQuery(format!("Table '{table_name}' not found"))
            })?;

        let turso_row = from_turso_row(&column_names, &row).map_err(crate::DatabaseError::Turso)?;

        let original_sql: String = turso_row
            .get("sql")
            .and_then(|v| match v {
                DatabaseValue::String(s) => Some(s),
                _ => None,
            })
            .ok_or_else(|| {
                crate::DatabaseError::InvalidQuery(format!("Table '{table_name}' not found"))
            })?;

        let temp_table = format!(
            "{}_temp_{}",
            table_name,
            switchy_time::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );

        let new_table_sql = modify_create_table_sql(
            &original_sql,
            table_name,
            &temp_table,
            column_name,
            new_data_type,
            new_nullable,
            new_default,
        )?;

        conn.execute(&new_table_sql, ())
            .await
            .map_err(TursoDatabaseError::Turso)?;

        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table_name})"))
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

        let column_info = stmt.columns();
        let column_names: Vec<String> = column_info.iter().map(|c| c.name().to_string()).collect();

        let mut rows = stmt
            .query(())
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

        let mut columns = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?
        {
            let turso_row = from_turso_row(&column_names, &row).map_err(crate::DatabaseError::Turso)?;
            if let Some(DatabaseValue::String(name)) = turso_row.get("name") {
                columns.push(name.clone());
            }
        }

        let column_list = columns
            .iter()
            .map(|col| {
                if col == column_name {
                    let cast_type = match new_data_type {
                        crate::schema::DataType::Text
                        | crate::schema::DataType::VarChar(_)
                        | crate::schema::DataType::Char(_)
                        | crate::schema::DataType::Date
                        | crate::schema::DataType::Time
                        | crate::schema::DataType::DateTime
                        | crate::schema::DataType::Timestamp
                        | crate::schema::DataType::Json
                        | crate::schema::DataType::Jsonb
                        | crate::schema::DataType::Uuid
                        | crate::schema::DataType::Xml
                        | crate::schema::DataType::Array(_)
                        | crate::schema::DataType::Inet
                        | crate::schema::DataType::MacAddr
                        | crate::schema::DataType::Custom(_) => "TEXT",
                        crate::schema::DataType::Bool
                        | crate::schema::DataType::TinyInt
                        | crate::schema::DataType::SmallInt
                        | crate::schema::DataType::Int
                        | crate::schema::DataType::BigInt
                        | crate::schema::DataType::Serial
                        | crate::schema::DataType::BigSerial => "INTEGER",
                        crate::schema::DataType::Real
                        | crate::schema::DataType::Double
                        | crate::schema::DataType::Decimal(_, _)
                        | crate::schema::DataType::Money => "REAL",
                        crate::schema::DataType::Blob | crate::schema::DataType::Binary(_) => {
                            "BLOB"
                        }
                    };
                    format!("CAST(`{col}` AS {cast_type}) AS `{col}`")
                } else {
                    format!("`{col}`")
                }
            })
            .collect::<Vec<_>>()
            .join(", ");

        conn.execute(
            &format!("INSERT INTO {temp_table} SELECT {column_list} FROM {table_name}"),
            (),
        )
        .await
        .map_err(TursoDatabaseError::Turso)?;

        conn.execute(&format!("DROP TABLE {table_name}"), ())
            .await
            .map_err(TursoDatabaseError::Turso)?;

        conn.execute(
            &format!("ALTER TABLE {temp_table} RENAME TO {table_name}"),
            (),
        )
        .await
        .map_err(TursoDatabaseError::Turso)?;

        for schema_sql in schema_objects {
            if !schema_sql.to_uppercase().contains("AUTOINDEX") {
                conn.execute(&schema_sql, ())
                    .await
                    .map_err(TursoDatabaseError::Turso)?;
            }
        }

        if foreign_keys_enabled == 1 {
            conn.execute("PRAGMA foreign_keys=ON", ())
                .await
                .map_err(TursoDatabaseError::Turso)?;

            let mut stmt = conn
                .prepare("PRAGMA foreign_key_check")
                .await
                .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

            let column_info = stmt.columns();
            let column_names: Vec<String> = column_info.iter().map(|c| c.name().to_string()).collect();

            let mut rows = stmt
                .query(())
                .await
                .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

            let mut fk_violations = Vec::new();
            while let Some(row) = rows
                .next()
                .await
                .map_err(|e| crate::DatabaseError::Turso(e.into()))?
            {
                let turso_row = from_turso_row(&column_names, &row).map_err(crate::DatabaseError::Turso)?;
                fk_violations.push(turso_row);
            }

            if !fk_violations.is_empty() {
                return Err(crate::DatabaseError::ForeignKeyViolation(
                    "Foreign key violations detected after table recreation".to_string(),
                ));
            }
        }

        Ok::<(), crate::DatabaseError>(())
    }
    .await;

    match result {
        Ok(()) => {
            conn.execute("COMMIT", ())
                .await
                .map_err(TursoDatabaseError::Turso)?;
            Ok(())
        }
        Err(e) => {
            let _ = conn.execute("ROLLBACK", ()).await;
            Err(e)
        }
    }
}

#[cfg(feature = "schema")]
#[allow(clippy::too_many_lines)]
async fn exec_alter_table(
    conn: &turso::Connection,
    statement: &crate::schema::AlterTableStatement<'_>,
) -> Result<(), crate::DatabaseError> {
    use crate::schema::AlterOperation;

    for operation in &statement.operations {
        match operation {
            AlterOperation::AddColumn {
                name,
                data_type,
                nullable,
                default,
            } => {
                let type_str = match data_type {
                    crate::schema::DataType::VarChar(len) => format!("VARCHAR({len})"),
                    crate::schema::DataType::Text
                    | crate::schema::DataType::Date
                    | crate::schema::DataType::Time
                    | crate::schema::DataType::DateTime
                    | crate::schema::DataType::Timestamp
                    | crate::schema::DataType::Json
                    | crate::schema::DataType::Jsonb
                    | crate::schema::DataType::Uuid
                    | crate::schema::DataType::Xml
                    | crate::schema::DataType::Array(_)
                    | crate::schema::DataType::Inet
                    | crate::schema::DataType::MacAddr => "TEXT".to_string(),
                    crate::schema::DataType::Char(len) => format!("CHAR({len})"),
                    crate::schema::DataType::Bool
                    | crate::schema::DataType::TinyInt
                    | crate::schema::DataType::SmallInt
                    | crate::schema::DataType::Int
                    | crate::schema::DataType::BigInt
                    | crate::schema::DataType::Serial
                    | crate::schema::DataType::BigSerial => "INTEGER".to_string(),
                    crate::schema::DataType::Real
                    | crate::schema::DataType::Double
                    | crate::schema::DataType::Decimal(_, _)
                    | crate::schema::DataType::Money => "REAL".to_string(),
                    crate::schema::DataType::Blob | crate::schema::DataType::Binary(_) => {
                        "BLOB".to_string()
                    }
                    crate::schema::DataType::Custom(type_name) => type_name.clone(),
                };

                let nullable_str = if *nullable { "" } else { " NOT NULL" };
                let default_str = match default {
                    Some(val) => {
                        let val_str = match val {
                            crate::DatabaseValue::String(s) => format!("'{s}'"),
                            crate::DatabaseValue::Int64(n) => n.to_string(),
                            crate::DatabaseValue::UInt64(n) => n.to_string(),
                            crate::DatabaseValue::Bool(b) => if *b { "1" } else { "0" }.to_string(),
                            crate::DatabaseValue::Real64(r) => r.to_string(),
                            crate::DatabaseValue::Real32(r) => r.to_string(),
                            crate::DatabaseValue::Null => "NULL".to_string(),
                            crate::DatabaseValue::Now => "CURRENT_TIMESTAMP".to_string(),
                            #[cfg(feature = "decimal")]
                            crate::DatabaseValue::Decimal(d)
                            | crate::DatabaseValue::DecimalOpt(Some(d)) => {
                                format!("'{d}'")
                            }
                            #[cfg(feature = "uuid")]
                            crate::DatabaseValue::Uuid(u)
                            | crate::DatabaseValue::UuidOpt(Some(u)) => {
                                format!("'{u}'")
                            }
                            _ => {
                                return Err(crate::DatabaseError::InvalidSchema(
                                    "Unsupported default value type for ALTER TABLE ADD COLUMN"
                                        .to_string(),
                                ));
                            }
                        };
                        format!(" DEFAULT {val_str}")
                    }
                    None => String::new(),
                };

                let sql = format!(
                    "ALTER TABLE {} ADD COLUMN `{}` {}{}{}",
                    statement.table_name, name, type_str, nullable_str, default_str
                );

                conn.execute(&sql, ())
                    .await
                    .map_err(TursoDatabaseError::Turso)?;
            }
            AlterOperation::DropColumn {
                name,
                #[cfg(feature = "cascade")]
                behavior,
            } => {
                #[cfg(feature = "cascade")]
                {
                    use crate::schema::DropBehavior;

                    match behavior {
                        DropBehavior::Cascade | DropBehavior::Restrict => {
                            return Err(crate::DatabaseError::InvalidQuery(
                                "CASCADE/RESTRICT not yet implemented for Turso ALTER TABLE DROP COLUMN - see Phase 10"
                                    .to_string(),
                            ));
                        }
                        DropBehavior::Default => {}
                    }
                }

                let sql = format!(
                    "ALTER TABLE {} DROP COLUMN `{}`",
                    statement.table_name, name
                );

                conn.execute(&sql, ())
                    .await
                    .map_err(TursoDatabaseError::Turso)?;
            }
            AlterOperation::RenameColumn { old_name, new_name } => {
                let sql = format!(
                    "ALTER TABLE {} RENAME COLUMN `{}` TO `{}`",
                    statement.table_name, old_name, new_name
                );

                conn.execute(&sql, ())
                    .await
                    .map_err(TursoDatabaseError::Turso)?;
            }
            AlterOperation::ModifyColumn {
                name,
                new_data_type,
                new_nullable,
                new_default,
            } => {
                if column_requires_table_recreation(conn, statement.table_name, name).await? {
                    exec_table_recreation_workaround(
                        conn,
                        statement.table_name,
                        name,
                        new_data_type,
                        *new_nullable,
                        new_default.as_ref(),
                    )
                    .await?;
                } else {
                    exec_modify_column_workaround(
                        conn,
                        statement.table_name,
                        name,
                        new_data_type.clone(),
                        *new_nullable,
                        new_default.as_ref(),
                    )
                    .await?;
                }
            }
        }
    }

    Ok(())
}

#[cfg(feature = "schema")]
fn sqlite_type_to_data_type(sqlite_type: &str) -> crate::schema::DataType {
    let normalized_type = sqlite_type.to_uppercase();

    match normalized_type.as_str() {
        "INTEGER" => crate::schema::DataType::BigInt,
        "TEXT" => crate::schema::DataType::Text,
        "REAL" | "DOUBLE" | "FLOAT" => crate::schema::DataType::Double,
        "BLOB" => crate::schema::DataType::Blob,
        "BOOLEAN" | "BOOL" => crate::schema::DataType::Bool,
        "DATE" => crate::schema::DataType::Date,
        "DATETIME" => crate::schema::DataType::DateTime,
        "TIMESTAMP" => crate::schema::DataType::Timestamp,
        "JSON" => crate::schema::DataType::Json,
        _ => crate::schema::DataType::Custom(sqlite_type.to_string()),
    }
}

#[cfg(feature = "schema")]
fn parse_default_value(default_str: Option<&str>) -> Option<crate::DatabaseValue> {
    default_str.and_then(|s| {
        if s == "NULL" {
            Some(crate::DatabaseValue::Null)
        } else if s.starts_with('\'') && s.ends_with('\'') {
            let content = &s[1..s.len() - 1];
            Some(crate::DatabaseValue::String(content.to_string()))
        } else if let Ok(num) = s.parse::<i64>() {
            Some(crate::DatabaseValue::Int64(num))
        } else if let Ok(real) = s.parse::<f64>() {
            Some(crate::DatabaseValue::Real64(real))
        } else if s == "0" || s.to_uppercase() == "FALSE" {
            Some(crate::DatabaseValue::Bool(false))
        } else if s == "1" || s.to_uppercase() == "TRUE" {
            Some(crate::DatabaseValue::Bool(true))
        } else {
            None
        }
    })
}

#[cfg(feature = "schema")]
fn check_autoincrement_in_sql(create_sql: Option<&str>, column_name: &str) -> bool {
    let Some(sql) = create_sql else {
        return false;
    };

    let normalized_sql = sql.to_uppercase();
    let normalized_column = column_name.to_uppercase();

    if let Some(column_start) = normalized_sql.find(&normalized_column) {
        let column_portion = &normalized_sql[column_start..];

        if column_portion.contains("PRIMARY KEY")
            && let Some(pk_pos) = column_portion.find("PRIMARY KEY")
        {
            let after_pk = &column_portion[pk_pos + "PRIMARY KEY".len()..];

            let end_pos = after_pk
                .find(',')
                .unwrap_or_else(|| after_pk.find(')').unwrap_or(after_pk.len()));
            let column_rest = &after_pk[..end_pos];

            return column_rest.contains("AUTOINCREMENT");
        }
    }

    false
}

#[cfg(feature = "schema")]
async fn get_table_indexes(
    db: &TursoDatabase,
    table: &str,
) -> Result<std::collections::BTreeMap<String, crate::schema::IndexInfo>, crate::DatabaseError> {
    use crate::Database;
    use crate::schema::IndexInfo;
    use std::collections::BTreeMap;

    let index_query = "SELECT name, sql FROM sqlite_master WHERE type='index' AND tbl_name=?";
    let index_rows = db
        .query_raw_params(index_query, &[DatabaseValue::String(table.to_string())])
        .await?;

    let mut indexes = BTreeMap::new();

    for index_row in index_rows {
        let index_name = match index_row.get("name") {
            Some(DatabaseValue::String(s)) => s.clone(),
            _ => continue,
        };

        let Some(DatabaseValue::String(sql)) = index_row.get("sql") else {
            let is_primary = index_name.starts_with("sqlite_autoindex_");
            indexes.insert(
                index_name.clone(),
                IndexInfo {
                    name: index_name,
                    unique: false,
                    columns: Vec::new(),
                    is_primary,
                },
            );
            continue;
        };

        let is_unique = sql.to_uppercase().contains("UNIQUE");
        let is_primary = index_name.starts_with("sqlite_autoindex_");

        let index_columns = sql
            .find('(')
            .and_then(|start| {
                sql.rfind(')').map(|end| {
                    let cols_str = &sql[start + 1..end];
                    cols_str
                        .split(',')
                        .map(|s| s.trim().trim_matches('`').trim_matches('"').to_string())
                        .collect()
                })
            })
            .unwrap_or_default();

        indexes.insert(
            index_name.clone(),
            IndexInfo {
                name: index_name,
                unique: is_unique,
                columns: index_columns,
                is_primary,
            },
        );
    }

    Ok(indexes)
}

/// Retrieves foreign key constraints for a table by parsing CREATE TABLE SQL.
///
/// # Known Limitations
///
/// * **Composite foreign keys**: Multiple columns are captured as a single string.
///   Example: `FOREIGN KEY (a, b) REFERENCES t(x, y)` → column = "a, b"
///   This matches `PRAGMA foreign_key_list` behavior in rusqlite/sqlx.
///
/// * **MATCH clauses**: Not captured or validated.
///   Example: `REFERENCES t(id) MATCH SIMPLE` → `MATCH SIMPLE` ignored
///
/// * **DEFERRABLE clauses**: Not captured.
///   Example: `REFERENCES t(id) DEFERRABLE INITIALLY DEFERRED` → clause ignored
///
/// * **Validation**: Does not verify that referenced tables/columns exist.
///
/// These limitations are acceptable because:
/// 1. `SQLite`'s `PRAGMA foreign_key_list` also doesn't split composite keys
/// 2. `MATCH`/`DEFERRABLE` are rarely used in `SQLite` applications
/// 3. Validation happens at constraint enforcement time, not introspection
///
/// # Errors
///
/// Returns `DatabaseError` if query to `sqlite_master` fails.
#[cfg(feature = "schema")]
async fn get_table_foreign_keys(
    db: &TursoDatabase,
    table: &str,
) -> Result<std::collections::BTreeMap<String, crate::schema::ForeignKeyInfo>, crate::DatabaseError>
{
    use crate::Database;
    use crate::schema::ForeignKeyInfo;
    use std::collections::BTreeMap;

    let create_sql_query = "SELECT sql FROM sqlite_master WHERE type='table' AND name=?";
    let create_sql_rows = db
        .query_raw_params(
            create_sql_query,
            &[DatabaseValue::String(table.to_string())],
        )
        .await?;

    let create_sql = create_sql_rows
        .into_iter()
        .find_map(|row| match row.get("sql") {
            Some(DatabaseValue::String(s)) => Some(s),
            _ => None,
        });

    let mut foreign_keys = BTreeMap::new();

    if let Some(sql) = create_sql {
        for cap in FK_PATTERN.captures_iter(&sql) {
            let column = strip_identifier_quotes(&cap[1]);
            let referenced_table = strip_identifier_quotes(&cap[2]);
            let referenced_column = strip_identifier_quotes(&cap[3]);

            let fk_actions = &cap[4];

            let on_update = ON_UPDATE_PATTERN.captures(fk_actions).and_then(|c| {
                let action = c[1].to_uppercase();
                if action == "NO ACTION" {
                    None
                } else {
                    Some(action)
                }
            });

            let on_delete = ON_DELETE_PATTERN.captures(fk_actions).and_then(|c| {
                let action = c[1].to_uppercase();
                if action == "NO ACTION" {
                    None
                } else {
                    Some(action)
                }
            });

            let fk_name = format!("{table}_{column}_{referenced_table}_{referenced_column}");

            foreign_keys.insert(
                fk_name.clone(),
                ForeignKeyInfo {
                    name: fk_name,
                    column,
                    referenced_table,
                    referenced_column,
                    on_update,
                    on_delete,
                },
            );
        }
    }

    Ok(foreign_keys)
}

/// Strips quotes from `SQLite` identifiers and unescapes internal quotes.
///
/// Handles all 4 `SQLite` identifier quoting styles:
/// - Double quotes: `"name"` or `"my ""quoted"" name"` → `my "quoted" name`
/// - Backticks: `` `name` `` or `` `my ``tick`` name` `` → `my `tick` name`
/// - Single quotes: `'name'` or `'my ''quoted'' name'` → `my 'quoted' name`
/// - Square brackets: `[name]` → `name` (no escaping needed)
/// - Unquoted: `name` → `name` (returned as-is)
#[cfg(feature = "schema")]
pub(crate) fn strip_identifier_quotes(identifier: &str) -> String {
    let identifier = identifier.trim();

    if identifier.len() < 2 {
        return identifier.to_string();
    }

    if identifier.starts_with('"') && identifier.ends_with('"') {
        identifier[1..identifier.len() - 1].replace("\"\"", "\"")
    } else if identifier.starts_with('`') && identifier.ends_with('`') {
        identifier[1..identifier.len() - 1].replace("``", "`")
    } else if identifier.starts_with('[') && identifier.ends_with(']') {
        identifier[1..identifier.len() - 1].to_string()
    } else if identifier.starts_with('\'') && identifier.ends_with('\'') {
        identifier[1..identifier.len() - 1].replace("''", "'")
    } else {
        identifier.to_string()
    }
}

/// Find all tables that depend on the given table (recursively)
///
/// Uses inline foreign key parsing from `sqlite_master` instead of PRAGMA
/// (Turso doesn't support `PRAGMA foreign_key_list`).
///
/// Returns tables in drop order (dependents first, target last).
///
/// # Errors
///
/// * Returns `DatabaseError` if database queries fail or table name validation fails
#[cfg(all(feature = "schema", feature = "cascade"))]
async fn find_cascade_dependents(
    conn: &turso::Connection,
    table_name: &str,
) -> Result<Vec<String>, crate::DatabaseError> {
    let mut all_dependents = std::collections::BTreeSet::new();
    let mut to_check = vec![table_name.to_string()];
    let mut checked = std::collections::BTreeSet::new();

    while let Some(current_table) = to_check.pop() {
        if !checked.insert(current_table.clone()) {
            continue;
        }

        // Get all tables
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            )
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

        let mut rows = stmt
            .query(())
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

        let mut table_names = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?
        {
            let name_value = row
                .get_value(0)
                .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

            if let turso::Value::Text(name) = name_value {
                table_names.push(name);
            }
        }

        // Check each table for foreign keys referencing current_table
        for check_table in table_names {
            if check_table == current_table {
                continue;
            }

            // Validate table name for security
            crate::schema::dependencies::validate_table_name_for_pragma(&check_table)?;

            // Get CREATE TABLE SQL to parse foreign keys
            let sql = {
                let mut sql_stmt = conn
                    .prepare("SELECT sql FROM sqlite_master WHERE type='table' AND name=?")
                    .await
                    .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

                let mut sql_rows = sql_stmt
                    .query((turso::Value::Text(check_table.clone()),))
                    .await
                    .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

                if let Some(sql_row) = sql_rows
                    .next()
                    .await
                    .map_err(|e| crate::DatabaseError::Turso(e.into()))?
                {
                    let sql_value = sql_row
                        .get_value(0)
                        .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

                    match sql_value {
                        turso::Value::Text(s) => Some(s),
                        _ => None,
                    }
                } else {
                    None
                }
            };

            if let Some(create_sql) = sql {
                // Parse foreign keys from CREATE TABLE SQL using Phase 4 regex
                for cap in FK_PATTERN.captures_iter(&create_sql) {
                    let referenced_table = strip_identifier_quotes(&cap[2]);

                    if referenced_table == current_table {
                        all_dependents.insert(check_table.clone());
                        to_check.push(check_table.clone());
                        break;
                    }
                }
            }
        }
    }

    // Build proper drop order (dependents first, target last)
    let mut drop_order: Vec<String> = all_dependents.into_iter().collect();
    drop_order.push(table_name.to_string());

    Ok(drop_order)
}

/// Check if a table has any dependents (for RESTRICT)
///
/// Uses inline foreign key parsing from `sqlite_master` instead of PRAGMA
/// (Turso doesn't support `PRAGMA foreign_key_list`).
///
/// # Errors
///
/// * Returns `DatabaseError` if database queries fail or table name validation fails
#[cfg(all(feature = "schema", feature = "cascade"))]
async fn has_dependents(
    conn: &turso::Connection,
    table_name: &str,
) -> Result<bool, crate::DatabaseError> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")
        .await
        .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

    let mut rows = stmt
        .query(())
        .await
        .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

    let mut table_names = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|e| crate::DatabaseError::Turso(e.into()))?
    {
        let name_value = row
            .get_value(0)
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

        if let turso::Value::Text(name) = name_value {
            table_names.push(name);
        }
    }

    for check_table in table_names {
        if check_table == table_name {
            continue;
        }

        crate::schema::dependencies::validate_table_name_for_pragma(&check_table)?;

        // Get CREATE TABLE SQL to parse foreign keys
        let sql = {
            let mut sql_stmt = conn
                .prepare("SELECT sql FROM sqlite_master WHERE type='table' AND name=?")
                .await
                .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

            let mut sql_rows = sql_stmt
                .query((turso::Value::Text(check_table.clone()),))
                .await
                .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

            if let Some(sql_row) = sql_rows
                .next()
                .await
                .map_err(|e| crate::DatabaseError::Turso(e.into()))?
            {
                let sql_value = sql_row
                    .get_value(0)
                    .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

                match sql_value {
                    turso::Value::Text(s) => Some(s),
                    _ => None,
                }
            } else {
                None
            }
        };

        if let Some(create_sql) = sql {
            // Parse foreign keys from CREATE TABLE SQL using Phase 4 regex
            for cap in FK_PATTERN.captures_iter(&create_sql) {
                let referenced_table = strip_identifier_quotes(&cap[2]);

                if referenced_table == table_name {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}

/// Get current foreign key enforcement state
///
/// **Note**: Turso does not currently support `PRAGMA foreign_keys`, so this function
/// will return an error. Kept for future compatibility.
///
/// # Errors
///
/// * Returns `DatabaseError` if PRAGMA query fails (always fails on Turso v0.2.2)
#[cfg(all(feature = "schema", feature = "cascade"))]
#[allow(dead_code)]
async fn get_foreign_key_state(conn: &turso::Connection) -> Result<bool, crate::DatabaseError> {
    let mut stmt = conn
        .prepare("PRAGMA foreign_keys")
        .await
        .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

    let mut rows = stmt
        .query(())
        .await
        .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

    if let Some(row) = rows
        .next()
        .await
        .map_err(|e| crate::DatabaseError::Turso(e.into()))?
    {
        let enabled_value = row
            .get_value(0)
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

        match enabled_value {
            turso::Value::Integer(i) => Ok(i != 0),
            _ => Err(crate::DatabaseError::InvalidQuery(
                "Expected integer for PRAGMA foreign_keys result".to_string(),
            )),
        }
    } else {
        Ok(false)
    }
}

/// Set foreign key enforcement state
///
/// **Note**: Turso does not currently support `PRAGMA foreign_keys`, so this function
/// will return an error. Kept for future compatibility.
///
/// # Errors
///
/// * Returns `DatabaseError` if PRAGMA execute fails (always fails on Turso v0.2.2)
#[cfg(all(feature = "schema", feature = "cascade"))]
#[allow(dead_code)]
async fn set_foreign_key_state(
    conn: &turso::Connection,
    enabled: bool,
) -> Result<(), crate::DatabaseError> {
    let pragma = if enabled {
        "PRAGMA foreign_keys = ON"
    } else {
        "PRAGMA foreign_keys = OFF"
    };

    conn.execute(pragma, ())
        .await
        .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Database;

    async fn create_test_db() -> TursoDatabase {
        TursoDatabase::new(":memory:")
            .await
            .expect("Failed to create in-memory Turso database")
    }

    #[switchy_async::test]
    async fn test_database_creation_memory() {
        let db = TursoDatabase::new(":memory:").await;
        assert!(db.is_ok(), "Should create in-memory database");
    }

    #[switchy_async::test]
    async fn test_database_creation_file() {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join("test_turso.db");
        let db_path_str = db_path.to_string_lossy();

        let db = TursoDatabase::new(&db_path_str).await;
        assert!(db.is_ok(), "Should create file-based database");

        let _ = std::fs::remove_file(&db_path);
    }

    #[switchy_async::test]
    async fn test_exec_raw_create_table() {
        let db = create_test_db().await;
        let result = db
            .exec_raw("CREATE TABLE test_users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)")
            .await;
        assert!(result.is_ok(), "Should create table");
    }

    #[switchy_async::test]
    async fn test_exec_raw_params_insert() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_users (id INTEGER, name TEXT)")
            .await
            .expect("Failed to create table");

        let params = vec![
            DatabaseValue::Int64(1),
            DatabaseValue::String("Alice".to_string()),
        ];

        let result = db
            .exec_raw_params("INSERT INTO test_users (id, name) VALUES (?, ?)", &params)
            .await;

        assert!(result.is_ok(), "Should insert data");
        assert_eq!(result.unwrap(), 1, "Should affect 1 row");
    }

    #[switchy_async::test]
    async fn test_query_raw_basic() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_users (id INTEGER, name TEXT)")
            .await
            .expect("Failed to create table");

        db.exec_raw("INSERT INTO test_users (id, name) VALUES (1, 'Bob')")
            .await
            .expect("Failed to insert data");

        let rows = db
            .query_raw("SELECT id, name FROM test_users")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1, "Should return 1 row");

        let row = &rows[0];
        assert!(row.get("id").is_some(), "Should have 'id' column");
        assert!(row.get("name").is_some(), "Should have 'name' column");
        assert_eq!(row.get("id"), Some(DatabaseValue::Int64(1)));
        assert_eq!(
            row.get("name"),
            Some(DatabaseValue::String("Bob".to_string()))
        );
    }

    #[switchy_async::test]
    async fn test_query_raw_params() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_users (id INTEGER, name TEXT, active INTEGER)")
            .await
            .expect("Failed to create table");

        let insert_params = vec![
            DatabaseValue::Int64(42),
            DatabaseValue::String("Charlie".to_string()),
            DatabaseValue::Bool(true),
        ];

        db.exec_raw_params(
            "INSERT INTO test_users (id, name, active) VALUES (?, ?, ?)",
            &insert_params,
        )
        .await
        .expect("Failed to insert");

        let query_params = vec![DatabaseValue::Int64(42)];

        let rows = db
            .query_raw_params("SELECT * FROM test_users WHERE id = ?", &query_params)
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.get("id"), Some(DatabaseValue::Int64(42)));
        assert_eq!(
            row.get("name"),
            Some(DatabaseValue::String("Charlie".to_string()))
        );
        assert_eq!(row.get("active"), Some(DatabaseValue::Int64(1)));
    }

    #[switchy_async::test]
    async fn test_parameter_binding_all_types() {
        let db = create_test_db().await;

        db.exec_raw(
            "CREATE TABLE test_types (
                int8_val INTEGER,
                int16_val INTEGER,
                int32_val INTEGER,
                int64_val INTEGER,
                uint8_val INTEGER,
                uint16_val INTEGER,
                uint32_val INTEGER,
                real32_val REAL,
                real64_val REAL,
                text_val TEXT,
                bool_val INTEGER,
                null_val TEXT
            )",
        )
        .await
        .expect("Failed to create table");

        let params = vec![
            DatabaseValue::Int8(i8::MAX),
            DatabaseValue::Int16(i16::MAX),
            DatabaseValue::Int32(i32::MAX),
            DatabaseValue::Int64(i64::MAX),
            DatabaseValue::UInt8(u8::MAX),
            DatabaseValue::UInt16(u16::MAX),
            DatabaseValue::UInt32(u32::MAX),
            DatabaseValue::Real32(1.23_f32),
            DatabaseValue::Real64(4.567_890),
            DatabaseValue::String("test string".to_string()),
            DatabaseValue::Bool(true),
            DatabaseValue::Null,
        ];

        let result = db
            .exec_raw_params(
                "INSERT INTO test_types VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                &params,
            )
            .await;

        assert!(result.is_ok(), "Should insert all types");

        let rows = db
            .query_raw("SELECT * FROM test_types")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1);
        let row = &rows[0];

        assert_eq!(
            row.get("int8_val"),
            Some(DatabaseValue::Int64(i64::from(i8::MAX)))
        );
        assert_eq!(
            row.get("int16_val"),
            Some(DatabaseValue::Int64(i64::from(i16::MAX)))
        );
        assert_eq!(
            row.get("int32_val"),
            Some(DatabaseValue::Int64(i64::from(i32::MAX)))
        );
        assert_eq!(row.get("int64_val"), Some(DatabaseValue::Int64(i64::MAX)));
        assert_eq!(
            row.get("uint8_val"),
            Some(DatabaseValue::Int64(i64::from(u8::MAX)))
        );
        assert_eq!(
            row.get("uint16_val"),
            Some(DatabaseValue::Int64(i64::from(u16::MAX)))
        );
        assert_eq!(
            row.get("uint32_val"),
            Some(DatabaseValue::Int64(i64::from(u32::MAX)))
        );
        assert!(matches!(row.get("bool_val"), Some(DatabaseValue::Int64(1))));
        assert_eq!(row.get("null_val"), Some(DatabaseValue::Null));
    }

    #[switchy_async::test]
    async fn test_parameter_binding_optional_types() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_opts (a INTEGER, b TEXT, c REAL)")
            .await
            .expect("Failed to create table");

        let params = vec![
            DatabaseValue::Int64Opt(Some(100)),
            DatabaseValue::StringOpt(None),
            DatabaseValue::Real64Opt(Some(99.9)),
        ];

        db.exec_raw_params("INSERT INTO test_opts VALUES (?, ?, ?)", &params)
            .await
            .expect("Failed to insert");

        let rows = db
            .query_raw("SELECT * FROM test_opts")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.get("a"), Some(DatabaseValue::Int64(100)));
        assert_eq!(row.get("b"), Some(DatabaseValue::Null));
        assert_eq!(row.get("c"), Some(DatabaseValue::Real64(99.9)));
    }

    #[cfg(feature = "decimal")]
    #[switchy_async::test]
    async fn test_decimal_storage_and_retrieval() {
        use rust_decimal::Decimal;
        use std::str::FromStr;

        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_decimals (id INTEGER, price TEXT)")
            .await
            .expect("Failed to create table");

        let decimal_val = Decimal::from_str("123.456789").expect("Failed to parse decimal");
        let params = vec![DatabaseValue::Int64(1), DatabaseValue::Decimal(decimal_val)];

        db.exec_raw_params("INSERT INTO test_decimals VALUES (?, ?)", &params)
            .await
            .expect("Failed to insert");

        let rows = db
            .query_raw("SELECT * FROM test_decimals")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(
            row.get("price"),
            Some(DatabaseValue::String("123.456789".to_string()))
        );
    }

    #[cfg(feature = "uuid")]
    #[switchy_async::test]
    async fn test_uuid_storage_and_retrieval() {
        use uuid::Uuid;

        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_uuids (id INTEGER, user_id TEXT)")
            .await
            .expect("Failed to create table");

        let uuid_val = Uuid::new_v4();
        let params = vec![DatabaseValue::Int64(1), DatabaseValue::Uuid(uuid_val)];

        db.exec_raw_params("INSERT INTO test_uuids VALUES (?, ?)", &params)
            .await
            .expect("Failed to insert");

        let rows = db
            .query_raw("SELECT * FROM test_uuids")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(
            row.get("user_id"),
            Some(DatabaseValue::String(uuid_val.to_string()))
        );
    }

    #[switchy_async::test]
    async fn test_datetime_storage_and_retrieval() {
        use chrono::NaiveDateTime;

        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_dates (id INTEGER, created_at TEXT)")
            .await
            .expect("Failed to create table");

        let dt = NaiveDateTime::parse_from_str("2024-01-15 12:30:45", "%Y-%m-%d %H:%M:%S")
            .expect("Failed to parse datetime");
        let params = vec![DatabaseValue::Int64(1), DatabaseValue::DateTime(dt)];

        db.exec_raw_params("INSERT INTO test_dates VALUES (?, ?)", &params)
            .await
            .expect("Failed to insert");

        let rows = db
            .query_raw("SELECT * FROM test_dates")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(
            row.get("created_at"),
            Some(DatabaseValue::String(
                dt.format("%Y-%m-%d %H:%M:%S").to_string()
            ))
        );
    }

    #[switchy_async::test]
    async fn test_now_transformation() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_now (id INTEGER, created_at TEXT)")
            .await
            .expect("Failed to create table");

        let params = vec![DatabaseValue::Int64(1), DatabaseValue::Now];

        let result = db
            .exec_raw_params("INSERT INTO test_now VALUES (?, ?)", &params)
            .await;

        assert!(
            result.is_ok(),
            "Now should be transformed to datetime('now')"
        );

        let rows = db
            .query_raw("SELECT * FROM test_now")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert!(
            matches!(row.get("created_at"), Some(DatabaseValue::String(_))),
            "Should have timestamp"
        );
    }

    #[switchy_async::test]
    async fn test_now_plus_transformation() {
        use crate::sql_interval::SqlInterval;

        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_now_plus (id INTEGER, expires_at TEXT)")
            .await
            .expect("Failed to create table");

        let interval = SqlInterval {
            years: 0,
            months: 0,
            days: 7,
            hours: 2,
            minutes: 30,
            seconds: 0,
            nanos: 0,
        };

        let params = vec![DatabaseValue::Int64(1), DatabaseValue::NowPlus(interval)];

        let result = db
            .exec_raw_params("INSERT INTO test_now_plus VALUES (?, ?)", &params)
            .await;

        assert!(
            result.is_ok(),
            "NowPlus should be transformed to datetime with modifiers"
        );

        let rows = db
            .query_raw("SELECT * FROM test_now_plus")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert!(
            matches!(row.get("expires_at"), Some(DatabaseValue::String(_))),
            "Should have future timestamp"
        );
    }

    #[switchy_async::test]
    async fn test_error_handling_invalid_query() {
        let db = create_test_db().await;

        let result = db.query_raw("SELECT * FROM nonexistent_table").await;
        assert!(result.is_err(), "Should return error for invalid query");
    }

    #[switchy_async::test]
    async fn test_error_handling_type_mismatch() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_errors (id INTEGER)")
            .await
            .expect("Failed to create table");

        let params = vec![DatabaseValue::String("not a number".to_string())];

        let result = db
            .exec_raw_params("INSERT INTO test_errors VALUES (?)", &params)
            .await;

        assert!(
            result.is_ok(),
            "SQLite should handle TEXT -> INTEGER conversion"
        );
    }

    #[switchy_async::test]
    async fn test_multiple_rows() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_multi (id INTEGER, value TEXT)")
            .await
            .expect("Failed to create table");

        for i in 1..=10 {
            let params = vec![
                DatabaseValue::Int64(i),
                DatabaseValue::String(format!("value_{i}")),
            ];
            db.exec_raw_params("INSERT INTO test_multi VALUES (?, ?)", &params)
                .await
                .expect("Failed to insert");
        }

        let rows = db
            .query_raw("SELECT * FROM test_multi ORDER BY id")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 10, "Should return 10 rows");

        for (idx, row) in rows.iter().enumerate() {
            let expected_id = i64::try_from(idx + 1).expect("Failed to convert");
            assert_eq!(row.get("id"), Some(DatabaseValue::Int64(expected_id)));
            assert_eq!(
                row.get("value"),
                Some(DatabaseValue::String(format!("value_{expected_id}")))
            );
        }
    }

    #[switchy_async::test]
    async fn test_empty_result_set() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_empty (id INTEGER)")
            .await
            .expect("Failed to create table");

        let rows = db
            .query_raw("SELECT * FROM test_empty")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 0, "Should return empty result set");
    }

    #[switchy_async::test]
    async fn test_column_name_preservation() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_cols (first_name TEXT, last_name TEXT, age INTEGER)")
            .await
            .expect("Failed to create table");

        db.exec_raw("INSERT INTO test_cols VALUES ('John', 'Doe', 30)")
            .await
            .expect("Failed to insert");

        let rows = db
            .query_raw("SELECT first_name, last_name, age FROM test_cols")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1);
        let row = &rows[0];

        assert!(row.get("first_name").is_some(), "Should have first_name");
        assert!(row.get("last_name").is_some(), "Should have last_name");
        assert!(row.get("age").is_some(), "Should have age");

        assert!(
            row.get("FirstName").is_none(),
            "Column names are case-sensitive"
        );
    }

    #[switchy_async::test]
    async fn test_null_handling() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_nulls (id INTEGER, nullable_field TEXT)")
            .await
            .expect("Failed to create table");

        let params = vec![DatabaseValue::Int64(1), DatabaseValue::Null];

        db.exec_raw_params("INSERT INTO test_nulls VALUES (?, ?)", &params)
            .await
            .expect("Failed to insert");

        let rows = db
            .query_raw("SELECT * FROM test_nulls")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.get("nullable_field"), Some(DatabaseValue::Null));
    }

    #[switchy_async::test]
    async fn test_uint64_overflow_error() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_uint64 (id INTEGER, big_val INTEGER)")
            .await
            .expect("Failed to create table");

        let params = vec![DatabaseValue::Int64(1), DatabaseValue::UInt64(u64::MAX)];

        let result = db
            .exec_raw_params("INSERT INTO test_uint64 VALUES (?, ?)", &params)
            .await;

        assert!(
            result.is_err(),
            "u64::MAX should overflow i64 and cause error"
        );
    }

    #[switchy_async::test]
    async fn test_uint64_valid_range() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_uint64_valid (id INTEGER, val INTEGER)")
            .await
            .expect("Failed to create table");

        let params = vec![
            DatabaseValue::Int64(1),
            DatabaseValue::UInt64(i64::MAX as u64),
        ];

        let result = db
            .exec_raw_params("INSERT INTO test_uint64_valid VALUES (?, ?)", &params)
            .await;

        assert!(result.is_ok(), "u64 within i64::MAX range should work");
    }

    #[switchy_async::test]
    async fn test_transaction_commit() {
        use crate::Database;

        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_tx (id INTEGER, name TEXT)")
            .await
            .expect("Failed to create table");

        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        tx.exec_raw("INSERT INTO test_tx VALUES (1, 'Alice')")
            .await
            .expect("Failed to insert");

        Box::new(tx).commit().await.expect("Failed to commit");

        let rows = db
            .query_raw("SELECT * FROM test_tx")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1, "Should have 1 row after commit");
        assert_eq!(rows[0].get("id"), Some(DatabaseValue::Int64(1)));
        assert_eq!(
            rows[0].get("name"),
            Some(DatabaseValue::String("Alice".to_string()))
        );
    }

    #[switchy_async::test]
    async fn test_transaction_rollback() {
        use crate::Database;

        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_tx_rollback (id INTEGER, name TEXT)")
            .await
            .expect("Failed to create table");

        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        tx.exec_raw("INSERT INTO test_tx_rollback VALUES (1, 'Bob')")
            .await
            .expect("Failed to insert");

        Box::new(tx).rollback().await.expect("Failed to rollback");

        let rows = db
            .query_raw("SELECT * FROM test_tx_rollback")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 0, "Should have 0 rows after rollback");
    }

    #[switchy_async::test]
    async fn test_transaction_query() {
        use crate::Database;

        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_tx_query (id INTEGER, value TEXT)")
            .await
            .expect("Failed to create table");

        db.exec_raw("INSERT INTO test_tx_query VALUES (1, 'original')")
            .await
            .expect("Failed to insert initial data");

        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        tx.exec_raw("INSERT INTO test_tx_query VALUES (2, 'in_tx')")
            .await
            .expect("Failed to insert in transaction");

        let rows = tx
            .query_raw("SELECT * FROM test_tx_query ORDER BY id")
            .await
            .expect("Failed to query in transaction");

        assert_eq!(rows.len(), 2, "Should see both rows within transaction");

        Box::new(tx).commit().await.expect("Failed to commit");

        let rows_after = db
            .query_raw("SELECT * FROM test_tx_query ORDER BY id")
            .await
            .expect("Failed to query after commit");

        assert_eq!(rows_after.len(), 2, "Should have 2 rows after commit");
    }

    #[switchy_async::test]
    async fn test_transaction_params() {
        use crate::Database;

        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_tx_params (id INTEGER, name TEXT, active INTEGER)")
            .await
            .expect("Failed to create table");

        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        let params = vec![
            DatabaseValue::Int64(100),
            DatabaseValue::String("Carol".to_string()),
            DatabaseValue::Bool(true),
        ];

        let affected = tx
            .exec_raw_params("INSERT INTO test_tx_params VALUES (?, ?, ?)", &params)
            .await
            .expect("Failed to insert with params");

        assert_eq!(affected, 1, "Should affect 1 row");

        let query_params = vec![DatabaseValue::Int64(100)];
        let rows = tx
            .query_raw_params("SELECT * FROM test_tx_params WHERE id = ?", &query_params)
            .await
            .expect("Failed to query with params");

        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].get("name"),
            Some(DatabaseValue::String("Carol".to_string()))
        );

        Box::new(tx).commit().await.expect("Failed to commit");
    }

    #[switchy_async::test]
    async fn test_transaction_nested_error() {
        use crate::Database;

        let db = create_test_db().await;

        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        let nested_result = tx.begin_transaction().await;

        assert!(
            nested_result.is_err(),
            "Should not allow nested transactions"
        );
        assert!(
            matches!(
                nested_result,
                Err(crate::DatabaseError::AlreadyInTransaction)
            ),
            "Should return AlreadyInTransaction error"
        );

        Box::new(tx).rollback().await.expect("Failed to rollback");
    }

    #[switchy_async::test]
    async fn test_transaction_state_guards() {
        use crate::Database;

        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_state (id INTEGER)")
            .await
            .expect("Failed to create table");

        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        tx.exec_raw("INSERT INTO test_state VALUES (1)")
            .await
            .expect("Failed to insert");

        Box::new(tx).commit().await.expect("Commit should succeed");

        let rows = db
            .query_raw("SELECT * FROM test_state")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1, "Transaction was committed successfully");
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_table_exists() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_table (id INTEGER)")
            .await
            .expect("Failed to create table");

        assert!(
            db.table_exists("test_table")
                .await
                .expect("Failed to check table existence"),
            "test_table should exist"
        );

        assert!(
            !db.table_exists("nonexistent_table")
                .await
                .expect("Failed to check table existence"),
            "nonexistent_table should not exist"
        );
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_list_tables() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE users (id INTEGER, name TEXT)")
            .await
            .expect("Failed to create users table");

        db.exec_raw("CREATE TABLE posts (id INTEGER, title TEXT)")
            .await
            .expect("Failed to create posts table");

        let tables = db.list_tables().await.expect("Failed to list tables");

        assert!(tables.len() >= 2, "Should have at least 2 tables");
        assert!(tables.contains(&"users".to_string()));
        assert!(tables.contains(&"posts".to_string()));
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_get_table_columns() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_columns (id INTEGER PRIMARY KEY, name TEXT NOT NULL, age INTEGER, email TEXT DEFAULT 'none')")
            .await
            .expect("Failed to create table");

        let columns = db
            .get_table_columns("test_columns")
            .await
            .expect("Failed to get columns");

        assert_eq!(columns.len(), 4, "Should have 4 columns");

        let id_col = columns
            .iter()
            .find(|c| c.name == "id")
            .expect("Should have id column");
        assert!(id_col.is_primary_key, "id should be primary key");
        assert_eq!(id_col.ordinal_position, 1);

        let name_col = columns
            .iter()
            .find(|c| c.name == "name")
            .expect("Should have name column");
        assert!(!name_col.nullable, "name should not be nullable");
        assert_eq!(name_col.ordinal_position, 2);

        let age_col = columns
            .iter()
            .find(|c| c.name == "age")
            .expect("Should have age column");
        assert!(age_col.nullable, "age should be nullable");
        assert_eq!(age_col.ordinal_position, 3);

        let email_col = columns
            .iter()
            .find(|c| c.name == "email")
            .expect("Should have email column");
        assert!(
            email_col.default_value.is_some(),
            "email should have default value"
        );
        assert_eq!(email_col.ordinal_position, 4);
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_column_exists() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_columns (id INTEGER, name TEXT)")
            .await
            .expect("Failed to create table");

        assert!(
            db.column_exists("test_columns", "id")
                .await
                .expect("Failed to check column existence"),
            "id column should exist"
        );

        assert!(
            db.column_exists("test_columns", "name")
                .await
                .expect("Failed to check column existence"),
            "name column should exist"
        );

        assert!(
            !db.column_exists("test_columns", "nonexistent")
                .await
                .expect("Failed to check column existence"),
            "nonexistent column should not exist"
        );
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_get_table_info() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_info (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
            .await
            .expect("Failed to create table");

        let table_info = db
            .get_table_info("test_info")
            .await
            .expect("Failed to get table info")
            .expect("Table should exist");

        assert_eq!(table_info.name, "test_info");
        assert_eq!(table_info.columns.len(), 2);
        assert!(table_info.columns.contains_key("id"));
        assert!(table_info.columns.contains_key("name"));

        let nonexistent = db
            .get_table_info("nonexistent")
            .await
            .expect("Failed to get table info");

        assert!(
            nonexistent.is_none(),
            "Nonexistent table should return None"
        );
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_autoincrement_detection() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_autoincr (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT)")
            .await
            .expect("Failed to create table");

        let columns = db
            .get_table_columns("test_autoincr")
            .await
            .expect("Failed to get columns");

        let id_col = columns
            .iter()
            .find(|c| c.name == "id")
            .expect("Should have id column");

        assert!(id_col.is_primary_key, "id should be primary key");
        assert!(id_col.auto_increment, "id should be auto_increment");

        let name_col = columns
            .iter()
            .find(|c| c.name == "name")
            .expect("Should have name column");

        assert!(
            !name_col.auto_increment,
            "name should not be auto_increment"
        );
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_primary_key_without_autoincrement() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_pk_only (id INTEGER PRIMARY KEY, name TEXT)")
            .await
            .expect("Failed to create table");

        let columns = db
            .get_table_columns("test_pk_only")
            .await
            .expect("Failed to get columns");

        let id_col = columns
            .iter()
            .find(|c| c.name == "id")
            .expect("Should have id column");

        assert!(id_col.is_primary_key, "id should be primary key");
        assert!(!id_col.auto_increment, "id should NOT be auto_increment");
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_table_info_with_indexes() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT UNIQUE, name TEXT)")
            .await
            .expect("Failed to create table");

        db.exec_raw("CREATE INDEX idx_users_name ON users(name)")
            .await
            .expect("Failed to create index");

        let table_info = db
            .get_table_info("users")
            .await
            .expect("Failed to get table info")
            .expect("Table should exist");

        assert!(
            !table_info.indexes.is_empty(),
            "Should have at least 1 index"
        );

        let has_name_index = table_info
            .indexes
            .values()
            .any(|idx| idx.columns.contains(&"name".to_string()));
        assert!(has_name_index, "Should have index on name column");
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_table_info_with_foreign_keys() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE departments (id INTEGER PRIMARY KEY, name TEXT)")
            .await
            .expect("Failed to create departments table");

        db.exec_raw(
            "CREATE TABLE employees (
                id INTEGER PRIMARY KEY,
                name TEXT,
                dept_id INTEGER,
                FOREIGN KEY (dept_id) REFERENCES departments(id) ON DELETE CASCADE
            )",
        )
        .await
        .expect("Failed to create employees table");

        let table_info = db
            .get_table_info("employees")
            .await
            .expect("Failed to get table info")
            .expect("Table should exist");

        assert_eq!(
            table_info.foreign_keys.len(),
            1,
            "Should have 1 foreign key"
        );

        let fk = table_info
            .foreign_keys
            .values()
            .next()
            .expect("Should have FK");
        assert_eq!(fk.column, "dept_id");
        assert_eq!(fk.referenced_table, "departments");
        assert_eq!(fk.referenced_column, "id");
        assert_eq!(fk.on_delete, Some("CASCADE".to_string()));
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_table_info_complete() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE categories (id INTEGER PRIMARY KEY, name TEXT UNIQUE)")
            .await
            .expect("Failed to create categories");

        db.exec_raw(
            "CREATE TABLE products (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                price REAL,
                category_id INTEGER,
                FOREIGN KEY (category_id) REFERENCES categories(id) ON UPDATE CASCADE ON DELETE SET NULL
            )",
        )
        .await
        .expect("Failed to create products");

        db.exec_raw("CREATE INDEX idx_products_name ON products(name)")
            .await
            .expect("Failed to create index");

        let table_info = db
            .get_table_info("products")
            .await
            .expect("Failed to get table info")
            .expect("Table should exist");

        assert_eq!(table_info.columns.len(), 4);
        assert!(table_info.columns.contains_key("id"));
        assert!(table_info.columns.contains_key("name"));
        assert!(table_info.columns.contains_key("price"));
        assert!(table_info.columns.contains_key("category_id"));

        let id_col = &table_info.columns["id"];
        assert!(id_col.auto_increment, "id should have AUTOINCREMENT");

        assert!(
            !table_info.indexes.is_empty(),
            "Should have at least 1 index"
        );

        assert_eq!(table_info.foreign_keys.len(), 1);
        let fk = table_info.foreign_keys.values().next().unwrap();
        assert_eq!(fk.column, "category_id");
        assert_eq!(fk.referenced_table, "categories");
        assert_eq!(fk.on_update, Some("CASCADE".to_string()));
        assert_eq!(fk.on_delete, Some("SET NULL".to_string()));
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_fk_action_set_default() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE parent (id INTEGER PRIMARY KEY)")
            .await
            .expect("Failed to create parent table");

        db.exec_raw(
            "CREATE TABLE child (
                id INTEGER PRIMARY KEY,
                parent_id INTEGER DEFAULT 0,
                FOREIGN KEY (parent_id) REFERENCES parent(id) ON DELETE SET DEFAULT
            )",
        )
        .await
        .expect("Failed to create child table");

        let table_info = db
            .get_table_info("child")
            .await
            .expect("Failed to get table info")
            .expect("Table should exist");

        assert_eq!(table_info.foreign_keys.len(), 1);
        let fk = table_info.foreign_keys.values().next().unwrap();
        assert_eq!(fk.on_delete, Some("SET DEFAULT".to_string()));
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_fk_action_no_action_explicit() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE parent (id INTEGER PRIMARY KEY)")
            .await
            .expect("Failed to create parent table");

        db.exec_raw(
            "CREATE TABLE child (
                id INTEGER PRIMARY KEY,
                parent_id INTEGER,
                FOREIGN KEY (parent_id) REFERENCES parent(id) ON DELETE NO ACTION
            )",
        )
        .await
        .expect("Failed to create child table");

        let table_info = db
            .get_table_info("child")
            .await
            .expect("Failed to get table info")
            .expect("Table should exist");

        assert_eq!(table_info.foreign_keys.len(), 1);
        let fk = table_info.foreign_keys.values().next().unwrap();
        assert_eq!(fk.on_delete, None, "NO ACTION should map to None");
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_fk_action_default_when_omitted() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE parent (id INTEGER PRIMARY KEY)")
            .await
            .expect("Failed to create parent table");

        db.exec_raw(
            "CREATE TABLE child (
                id INTEGER PRIMARY KEY,
                parent_id INTEGER,
                FOREIGN KEY (parent_id) REFERENCES parent(id)
            )",
        )
        .await
        .expect("Failed to create child table");

        let table_info = db
            .get_table_info("child")
            .await
            .expect("Failed to get table info")
            .expect("Table should exist");

        assert_eq!(table_info.foreign_keys.len(), 1);
        let fk = table_info.foreign_keys.values().next().unwrap();
        assert_eq!(
            fk.on_update, None,
            "Omitted action should default to None (NO ACTION)"
        );
        assert_eq!(
            fk.on_delete, None,
            "Omitted action should default to None (NO ACTION)"
        );
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_fk_all_five_actions() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE parent (id INTEGER PRIMARY KEY)")
            .await
            .expect("Failed to create parent table");

        let test_cases = [
            ("NO ACTION", None),
            ("RESTRICT", Some("RESTRICT".to_string())),
            ("SET NULL", Some("SET NULL".to_string())),
            ("SET DEFAULT", Some("SET DEFAULT".to_string())),
            ("CASCADE", Some("CASCADE".to_string())),
        ];

        for (action, expected) in &test_cases {
            let table_name = format!("child_{}", action.to_lowercase().replace(' ', "_"));
            db.exec_raw(&format!(
                "CREATE TABLE {table_name} (
                    id INTEGER PRIMARY KEY,
                    parent_id INTEGER,
                    FOREIGN KEY (parent_id) REFERENCES parent(id) ON DELETE {action}
                )"
            ))
            .await
            .unwrap_or_else(|_| panic!("Failed to create {table_name}"));

            let table_info = db
                .get_table_info(&table_name)
                .await
                .expect("Failed to get table info")
                .expect("Table should exist");

            assert_eq!(table_info.foreign_keys.len(), 1);
            let fk = table_info.foreign_keys.values().next().unwrap();
            assert_eq!(
                fk.on_delete, *expected,
                "Action {action} not detected correctly"
            );
        }
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_fk_lowercase_syntax() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE parent (id INTEGER PRIMARY KEY)")
            .await
            .expect("Failed to create parent table");

        db.exec_raw(
            "CREATE TABLE child (
                id INTEGER PRIMARY KEY,
                parent_id INTEGER,
                foreign key (parent_id) references parent(id) on delete cascade
            )",
        )
        .await
        .expect("Failed to create child table with lowercase FK");

        let table_info = db
            .get_table_info("child")
            .await
            .expect("Failed to get table info")
            .expect("Table should exist");

        assert_eq!(
            table_info.foreign_keys.len(),
            1,
            "Should detect lowercase 'foreign key'"
        );
        let fk = table_info.foreign_keys.values().next().unwrap();
        assert_eq!(fk.column, "parent_id");
        assert_eq!(fk.referenced_table, "parent");
        assert_eq!(fk.referenced_column, "id");
        assert_eq!(
            fk.on_delete,
            Some("CASCADE".to_string()),
            "Should detect lowercase 'on delete cascade'"
        );
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_fk_mixed_case_actions() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE parent (id INTEGER PRIMARY KEY)")
            .await
            .expect("Failed to create parent table");

        db.exec_raw(
            "CREATE TABLE child (
                id INTEGER PRIMARY KEY,
                parent_id INTEGER,
                FOREIGN KEY (parent_id) REFERENCES parent(id) On UpDaTe CaScAdE On DeLeTe SeT nUlL
            )",
        )
        .await
        .expect("Failed to create child table with mixed case");

        let table_info = db
            .get_table_info("child")
            .await
            .expect("Failed to get table info")
            .expect("Table should exist");

        assert_eq!(table_info.foreign_keys.len(), 1);
        let fk = table_info.foreign_keys.values().next().unwrap();
        assert_eq!(
            fk.on_update,
            Some("CASCADE".to_string()),
            "Should detect mixed case 'On UpDaTe CaScAdE'"
        );
        assert_eq!(
            fk.on_delete,
            Some("SET NULL".to_string()),
            "Should detect mixed case 'On DeLeTe SeT nUlL'"
        );
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_fk_lowercase_references() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE parent (id INTEGER PRIMARY KEY)")
            .await
            .expect("Failed to create parent table");

        db.exec_raw(
            "CREATE TABLE child (
                id INTEGER PRIMARY KEY,
                parent_id INTEGER,
                foreign key (parent_id) references parent(id)
            )",
        )
        .await
        .expect("Failed to create child table");

        let table_info = db
            .get_table_info("child")
            .await
            .expect("Failed to get table info")
            .expect("Table should exist");

        assert_eq!(
            table_info.foreign_keys.len(),
            1,
            "Should detect lowercase 'references'"
        );
        let fk = table_info.foreign_keys.values().next().unwrap();
        assert_eq!(fk.referenced_table, "parent");
        assert_eq!(fk.referenced_column, "id");
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_fk_unicode_table_names() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE café (id INTEGER PRIMARY KEY)")
            .await
            .expect("Failed to create table with accented name");

        db.exec_raw(
            "CREATE TABLE entrée (
                id INTEGER PRIMARY KEY,
                café_id INTEGER,
                FOREIGN KEY (café_id) REFERENCES café(id) ON DELETE CASCADE
            )",
        )
        .await
        .expect("Failed to create child table with Unicode FK");

        let table_info = db
            .get_table_info("entrée")
            .await
            .expect("Failed to get table info")
            .expect("Table should exist");

        assert_eq!(
            table_info.foreign_keys.len(),
            1,
            "Should parse FK with Unicode table names"
        );
        let fk = table_info.foreign_keys.values().next().unwrap();
        assert_eq!(fk.column, "café_id");
        assert_eq!(fk.referenced_table, "café");
        assert_eq!(fk.referenced_column, "id");
        assert_eq!(fk.on_delete, Some("CASCADE".to_string()));
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_fk_cyrillic_identifiers() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE родитель (идентификатор INTEGER PRIMARY KEY)")
            .await
            .expect("Failed to create table with Cyrillic name");

        db.exec_raw(
            "CREATE TABLE ребёнок (
                идентификатор INTEGER PRIMARY KEY,
                родитель_ид INTEGER,
                FOREIGN KEY (родитель_ид) REFERENCES родитель(идентификатор) ON UPDATE RESTRICT
            )",
        )
        .await
        .expect("Failed to create child table with Cyrillic FK");

        let table_info = db
            .get_table_info("ребёнок")
            .await
            .expect("Failed to get table info")
            .expect("Table should exist");

        assert_eq!(
            table_info.foreign_keys.len(),
            1,
            "Should parse FK with Cyrillic identifiers"
        );
        let fk = table_info.foreign_keys.values().next().unwrap();
        assert_eq!(fk.column, "родитель_ид");
        assert_eq!(fk.referenced_table, "родитель");
        assert_eq!(fk.referenced_column, "идентификатор");
        assert_eq!(fk.on_update, Some("RESTRICT".to_string()));
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_fk_emoji_and_mixed_scripts() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE 部門 (id INTEGER PRIMARY KEY)")
            .await
            .expect("Failed to create table with CJK name");

        db.exec_raw(
            "CREATE TABLE 従業員 (
                id INTEGER PRIMARY KEY,
                部門_id INTEGER,
                FOREIGN KEY (部門_id) REFERENCES 部門(id) ON DELETE SET NULL ON UPDATE SET DEFAULT
            )",
        )
        .await
        .expect("Failed to create child table with CJK FK");

        let table_info = db
            .get_table_info("従業員")
            .await
            .expect("Failed to get table info")
            .expect("Table should exist");

        assert_eq!(
            table_info.foreign_keys.len(),
            1,
            "Should parse FK with CJK characters"
        );
        let fk = table_info.foreign_keys.values().next().unwrap();
        assert_eq!(fk.column, "部門_id");
        assert_eq!(fk.referenced_table, "部門");
        assert_eq!(fk.referenced_column, "id");
        assert_eq!(fk.on_delete, Some("SET NULL".to_string()));
        assert_eq!(fk.on_update, Some("SET DEFAULT".to_string()));
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_fk_multiple_different_actions() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE parent (id INTEGER PRIMARY KEY)")
            .await
            .expect("Failed to create parent table");

        db.exec_raw(
            "CREATE TABLE child (
                id INTEGER PRIMARY KEY,
                a_id INTEGER,
                b_id INTEGER,
                c_id INTEGER,
                FOREIGN KEY (a_id) REFERENCES parent(id) ON DELETE CASCADE,
                FOREIGN KEY (b_id) REFERENCES parent(id) ON UPDATE SET NULL,
                FOREIGN KEY (c_id) REFERENCES parent(id) ON DELETE RESTRICT ON UPDATE SET DEFAULT
            )",
        )
        .await
        .expect("Failed to create child table");

        let table_info = db
            .get_table_info("child")
            .await
            .expect("Failed to get table info")
            .expect("Table should exist");

        assert_eq!(
            table_info.foreign_keys.len(),
            3,
            "Should have 3 foreign keys"
        );

        let fk_a = table_info
            .foreign_keys
            .values()
            .find(|fk| fk.column == "a_id")
            .expect("Should have FK on a_id");
        assert_eq!(fk_a.on_delete, Some("CASCADE".to_string()));
        assert_eq!(fk_a.on_update, None, "a_id should not have ON UPDATE");

        let fk_b = table_info
            .foreign_keys
            .values()
            .find(|fk| fk.column == "b_id")
            .expect("Should have FK on b_id");
        assert_eq!(fk_b.on_delete, None, "b_id should not have ON DELETE");
        assert_eq!(fk_b.on_update, Some("SET NULL".to_string()));

        let fk_c = table_info
            .foreign_keys
            .values()
            .find(|fk| fk.column == "c_id")
            .expect("Should have FK on c_id");
        assert_eq!(fk_c.on_delete, Some("RESTRICT".to_string()));
        assert_eq!(fk_c.on_update, Some("SET DEFAULT".to_string()));
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_fk_quoted_table_name_with_spaces() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE \"my parent\" (id INTEGER PRIMARY KEY)")
            .await
            .expect("Failed to create table with spaces");

        db.exec_raw(
            "CREATE TABLE child (
                id INTEGER PRIMARY KEY,
                parent_id INTEGER,
                FOREIGN KEY (parent_id) REFERENCES \"my parent\"(id) ON DELETE CASCADE
            )",
        )
        .await
        .expect("Failed to create child table");

        let table_info = db
            .get_table_info("child")
            .await
            .expect("Failed to get table info")
            .expect("Table should exist");

        assert_eq!(
            table_info.foreign_keys.len(),
            1,
            "Should have 1 foreign key"
        );
        let fk = table_info.foreign_keys.values().next().unwrap();
        assert_eq!(
            fk.referenced_table, "my parent",
            "Quotes should be stripped from table name"
        );
        assert_eq!(fk.on_delete, Some("CASCADE".to_string()));
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_fk_escaped_double_quotes_in_table_name() {
        let db = create_test_db().await;

        db.exec_raw(r#"CREATE TABLE "my ""parent"" table" (id INTEGER PRIMARY KEY)"#)
            .await
            .expect("Failed to create table with escaped quotes");

        db.exec_raw(
            r#"CREATE TABLE child (
                id INTEGER PRIMARY KEY,
                pid INTEGER,
                FOREIGN KEY (pid) REFERENCES "my ""parent"" table"(id) ON DELETE CASCADE
            )"#,
        )
        .await
        .expect("Failed to create child table");

        let table_info = db
            .get_table_info("child")
            .await
            .expect("Failed to get table info")
            .expect("Table should exist");

        assert_eq!(table_info.foreign_keys.len(), 1);
        let fk = table_info.foreign_keys.values().next().unwrap();
        assert_eq!(
            fk.referenced_table, r#"my "parent" table"#,
            "Escaped quotes should be unescaped"
        );
        assert_eq!(fk.on_delete, Some("CASCADE".to_string()));
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_fk_escaped_backticks_in_table_name() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE `my ``parent`` table` (id INTEGER PRIMARY KEY)")
            .await
            .expect("Failed to create table with escaped backticks");

        db.exec_raw(
            "CREATE TABLE child (
                id INTEGER PRIMARY KEY,
                pid INTEGER,
                FOREIGN KEY (pid) REFERENCES `my ``parent`` table`(id) ON UPDATE RESTRICT
            )",
        )
        .await
        .expect("Failed to create child table");

        let table_info = db
            .get_table_info("child")
            .await
            .expect("Failed to get table info")
            .expect("Table should exist");

        assert_eq!(table_info.foreign_keys.len(), 1);
        let fk = table_info.foreign_keys.values().next().unwrap();
        assert_eq!(
            fk.referenced_table, "my `parent` table",
            "Escaped backticks should be unescaped"
        );
        assert_eq!(fk.on_update, Some("RESTRICT".to_string()));
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_fk_square_bracket_quoted_table_name() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE [my parent table] (id INTEGER PRIMARY KEY)")
            .await
            .expect("Failed to create table with square brackets");

        db.exec_raw(
            "CREATE TABLE child (
                id INTEGER PRIMARY KEY,
                pid INTEGER,
                FOREIGN KEY (pid) REFERENCES [my parent table](id) ON DELETE SET NULL
            )",
        )
        .await
        .expect("Failed to create child table");

        let table_info = db
            .get_table_info("child")
            .await
            .expect("Failed to get table info")
            .expect("Table should exist");

        assert_eq!(table_info.foreign_keys.len(), 1);
        let fk = table_info.foreign_keys.values().next().unwrap();
        assert_eq!(
            fk.referenced_table, "my parent table",
            "Square brackets should be stripped"
        );
        assert_eq!(fk.on_delete, Some("SET NULL".to_string()));
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_fk_single_quoted_table_name() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE 'my parent' (id INTEGER PRIMARY KEY)")
            .await
            .expect("Failed to create table with single quotes");

        db.exec_raw(
            "CREATE TABLE child (
                id INTEGER PRIMARY KEY,
                pid INTEGER,
                FOREIGN KEY (pid) REFERENCES 'my parent'(id) ON UPDATE CASCADE
            )",
        )
        .await
        .expect("Failed to create child table");

        let table_info = db
            .get_table_info("child")
            .await
            .expect("Failed to get table info")
            .expect("Table should exist");

        assert_eq!(table_info.foreign_keys.len(), 1);
        let fk = table_info.foreign_keys.values().next().unwrap();
        assert_eq!(
            fk.referenced_table, "my parent",
            "Single quotes should be stripped"
        );
        assert_eq!(fk.on_update, Some("CASCADE".to_string()));
    }

    #[cfg(all(feature = "schema", feature = "cascade"))]
    #[switchy_async::test]
    async fn test_cascade_find_dependents_simple() {
        use crate::Database;

        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE parent (id INTEGER PRIMARY KEY)")
            .await
            .expect("Failed to create parent table");

        db.exec_raw(
            "CREATE TABLE child (id INTEGER, parent_id INTEGER, FOREIGN KEY(parent_id) REFERENCES parent(id))",
        )
        .await
        .expect("Failed to create child table");

        let dependents = find_cascade_dependents(&db.connection, "parent")
            .await
            .expect("Failed to find dependents");

        assert_eq!(dependents.len(), 2, "Should find child and parent");
        assert_eq!(dependents[0], "child", "Child should be first");
        assert_eq!(dependents[1], "parent", "Parent should be last");
    }

    #[cfg(all(feature = "schema", feature = "cascade"))]
    #[switchy_async::test]
    async fn test_cascade_has_dependents_true() {
        use crate::Database;

        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE parent (id INTEGER PRIMARY KEY)")
            .await
            .expect("Failed to create parent table");

        db.exec_raw(
            "CREATE TABLE child (id INTEGER, parent_id INTEGER, FOREIGN KEY(parent_id) REFERENCES parent(id))",
        )
        .await
        .expect("Failed to create child table");

        let has_deps = has_dependents(&db.connection, "parent")
            .await
            .expect("Failed to check dependents");

        assert!(has_deps, "Parent should have dependents");
    }

    #[cfg(all(feature = "schema", feature = "cascade"))]
    #[switchy_async::test]
    async fn test_cascade_has_dependents_false() {
        use crate::Database;

        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE standalone (id INTEGER PRIMARY KEY)")
            .await
            .expect("Failed to create standalone table");

        let has_deps = has_dependents(&db.connection, "standalone")
            .await
            .expect("Failed to check dependents");

        assert!(!has_deps, "Standalone table should have no dependents");
    }

    #[cfg(all(feature = "schema", feature = "cascade"))]
    #[switchy_async::test]
    async fn test_cascade_nested_dependencies() {
        use crate::Database;

        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE grandparent (id INTEGER PRIMARY KEY)")
            .await
            .expect("Failed to create grandparent table");

        db.exec_raw(
            "CREATE TABLE parent (id INTEGER PRIMARY KEY, grandparent_id INTEGER, FOREIGN KEY(grandparent_id) REFERENCES grandparent(id))",
        )
        .await
        .expect("Failed to create parent table");

        db.exec_raw(
            "CREATE TABLE child (id INTEGER, parent_id INTEGER, FOREIGN KEY(parent_id) REFERENCES parent(id))",
        )
        .await
        .expect("Failed to create child table");

        let dependents = find_cascade_dependents(&db.connection, "grandparent")
            .await
            .expect("Failed to find dependents");

        assert_eq!(
            dependents.len(),
            3,
            "Should find child, parent, and grandparent"
        );
        assert!(
            dependents.contains(&"child".to_string()),
            "Should include child"
        );
        assert!(
            dependents.contains(&"parent".to_string()),
            "Should include parent"
        );
        assert_eq!(dependents[2], "grandparent", "Grandparent should be last");
    }

    #[switchy_async::test]
    #[should_panic = "Turso v0.2.2 does not support SAVEPOINT syntax yet. This feature will be available in future Turso versions. Consider using multiple transactions or upgrade to a newer Turso version when available."]
    async fn test_savepoint_not_supported() {
        use crate::Database;

        let db = create_test_db().await;

        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        // Turso v0.2.2 does not support SAVEPOINT
        let result = tx.savepoint("sp1").await;

        assert!(
            result.is_err(),
            "SAVEPOINT should return error in Turso v0.2.2"
        );

        match result {
            Err(crate::DatabaseError::InvalidQuery(msg)) => {
                assert!(
                    msg.contains("does not support SAVEPOINT"),
                    "Error should explain Turso limitation: {msg}"
                );
            }
            Ok(_) => panic!("Expected InvalidQuery error, but savepoint was created successfully"),
            Err(e) => panic!("Expected InvalidQuery error, got different error type: {e}"),
        }
    }

    #[switchy_async::test]
    async fn test_savepoint_invalid_name() {
        use crate::Database;

        let db = create_test_db().await;
        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        // Try to create savepoint with invalid name (SQL injection attempt)
        let result = tx.savepoint("sp1'; DROP TABLE users; --").await;
        assert!(result.is_err(), "Invalid savepoint name should error");

        // Try with special characters
        let result = tx.savepoint("sp-test").await;
        assert!(result.is_err(), "Savepoint name with dash should error");
    }
}
