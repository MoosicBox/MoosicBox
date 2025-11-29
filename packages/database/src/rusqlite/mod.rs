//! `SQLite` database backend using rusqlite
//!
//! This module provides `SQLite` database support using the `rusqlite` crate for synchronous
//! `SQLite` access wrapped in async interfaces. It implements schema introspection using
//! `SQLite`'s PRAGMA commands.
//!
//! # Schema Introspection Implementation
//!
//! `SQLite` introspection uses several PRAGMA commands to discover database structure:
//!
//! ## PRAGMA Commands Used
//!
//! - **`PRAGMA table_info(table_name)`**: Gets column information including:
//!   - Column ID (ordinal position, 0-based)
//!   - Column name
//!   - Data type string (e.g., "INTEGER", "TEXT", "REAL", "BOOLEAN")
//!   - NOT NULL constraint (boolean)
//!   - Default value (as string, may be NULL)
//!   - Primary key flag (boolean)
//!
//! - **`PRAGMA index_list(table_name)`**: Gets all indexes for a table including:
//!   - Index name
//!   - Unique constraint flag
//!   - Origin ('c' for CREATE INDEX, 'u' for UNIQUE, 'pk' for PRIMARY KEY)
//!
//! - **`PRAGMA index_info(index_name)`**: Gets columns in an index including:
//!   - Column ordinal position within index
//!   - Column name
//!
//! - **`PRAGMA foreign_key_list(table_name)`**: Gets foreign key constraints including:
//!   - Referenced table name
//!   - Local column name
//!   - Referenced column name
//!   - ON UPDATE action
//!   - ON DELETE action
//!
//! ## SQLite-Specific Limitations
//!
//! ### Data Type Mapping
//!
//! `SQLite` has dynamic typing, but CREATE TABLE statements use type affinity strings.
//! Our introspection maps common type names to [`DataType`](crate::schema::DataType):
//!
//! - `INTEGER` → `BigInt` (`SQLite` integers can be up to 8 bytes)
//! - `TEXT` → `Text`
//! - `REAL` → `Double` (`SQLite` uses double-precision floating point)
//! - `BOOLEAN` → `Bool`
//! - Other types → `UnsupportedDataType` error
//!
//! ### Auto-increment Detection
//!
//! `SQLite` auto-increment detection is **limited**. The `PRAGMA table_info()` does not
//! indicate AUTOINCREMENT columns directly. True auto-increment detection requires:
//! 1. Parsing the original CREATE TABLE statement
//! 2. Checking for `AUTOINCREMENT` keyword on INTEGER PRIMARY KEY columns
//!
//! Current implementation sets `auto_increment: false` for all columns.
//!
//! ### Primary Key Behavior
//!
//! **Important**: In `SQLite`, PRIMARY KEY does NOT imply NOT NULL (unlike other databases):
//! - `CREATE TABLE users (id INTEGER PRIMARY KEY)` - id can be NULL
//! - `CREATE TABLE users (id INTEGER PRIMARY KEY NOT NULL)` - id cannot be NULL
//!
//! This differs from PostgreSQL/MySQL where PRIMARY KEY implies NOT NULL.
//!
//! ### Default Value Parsing
//!
//! `SQLite` stores default values as strings in the schema. Our parser handles:
//! - `NULL` → None
//! - String literals: `'value'` → `DatabaseValue::String("value")`
//! - Numeric literals: `42` → `DatabaseValue::Int64(42)`
//! - Boolean literals: `1`/`TRUE`, `0`/`FALSE` → `DatabaseValue::Bool`
//! - Real literals: `3.14` → `DatabaseValue::Real64(3.14)`
//! - Complex expressions → None (not parsed)
//!
//! ### PRAGMA Considerations
//!
//! - **Case Sensitivity**: PRAGMA commands are case-sensitive
//! - **Attached Databases**: `table_exists()` searches all attached databases
//! - **Temporary Tables**: Temporary tables are included in results
//! - **Schema Modifications**: PRAGMA results reflect current schema, not historical
//!
//! # Connection Pool Architecture
//!
//! This implementation uses a connection pool to enable concurrent operations:
//! - 5 connections per database instance
//! - Round-robin connection selection
//! - Shared in-memory databases using `SQLite` URI syntax for tests
//! - Thread-safe access through `Arc<Mutex<Connection>>`
//!
//! # Transaction Support
//!
//! Transactions get dedicated connections from the pool to ensure isolation.
//! Each transaction uses SQL commands: `BEGIN`, `COMMIT`, `ROLLBACK`.

use std::{
    ops::Deref,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use async_trait::async_trait;
use rusqlite::{Connection, Row, Rows, Statement, types::Value};
use switchy_async::sync::Mutex;
use thiserror::Error;

use crate::{
    Database, DatabaseError, DatabaseTransaction, DatabaseValue, DeleteStatement, InsertStatement,
    SelectQuery, UpdateStatement, UpsertMultiStatement, UpsertStatement,
    query::{BooleanExpression, Expression, ExpressionType, Join, Sort, SortDirection},
    query_transform::{QuestionMarkHandler, transform_query_for_params},
    sql_interval::SqlInterval,
};

/// Format `SqlInterval` as `SQLite` datetime modifiers
fn format_sqlite_interval(interval: &SqlInterval) -> Vec<String> {
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

    // Handle seconds with subsecond precision
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

/// `SQLite` database connection pool using `rusqlite`
///
/// Manages a pool of `SQLite` connections using round-robin selection for distributing
/// queries across multiple connections. Each connection is protected by a mutex for
/// thread-safe access.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct RusqliteDatabase {
    connections: Vec<Arc<Mutex<Connection>>>,
    next_connection: AtomicUsize,
}

impl RusqliteDatabase {
    /// Creates a new `SQLite` database instance from a vector of connections
    ///
    /// The connections are used in round-robin fashion to distribute load.
    #[must_use]
    pub const fn new(connections: Vec<Arc<Mutex<Connection>>>) -> Self {
        Self {
            connections,
            next_connection: AtomicUsize::new(0),
        }
    }

    fn get_connection(&self) -> Arc<Mutex<Connection>> {
        let index = self.next_connection.fetch_add(1, Ordering::Relaxed) % self.connections.len();
        self.connections[index].clone()
    }
}

/// `SQLite` database transaction using `rusqlite`
///
/// Represents an active transaction on a `SQLite` connection. Provides ACID guarantees
/// for a series of database operations. Must be explicitly committed or rolled back.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct RusqliteTransaction {
    connection: Arc<Mutex<Connection>>,
    committed: AtomicBool,
    rolled_back: AtomicBool,
}

impl RusqliteTransaction {
    /// Creates a new `SQLite` transaction from a `rusqlite` connection
    #[must_use]
    pub const fn new(connection: Arc<Mutex<Connection>>) -> Self {
        Self {
            connection,
            committed: AtomicBool::new(false),
            rolled_back: AtomicBool::new(false),
        }
    }
}

trait ToSql {
    fn to_sql(&self) -> String;
}

impl<T: Expression + ?Sized> ToSql for T {
    #[allow(clippy::too_many_lines)]
    fn to_sql(&self) -> String {
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
                    joins.iter().map(Join::to_sql).collect::<Vec<_>>().join(" ")
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
                                .map(Sort::to_sql)
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

/// Errors specific to `SQLite` database operations using `rusqlite`
///
/// Wraps errors from the underlying `rusqlite` driver plus additional error types
/// for query validation and result handling.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Error)]
pub enum RusqliteDatabaseError {
    /// Error from the underlying `rusqlite` driver
    #[error(transparent)]
    Rusqlite(#[from] rusqlite::Error),
    /// Returned row did not contain an ID column
    #[error("No ID")]
    NoId,
    /// Query returned no rows when at least one was expected
    #[error("No row")]
    NoRow,
    /// The request was malformed or invalid
    #[error("Invalid request")]
    InvalidRequest,
    /// UPSERT operation missing required unique constraint specification
    #[error("Missing unique")]
    MissingUnique,
}

impl From<RusqliteDatabaseError> for DatabaseError {
    fn from(value: RusqliteDatabaseError) -> Self {
        Self::Rusqlite(value)
    }
}

/// Get column dependencies (indexes and foreign keys) for a specific column in `SQLite`
#[cfg(feature = "cascade")]
fn rusqlite_get_column_dependencies(
    connection: &Connection,
    table_name: &str,
    column_name: &str,
) -> Result<(Vec<String>, Vec<String>), DatabaseError> {
    let mut indexes = Vec::new();
    let mut foreign_keys = Vec::new();

    // Find indexes that use this column
    let index_list_query = format!("PRAGMA index_list({table_name})");
    let mut stmt = connection
        .prepare(&index_list_query)
        .map_err(RusqliteDatabaseError::Rusqlite)?;
    let index_rows = stmt
        .query_map([], |row| row.get::<_, String>("name"))
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    for index_result in index_rows {
        let index_name = index_result.map_err(RusqliteDatabaseError::Rusqlite)?;

        // Check if this index uses the column we're interested in
        let index_info_query = format!("PRAGMA index_info({index_name})");
        let mut col_stmt = connection
            .prepare(&index_info_query)
            .map_err(RusqliteDatabaseError::Rusqlite)?;
        let column_rows = col_stmt
            .query_map([], |row| row.get::<_, String>("name"))
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        for col_result in column_rows {
            let col_name = col_result.map_err(RusqliteDatabaseError::Rusqlite)?;
            if col_name == column_name {
                indexes.push(index_name.clone());
                break;
            }
        }
    }

    // Find foreign key constraints that use this column
    let fk_list_query = format!("PRAGMA foreign_key_list({table_name})");
    let mut fk_stmt = connection
        .prepare(&fk_list_query)
        .map_err(RusqliteDatabaseError::Rusqlite)?;
    let fk_rows = fk_stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>("id")?,
                row.get::<_, String>("from")?,
                row.get::<_, String>("table")?,
                row.get::<_, String>("to")?,
            ))
        })
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    for fk_result in fk_rows {
        let (id, from_column, to_table, to_column) =
            fk_result.map_err(RusqliteDatabaseError::Rusqlite)?;
        if from_column == column_name {
            foreign_keys.push(format!("FK_{id}_{table_name}_{to_table}_{to_column}"));
        }
    }

    Ok((indexes, foreign_keys))
}

#[async_trait]
impl Database for RusqliteDatabase {
    async fn query(&self, query: &SelectQuery<'_>) -> Result<Vec<crate::Row>, DatabaseError> {
        let connection = self.get_connection();
        Ok(select(
            &*connection.lock().await,
            query.table_name,
            query.distinct,
            query.columns,
            query.filters.as_deref(),
            query.joins.as_deref(),
            query.sorts.as_deref(),
            query.limit,
        )?)
    }

    async fn query_first(
        &self,
        query: &SelectQuery<'_>,
    ) -> Result<Option<crate::Row>, DatabaseError> {
        let connection = self.get_connection();
        Ok(find_row(
            &*connection.lock().await,
            query.table_name,
            query.distinct,
            query.columns,
            query.filters.as_deref(),
            query.joins.as_deref(),
            query.sorts.as_deref(),
        )?)
    }

    async fn exec_delete(
        &self,
        statement: &DeleteStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        let connection = self.get_connection();
        Ok(delete(
            &*connection.lock().await,
            statement.table_name,
            statement.filters.as_deref(),
            statement.limit,
        )?)
    }

    async fn exec_delete_first(
        &self,
        statement: &DeleteStatement<'_>,
    ) -> Result<Option<crate::Row>, DatabaseError> {
        let connection = self.get_connection();
        Ok(delete(
            &*connection.lock().await,
            statement.table_name,
            statement.filters.as_deref(),
            Some(1),
        )?
        .into_iter()
        .next())
    }

    async fn exec_insert(
        &self,
        statement: &InsertStatement<'_>,
    ) -> Result<crate::Row, DatabaseError> {
        let connection = self.get_connection();
        Ok(insert_and_get_row(
            &*connection.lock().await,
            statement.table_name,
            &statement.values,
        )?)
    }

    async fn exec_update(
        &self,
        statement: &UpdateStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        let connection = self.get_connection();
        Ok(update_and_get_rows(
            &*connection.lock().await,
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            statement.limit,
        )?)
    }

    async fn exec_update_first(
        &self,
        statement: &UpdateStatement<'_>,
    ) -> Result<Option<crate::Row>, DatabaseError> {
        let connection = self.get_connection();
        Ok(update_and_get_row(
            &*connection.lock().await,
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            statement.limit,
        )?)
    }

    async fn exec_upsert(
        &self,
        statement: &UpsertStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        let connection = self.get_connection();
        Ok(upsert(
            &*connection.lock().await,
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            statement.limit,
        )?)
    }

    async fn exec_upsert_first(
        &self,
        statement: &UpsertStatement<'_>,
    ) -> Result<crate::Row, DatabaseError> {
        let connection = self.get_connection();
        Ok(upsert_and_get_row(
            &*connection.lock().await,
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            statement.limit,
        )?)
    }

    async fn exec_upsert_multi(
        &self,
        statement: &UpsertMultiStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        let connection = self.get_connection();
        Ok(upsert_multi(
            &*connection.lock().await,
            statement.table_name,
            statement
                .unique
                .as_ref()
                .ok_or(RusqliteDatabaseError::MissingUnique)?,
            &statement.values,
        )?)
    }

    async fn exec_raw(&self, statement: &str) -> Result<(), DatabaseError> {
        let connection = self.get_connection();
        log::trace!("exec_raw: query:\n{statement}");

        connection
            .lock()
            .await
            .execute_batch(statement)
            .map_err(RusqliteDatabaseError::Rusqlite)?;
        Ok(())
    }

    #[cfg(feature = "schema")]
    async fn exec_create_table(
        &self,
        statement: &crate::schema::CreateTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let connection = self.get_connection();
        rusqlite_exec_create_table(&*connection.lock().await, statement)
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_table(
        &self,
        statement: &crate::schema::DropTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let connection = self.get_connection();
        rusqlite_exec_drop_table(&*connection.lock().await, statement)
    }

    #[cfg(feature = "schema")]
    async fn exec_create_index(
        &self,
        statement: &crate::schema::CreateIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let connection = self.get_connection();
        rusqlite_exec_create_index(&*connection.lock().await, statement)
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_index(
        &self,
        statement: &crate::schema::DropIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let connection = self.get_connection();
        rusqlite_exec_drop_index(&*connection.lock().await, statement)
    }

    #[cfg(feature = "schema")]
    async fn exec_alter_table(
        &self,
        statement: &crate::schema::AlterTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let connection = self.get_connection();
        rusqlite_exec_alter_table(&*connection.lock().await, statement)
    }

    #[cfg(feature = "schema")]
    async fn table_exists(&self, table_name: &str) -> Result<bool, DatabaseError> {
        let connection = self.get_connection();
        rusqlite_table_exists(&*connection.lock().await, table_name)
    }

    #[cfg(feature = "schema")]
    async fn list_tables(&self) -> Result<Vec<String>, DatabaseError> {
        let connection = self.get_connection();
        rusqlite_list_tables(&*connection.lock().await)
    }

    #[cfg(feature = "schema")]
    async fn get_table_info(
        &self,
        table_name: &str,
    ) -> Result<Option<crate::schema::TableInfo>, DatabaseError> {
        let connection = self.get_connection();
        rusqlite_get_table_info(&*connection.lock().await, table_name)
    }

    #[cfg(feature = "schema")]
    async fn get_table_columns(
        &self,
        table_name: &str,
    ) -> Result<Vec<crate::schema::ColumnInfo>, DatabaseError> {
        let connection = self.get_connection();
        rusqlite_get_table_columns(&*connection.lock().await, table_name)
    }

    #[cfg(feature = "schema")]
    async fn column_exists(
        &self,
        table_name: &str,
        column_name: &str,
    ) -> Result<bool, DatabaseError> {
        let connection = self.get_connection();
        rusqlite_column_exists(&*connection.lock().await, table_name, column_name)
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn query_raw(&self, query: &str) -> Result<Vec<crate::Row>, DatabaseError> {
        let connection = self.get_connection();
        let connection = connection.lock().await;

        let mut stmt = connection
            .prepare(query)
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        // Get column names from the statement
        let column_names: Vec<String> =
            stmt.column_names().iter().map(|&s| s.to_string()).collect();

        // Execute query and use existing to_rows helper
        let rows = stmt
            .query([])
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        // Use the existing to_rows function from rusqlite/mod.rs
        to_rows(&column_names, rows).map_err(|e| DatabaseError::QueryFailed(e.to_string()))
    }

    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn crate::DatabaseTransaction>, DatabaseError> {
        // Get dedicated connection from pool for transaction
        let connection = self.get_connection();

        // Execute BEGIN TRANSACTION on the dedicated connection
        connection
            .lock()
            .await
            .execute("BEGIN TRANSACTION", [])
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        // Create and return the transaction with dedicated connection
        Ok(Box::new(RusqliteTransaction::new(connection)))
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn exec_raw_params(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<u64, DatabaseError> {
        // Transform query to handle Now/NowPlus parameters
        let (transformed_query, filtered_params) =
            sqlite_transform_query_for_params(query, params)?;

        let connection = self.get_connection();
        let connection_guard = connection.lock().await;

        let mut stmt = connection_guard
            .prepare(&transformed_query)
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        // Convert only filtered params to RusqliteDatabaseValue
        let rusqlite_params: Vec<RusqliteDatabaseValue> =
            filtered_params.iter().map(|p| p.clone().into()).collect();

        log::trace!(
            "\
            exec_raw_params: query:\n\
            '{transformed_query}' (transformed from '{query}')\n\
            params: {params:?}\n\
            filtered: {filtered_params:?}\n\
            raw: {rusqlite_params:?}\
            "
        );

        // Use existing bind_values function to bind parameters
        bind_values(&mut stmt, Some(&rusqlite_params), false, 0)
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let rows_affected = stmt
            .raw_execute()
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        Ok(rows_affected as u64)
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn query_raw_params(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        // Transform query to handle Now/NowPlus parameters
        let (transformed_query, filtered_params) =
            sqlite_transform_query_for_params(query, params)?;

        let connection = self.get_connection();
        let connection_guard = connection.lock().await;

        let mut stmt = connection_guard
            .prepare(&transformed_query)
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        // Get column names
        let column_names: Vec<String> =
            stmt.column_names().iter().map(|&s| s.to_string()).collect();

        // Convert only filtered params using existing conversion
        let rusqlite_params: Vec<RusqliteDatabaseValue> =
            filtered_params.iter().map(|p| p.clone().into()).collect();

        log::trace!(
            "\
            query_raw_params: query:\n\
            '{transformed_query}' (transformed from '{query}')\n\
            params: {params:?}\n\
            filtered: {filtered_params:?}\n\
            raw: {rusqlite_params:?}\
            "
        );

        // Use existing bind_values function to bind parameters
        bind_values(&mut stmt, Some(&rusqlite_params), false, 0)
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        // Execute and use existing to_rows helper
        to_rows(&column_names, stmt.raw_query())
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))
    }
}

#[async_trait]
impl Database for RusqliteTransaction {
    async fn query(&self, query: &SelectQuery<'_>) -> Result<Vec<crate::Row>, DatabaseError> {
        Ok(select(
            &*self.connection.lock().await,
            query.table_name,
            query.distinct,
            query.columns,
            query.filters.as_deref(),
            query.joins.as_deref(),
            query.sorts.as_deref(),
            query.limit,
        )?)
    }

    async fn query_first(
        &self,
        query: &SelectQuery<'_>,
    ) -> Result<Option<crate::Row>, DatabaseError> {
        Ok(find_row(
            &*self.connection.lock().await,
            query.table_name,
            query.distinct,
            query.columns,
            query.filters.as_deref(),
            query.joins.as_deref(),
            query.sorts.as_deref(),
        )?)
    }

    async fn exec_delete(
        &self,
        statement: &DeleteStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        Ok(delete(
            &*self.connection.lock().await,
            statement.table_name,
            statement.filters.as_deref(),
            statement.limit,
        )?)
    }

    async fn exec_delete_first(
        &self,
        statement: &DeleteStatement<'_>,
    ) -> Result<Option<crate::Row>, DatabaseError> {
        Ok(delete(
            &*self.connection.lock().await,
            statement.table_name,
            statement.filters.as_deref(),
            Some(1),
        )?
        .into_iter()
        .next())
    }

    async fn exec_insert(
        &self,
        statement: &InsertStatement<'_>,
    ) -> Result<crate::Row, DatabaseError> {
        Ok(insert_and_get_row(
            &*self.connection.lock().await,
            statement.table_name,
            &statement.values,
        )?)
    }

    async fn exec_update(
        &self,
        statement: &UpdateStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        Ok(update_and_get_rows(
            &*self.connection.lock().await,
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            statement.limit,
        )?)
    }

    async fn exec_update_first(
        &self,
        statement: &UpdateStatement<'_>,
    ) -> Result<Option<crate::Row>, DatabaseError> {
        Ok(update_and_get_row(
            &*self.connection.lock().await,
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            statement.limit,
        )?)
    }

    async fn exec_upsert(
        &self,
        statement: &UpsertStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        Ok(upsert(
            &*self.connection.lock().await,
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            statement.limit,
        )?)
    }

    async fn exec_upsert_first(
        &self,
        statement: &UpsertStatement<'_>,
    ) -> Result<crate::Row, DatabaseError> {
        Ok(upsert(
            &*self.connection.lock().await,
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            statement.limit,
        )?
        .into_iter()
        .next()
        .ok_or(DatabaseError::NoRow)?)
    }

    async fn exec_upsert_multi(
        &self,
        statement: &UpsertMultiStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        let mut results = Vec::new();

        for values in &statement.values {
            results.extend(upsert(
                &*self.connection.lock().await,
                statement.table_name,
                values,
                None,
                None,
            )?);
        }

        Ok(results)
    }

    async fn exec_raw(&self, statement: &str) -> Result<(), DatabaseError> {
        self.connection
            .lock()
            .await
            .execute_batch(statement)
            .map_err(RusqliteDatabaseError::Rusqlite)?;
        Ok(())
    }

    #[cfg(feature = "schema")]
    async fn exec_create_table(
        &self,
        statement: &crate::schema::CreateTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        rusqlite_exec_create_table(&*self.connection.lock().await, statement)
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_table(
        &self,
        statement: &crate::schema::DropTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        rusqlite_exec_drop_table(&*self.connection.lock().await, statement)
    }

    #[cfg(feature = "schema")]
    async fn exec_create_index(
        &self,
        statement: &crate::schema::CreateIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        rusqlite_exec_create_index(&*self.connection.lock().await, statement)
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_index(
        &self,
        statement: &crate::schema::DropIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        rusqlite_exec_drop_index(&*self.connection.lock().await, statement)
    }

    #[cfg(feature = "schema")]
    async fn exec_alter_table(
        &self,
        statement: &crate::schema::AlterTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        rusqlite_exec_alter_table(&*self.connection.lock().await, statement)
    }

    #[cfg(feature = "schema")]
    async fn table_exists(&self, table_name: &str) -> Result<bool, DatabaseError> {
        rusqlite_table_exists(&*self.connection.lock().await, table_name)
    }

    #[cfg(feature = "schema")]
    async fn list_tables(&self) -> Result<Vec<String>, DatabaseError> {
        rusqlite_list_tables(&*self.connection.lock().await)
    }

    #[cfg(feature = "schema")]
    async fn get_table_info(
        &self,
        table_name: &str,
    ) -> Result<Option<crate::schema::TableInfo>, DatabaseError> {
        rusqlite_get_table_info(&*self.connection.lock().await, table_name)
    }

    #[cfg(feature = "schema")]
    async fn get_table_columns(
        &self,
        table_name: &str,
    ) -> Result<Vec<crate::schema::ColumnInfo>, DatabaseError> {
        rusqlite_get_table_columns(&*self.connection.lock().await, table_name)
    }

    #[cfg(feature = "schema")]
    async fn column_exists(
        &self,
        table_name: &str,
        column_name: &str,
    ) -> Result<bool, DatabaseError> {
        rusqlite_column_exists(&*self.connection.lock().await, table_name, column_name)
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn query_raw(&self, query: &str) -> Result<Vec<crate::Row>, DatabaseError> {
        let connection = self.connection.lock().await;

        let mut stmt = connection
            .prepare(query)
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        // Get column names from the statement
        let column_names: Vec<String> =
            stmt.column_names().iter().map(|&s| s.to_string()).collect();

        // Execute query and use existing to_rows helper
        let rows = stmt
            .query([])
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        // Use the existing to_rows function from rusqlite/mod.rs
        to_rows(&column_names, rows).map_err(|e| DatabaseError::QueryFailed(e.to_string()))
    }

    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn crate::DatabaseTransaction>, DatabaseError> {
        // Transactions cannot be nested
        Err(DatabaseError::AlreadyInTransaction)
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn exec_raw_params(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<u64, DatabaseError> {
        // Transform query to handle Now/NowPlus parameters
        let (transformed_query, filtered_params) =
            sqlite_transform_query_for_params(query, params)?;

        let connection_guard = self.connection.lock().await;

        let mut stmt = connection_guard
            .prepare(&transformed_query)
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        // Convert only filtered params to RusqliteDatabaseValue
        let rusqlite_params: Vec<RusqliteDatabaseValue> =
            filtered_params.iter().map(|p| p.clone().into()).collect();

        // Use existing bind_values function to bind parameters
        bind_values(&mut stmt, Some(&rusqlite_params), false, 0)
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let rows_affected = stmt
            .raw_execute()
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        Ok(rows_affected as u64)
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn query_raw_params(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        // Transform query to handle Now/NowPlus parameters
        let (transformed_query, filtered_params) =
            sqlite_transform_query_for_params(query, params)?;

        let connection_guard = self.connection.lock().await;

        let mut stmt = connection_guard
            .prepare(&transformed_query)
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        // Get column names
        let column_names: Vec<String> =
            stmt.column_names().iter().map(|&s| s.to_string()).collect();

        // Convert only filtered params using existing conversion
        let rusqlite_params: Vec<RusqliteDatabaseValue> =
            filtered_params.iter().map(|p| p.clone().into()).collect();

        // Use existing bind_values function to bind parameters
        bind_values(&mut stmt, Some(&rusqlite_params), false, 0)
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        // Execute and use existing to_rows helper
        to_rows(&column_names, stmt.raw_query())
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))
    }
}

struct RusqliteSavepoint {
    name: String,
    connection: Arc<Mutex<Connection>>,
    released: AtomicBool,
    rolled_back: AtomicBool,
}

#[async_trait]
impl crate::Savepoint for RusqliteSavepoint {
    async fn release(self: Box<Self>) -> Result<(), DatabaseError> {
        if self.released.swap(true, Ordering::SeqCst) {
            return Err(DatabaseError::InvalidSavepointName(format!(
                "Savepoint '{}' already released",
                self.name
            )));
        }

        if self.rolled_back.load(Ordering::SeqCst) {
            return Err(DatabaseError::InvalidSavepointName(format!(
                "Savepoint '{}' already rolled back",
                self.name
            )));
        }

        self.connection
            .lock()
            .await
            .execute(&format!("RELEASE SAVEPOINT {}", self.name), [])
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        Ok(())
    }

    async fn rollback_to(self: Box<Self>) -> Result<(), DatabaseError> {
        if self.rolled_back.swap(true, Ordering::SeqCst) {
            return Err(DatabaseError::InvalidSavepointName(format!(
                "Savepoint '{}' already rolled back",
                self.name
            )));
        }

        if self.released.load(Ordering::SeqCst) {
            return Err(DatabaseError::InvalidSavepointName(format!(
                "Savepoint '{}' already released",
                self.name
            )));
        }

        self.connection
            .lock()
            .await
            .execute(&format!("ROLLBACK TO SAVEPOINT {}", self.name), [])
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

#[async_trait]
impl DatabaseTransaction for RusqliteTransaction {
    async fn commit(self: Box<Self>) -> Result<(), DatabaseError> {
        if self.committed.load(Ordering::SeqCst) {
            return Err(DatabaseError::TransactionCommitted);
        }

        if self.rolled_back.load(Ordering::SeqCst) {
            return Err(DatabaseError::TransactionRolledBack);
        }

        self.connection
            .lock()
            .await
            .execute("COMMIT", [])
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        self.committed.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn rollback(self: Box<Self>) -> Result<(), DatabaseError> {
        if self.committed.load(Ordering::SeqCst) {
            return Err(DatabaseError::TransactionCommitted);
        }

        if self.rolled_back.load(Ordering::SeqCst) {
            return Err(DatabaseError::TransactionRolledBack);
        }

        self.connection
            .lock()
            .await
            .execute("ROLLBACK", [])
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        self.rolled_back.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn savepoint(&self, name: &str) -> Result<Box<dyn crate::Savepoint>, DatabaseError> {
        crate::validate_savepoint_name(name)?;

        // Execute SAVEPOINT SQL
        self.connection
            .lock()
            .await
            .execute(&format!("SAVEPOINT {name}"), [])
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        Ok(Box::new(RusqliteSavepoint {
            name: name.to_string(),
            connection: Arc::clone(&self.connection),
            released: AtomicBool::new(false),
            rolled_back: AtomicBool::new(false),
        }))
    }

    /// SQLite-optimized CASCADE target discovery using PRAGMA `foreign_key_list`
    #[cfg(feature = "cascade")]
    async fn find_cascade_targets(
        &self,
        table_name: &str,
    ) -> Result<crate::schema::DropPlan, DatabaseError> {
        let mut all_dependents = std::collections::BTreeSet::new();
        let mut to_check = vec![table_name.to_string()];
        let mut checked = std::collections::BTreeSet::new();

        while let Some(current_table) = to_check.pop() {
            if !checked.insert(current_table.clone()) {
                continue;
            }

            // Get all tables using query_raw
            let tables_query =
                "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'";
            let tables = self.query_raw(tables_query).await?;

            for table_row in tables {
                if let Some((_, crate::DatabaseValue::String(check_table))) =
                    table_row.columns.first()
                {
                    if check_table == &current_table {
                        continue;
                    }

                    // Validate table name for PRAGMA (cannot be parameterized)
                    crate::schema::dependencies::validate_table_name_for_pragma(check_table)?;
                    let fk_query = format!("PRAGMA foreign_key_list({check_table})");
                    let fk_rows = self.query_raw(&fk_query).await?;

                    for fk_row in fk_rows {
                        // Column 2 is the referenced table
                        // This assumes PRAGMA foreign_key_list column order:
                        // id, seq, table, from, to, on_update, on_delete, match
                        if let Some((_, crate::DatabaseValue::String(ref_table))) =
                            fk_row.columns.get(2)
                            && ref_table == &current_table
                        {
                            all_dependents.insert(check_table.clone());
                            to_check.push(check_table.clone());
                            break;
                        }
                    }
                }
            }
        }

        // Build proper drop order (dependents first)
        let mut drop_order: Vec<String> = all_dependents.into_iter().collect();
        drop_order.push(table_name.to_string());

        // Simplified cycle detection for Phase 15.1.4 - real implementation would track properly
        Ok(crate::schema::DropPlan::Simple(drop_order))
    }

    /// SQLite-optimized dependency check with early termination
    #[cfg(feature = "cascade")]
    async fn has_any_dependents(&self, table_name: &str) -> Result<bool, DatabaseError> {
        let tables_query =
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'";
        let tables = self.query_raw(tables_query).await?;

        for table_row in tables {
            if let Some((_, crate::DatabaseValue::String(check_table))) = table_row.columns.first()
            {
                if check_table == table_name {
                    continue;
                }

                crate::schema::dependencies::validate_table_name_for_pragma(check_table)?;
                let fk_query = format!("PRAGMA foreign_key_list({check_table})");
                let fk_rows = self.query_raw(&fk_query).await?;

                for fk_row in fk_rows {
                    if let Some((_, crate::DatabaseValue::String(ref_table))) =
                        fk_row.columns.get(2)
                        && ref_table == table_name
                    {
                        return Ok(true); // Found dependent, stop immediately
                    }
                }
            }
        }

        Ok(false)
    }

    /// Get direct dependents of a table (SQLite-optimized)
    #[cfg(feature = "cascade")]
    async fn get_direct_dependents(
        &self,
        table_name: &str,
    ) -> Result<std::collections::BTreeSet<String>, DatabaseError> {
        let mut dependents = std::collections::BTreeSet::new();
        let tables_query =
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'";
        let tables = self.query_raw(tables_query).await?;

        for table_row in tables {
            if let Some((_, crate::DatabaseValue::String(check_table))) = table_row.columns.first()
            {
                if check_table == table_name {
                    continue;
                }

                crate::schema::dependencies::validate_table_name_for_pragma(check_table)?;
                let fk_query = format!("PRAGMA foreign_key_list({check_table})");
                let fk_rows = self.query_raw(&fk_query).await?;

                for fk_row in fk_rows {
                    if let Some((_, crate::DatabaseValue::String(ref_table))) =
                        fk_row.columns.get(2)
                        && ref_table == table_name
                    {
                        dependents.insert(check_table.clone());
                        break;
                    }
                }
            }
        }

        Ok(dependents)
    }
}

impl From<Value> for DatabaseValue {
    fn from(value: Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::Integer(value) => Self::Int64(value),
            Value::Real(value) => Self::Real64(value),
            Value::Text(value) => Self::String(value),
            Value::Blob(_value) => unimplemented!("Blob types are not supported yet"),
        }
    }
}

fn from_row(column_names: &[String], row: &Row<'_>) -> Result<crate::Row, RusqliteDatabaseError> {
    let mut columns = vec![];

    for column in column_names {
        columns.push((column.clone(), row.get::<_, Value>(column.as_str())?.into()));
    }

    Ok(crate::Row { columns })
}

#[cfg(feature = "schema")]
#[allow(clippy::too_many_lines)]
fn rusqlite_exec_create_table(
    connection: &Connection,
    statement: &crate::schema::CreateTableStatement<'_>,
) -> Result<(), DatabaseError> {
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
            return Err(DatabaseError::InvalidSchema(format!(
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

        if let Some(default) = &column.default {
            query.push_str(" DEFAULT ");

            match default {
                DatabaseValue::Null
                | DatabaseValue::StringOpt(None)
                | DatabaseValue::BoolOpt(None)
                | DatabaseValue::Int8Opt(None)
                | DatabaseValue::Int16Opt(None)
                | DatabaseValue::Int32Opt(None)
                | DatabaseValue::Int64Opt(None)
                | DatabaseValue::UInt8Opt(None)
                | DatabaseValue::UInt16Opt(None)
                | DatabaseValue::UInt32Opt(None)
                | DatabaseValue::UInt64Opt(None)
                | DatabaseValue::Real64Opt(None)
                | DatabaseValue::Real32Opt(None) => {
                    query.push_str("NULL");
                }
                #[cfg(feature = "decimal")]
                DatabaseValue::DecimalOpt(None) => {
                    query.push_str("NULL");
                }
                #[cfg(feature = "uuid")]
                DatabaseValue::UuidOpt(None) => {
                    query.push_str("NULL");
                }
                DatabaseValue::StringOpt(Some(x)) | DatabaseValue::String(x) => {
                    query.push('\'');
                    query.push_str(x);
                    query.push('\'');
                }
                DatabaseValue::BoolOpt(Some(x)) | DatabaseValue::Bool(x) => {
                    query.push_str(if *x { "1" } else { "0" });
                }
                DatabaseValue::Int8Opt(Some(x)) | DatabaseValue::Int8(x) => {
                    query.push_str(&x.to_string());
                }
                DatabaseValue::Int16Opt(Some(x)) | DatabaseValue::Int16(x) => {
                    query.push_str(&x.to_string());
                }
                DatabaseValue::Int32Opt(Some(x)) | DatabaseValue::Int32(x) => {
                    query.push_str(&x.to_string());
                }
                DatabaseValue::Int64Opt(Some(x)) | DatabaseValue::Int64(x) => {
                    query.push_str(&x.to_string());
                }
                DatabaseValue::UInt8Opt(Some(x)) | DatabaseValue::UInt8(x) => {
                    query.push_str(&x.to_string());
                }
                DatabaseValue::UInt16Opt(Some(x)) | DatabaseValue::UInt16(x) => {
                    query.push_str(&x.to_string());
                }
                DatabaseValue::UInt32Opt(Some(x)) | DatabaseValue::UInt32(x) => {
                    query.push_str(&x.to_string());
                }
                DatabaseValue::UInt64Opt(Some(x)) | DatabaseValue::UInt64(x) => {
                    query.push_str(&x.to_string());
                }
                DatabaseValue::Real64Opt(Some(x)) | DatabaseValue::Real64(x) => {
                    query.push_str(&x.to_string());
                }
                DatabaseValue::Real32Opt(Some(x)) | DatabaseValue::Real32(x) => {
                    query.push_str(&x.to_string());
                }
                #[cfg(feature = "decimal")]
                DatabaseValue::DecimalOpt(Some(x)) | DatabaseValue::Decimal(x) => {
                    query.push('\'');
                    query.push_str(&x.to_string());
                    query.push('\'');
                }
                #[cfg(feature = "uuid")]
                DatabaseValue::Uuid(u) | DatabaseValue::UuidOpt(Some(u)) => {
                    query.push('\'');
                    query.push_str(&u.to_string());
                    query.push('\'');
                }
                DatabaseValue::NowPlus(interval) => {
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
                DatabaseValue::Now => {
                    query.push_str("(strftime('%Y-%m-%dT%H:%M:%f', 'now'))");
                }
                DatabaseValue::DateTime(x) => {
                    query.push('\'');
                    query.push_str(&x.and_utc().to_rfc3339());
                    query.push('\'');
                }
            }
        }
    }

    moosicbox_assert::assert!(!first);

    if let Some(primary_key) = &statement.primary_key {
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

    connection
        .execute(&query, [])
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    Ok(())
}

#[cfg(feature = "schema")]
fn rusqlite_exec_drop_table(
    connection: &Connection,
    statement: &crate::schema::DropTableStatement<'_>,
) -> Result<(), DatabaseError> {
    #[cfg(feature = "cascade")]
    {
        use crate::schema::DropBehavior;
        match statement.behavior {
            DropBehavior::Cascade => {
                return rusqlite_exec_drop_table_cascade(connection, statement);
            }
            DropBehavior::Restrict => {
                return rusqlite_exec_drop_table_restrict(connection, statement);
            }
            DropBehavior::Default => {} // Fall through to basic DROP TABLE
        }
    }

    // Basic DROP TABLE without CASCADE/RESTRICT
    let mut query = "DROP TABLE ".to_string();

    if statement.if_exists {
        query.push_str("IF EXISTS ");
    }

    query.push_str(statement.table_name);

    connection
        .execute(&query, [])
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    Ok(())
}

/// Implement manual CASCADE for `SQLite` using internal FK helpers
#[cfg(all(feature = "schema", feature = "cascade"))]
fn rusqlite_exec_drop_table_cascade(
    connection: &Connection,
    statement: &crate::schema::DropTableStatement<'_>,
) -> Result<(), DatabaseError> {
    // Get all tables that need to be dropped (dependents first, then target)
    let drop_order = rusqlite_find_cascade_dependents(connection, statement.table_name)?;

    // Enable foreign key enforcement temporarily for consistency
    let fk_enabled = rusqlite_get_foreign_key_state(connection)?;
    rusqlite_set_foreign_key_state(connection, true)?;

    let result = (|| -> Result<(), DatabaseError> {
        // Drop all dependent tables first, then the target table
        for table_to_drop in &drop_order {
            let mut query = "DROP TABLE ".to_string();
            if statement.if_exists {
                query.push_str("IF EXISTS ");
            }
            query.push_str(table_to_drop);

            connection
                .execute(&query, [])
                .map_err(RusqliteDatabaseError::Rusqlite)?;
        }
        Ok(())
    })();

    // Restore original foreign key state
    rusqlite_set_foreign_key_state(connection, fk_enabled)?;

    result
}

/// Implement manual RESTRICT for `SQLite` using internal FK helpers
#[cfg(all(feature = "schema", feature = "cascade"))]
fn rusqlite_exec_drop_table_restrict(
    connection: &Connection,
    statement: &crate::schema::DropTableStatement<'_>,
) -> Result<(), DatabaseError> {
    // Check if table has any dependents - if so, fail
    if rusqlite_has_dependents(connection, statement.table_name)? {
        return Err(DatabaseError::InvalidQuery(format!(
            "Cannot drop table '{}' because other tables depend on it",
            statement.table_name
        )));
    }

    // No dependents, proceed with normal drop
    let mut query = "DROP TABLE ".to_string();
    if statement.if_exists {
        query.push_str("IF EXISTS ");
    }
    query.push_str(statement.table_name);

    connection
        .execute(&query, [])
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    Ok(())
}

/// Find all tables that depend on the given table (for CASCADE)
#[cfg(all(feature = "schema", feature = "cascade"))]
fn rusqlite_find_cascade_dependents(
    connection: &Connection,
    table_name: &str,
) -> Result<Vec<String>, DatabaseError> {
    let mut all_dependents = std::collections::BTreeSet::new();
    let mut to_check = vec![table_name.to_string()];
    let mut checked = std::collections::BTreeSet::new();

    while let Some(current_table) = to_check.pop() {
        if !checked.insert(current_table.clone()) {
            continue;
        }

        // Get all tables using PRAGMA
        let mut stmt = connection
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            )
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        let table_names: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(RusqliteDatabaseError::Rusqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        for check_table in table_names {
            if check_table == current_table {
                continue;
            }

            // Validate table name for PRAGMA (cannot be parameterized)
            crate::schema::dependencies::validate_table_name_for_pragma(&check_table)?;

            let mut fk_stmt = connection
                .prepare(&format!("PRAGMA foreign_key_list({check_table})"))
                .map_err(RusqliteDatabaseError::Rusqlite)?;

            let fk_rows: Vec<String> = fk_stmt
                .query_map([], |row| row.get::<_, String>(2)) // Column 2 is referenced table
                .map_err(RusqliteDatabaseError::Rusqlite)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(RusqliteDatabaseError::Rusqlite)?;

            for ref_table in fk_rows {
                if ref_table == current_table {
                    all_dependents.insert(check_table.clone());
                    to_check.push(check_table.clone());
                    break;
                }
            }
        }
    }

    // Build proper drop order (dependents first)
    let mut drop_order: Vec<String> = all_dependents.into_iter().collect();
    drop_order.push(table_name.to_string());

    Ok(drop_order)
}

/// Check if a table has any dependents (for RESTRICT)
#[cfg(all(feature = "schema", feature = "cascade"))]
fn rusqlite_has_dependents(
    connection: &Connection,
    table_name: &str,
) -> Result<bool, DatabaseError> {
    let mut stmt = connection
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    let table_names: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(RusqliteDatabaseError::Rusqlite)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    for check_table in table_names {
        if check_table == table_name {
            continue;
        }

        crate::schema::dependencies::validate_table_name_for_pragma(&check_table)?;

        let mut fk_stmt = connection
            .prepare(&format!("PRAGMA foreign_key_list({check_table})"))
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        let has_dep: bool = fk_stmt
            .query_map([], |row| row.get::<_, String>(2)) // Column 2 is referenced table
            .map_err(RusqliteDatabaseError::Rusqlite)?
            .any(|ref_table_result| {
                ref_table_result.is_ok_and(|ref_table| ref_table == table_name)
            });

        if has_dep {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Get current foreign key enforcement state
#[cfg(all(feature = "schema", feature = "cascade"))]
fn rusqlite_get_foreign_key_state(connection: &Connection) -> Result<bool, DatabaseError> {
    let mut stmt = connection
        .prepare("PRAGMA foreign_keys")
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    let enabled: i64 = stmt
        .query_row([], |row| row.get(0))
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    Ok(enabled != 0)
}

/// Set foreign key enforcement state
#[cfg(all(feature = "schema", feature = "cascade"))]
fn rusqlite_set_foreign_key_state(
    connection: &Connection,
    enabled: bool,
) -> Result<(), DatabaseError> {
    let pragma = if enabled {
        "PRAGMA foreign_keys = ON"
    } else {
        "PRAGMA foreign_keys = OFF"
    };

    connection
        .execute(pragma, [])
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    Ok(())
}

#[cfg(feature = "schema")]
pub(crate) fn rusqlite_exec_create_index(
    connection: &Connection,
    statement: &crate::schema::CreateIndexStatement<'_>,
) -> Result<(), DatabaseError> {
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

    connection
        .execute(&sql, [])
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    Ok(())
}

#[cfg(feature = "schema")]
pub(crate) fn rusqlite_exec_drop_index(
    connection: &Connection,
    statement: &crate::schema::DropIndexStatement<'_>,
) -> Result<(), DatabaseError> {
    let if_exists_str = if statement.if_exists {
        "IF EXISTS "
    } else {
        ""
    };

    let sql = format!("DROP INDEX {}{}", if_exists_str, statement.index_name);

    connection
        .execute(&sql, [])
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    Ok(())
}

#[cfg(feature = "schema")]
#[allow(clippy::too_many_lines)]
pub(crate) fn rusqlite_exec_alter_table(
    connection: &Connection,
    statement: &crate::schema::AlterTableStatement<'_>,
) -> Result<(), DatabaseError> {
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
                                return Err(DatabaseError::InvalidSchema(
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

                connection
                    .execute(&sql, [])
                    .map_err(RusqliteDatabaseError::Rusqlite)?;
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
                        DropBehavior::Cascade => {
                            // Get column dependencies before dropping
                            let (indexes, foreign_keys) = rusqlite_get_column_dependencies(
                                connection,
                                statement.table_name,
                                name,
                            )?;

                            // Drop indexes (SQLite can drop indexes individually)
                            for index_name in indexes {
                                let drop_index_sql = format!("DROP INDEX IF EXISTS `{index_name}`");
                                log::trace!("SQLite CASCADE dropping index: {drop_index_sql}");
                                connection
                                    .execute(&drop_index_sql, [])
                                    .map_err(RusqliteDatabaseError::Rusqlite)?;
                            }

                            // Log warning about FK limitations in SQLite
                            if !foreign_keys.is_empty() {
                                log::warn!(
                                    "SQLite CASCADE: Cannot drop individual foreign key constraints. \
                                          Column '{}.{}' has {} FK constraint(s) that cannot be automatically dropped",
                                    statement.table_name,
                                    name,
                                    foreign_keys.len()
                                );
                            }
                        }
                        DropBehavior::Restrict => {
                            // Check for dependencies and fail if any exist
                            let (indexes, foreign_keys) = rusqlite_get_column_dependencies(
                                connection,
                                statement.table_name,
                                name,
                            )?;

                            if !indexes.is_empty() || !foreign_keys.is_empty() {
                                return Err(DatabaseError::ForeignKeyViolation(format!(
                                    "Cannot drop column {}.{}: has {} index(es) and {} foreign key(s)",
                                    statement.table_name,
                                    name,
                                    indexes.len(),
                                    foreign_keys.len()
                                )));
                            }
                        }
                        DropBehavior::Default => {
                            // SQLite default behavior (fail on any constraint violations)
                        }
                    }
                }

                let sql = format!(
                    "ALTER TABLE {} DROP COLUMN `{}`",
                    statement.table_name, name
                );

                connection
                    .execute(&sql, [])
                    .map_err(RusqliteDatabaseError::Rusqlite)?;
            }
            AlterOperation::RenameColumn { old_name, new_name } => {
                let sql = format!(
                    "ALTER TABLE {} RENAME COLUMN `{}` TO `{}`",
                    statement.table_name, old_name, new_name
                );

                connection
                    .execute(&sql, [])
                    .map_err(RusqliteDatabaseError::Rusqlite)?;
            }
            AlterOperation::ModifyColumn {
                name,
                new_data_type,
                new_nullable,
                new_default,
            } => {
                // Use decision tree to determine correct workaround approach
                if column_requires_table_recreation(connection, statement.table_name, name)
                    .map_err(DatabaseError::Rusqlite)?
                {
                    // Use table recreation for complex columns (PRIMARY KEY, UNIQUE, CHECK, GENERATED)
                    rusqlite_exec_table_recreation_workaround(
                        connection,
                        statement.table_name,
                        name,
                        new_data_type,
                        *new_nullable,
                        new_default.as_ref(),
                    )?;
                } else {
                    // Use column-based workaround for simple columns
                    rusqlite_exec_modify_column_workaround(
                        connection,
                        statement.table_name,
                        name,
                        new_data_type.clone(),
                        *new_nullable,
                        new_default.as_ref(),
                    )?;
                }
            }
        }
    }

    Ok(())
}

#[cfg(feature = "schema")]
#[allow(clippy::too_many_lines)]
fn rusqlite_exec_modify_column_workaround(
    connection: &Connection,
    table_name: &str,
    column_name: &str,
    new_data_type: crate::schema::DataType,
    new_nullable: Option<bool>,
    new_default: Option<&crate::DatabaseValue>,
) -> Result<(), DatabaseError> {
    // Implementation of the column-based workaround for MODIFY COLUMN
    // This is a simplified version - the full implementation would check for constraints

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
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );

    // Execute the column-based workaround in a transaction
    connection
        .execute("BEGIN TRANSACTION", [])
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    // Step 1: Add temporary column with new type
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
                    return Err(DatabaseError::InvalidSchema(
                        "Unsupported default value type for MODIFY COLUMN".to_string(),
                    ));
                }
            };
            format!(" DEFAULT {val_str}")
        }
        None => String::new(),
    };

    let result = (|| -> Result<(), RusqliteDatabaseError> {
        connection
            .execute(
                &format!(
                    "ALTER TABLE {table_name} ADD COLUMN `{temp_column}` {type_str}{nullable_str}{default_str}"
                ),
                [],
            )
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        // Step 2: Copy and convert data
        connection
            .execute(
                &format!(
                    "UPDATE {table_name} SET `{temp_column}` = CAST(`{column_name}` AS {type_str})"
                ),
                [],
            )
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        // Step 3: Drop original column
        connection
            .execute(
                &format!("ALTER TABLE {table_name} DROP COLUMN `{column_name}`"),
                [],
            )
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        // Step 4: Add column with original name and new type
        connection
            .execute(
                &format!(
                    "ALTER TABLE {table_name} ADD COLUMN `{column_name}` {type_str}{nullable_str}{default_str}"
                ),
                [],
            )
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        // Step 5: Copy data from temp to final column
        connection
            .execute(
                &format!("UPDATE {table_name} SET `{column_name}` = `{temp_column}`"),
                [],
            )
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        // Step 6: Drop temporary column
        connection
            .execute(
                &format!("ALTER TABLE {table_name} DROP COLUMN `{temp_column}`"),
                [],
            )
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        Ok(())
    })();

    match result {
        Ok(()) => {
            connection
                .execute("COMMIT", [])
                .map_err(RusqliteDatabaseError::Rusqlite)?;
        }
        Err(e) => {
            let _ = connection.execute("ROLLBACK", []);
            return Err(DatabaseError::Rusqlite(e));
        }
    }

    Ok(())
}

#[cfg(feature = "schema")]
fn column_requires_table_recreation(
    connection: &Connection,
    table_name: &str,
    column_name: &str,
) -> Result<bool, RusqliteDatabaseError> {
    // Check if column is PRIMARY KEY
    let mut stmt = connection
        .prepare("SELECT sql FROM sqlite_master WHERE type='table' AND name=?")
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    let table_sql: String = stmt
        .query_row([table_name], |row| row.get(0))
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    // Parse CREATE TABLE SQL to check for constraints
    let table_sql_upper = table_sql.to_uppercase();
    let column_name_upper = column_name.to_uppercase();

    // Check for PRIMARY KEY - look for the pattern: column_name ... PRIMARY KEY
    if table_sql_upper.contains(&format!("{column_name_upper} "))
        && table_sql_upper.contains("PRIMARY KEY")
    {
        let column_pos = table_sql_upper.find(&column_name_upper);
        let pk_pos = table_sql_upper.find("PRIMARY KEY");
        if let (Some(col_pos), Some(pk_pos)) = (column_pos, pk_pos) {
            // If PRIMARY KEY appears within 200 characters after column name, likely the same column
            if pk_pos > col_pos && (pk_pos - col_pos) < 200 {
                return Ok(true);
            }
        }
    }

    // Check for UNIQUE constraint on this column
    if table_sql_upper.contains(&format!("{column_name_upper} "))
        && table_sql_upper.contains("UNIQUE")
    {
        let column_pos = table_sql_upper.find(&column_name_upper);
        let unique_pos = table_sql_upper.find("UNIQUE");
        if let (Some(col_pos), Some(unique_pos)) = (column_pos, unique_pos) {
            // If UNIQUE appears within 100 characters after column name
            if unique_pos > col_pos && (unique_pos - col_pos) < 100 {
                return Ok(true);
            }
        }
    }

    // Check for CHECK constraint mentioning this column
    if table_sql_upper.contains("CHECK") && table_sql_upper.contains(&column_name_upper) {
        return Ok(true);
    }

    // Check for GENERATED column
    if table_sql_upper.contains(&format!("{column_name_upper} "))
        && table_sql_upper.contains("GENERATED")
    {
        return Ok(true);
    }

    // Check for UNIQUE indexes on this column
    let mut index_stmt = connection
        .prepare(
            "SELECT sql FROM sqlite_master WHERE type='index' AND tbl_name=? AND sql IS NOT NULL",
        )
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    let index_rows: Vec<String> = index_stmt
        .query_map([table_name], |row| row.get::<_, String>(0))
        .map_err(RusqliteDatabaseError::Rusqlite)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    for index_sql in index_rows {
        let index_sql_upper = index_sql.to_uppercase();
        if index_sql_upper.contains("UNIQUE") && index_sql_upper.contains(&column_name_upper) {
            return Ok(true);
        }
    }

    // If none of the above conditions are met, we can use the simple column-based approach
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
) -> Result<String, RusqliteDatabaseError> {
    // Simple regex-based approach to modify column definition
    // This handles most common cases but could be enhanced with a proper SQL parser

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

    // Build the new column definition
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
            crate::DatabaseValue::NowPlus(_) => return Err(RusqliteDatabaseError::InvalidRequest),
        };

        write!(new_column_def, " DEFAULT {default_str}").unwrap();
    }

    // Find and replace the column definition using regex
    // Pattern matches: column_name followed by type and optional constraints
    let column_pattern = format!(
        r"`?{}`?\s+\w+(\s+(NOT\s+NULL|PRIMARY\s+KEY|UNIQUE|CHECK\s*\([^)]+\)|DEFAULT\s+[^,\s)]+|GENERATED\s+[^,)]+))*",
        regex::escape(column_name)
    );

    let re =
        regex::Regex::new(&column_pattern).map_err(|_| RusqliteDatabaseError::InvalidRequest)?;

    let modified_sql = re.replace(original_sql, new_column_def.as_str());

    // Replace table name
    let final_sql = modified_sql.replace(original_table_name, new_table_name);

    Ok(final_sql)
}

#[cfg(all(test, feature = "schema"))]
mod sql_parsing_tests {
    use super::*;

    #[test]
    fn test_modify_create_table_sql_simple_column() {
        let original_sql =
            "CREATE TABLE test_table (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)";
        let result = modify_create_table_sql(
            original_sql,
            "test_table",
            "temp_table",
            "age",
            &crate::schema::DataType::BigInt,
            Some(false),
            Some(&crate::DatabaseValue::Int64(18)),
        )
        .unwrap();

        // Should replace table name and modify the age column
        assert!(result.contains("temp_table"));
        assert!(result.contains("`age` INTEGER NOT NULL DEFAULT 18"));
        // Other columns should remain unchanged
        assert!(result.contains("id INTEGER PRIMARY KEY"));
        assert!(result.contains("name TEXT"));
    }

    #[test]
    fn test_modify_create_table_sql_change_data_type() {
        let original_sql =
            "CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT, active BOOLEAN)";
        let result = modify_create_table_sql(
            original_sql,
            "users",
            "users_temp",
            "active",
            &crate::schema::DataType::SmallInt,
            Some(true),
            None,
        )
        .unwrap();

        // Should change BOOLEAN to INTEGER (SmallInt maps to INTEGER in SQLite)
        assert!(result.contains("users_temp"));
        assert!(result.contains("`active` INTEGER"));
        // Should not add NOT NULL since nullable=true
        assert!(!result.contains("`active` INTEGER NOT NULL"));
        // Should not add DEFAULT since none provided
        assert!(!result.contains("DEFAULT"));
    }
}

#[cfg(feature = "schema")]
#[allow(clippy::too_many_lines)]
fn rusqlite_exec_table_recreation_workaround(
    connection: &Connection,
    table_name: &str,
    column_name: &str,
    new_data_type: &crate::schema::DataType,
    new_nullable: Option<bool>,
    new_default: Option<&crate::DatabaseValue>,
) -> Result<(), DatabaseError> {
    // Begin transaction
    connection
        .execute("BEGIN TRANSACTION", [])
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    let result = (|| -> Result<(), RusqliteDatabaseError> {
        // Step 1: Check and disable foreign keys if enabled
        let foreign_keys_enabled: i32 = connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        if foreign_keys_enabled == 1 {
            connection
                .execute("PRAGMA foreign_keys=OFF", [])
                .map_err(RusqliteDatabaseError::Rusqlite)?;
        }

        // Step 2: Save existing schema objects (indexes, triggers, views)
        let mut schema_stmt = connection
            .prepare("SELECT sql FROM sqlite_master WHERE tbl_name=? AND type IN ('index','trigger','view') AND sql IS NOT NULL")
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        let schema_objects: Vec<String> = schema_stmt
            .query_map([table_name], |row| row.get::<_, String>(0))
            .map_err(RusqliteDatabaseError::Rusqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        // Step 3: Get original table schema and column info
        let original_sql: String = connection
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name=?",
                [table_name],
                |row| row.get(0),
            )
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        // Step 4: Create temporary table name
        let temp_table = format!(
            "{}_temp_{}",
            table_name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );

        // Step 5: Parse and modify the CREATE TABLE SQL to update the column definition
        let new_table_sql = modify_create_table_sql(
            &original_sql,
            table_name,
            &temp_table,
            column_name,
            new_data_type,
            new_nullable,
            new_default,
        )?;

        connection
            .execute(&new_table_sql, [])
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        // Step 6: Get column list for INSERT SELECT
        let mut columns_stmt = connection
            .prepare(&format!("PRAGMA table_info({table_name})"))
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        let columns: Vec<String> = columns_stmt
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(RusqliteDatabaseError::Rusqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        // Step 7: Copy data with potential type conversion for the modified column
        let column_list = columns
            .iter()
            .map(|col| {
                if col == column_name {
                    // Apply CAST for the modified column to ensure proper type conversion
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

        connection
            .execute(
                &format!("INSERT INTO {temp_table} SELECT {column_list} FROM {table_name}"),
                [],
            )
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        // Step 8: Drop old table
        connection
            .execute(&format!("DROP TABLE {table_name}"), [])
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        // Step 9: Rename temp table to original name
        connection
            .execute(
                &format!("ALTER TABLE {temp_table} RENAME TO {table_name}"),
                [],
            )
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        // Step 10: Recreate schema objects
        for schema_sql in schema_objects {
            // Skip auto-indexes and internal indexes
            if !schema_sql.to_uppercase().contains("AUTOINDEX") {
                connection
                    .execute(&schema_sql, [])
                    .map_err(RusqliteDatabaseError::Rusqlite)?;
            }
        }

        // Step 11: Re-enable foreign keys if they were enabled
        if foreign_keys_enabled == 1 {
            connection
                .execute("PRAGMA foreign_keys=ON", [])
                .map_err(RusqliteDatabaseError::Rusqlite)?;

            // Step 12: Check foreign key integrity
            let mut fk_stmt = connection
                .prepare("PRAGMA foreign_key_check")
                .map_err(RusqliteDatabaseError::Rusqlite)?;
            let fk_violations: Vec<String> = fk_stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(RusqliteDatabaseError::Rusqlite)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(RusqliteDatabaseError::Rusqlite)?;

            if !fk_violations.is_empty() {
                return Err(RusqliteDatabaseError::Rusqlite(
                    rusqlite::Error::SqliteFailure(
                        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY),
                        Some("Foreign key violations detected after table recreation".to_string()),
                    ),
                ));
            }
        }

        Ok(())
    })();

    match result {
        Ok(()) => {
            connection
                .execute("COMMIT", [])
                .map_err(RusqliteDatabaseError::Rusqlite)?;
            Ok(())
        }
        Err(e) => {
            let _ = connection.execute("ROLLBACK", []);
            Err(DatabaseError::Rusqlite(e))
        }
    }
}

fn update_and_get_row(
    connection: &Connection,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Option<crate::Row>, RusqliteDatabaseError> {
    let select_query = limit.map(|_| {
        format!(
            "SELECT rowid FROM {table_name} {}",
            build_where_clause(filters),
        )
    });

    let query = format!(
        "UPDATE {table_name} {} {} RETURNING *",
        build_set_clause(values),
        build_update_where_clause(filters, limit, select_query.as_deref()),
    );

    let all_values = values
        .iter()
        .flat_map(|(_, value)| value.params().unwrap_or(vec![]).into_iter().cloned())
        .map(std::convert::Into::into)
        .collect::<Vec<_>>();
    let mut all_filter_values = filters
        .map(|filters| {
            filters
                .iter()
                .flat_map(|value| value.params().unwrap_or_default().into_iter().cloned())
                .map(std::convert::Into::into)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if limit.is_some() {
        all_filter_values.extend(all_filter_values.clone());
    }

    let all_values = [all_values, all_filter_values].concat();

    log::trace!("Running update query: {query} with params: {all_values:?}");

    let mut statement = connection.prepare_cached(&query)?;

    bind_values(&mut statement, Some(&all_values), false, 0)?;

    let column_names = statement
        .column_names()
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>();

    let mut query = statement.raw_query();

    query
        .next()?
        .map(|row| from_row(&column_names, row))
        .transpose()
}

fn update_and_get_rows(
    connection: &Connection,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, RusqliteDatabaseError> {
    let select_query = limit.map(|_| {
        format!(
            "SELECT rowid FROM {table_name} {}",
            build_where_clause(filters),
        )
    });

    let query = format!(
        "UPDATE {table_name} {} {} RETURNING *",
        build_set_clause(values),
        build_update_where_clause(filters, limit, select_query.as_deref()),
    );

    let all_values = values
        .iter()
        .flat_map(|(_, value)| value.params().unwrap_or(vec![]).into_iter().cloned())
        .map(std::convert::Into::into)
        .collect::<Vec<_>>();
    let mut all_filter_values = filters
        .map(|filters| {
            filters
                .iter()
                .flat_map(|value| value.params().unwrap_or_default().into_iter().cloned())
                .map(std::convert::Into::into)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if limit.is_some() {
        all_filter_values.extend(all_filter_values.clone());
    }

    let all_values = [all_values, all_filter_values].concat();

    log::trace!("Running update query: {query} with params: {all_values:?}");

    let mut statement = connection.prepare_cached(&query)?;
    bind_values(&mut statement, Some(&all_values), false, 0)?;
    let column_names = statement
        .column_names()
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>();

    to_rows(&column_names, statement.raw_query())
}

fn build_join_clauses(joins: Option<&[Join]>) -> String {
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

fn build_where_clause(filters: Option<&[Box<dyn BooleanExpression>]>) -> String {
    filters.map_or_else(String::new, |filters| {
        if filters.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", build_where_props(filters).join(" AND "))
        }
    })
}

fn build_where_props(filters: &[Box<dyn BooleanExpression>]) -> Vec<String> {
    filters
        .iter()
        .map(|filter| filter.deref().to_sql())
        .collect()
}

fn build_sort_clause(sorts: Option<&[Sort]>) -> String {
    sorts.map_or_else(String::new, |sorts| {
        if sorts.is_empty() {
            String::new()
        } else {
            format!("ORDER BY {}", build_sort_props(sorts).join(", "))
        }
    })
}

fn build_sort_props(sorts: &[Sort]) -> Vec<String> {
    sorts.iter().map(Sort::to_sql).collect()
}

fn build_update_where_clause(
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
    query: Option<&str>,
) -> String {
    let clause = build_where_clause(filters);
    let limit_clause = build_update_limit_clause(limit, query);

    let clause = if limit_clause.is_empty() {
        clause
    } else if clause.is_empty() {
        "WHERE".into()
    } else {
        clause + " AND"
    };

    format!("{clause} {limit_clause}").trim().to_string()
}

fn build_update_limit_clause(limit: Option<usize>, query: Option<&str>) -> String {
    limit.map_or_else(String::new, |limit| {
        query.map_or_else(String::new, |query| {
            format!("rowid IN ({query} LIMIT {limit})")
        })
    })
}

fn build_set_clause(values: &[(&str, Box<dyn Expression>)]) -> String {
    if values.is_empty() {
        String::new()
    } else {
        format!("SET {}", build_set_props(values).join(", "))
    }
}

fn build_set_props(values: &[(&str, Box<dyn Expression>)]) -> Vec<String> {
    values
        .iter()
        .map(|(name, value)| format!("{name}=({})", value.deref().to_sql()))
        .collect()
}

fn build_values_clause(values: &[(&str, Box<dyn Expression>)]) -> String {
    if values.is_empty() {
        "DEFAULT VALUES".to_string()
    } else {
        format!("VALUES({})", build_values_props(values).join(", "))
    }
}

fn build_values_props(values: &[(&str, Box<dyn Expression>)]) -> Vec<String> {
    values
        .iter()
        .map(|(_, value)| value.deref().to_sql())
        .collect()
}

#[allow(clippy::too_many_lines)]
fn bind_values(
    statement: &mut Statement<'_>,
    values: Option<&[RusqliteDatabaseValue]>,
    constant_inc: bool,
    offset: usize,
) -> Result<usize, RusqliteDatabaseError> {
    if let Some(values) = values {
        let mut i = 1 + offset;
        for value in values {
            match &**value {
                DatabaseValue::Null
                | DatabaseValue::StringOpt(None)
                | DatabaseValue::BoolOpt(None)
                | DatabaseValue::Int8Opt(None)
                | DatabaseValue::Int16Opt(None)
                | DatabaseValue::Int32Opt(None)
                | DatabaseValue::Int64Opt(None)
                | DatabaseValue::UInt8Opt(None)
                | DatabaseValue::UInt16Opt(None)
                | DatabaseValue::UInt32Opt(None)
                | DatabaseValue::UInt64Opt(None)
                | DatabaseValue::Real64Opt(None)
                | DatabaseValue::Real32Opt(None)
                | DatabaseValue::Now
                | DatabaseValue::NowPlus(..) => (),
                #[cfg(feature = "decimal")]
                DatabaseValue::DecimalOpt(None) => (),
                #[cfg(feature = "uuid")]
                DatabaseValue::UuidOpt(None) => (),
                DatabaseValue::Bool(value) | DatabaseValue::BoolOpt(Some(value)) => {
                    statement.raw_bind_parameter(i, i32::from(*value))?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::String(value) | DatabaseValue::StringOpt(Some(value)) => {
                    statement.raw_bind_parameter(i, value)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::Int8(value) | DatabaseValue::Int8Opt(Some(value)) => {
                    statement.raw_bind_parameter(i, i64::from(*value))?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::Int16(value) | DatabaseValue::Int16Opt(Some(value)) => {
                    statement.raw_bind_parameter(i, i64::from(*value))?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::Int32(value) | DatabaseValue::Int32Opt(Some(value)) => {
                    statement.raw_bind_parameter(i, i64::from(*value))?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::Int64(value) | DatabaseValue::Int64Opt(Some(value)) => {
                    statement.raw_bind_parameter(i, *value)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::UInt8(value) | DatabaseValue::UInt8Opt(Some(value)) => {
                    let signed = i8::try_from(*value).ok();
                    statement.raw_bind_parameter(i, signed)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::UInt16(value) | DatabaseValue::UInt16Opt(Some(value)) => {
                    let signed = i16::try_from(*value).ok();
                    statement.raw_bind_parameter(i, signed)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::UInt32(value) | DatabaseValue::UInt32Opt(Some(value)) => {
                    let signed = i32::try_from(*value).ok();
                    statement.raw_bind_parameter(i, signed)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::UInt64(value) | DatabaseValue::UInt64Opt(Some(value)) => {
                    statement.raw_bind_parameter(i, *value)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::Real64(value) | DatabaseValue::Real64Opt(Some(value)) => {
                    statement.raw_bind_parameter(i, *value)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::Real32(value) | DatabaseValue::Real32Opt(Some(value)) => {
                    statement.raw_bind_parameter(i, f64::from(*value))?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                #[cfg(feature = "decimal")]
                DatabaseValue::Decimal(value) | DatabaseValue::DecimalOpt(Some(value)) => {
                    statement.raw_bind_parameter(i, value.to_string())?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                #[cfg(feature = "uuid")]
                DatabaseValue::Uuid(value) | DatabaseValue::UuidOpt(Some(value)) => {
                    statement.raw_bind_parameter(i, value.to_string())?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::DateTime(value) => {
                    // FIXME: Actually format the date
                    statement.raw_bind_parameter(i, value.to_string())?;
                    if !constant_inc {
                        i += 1;
                    }
                }
            }
            if constant_inc {
                i += 1;
            }
        }
        Ok(i - 1)
    } else {
        Ok(0)
    }
}

fn to_rows(
    column_names: &[String],
    mut rows: Rows<'_>,
) -> Result<Vec<crate::Row>, RusqliteDatabaseError> {
    let mut results = vec![];

    while let Some(row) = rows.next()? {
        results.push(from_row(column_names, row)?);
    }

    log::trace!(
        "Got {} row{}",
        results.len(),
        if results.len() == 1 { "" } else { "s" }
    );

    Ok(results)
}

fn to_values(values: &[(&str, DatabaseValue)]) -> Vec<RusqliteDatabaseValue> {
    values
        .iter()
        .map(|(_key, value)| value.clone())
        .map(std::convert::Into::into)
        .collect::<Vec<_>>()
}

fn exprs_to_values(values: &[(&str, Box<dyn Expression>)]) -> Vec<RusqliteDatabaseValue> {
    values
        .iter()
        .flat_map(|value| value.1.values().into_iter())
        .flatten()
        .cloned()
        .map(std::convert::Into::into)
        .collect::<Vec<_>>()
}

fn bexprs_to_values(values: &[Box<dyn BooleanExpression>]) -> Vec<RusqliteDatabaseValue> {
    values
        .iter()
        .flat_map(|value| value.values().into_iter())
        .flatten()
        .cloned()
        .map(std::convert::Into::into)
        .collect::<Vec<_>>()
}

#[allow(unused)]
fn to_values_opt(values: Option<&[(&str, DatabaseValue)]>) -> Option<Vec<RusqliteDatabaseValue>> {
    values.map(to_values)
}

#[allow(unused)]
fn exprs_to_values_opt(
    values: Option<&[(&str, Box<dyn Expression>)]>,
) -> Option<Vec<RusqliteDatabaseValue>> {
    values.map(exprs_to_values)
}

fn bexprs_to_values_opt(
    values: Option<&[Box<dyn BooleanExpression>]>,
) -> Option<Vec<RusqliteDatabaseValue>> {
    values.map(bexprs_to_values)
}

#[allow(clippy::too_many_arguments)]
fn select(
    connection: &Connection,
    table_name: &str,
    distinct: bool,
    columns: &[&str],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    joins: Option<&[Join<'_>]>,
    sort: Option<&[Sort]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, RusqliteDatabaseError> {
    let query = format!(
        "SELECT {} {} FROM {table_name} {} {} {} {}",
        if distinct { "DISTINCT" } else { "" },
        columns.join(", "),
        build_join_clauses(joins),
        build_where_clause(filters),
        build_sort_clause(sort),
        limit.map_or_else(String::new, |limit| format!("LIMIT {limit}"))
    );

    log::trace!(
        "Running select query: {query} with params: {:?}",
        filters.map(|f| f.iter().filter_map(|x| x.params()).collect::<Vec<_>>())
    );

    let mut statement = connection.prepare_cached(&query)?;
    let column_names = statement
        .column_names()
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>();

    bind_values(
        &mut statement,
        bexprs_to_values_opt(filters).as_deref(),
        false,
        0,
    )?;

    to_rows(&column_names, statement.raw_query())
}

fn delete(
    connection: &Connection,
    table_name: &str,
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, RusqliteDatabaseError> {
    let where_clause = build_where_clause(filters);

    let select_query = limit.map(|_| format!("SELECT rowid FROM {table_name} {where_clause}",));

    let query = format!(
        "DELETE FROM {table_name} {} RETURNING *",
        build_update_where_clause(filters, limit, select_query.as_deref()),
    );

    let mut all_filter_values: Vec<RusqliteDatabaseValue> = filters
        .map(|filters| {
            filters
                .iter()
                .flat_map(|value| value.params().unwrap_or_default().into_iter().cloned())
                .map(std::convert::Into::into)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if limit.is_some() {
        all_filter_values.extend(all_filter_values.clone());
    }

    log::trace!(
        "Running delete query: {query} with params: {:?}",
        all_filter_values
            .iter()
            .filter_map(super::query::Expression::params)
            .collect::<Vec<_>>()
    );

    let mut statement = connection.prepare_cached(&query)?;
    let column_names = statement
        .column_names()
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>();

    bind_values(&mut statement, Some(&all_filter_values), false, 0)?;

    to_rows(&column_names, statement.raw_query())
}

fn find_row(
    connection: &Connection,
    table_name: &str,
    distinct: bool,
    columns: &[&str],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    joins: Option<&[Join]>,
    sort: Option<&[Sort]>,
) -> Result<Option<crate::Row>, RusqliteDatabaseError> {
    let query = format!(
        "SELECT {} {} FROM {table_name} {} {} {} LIMIT 1",
        if distinct { "DISTINCT" } else { "" },
        columns.join(", "),
        build_join_clauses(joins),
        build_where_clause(filters),
        build_sort_clause(sort),
    );

    let mut statement = connection.prepare_cached(&query)?;
    let column_names = statement
        .column_names()
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>();

    bind_values(
        &mut statement,
        bexprs_to_values_opt(filters).as_deref(),
        false,
        0,
    )?;

    log::trace!(
        "Running find_row query: {query} with params: {:?}",
        filters.map(|f| f.iter().filter_map(|x| x.params()).collect::<Vec<_>>())
    );

    let mut query = statement.raw_query();

    query
        .next()?
        .map(|row| from_row(&column_names, row))
        .transpose()
}

fn insert_and_get_row(
    connection: &Connection,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
) -> Result<crate::Row, RusqliteDatabaseError> {
    let column_names = values
        .iter()
        .map(|(key, _v)| format!("`{key}`"))
        .collect::<Vec<_>>()
        .join(", ");

    let insert_columns = if values.is_empty() {
        String::new()
    } else {
        format!("({column_names})")
    };
    let query = format!(
        "INSERT INTO {table_name} {insert_columns} {} RETURNING *",
        build_values_clause(values),
    );

    let mut statement = connection.prepare_cached(&query)?;
    let column_names = statement
        .column_names()
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>();

    bind_values(&mut statement, Some(&exprs_to_values(values)), false, 0)?;

    log::trace!(
        "Running insert_and_get_row query: {query} with params: {:?}",
        values
            .iter()
            .filter_map(|(_, x)| x.params())
            .collect::<Vec<_>>()
    );

    let mut query = statement.raw_query();

    query
        .next()?
        .map(|row| from_row(&column_names, row))
        .ok_or(RusqliteDatabaseError::NoRow)?
}

/// # Errors
///
/// Will return `Err` if the update multi execution failed.
pub fn update_multi(
    connection: &Connection,
    table_name: &str,
    values: &[Vec<(&str, Box<dyn Expression>)>],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    mut limit: Option<usize>,
) -> Result<Vec<crate::Row>, RusqliteDatabaseError> {
    let mut results = vec![];

    if values.is_empty() {
        return Ok(results);
    }

    let mut pos = 0;
    let mut i = 0;
    let mut last_i = i;

    for value in values {
        let count = value.len();
        if pos + count >= (i16::MAX - 1) as usize {
            results.append(&mut update_chunk(
                connection,
                table_name,
                &values[last_i..i],
                filters,
                limit,
            )?);
            last_i = i;
            pos = 0;
        }
        i += 1;
        pos += count;

        if let Some(value) = limit {
            if count >= value {
                return Ok(results);
            }

            limit.replace(value - count);
        }
    }

    if i > last_i {
        results.append(&mut update_chunk(
            connection,
            table_name,
            &values[last_i..],
            filters,
            limit,
        )?);
    }

    Ok(results)
}

fn update_chunk(
    connection: &Connection,
    table_name: &str,
    values: &[Vec<(&str, Box<dyn Expression>)>],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, RusqliteDatabaseError> {
    let first = values[0].as_slice();
    let expected_value_size = first.len();

    if let Some(bad_row) = values.iter().skip(1).find(|v| {
        v.len() != expected_value_size || v.iter().enumerate().any(|(i, c)| c.0 != first[i].0)
    }) {
        log::error!("Bad row: {bad_row:?}. Expected to match schema of first row: {first:?}");
        return Err(RusqliteDatabaseError::InvalidRequest);
    }

    let set_clause = values[0]
        .iter()
        .map(|(name, _value)| format!("`{name}` = EXCLUDED.`{name}`"))
        .collect::<Vec<_>>()
        .join(", ");

    let column_names = values[0]
        .iter()
        .map(|(key, _v)| format!("`{key}`"))
        .collect::<Vec<_>>()
        .join(", ");

    let select_query = limit.map(|_| {
        format!(
            "SELECT rowid FROM {table_name} {}",
            build_where_clause(filters),
        )
    });

    let query = format!(
        "
        UPDATE {table_name} ({column_names})
        {}
        SET {set_clause}
        RETURNING *",
        build_update_where_clause(filters, limit, select_query.as_deref()),
    );

    let all_values = values
        .iter()
        .flat_map(std::iter::IntoIterator::into_iter)
        .flat_map(|(_, value)| value.params().unwrap_or(vec![]).into_iter().cloned())
        .map(std::convert::Into::into)
        .collect::<Vec<_>>();
    let mut all_filter_values = filters
        .as_ref()
        .map(|filters| {
            filters
                .iter()
                .flat_map(|value| {
                    value
                        .params()
                        .unwrap_or_default()
                        .into_iter()
                        .cloned()
                        .map(std::convert::Into::into)
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if limit.is_some() {
        all_filter_values.extend(all_filter_values.clone());
    }

    let all_values = [all_values, all_filter_values].concat();

    log::trace!("Running update chunk query: {query} with params: {all_values:?}");

    let mut statement = connection.prepare_cached(&query)?;
    let column_names = statement
        .column_names()
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>();

    bind_values(&mut statement, Some(&all_values), true, 0)?;

    to_rows(&column_names, statement.raw_query())
}

/// # Errors
///
/// Will return `Err` if the upsert multi execution failed.
pub fn upsert_multi(
    connection: &Connection,
    table_name: &str,
    unique: &[Box<dyn Expression>],
    values: &[Vec<(&str, Box<dyn Expression>)>],
) -> Result<Vec<crate::Row>, RusqliteDatabaseError> {
    let mut results = vec![];

    if values.is_empty() {
        return Ok(results);
    }

    let mut pos = 0;
    let mut i = 0;
    let mut last_i = i;

    for value in values {
        let count = value.len();
        if pos + count >= (i16::MAX - 1) as usize {
            results.append(&mut upsert_chunk(
                connection,
                table_name,
                unique,
                &values[last_i..i],
            )?);
            last_i = i;
            pos = 0;
        }
        i += 1;
        pos += count;
    }

    if i > last_i {
        results.append(&mut upsert_chunk(
            connection,
            table_name,
            unique,
            &values[last_i..],
        )?);
    }

    Ok(results)
}

fn upsert_chunk(
    connection: &Connection,
    table_name: &str,
    unique: &[Box<dyn Expression>],
    values: &[Vec<(&str, Box<dyn Expression>)>],
) -> Result<Vec<crate::Row>, RusqliteDatabaseError> {
    let first = values[0].as_slice();
    let expected_value_size = first.len();

    if let Some(bad_row) = values.iter().skip(1).find(|v| {
        v.len() != expected_value_size || v.iter().enumerate().any(|(i, c)| c.0 != first[i].0)
    }) {
        log::error!("Bad row: {bad_row:?}. Expected to match schema of first row: {first:?}");
        return Err(RusqliteDatabaseError::InvalidRequest);
    }

    let set_clause = values[0]
        .iter()
        .map(|(name, _value)| format!("`{name}` = EXCLUDED.`{name}`"))
        .collect::<Vec<_>>()
        .join(", ");

    let column_names = values[0]
        .iter()
        .map(|(key, _v)| format!("`{key}`"))
        .collect::<Vec<_>>()
        .join(", ");

    let values_str_list = values
        .iter()
        .map(|v| format!("({})", build_values_props(v).join(", ")))
        .collect::<Vec<_>>();

    let values_str = values_str_list.join(", ");
    let values_str = if values_str.is_empty() {
        "DEFAULT VALUES".to_string()
    } else {
        format!("VALUES {values_str}")
    };

    let unique_conflict = unique
        .iter()
        .map(|x| x.to_sql())
        .collect::<Vec<_>>()
        .join(", ");

    let insert_columns = if values.is_empty() {
        String::new()
    } else {
        format!("({column_names})")
    };
    let query = format!(
        "
        INSERT INTO {table_name} {insert_columns} {values_str}
        ON CONFLICT({unique_conflict}) DO UPDATE
            SET {set_clause}
        RETURNING *"
    );

    let all_values = &values
        .iter()
        .flat_map(std::iter::IntoIterator::into_iter)
        .flat_map(|(_, value)| value.params().unwrap_or(vec![]).into_iter().cloned())
        .map(std::convert::Into::into)
        .collect::<Vec<_>>();

    log::trace!("Running upsert chunk query: {query} with params: {all_values:?}");

    let mut statement = connection.prepare_cached(&query)?;
    let column_names = statement
        .column_names()
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>();

    bind_values(&mut statement, Some(all_values), true, 0)?;

    to_rows(&column_names, statement.raw_query())
}

fn upsert(
    connection: &Connection,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, RusqliteDatabaseError> {
    let rows = update_and_get_rows(connection, table_name, values, filters, limit)?;

    Ok(if rows.is_empty() {
        vec![insert_and_get_row(connection, table_name, values)?]
    } else {
        rows
    })
}

#[allow(unused)]
fn upsert_and_get_row(
    connection: &Connection,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<crate::Row, RusqliteDatabaseError> {
    match find_row(connection, table_name, false, &["*"], filters, None, None)? {
        Some(row) => {
            let updated =
                update_and_get_row(connection, table_name, values, filters, limit)?.unwrap();

            let str1 = format!("{row:?}");
            let str2 = format!("{updated:?}");

            if str1 == str2 {
                log::trace!("No updates to {table_name}");
            } else {
                log::debug!("Changed {table_name} from {str1} to {str2}");
            }

            Ok(updated)
        }
        None => Ok(insert_and_get_row(connection, table_name, values)?),
    }
}

#[allow(clippy::module_name_repetitions)]
/// Wrapper type for converting `DatabaseValue` to rusqlite-specific parameter types
#[derive(Debug, Clone)]
pub struct RusqliteDatabaseValue(DatabaseValue);

impl From<DatabaseValue> for RusqliteDatabaseValue {
    fn from(value: DatabaseValue) -> Self {
        Self(value)
    }
}

impl Deref for RusqliteDatabaseValue {
    type Target = DatabaseValue;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Expression for RusqliteDatabaseValue {
    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        Some(vec![self])
    }

    fn is_null(&self) -> bool {
        matches!(
            self.0,
            DatabaseValue::Null
                | DatabaseValue::BoolOpt(None)
                | DatabaseValue::Real64Opt(None)
                | DatabaseValue::StringOpt(None)
                | DatabaseValue::Int64Opt(None)
                | DatabaseValue::UInt64Opt(None)
        )
    }

    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::DatabaseValue(self)
    }
}

#[cfg(feature = "schema")]
fn rusqlite_table_exists(connection: &Connection, table_name: &str) -> Result<bool, DatabaseError> {
    let query = "SELECT name FROM sqlite_master WHERE type='table' AND name=?";
    let mut stmt = connection
        .prepare_cached(query)
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    let exists = stmt
        .exists([table_name])
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    Ok(exists)
}

#[cfg(feature = "schema")]
fn rusqlite_list_tables(connection: &Connection) -> Result<Vec<String>, DatabaseError> {
    let query = "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'";
    let mut stmt = connection
        .prepare_cached(query)
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    let mut tables = Vec::new();
    let rows = stmt
        .query_map([], |row| {
            let name: String = row.get(0)?;
            Ok(name)
        })
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    for row in rows {
        let table_name = row.map_err(RusqliteDatabaseError::Rusqlite)?;
        tables.push(table_name);
    }

    Ok(tables)
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
fn parse_default_value(default_str: Option<String>) -> Option<crate::DatabaseValue> {
    default_str.and_then(|s| {
        if s == "NULL" {
            Some(crate::DatabaseValue::Null)
        } else if s.starts_with('\'') && s.ends_with('\'') {
            // String literal
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
fn rusqlite_get_table_columns(
    connection: &Connection,
    table_name: &str,
) -> Result<Vec<crate::schema::ColumnInfo>, DatabaseError> {
    use crate::schema::ColumnInfo;

    let query = format!("PRAGMA table_info({table_name})");
    let mut stmt = connection
        .prepare_cached(&query)
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    let column_rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, u32>(0)?,            // cid (ordinal position)
                row.get::<_, String>(1)?,         // name
                row.get::<_, String>(2)?,         // type
                row.get::<_, bool>(3)?,           // notnull
                row.get::<_, Option<String>>(4)?, // dflt_value
                row.get::<_, bool>(5)?,           // pk (primary key)
            ))
        })
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    let mut columns = Vec::new();

    for column_result in column_rows {
        let (ordinal, name, type_str, not_null, default_value, is_pk) =
            column_result.map_err(RusqliteDatabaseError::Rusqlite)?;

        let data_type = sqlite_type_to_data_type(&type_str);
        let default_val = parse_default_value(default_value);

        let auto_increment = if is_pk {
            // Check if this column has AUTOINCREMENT in the CREATE TABLE statement
            check_sqlite_autoincrement(connection, table_name, &name)?
        } else {
            false
        };

        columns.push(ColumnInfo {
            name,
            data_type,
            nullable: !not_null,
            is_primary_key: is_pk,
            auto_increment,
            default_value: default_val,
            ordinal_position: ordinal + 1, // Convert 0-based to 1-based
        });
    }

    Ok(columns)
}

#[cfg(feature = "schema")]
fn check_sqlite_autoincrement(
    connection: &Connection,
    table_name: &str,
    column_name: &str,
) -> Result<bool, RusqliteDatabaseError> {
    use rusqlite::OptionalExtension as _;

    // Query the CREATE TABLE statement from sqlite_master
    let query = "SELECT sql FROM sqlite_master WHERE type='table' AND name=?";
    let mut stmt = connection
        .prepare_cached(query)
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    let sql: Option<String> = stmt
        .query_row([table_name], |row| row.get(0))
        .optional()
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    if let Some(create_sql) = sql {
        // Parse the CREATE TABLE statement for AUTOINCREMENT
        // Look for pattern: column_name TYPE PRIMARY KEY AUTOINCREMENT
        let normalized_sql = create_sql.to_uppercase();
        let normalized_column = column_name.to_uppercase();

        // Find the column definition
        if let Some(column_start) = normalized_sql.find(&normalized_column) {
            // Get the portion from the column name onwards
            let column_portion = &normalized_sql[column_start..];

            // Look for PRIMARY KEY followed by AUTOINCREMENT
            if column_portion.contains("PRIMARY KEY") {
                // Find PRIMARY KEY position relative to column start
                if let Some(pk_pos) = column_portion.find("PRIMARY KEY") {
                    // Get text after PRIMARY KEY
                    let after_pk = &column_portion[pk_pos + "PRIMARY KEY".len()..];

                    // Check if AUTOINCREMENT appears before the next comma or closing paren
                    let end_pos = after_pk
                        .find(',')
                        .unwrap_or_else(|| after_pk.find(')').unwrap_or(after_pk.len()));
                    let column_rest = &after_pk[..end_pos];

                    return Ok(column_rest.contains("AUTOINCREMENT"));
                }
            }
        }
    }

    Ok(false)
}

#[cfg(feature = "schema")]
fn rusqlite_column_exists(
    connection: &Connection,
    table_name: &str,
    column_name: &str,
) -> Result<bool, DatabaseError> {
    let columns = rusqlite_get_table_columns(connection, table_name)?;
    Ok(columns.iter().any(|col| col.name == column_name))
}

#[cfg(feature = "schema")]
fn rusqlite_get_table_info(
    connection: &Connection,
    table_name: &str,
) -> Result<Option<crate::schema::TableInfo>, DatabaseError> {
    use crate::schema::{ForeignKeyInfo, IndexInfo, TableInfo};
    use std::collections::BTreeMap;

    // First check if table exists
    if !rusqlite_table_exists(connection, table_name)? {
        return Ok(None);
    }

    // Get columns
    let columns_list = rusqlite_get_table_columns(connection, table_name)?;
    let mut columns = BTreeMap::new();
    for col in columns_list {
        columns.insert(col.name.clone(), col);
    }

    // Get indexes
    let index_query = format!("PRAGMA index_list({table_name})");
    let mut index_stmt = connection
        .prepare_cached(&index_query)
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    let index_rows = index_stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?, // name
                row.get::<_, bool>(2)?,   // unique
                row.get::<_, String>(3)?, // origin ('c' for CREATE INDEX, 'u' for UNIQUE, 'pk' for PRIMARY KEY)
            ))
        })
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    let mut indexes = BTreeMap::new();
    for index_result in index_rows {
        let (index_name, is_unique, origin) =
            index_result.map_err(RusqliteDatabaseError::Rusqlite)?;

        // Get index columns
        let index_info_query = format!("PRAGMA index_info({index_name})");
        let mut index_info_stmt = connection
            .prepare_cached(&index_info_query)
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        let index_column_rows = index_info_stmt
            .query_map([], |row| row.get::<_, String>(2)) // column name
            .map_err(RusqliteDatabaseError::Rusqlite)?;

        let mut index_columns = Vec::new();
        for col_result in index_column_rows {
            index_columns.push(col_result.map_err(RusqliteDatabaseError::Rusqlite)?);
        }

        indexes.insert(
            index_name.clone(),
            IndexInfo {
                name: index_name,
                unique: is_unique,
                columns: index_columns,
                is_primary: origin == "pk",
            },
        );
    }

    // Get foreign keys
    let fk_query = format!("PRAGMA foreign_key_list({table_name})");
    let mut fk_stmt = connection
        .prepare_cached(&fk_query)
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    let fk_rows = fk_stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(2)?, // table (referenced table)
                row.get::<_, String>(3)?, // from (column in current table)
                row.get::<_, String>(4)?, // to (column in referenced table)
                row.get::<_, String>(5)?, // on_update
                row.get::<_, String>(6)?, // on_delete
            ))
        })
        .map_err(RusqliteDatabaseError::Rusqlite)?;

    let mut foreign_keys = BTreeMap::new();
    for fk_result in fk_rows {
        let (referenced_table, column, referenced_column, on_update, on_delete) =
            fk_result.map_err(RusqliteDatabaseError::Rusqlite)?;

        let fk_name = format!("{table_name}_{column}_{referenced_table}_{referenced_column}");

        foreign_keys.insert(
            fk_name.clone(),
            ForeignKeyInfo {
                name: fk_name,
                column,
                referenced_table,
                referenced_column,
                on_update: if on_update == "NO ACTION" {
                    None
                } else {
                    Some(on_update)
                },
                on_delete: if on_delete == "NO ACTION" {
                    None
                } else {
                    Some(on_delete)
                },
            },
        );
    }

    Ok(Some(TableInfo {
        name: table_name.to_string(),
        columns,
        indexes,
        foreign_keys,
    }))
}

fn sqlite_transform_query_for_params(
    query: &str,
    params: &[DatabaseValue],
) -> Result<(String, Vec<DatabaseValue>), DatabaseError> {
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
    .map_err(DatabaseError::QueryFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::{FilterableQuery, where_eq};
    use rusqlite::Connection;
    use std::sync::Arc;
    use std::time::Duration;
    use switchy_async::sync::Mutex;

    const CONNECTION_POOL_SIZE: u8 = 5;

    fn create_test_db() -> RusqliteDatabase {
        // Use unique in-memory database name for each test to avoid conflicts
        let test_id = std::thread::current().id();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_url =
            format!("file:testdb_{test_id:?}_{timestamp}:?mode=memory&cache=shared&uri=true");

        let mut connections = Vec::new();

        for i in 0..CONNECTION_POOL_SIZE {
            let conn = Connection::open(&db_url).expect("Failed to create shared memory database");

            // Only create table in first connection since shared memory shares schema
            if i == 0 {
                conn.execute(
                    "CREATE TABLE test_table (id INTEGER PRIMARY KEY, name TEXT, value INTEGER)",
                    [],
                )
                .expect("Failed to create test table");
            }

            connections.push(Arc::new(Mutex::new(conn)));
        }

        RusqliteDatabase::new(connections)
    }

    #[switchy_async::test]
    async fn test_basic_transaction_commit() {
        let db = create_test_db();

        // Begin a transaction
        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        // Insert data within transaction
        let insert_stmt = crate::query::insert("test_table")
            .value("name", DatabaseValue::String("test_name".to_string()))
            .value("value", DatabaseValue::Int64(42));

        insert_stmt
            .execute(&*tx)
            .await
            .expect("Failed to insert in transaction");

        // Commit the transaction
        tx.commit().await.expect("Failed to commit transaction");

        // Verify data was committed
        let select_stmt = crate::query::select("test_table")
            .columns(&["name", "value"])
            .filter(Box::new(where_eq(
                "name",
                DatabaseValue::String("test_name".to_string()),
            )));

        let rows = select_stmt
            .execute(&db)
            .await
            .expect("Failed to select after commit");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].get("name"),
            Some(DatabaseValue::String("test_name".to_string()))
        );
        assert_eq!(rows[0].get("value"), Some(DatabaseValue::Int64(42)));
    }

    #[switchy_async::test(real_time)]
    async fn test_transaction_isolation() {
        let db = create_test_db();

        // Begin a transaction
        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        // Insert data within the transaction
        let insert_stmt = crate::query::insert("test_table")
            .value("name", DatabaseValue::String("tx_data".to_string()))
            .value("value", DatabaseValue::Int64(100));
        insert_stmt
            .execute(&*tx)
            .await
            .expect("Failed to insert in transaction");

        // Query from main database - handle both timeout (rusqlite) and success (sqlx)
        let select_stmt = crate::query::select("test_table").filter(Box::new(where_eq(
            "name",
            DatabaseValue::String("tx_data".to_string()),
        )));

        // Use timeout but handle both database lock (connection pool) and timeout cases gracefully
        let rows = match switchy_async::time::timeout(Duration::from_millis(100), select_stmt.execute(&db)).await {
            Ok(Ok(rows)) => rows,  // Query succeeded (sqlx case)
            Ok(Err(_))         // Database lock error (connection pool isolation working)
            | Err(_) => vec![] // Timeout (serialized case)
        };

        // Key assertion: uncommitted data not visible (works for all backends)
        assert_eq!(rows.len(), 0, "Should not see uncommitted transaction data");

        // Commit the transaction
        tx.commit().await.expect("Failed to commit transaction");

        // Now verify the data is visible after commit
        let select_stmt2 = crate::query::select("test_table").filter(Box::new(where_eq(
            "name",
            DatabaseValue::String("tx_data".to_string()),
        )));
        let rows = select_stmt2
            .execute(&db)
            .await
            .expect("Failed to query after commit");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("value"), Some(DatabaseValue::Int64(100)));
    }

    #[switchy_async::test(real_time)]
    async fn test_concurrent_transactions_with_connection_pool() {
        let db = Arc::new(create_test_db());

        let db1 = Arc::clone(&db);
        let db2 = Arc::clone(&db);

        let start_time = std::time::Instant::now();

        // Start two transactions concurrently
        let tx1_task = switchy_async::task::spawn(async move {
            let tx = db1
                .begin_transaction()
                .await
                .expect("Failed to begin transaction 1");

            // Insert data in first transaction
            let insert_stmt = crate::query::insert("test_table")
                .value("name", DatabaseValue::String("tx1_data".to_string()))
                .value("value", DatabaseValue::Int64(1));
            insert_stmt
                .execute(&*tx)
                .await
                .expect("Failed to insert in transaction 1");

            // Simulate some work
            switchy_async::time::sleep(Duration::from_millis(100)).await;

            tx.commit().await.expect("Failed to commit transaction 1");
            std::time::Instant::now()
        });

        let tx2_task = switchy_async::task::spawn(async move {
            // Small delay to ensure tx1 starts first
            switchy_async::time::sleep(Duration::from_millis(10)).await;

            let tx = db2
                .begin_transaction()
                .await
                .expect("Failed to begin transaction 2");

            // Insert data in second transaction - may encounter database lock with concurrent access
            let insert_stmt = crate::query::insert("test_table")
                .value("name", DatabaseValue::String("tx2_data".to_string()))
                .value("value", DatabaseValue::Int64(2));

            // With connection pool, transactions may run concurrently and encounter locks
            match insert_stmt.execute(&*tx).await {
                Ok(_) => {
                    tx.commit().await.expect("Failed to commit transaction 2");
                }
                Err(_) => {
                    // Database lock is expected with concurrent transactions - rollback
                    let _ = tx.rollback().await;
                }
            }
            std::time::Instant::now()
        });

        let (tx1_end, tx2_end) = tokio::join!(tx1_task, tx2_task);
        let tx1_end = tx1_end.expect("Transaction 1 task failed");
        let tx2_end = tx2_end.expect("Transaction 2 task failed");

        let tx1_total_time = tx1_end.duration_since(start_time);
        let _tx2_total_time = tx2_end.duration_since(start_time);

        // The first transaction should have taken at least 100ms (the sleep time)
        assert!(
            tx1_total_time >= Duration::from_millis(100),
            "First transaction should complete normally, total time was {tx1_total_time:?}"
        );

        // With connection pool, tx2 can run concurrently and may complete faster
        // We just verify that at least one transaction completed successfully
        let select_stmt = crate::query::select("test_table");
        let rows = select_stmt
            .execute(db.as_ref())
            .await
            .expect("Failed to query after transactions");
        assert!(
            !rows.is_empty(),
            "At least one transaction should have inserted data (found {})",
            rows.len()
        );
    }

    #[switchy_async::test]
    async fn test_transaction_rollback() {
        let db = create_test_db();

        // Begin a transaction
        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        // Insert data within transaction
        let insert_stmt = crate::query::insert("test_table")
            .value("name", DatabaseValue::String("rollback_test".to_string()))
            .value("value", DatabaseValue::Int64(100));

        insert_stmt
            .execute(&*tx)
            .await
            .expect("Failed to insert in transaction");

        // Rollback the transaction
        tx.rollback().await.expect("Failed to rollback transaction");

        // Verify data was not committed
        let select_stmt = crate::query::select("test_table").filter(Box::new(where_eq(
            "name",
            DatabaseValue::String("rollback_test".to_string()),
        )));

        let rows = select_stmt
            .execute(&db)
            .await
            .expect("Failed to select after rollback");
        assert_eq!(rows.len(), 0);
    }

    #[switchy_async::test]
    async fn test_nested_transaction_rejection() {
        let db = create_test_db();
        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        // Try to begin nested transaction - should fail
        let result = tx.begin_transaction().await;
        assert!(matches!(result, Err(DatabaseError::AlreadyInTransaction)));

        tx.rollback().await.expect("Failed to rollback");
    }

    #[cfg(feature = "schema")]
    fn create_introspection_test_db() -> RusqliteDatabase {
        use rusqlite::Connection;
        use std::sync::Arc;
        use switchy_async::sync::Mutex;

        let test_id = std::thread::current().id();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_url = format!(
            "file:introspection_testdb_{test_id:?}_{timestamp}:?mode=memory&cache=shared&uri=true"
        );

        let mut connections = Vec::new();

        for i in 0..CONNECTION_POOL_SIZE {
            let conn = Connection::open(&db_url).expect("Failed to create shared memory database");

            if i == 0 {
                // Create a comprehensive test schema
                conn.execute(
                    "CREATE TABLE users (
                        id INTEGER PRIMARY KEY,
                        name TEXT NOT NULL,
                        email TEXT UNIQUE,
                        age INTEGER,
                        is_active BOOLEAN DEFAULT 1,
                        balance REAL DEFAULT 0.0,
                        created_at TEXT
                    )",
                    [],
                )
                .expect("Failed to create users table");

                conn.execute(
                    "CREATE TABLE posts (
                        id INTEGER PRIMARY KEY,
                        user_id INTEGER NOT NULL,
                        title TEXT NOT NULL,
                        content TEXT,
                        FOREIGN KEY (user_id) REFERENCES users (id)
                    )",
                    [],
                )
                .expect("Failed to create posts table");

                conn.execute("CREATE INDEX idx_users_email ON users (email)", [])
                    .expect("Failed to create index");

                conn.execute("CREATE UNIQUE INDEX idx_posts_title ON posts (title)", [])
                    .expect("Failed to create unique index");
            }

            connections.push(Arc::new(Mutex::new(conn)));
        }

        RusqliteDatabase::new(connections)
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_table_exists() {
        let db = create_introspection_test_db();

        // Test existing table
        let exists = db
            .table_exists("users")
            .await
            .expect("Failed to check table existence");
        assert!(exists, "users table should exist");

        // Test non-existing table
        let exists = db
            .table_exists("nonexistent")
            .await
            .expect("Failed to check table existence");
        assert!(!exists, "nonexistent table should not exist");

        // Test with transaction
        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");
        let exists = tx
            .table_exists("posts")
            .await
            .expect("Failed to check table existence in transaction");
        assert!(exists, "posts table should exist in transaction");
        tx.rollback().await.expect("Failed to rollback");
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_list_tables() {
        let db = create_introspection_test_db();

        // List all tables in the database
        let tables = db.list_tables().await.expect("Failed to list tables");

        // Should contain the test tables created in setup
        assert!(
            tables.contains(&"users".to_string()),
            "Should contain users table"
        );
        assert!(
            tables.contains(&"posts".to_string()),
            "Should contain posts table"
        );

        // Should not contain SQLite internal tables
        for table in &tables {
            assert!(
                !table.starts_with("sqlite_"),
                "Should not contain SQLite internal table: {table}"
            );
        }

        // Should have exactly 2 tables
        assert_eq!(tables.len(), 2, "Should have exactly 2 tables");

        // Test with transaction
        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        // Create a table in transaction
        tx.exec_raw("CREATE TABLE temp_table (id INTEGER)")
            .await
            .expect("Failed to create table in transaction");

        let tables_in_tx = tx
            .list_tables()
            .await
            .expect("Failed to list tables in transaction");

        // Should now contain 3 tables
        assert_eq!(tables_in_tx.len(), 3, "Should have 3 tables in transaction");
        assert!(tables_in_tx.contains(&"temp_table".to_string()));

        tx.rollback().await.expect("Failed to rollback");

        // After rollback, should be back to 2 tables
        let tables_after_rollback = db
            .list_tables()
            .await
            .expect("Failed to list tables after rollback");
        assert_eq!(
            tables_after_rollback.len(),
            2,
            "Should be back to 2 tables after rollback"
        );
        assert!(!tables_after_rollback.contains(&"temp_table".to_string()));
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_list_tables_empty_database() {
        // Create a fresh database without any tables
        let db = create_test_db(); // This creates a database with test_table

        // Drop the test table to make it empty
        db.exec_raw("DROP TABLE IF EXISTS test_table")
            .await
            .expect("Failed to drop test table");

        let tables = db.list_tables().await.expect("Failed to list tables");

        assert!(tables.is_empty(), "Empty database should have no tables");
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_list_tables_after_create_drop() {
        let db = create_test_db();

        // Drop the initial test table
        db.exec_raw("DROP TABLE IF EXISTS test_table")
            .await
            .expect("Failed to drop test table");

        // Initially should be empty
        let tables = db.list_tables().await.expect("Failed to list tables");
        assert!(tables.is_empty());

        // Create a table
        db.exec_raw("CREATE TABLE dynamic_table (id INTEGER, name TEXT)")
            .await
            .expect("Failed to create table");

        let tables = db.list_tables().await.expect("Failed to list tables");
        assert_eq!(tables.len(), 1);
        assert!(tables.contains(&"dynamic_table".to_string()));

        // Create another table
        db.exec_raw("CREATE TABLE another_table (value REAL)")
            .await
            .expect("Failed to create second table");

        let mut tables = db.list_tables().await.expect("Failed to list tables");
        tables.sort(); // Sort for deterministic comparison
        assert_eq!(tables, vec!["another_table", "dynamic_table"]);

        // Drop one table
        db.exec_raw("DROP TABLE dynamic_table")
            .await
            .expect("Failed to drop table");

        let tables = db.list_tables().await.expect("Failed to list tables");
        assert_eq!(tables.len(), 1);
        assert!(tables.contains(&"another_table".to_string()));
        assert!(!tables.contains(&"dynamic_table".to_string()));
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_column_exists() {
        let db = create_introspection_test_db();

        // Test existing column
        let exists = db
            .column_exists("users", "name")
            .await
            .expect("Failed to check column existence");
        assert!(exists, "name column should exist");

        // Test non-existing column
        let exists = db
            .column_exists("users", "nonexistent")
            .await
            .expect("Failed to check column existence");
        assert!(!exists, "nonexistent column should not exist");

        // Test non-existing table
        let result = db.column_exists("nonexistent_table", "name").await;
        // This should succeed but return false since table doesn't exist
        assert!(
            !result.unwrap(),
            "column in nonexistent table should not exist"
        );

        // Test with transaction
        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");
        let exists = tx
            .column_exists("posts", "title")
            .await
            .expect("Failed to check column existence in transaction");
        assert!(exists, "title column should exist in transaction");
        tx.rollback().await.expect("Failed to rollback");
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_get_table_columns() {
        let db = create_introspection_test_db();

        // Test users table columns
        let columns = db
            .get_table_columns("users")
            .await
            .expect("Failed to get table columns");

        assert!(!columns.is_empty(), "Should have columns");

        // Find specific columns and verify their properties
        let id_col = columns
            .iter()
            .find(|c| c.name == "id")
            .expect("id column should exist");
        assert_eq!(id_col.data_type, crate::schema::DataType::BigInt);
        // SQLite PRIMARY KEY columns are nullable unless explicitly NOT NULL
        assert!(
            id_col.nullable,
            "id should be nullable (SQLite PRIMARY KEY without NOT NULL)"
        );
        assert!(id_col.is_primary_key, "id should be primary key");

        let name_col = columns
            .iter()
            .find(|c| c.name == "name")
            .expect("name column should exist");
        assert_eq!(name_col.data_type, crate::schema::DataType::Text);
        assert!(!name_col.nullable, "name should not be nullable");
        assert!(!name_col.is_primary_key, "name should not be primary key");

        let is_active_col = columns
            .iter()
            .find(|c| c.name == "is_active")
            .expect("is_active column should exist");
        assert_eq!(is_active_col.data_type, crate::schema::DataType::Bool);
        assert!(is_active_col.nullable, "is_active should be nullable");
        assert!(
            !is_active_col.is_primary_key,
            "is_active should not be primary key"
        );
        // Note: Default value parsing is complex for SQLite, so we won't assert on it

        let balance_col = columns
            .iter()
            .find(|c| c.name == "balance")
            .expect("balance column should exist");
        assert_eq!(balance_col.data_type, crate::schema::DataType::Double);

        // Test with transaction
        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");
        let tx_columns = tx
            .get_table_columns("posts")
            .await
            .expect("Failed to get table columns in transaction");
        assert!(!tx_columns.is_empty(), "Should have columns in transaction");
        tx.rollback().await.expect("Failed to rollback");

        // Test non-existent table - should succeed but return empty vec
        let empty_columns = db
            .get_table_columns("nonexistent")
            .await
            .expect("Failed to get columns for nonexistent table");
        assert!(
            empty_columns.is_empty(),
            "Nonexistent table should have no columns"
        );
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_get_table_info() {
        let db = create_introspection_test_db();

        // Test existing table
        let table_info = db
            .get_table_info("users")
            .await
            .expect("Failed to get table info");
        assert!(table_info.is_some(), "users table info should exist");

        let info = table_info.unwrap();
        assert_eq!(info.name, "users");

        // Check columns
        assert!(!info.columns.is_empty(), "Should have columns");
        assert!(info.columns.contains_key("id"), "Should have id column");
        assert!(info.columns.contains_key("name"), "Should have name column");
        assert!(
            info.columns.contains_key("email"),
            "Should have email column"
        );

        // Check indexes (should include the email index we created)
        assert!(!info.indexes.is_empty(), "Should have indexes");
        let email_index = info
            .indexes
            .values()
            .find(|idx| idx.columns.contains(&"email".to_string()));
        assert!(email_index.is_some(), "Should have email index");

        // Test posts table with foreign key
        let posts_info = db
            .get_table_info("posts")
            .await
            .expect("Failed to get posts table info");
        assert!(posts_info.is_some(), "posts table info should exist");

        let posts = posts_info.unwrap();
        assert_eq!(posts.name, "posts");

        // Check foreign keys
        assert!(!posts.foreign_keys.is_empty(), "Should have foreign keys");
        let fk = posts
            .foreign_keys
            .values()
            .next()
            .expect("Should have at least one foreign key");
        assert_eq!(fk.referenced_table, "users");
        assert_eq!(fk.column, "user_id");
        assert_eq!(fk.referenced_column, "id");

        // Test non-existent table
        let no_info = db
            .get_table_info("nonexistent")
            .await
            .expect("Failed to get nonexistent table info");
        assert!(no_info.is_none(), "Nonexistent table should return None");

        // Test with transaction
        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");
        let tx_info = tx
            .get_table_info("users")
            .await
            .expect("Failed to get table info in transaction");
        assert!(tx_info.is_some(), "Should get table info in transaction");
        tx.rollback().await.expect("Failed to rollback");
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_unsupported_data_types() {
        use rusqlite::Connection;
        use std::sync::Arc;
        use switchy_async::sync::Mutex;

        let test_id = std::thread::current().id();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_url = format!(
            "file:unsupported_testdb_{test_id:?}_{timestamp}:?mode=memory&cache=shared&uri=true"
        );

        let mut connections = Vec::new();
        for i in 0..CONNECTION_POOL_SIZE {
            let conn = Connection::open(&db_url).expect("Failed to create shared memory database");

            if i == 0 {
                // Create table with unsupported type
                conn.execute(
                    "CREATE TABLE test_unsupported (
                        id INTEGER PRIMARY KEY,
                        data BLOB
                    )",
                    [],
                )
                .expect("Failed to create test table");
            }

            connections.push(Arc::new(Mutex::new(conn)));
        }

        let db = RusqliteDatabase::new(connections);

        // This should now succeed and return a Custom DataType
        let result = db.get_table_columns("test_unsupported").await;
        assert!(
            result.is_ok(),
            "Should succeed with Custom DataType fallback"
        );

        let columns = result.unwrap();
        assert_eq!(columns.len(), 2, "Should have 2 columns");

        // Find the BLOB column
        let blob_column = columns.iter().find(|col| col.name == "data").unwrap();
        match &blob_column.data_type {
            crate::schema::DataType::Blob => {
                // BLOB is now a supported type, should work fine
            }
            other => panic!("Expected Blob DataType, got: {other:?}"),
        }
    }

    #[switchy_async::test]
    async fn test_savepoint_basic() {
        let connection = Arc::new(Mutex::new(Connection::open_in_memory().unwrap()));
        let transaction = RusqliteTransaction::new(Arc::clone(&connection));

        let savepoint = transaction.savepoint("test_sp").await.unwrap();
        assert_eq!(savepoint.name(), "test_sp");

        // Release should work
        savepoint.release().await.unwrap();
    }

    #[switchy_async::test]
    async fn test_savepoint_release() {
        let connection = Arc::new(Mutex::new(Connection::open_in_memory().unwrap()));
        let transaction = RusqliteTransaction::new(Arc::clone(&connection));

        let savepoint = transaction.savepoint("test_release").await.unwrap();
        savepoint.release().await.unwrap();
    }

    #[switchy_async::test]
    async fn test_savepoint_rollback() {
        let connection = Arc::new(Mutex::new(Connection::open_in_memory().unwrap()));
        let transaction = RusqliteTransaction::new(Arc::clone(&connection));

        let savepoint = transaction.savepoint("test_rollback").await.unwrap();
        savepoint.rollback_to().await.unwrap();
    }

    #[switchy_async::test]
    async fn test_savepoint_name_validation() {
        let connection = Arc::new(Mutex::new(Connection::open_in_memory().unwrap()));
        let transaction = RusqliteTransaction::new(Arc::clone(&connection));

        // Empty name should fail
        let result = transaction.savepoint("").await;
        assert!(result.is_err());

        // Invalid characters should fail
        let result = transaction.savepoint("test;drop").await;
        assert!(result.is_err());

        // Starting with number should fail
        let result = transaction.savepoint("1invalid").await;
        assert!(result.is_err());
    }
}
