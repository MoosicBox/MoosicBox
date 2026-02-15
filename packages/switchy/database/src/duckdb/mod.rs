//! `DuckDB` database backend
//!
//! This module provides `DuckDB` database support using the `duckdb` crate for synchronous
//! `DuckDB` access wrapped in async interfaces. `DuckDB` is an in-process analytical database
//! optimised for OLAP workloads.
//!
//! # Connection Pool Architecture
//!
//! This implementation uses a connection pool to enable concurrent operations:
//! - Configurable number of connections per database instance (default 5)
//! - Round-robin connection selection
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

use ::duckdb::{Connection, types::Value};
use async_trait::async_trait;
use std::fmt::Write as _;
use switchy_async::sync::Mutex;
use thiserror::Error;

use crate::{
    Database, DatabaseError, DatabaseTransaction, DatabaseValue, DeleteStatement, InsertStatement,
    SelectQuery, UpdateStatement, UpsertMultiStatement, UpsertStatement,
    query::{BooleanExpression, Expression, ExpressionType, Join, Sort, SortDirection},
    query_transform::{QuestionMarkHandler, transform_query_for_params},
    sql_interval::SqlInterval,
};

/// Format `SqlInterval` as a `DuckDB` `INTERVAL` literal string.
///
/// Produces `PostgreSQL`-compatible interval strings like `'1 year 2 days 3 hours'`
/// which `DuckDB` accepts directly.
fn format_duckdb_interval(interval: &SqlInterval) -> String {
    let mut parts = Vec::new();

    if interval.years != 0 {
        parts.push(format!(
            "{} year{}",
            interval.years,
            if interval.years.abs() == 1 { "" } else { "s" }
        ));
    }
    if interval.months != 0 {
        parts.push(format!(
            "{} month{}",
            interval.months,
            if interval.months.abs() == 1 { "" } else { "s" }
        ));
    }
    if interval.days != 0 {
        parts.push(format!(
            "{} day{}",
            interval.days,
            if interval.days.abs() == 1 { "" } else { "s" }
        ));
    }
    if interval.hours != 0 {
        parts.push(format!(
            "{} hour{}",
            interval.hours,
            if interval.hours.abs() == 1 { "" } else { "s" }
        ));
    }
    if interval.minutes != 0 {
        parts.push(format!(
            "{} minute{}",
            interval.minutes,
            if interval.minutes.abs() == 1 { "" } else { "s" }
        ));
    }
    if interval.seconds != 0 || interval.nanos != 0 {
        if interval.nanos == 0 {
            parts.push(format!(
                "{} second{}",
                interval.seconds,
                if interval.seconds.abs() == 1 { "" } else { "s" }
            ));
        } else {
            #[allow(clippy::cast_precision_loss)]
            let fractional =
                interval.seconds as f64 + (f64::from(interval.nanos) / 1_000_000_000.0);
            parts.push(format!("{fractional} seconds"));
        }
    }

    if parts.is_empty() {
        "0 seconds".to_string()
    } else {
        parts.join(" ")
    }
}

/// `DuckDB` database connection pool
///
/// Manages a pool of `DuckDB` connections using round-robin selection for distributing
/// queries across multiple connections. Each connection is protected by a mutex for
/// thread-safe access.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct DuckDbDatabase {
    connections: Vec<Arc<Mutex<Connection>>>,
    next_connection: AtomicUsize,
}

impl DuckDbDatabase {
    /// Creates a new `DuckDB` database instance from a vector of connections
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

/// `DuckDB` database transaction
///
/// Represents an active transaction on a `DuckDB` connection. Provides ACID guarantees
/// for a series of database operations. Must be explicitly committed or rolled back.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct DuckDbTransaction {
    connection: Arc<Mutex<Connection>>,
    committed: AtomicBool,
    rolled_back: AtomicBool,
}

impl DuckDbTransaction {
    /// Creates a new `DuckDB` transaction from a connection
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
                "COALESCE({})",
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
                DatabaseValue::Now => "NOW()::TIMESTAMP".to_string(),
                DatabaseValue::NowPlus(interval) => {
                    let interval_str = format_duckdb_interval(interval);
                    format!("(NOW()::TIMESTAMP + INTERVAL '{interval_str}')")
                }
                _ => "?".to_string(),
            },
        }
    }
}

/// Errors specific to `DuckDB` database operations
///
/// Wraps errors from the underlying `duckdb` driver plus additional error types
/// for query validation and result handling.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Error)]
pub enum DuckDbDatabaseError {
    /// Error from the underlying `duckdb` driver
    #[error(transparent)]
    DuckDb(#[from] ::duckdb::Error),
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

impl From<DuckDbDatabaseError> for DatabaseError {
    fn from(value: DuckDbDatabaseError) -> Self {
        Self::DuckDb(value)
    }
}

impl From<Value> for DatabaseValue {
    fn from(value: Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::Boolean(v) => Self::Bool(v),
            Value::TinyInt(v) => Self::Int8(v),
            Value::SmallInt(v) => Self::Int16(v),
            Value::Int(v) => Self::Int32(v),
            Value::BigInt(v) => Self::Int64(v),
            Value::UTinyInt(v) => Self::UInt8(v),
            Value::USmallInt(v) => Self::UInt16(v),
            Value::UInt(v) => Self::UInt32(v),
            Value::UBigInt(v) => Self::UInt64(v),
            Value::Float(v) => Self::Real32(v),
            Value::Double(v) => Self::Real64(v),
            Value::Text(v) | Value::Enum(v) => Self::String(v),
            #[allow(clippy::cast_possible_truncation)]
            Value::HugeInt(v) => Self::Int64(v as i64),
            Value::Decimal(v) => {
                // Lossy: convert to f64
                use std::str::FromStr as _;
                Self::Real64(f64::from_str(&v.to_string()).unwrap_or(0.0))
            }
            Value::Timestamp(_, micros) => {
                // Microseconds since epoch -> NaiveDateTime
                let secs = micros / 1_000_000;
                let nsecs = u32::try_from((micros % 1_000_000) * 1000).unwrap_or(0);
                chrono::DateTime::from_timestamp(secs, nsecs)
                    .map_or(Self::Null, |dt| Self::DateTime(dt.naive_utc()))
            }
            Value::Date32(days) => {
                // Days since epoch -> NaiveDateTime at midnight
                chrono::NaiveDate::from_num_days_from_ce_opt(days + 719_163)
                    .map_or(Self::Null, |d| {
                        Self::DateTime(d.and_hms_opt(0, 0, 0).unwrap_or_default())
                    })
            }
            Value::Time64(_, micros) => {
                // Lossy: time-only value stored as string
                let secs = micros / 1_000_000;
                let mins = secs / 60;
                let hours = mins / 60;
                Self::String(format!("{:02}:{:02}:{:02}", hours, mins % 60, secs % 60))
            }
            Value::Blob(v) => {
                log::warn!("Lossy conversion: DuckDB Blob -> String (hex-encoded)");
                let mut hex = String::with_capacity(v.len() * 2);
                for b in &v {
                    write!(hex, "{b:02x}").unwrap();
                }
                Self::String(hex)
            }
            Value::Interval {
                months,
                days,
                nanos,
            } => {
                log::warn!("Lossy conversion: DuckDB Interval -> String");
                Self::String(format!("{months} months {days} days {nanos} nanoseconds"))
            }
            Value::List(values) => {
                log::warn!("Lossy conversion: DuckDB List -> String (JSON-like)");
                let items: Vec<String> = values.into_iter().map(|v| format!("{v:?}")).collect();
                Self::String(format!("[{}]", items.join(", ")))
            }
            Value::Array(values) => {
                log::warn!("Lossy conversion: DuckDB Array -> String (JSON-like)");
                let items: Vec<String> = values.into_iter().map(|v| format!("{v:?}")).collect();
                Self::String(format!("[{}]", items.join(", ")))
            }
            Value::Struct(map) => {
                log::warn!("Lossy conversion: DuckDB Struct -> String (JSON-like)");
                let items: Vec<String> = map.iter().map(|(k, v)| format!("{k}: {v:?}")).collect();
                Self::String(format!("{{{}}}", items.join(", ")))
            }
            Value::Map(map) => {
                log::warn!("Lossy conversion: DuckDB Map -> String (JSON-like)");
                let items: Vec<String> = map.iter().map(|(k, v)| format!("{k:?}: {v:?}")).collect();
                Self::String(format!("{{{}}}", items.join(", ")))
            }
            Value::Union(inner) => {
                log::warn!("Lossy conversion: DuckDB Union -> inner value");
                (*inner).into()
            }
        }
    }
}

fn from_row(
    column_names: &[String],
    row: &::duckdb::Row<'_>,
) -> Result<crate::Row, DuckDbDatabaseError> {
    let mut columns = vec![];

    for (i, column) in column_names.iter().enumerate() {
        let value: Value = row.get(i).map_err(DuckDbDatabaseError::DuckDb)?;
        columns.push((column.clone(), value.into()));
    }

    Ok(crate::Row { columns })
}

fn to_rows(
    column_names: &[String],
    rows: &mut ::duckdb::Rows<'_>,
) -> Result<Vec<crate::Row>, DuckDbDatabaseError> {
    let mut results = vec![];

    while let Some(row) = rows.next().map_err(DuckDbDatabaseError::DuckDb)? {
        results.push(from_row(column_names, row)?);
    }

    log::trace!(
        "Got {} row{}",
        results.len(),
        if results.len() == 1 { "" } else { "s" }
    );

    Ok(results)
}

// ---------------------------------------------------------------------------
// Query builder helpers
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Parameter binding
// ---------------------------------------------------------------------------

/// Bind `DatabaseValue` params to a `DuckDB` prepared statement.
///
/// When `bind_nulls` is `false` (query-builder path), null-like variants are
/// skipped because the query builder already inlined `NULL` into the SQL text
/// and emitted no `?` placeholder for them.
///
/// When `bind_nulls` is `true` (raw-params path), null-like variants are bound
/// as SQL NULL so that the bind index stays aligned with the caller's `?`
/// placeholders.
#[allow(clippy::too_many_lines)]
fn bind_values_inner(
    statement: &mut ::duckdb::Statement<'_>,
    values: Option<&[DuckDbDatabaseValue]>,
    constant_inc: bool,
    bind_nulls: bool,
    offset: usize,
) -> Result<usize, DuckDbDatabaseError> {
    if let Some(values) = values {
        let mut i = 1 + offset;
        for value in values {
            match &**value {
                DatabaseValue::Now | DatabaseValue::NowPlus(..) => (),
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
                    if bind_nulls {
                        statement
                            .raw_bind_parameter(i, Option::<i32>::None)
                            .map_err(DuckDbDatabaseError::DuckDb)?;
                        if !constant_inc {
                            i += 1;
                        }
                    }
                }
                #[cfg(feature = "decimal")]
                DatabaseValue::DecimalOpt(None) => {
                    if bind_nulls {
                        statement
                            .raw_bind_parameter(i, Option::<i32>::None)
                            .map_err(DuckDbDatabaseError::DuckDb)?;
                        if !constant_inc {
                            i += 1;
                        }
                    }
                }
                #[cfg(feature = "uuid")]
                DatabaseValue::UuidOpt(None) => {
                    if bind_nulls {
                        statement
                            .raw_bind_parameter(i, Option::<i32>::None)
                            .map_err(DuckDbDatabaseError::DuckDb)?;
                        if !constant_inc {
                            i += 1;
                        }
                    }
                }
                DatabaseValue::Bool(v) | DatabaseValue::BoolOpt(Some(v)) => {
                    statement
                        .raw_bind_parameter(i, *v)
                        .map_err(DuckDbDatabaseError::DuckDb)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::String(v) | DatabaseValue::StringOpt(Some(v)) => {
                    statement
                        .raw_bind_parameter(i, v.as_str())
                        .map_err(DuckDbDatabaseError::DuckDb)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::Int8(v) | DatabaseValue::Int8Opt(Some(v)) => {
                    statement
                        .raw_bind_parameter(i, *v)
                        .map_err(DuckDbDatabaseError::DuckDb)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::Int16(v) | DatabaseValue::Int16Opt(Some(v)) => {
                    statement
                        .raw_bind_parameter(i, *v)
                        .map_err(DuckDbDatabaseError::DuckDb)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::Int32(v) | DatabaseValue::Int32Opt(Some(v)) => {
                    statement
                        .raw_bind_parameter(i, *v)
                        .map_err(DuckDbDatabaseError::DuckDb)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::Int64(v) | DatabaseValue::Int64Opt(Some(v)) => {
                    statement
                        .raw_bind_parameter(i, *v)
                        .map_err(DuckDbDatabaseError::DuckDb)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::UInt8(v) | DatabaseValue::UInt8Opt(Some(v)) => {
                    statement
                        .raw_bind_parameter(i, *v)
                        .map_err(DuckDbDatabaseError::DuckDb)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::UInt16(v) | DatabaseValue::UInt16Opt(Some(v)) => {
                    statement
                        .raw_bind_parameter(i, *v)
                        .map_err(DuckDbDatabaseError::DuckDb)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::UInt32(v) | DatabaseValue::UInt32Opt(Some(v)) => {
                    statement
                        .raw_bind_parameter(i, *v)
                        .map_err(DuckDbDatabaseError::DuckDb)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::UInt64(v) | DatabaseValue::UInt64Opt(Some(v)) => {
                    statement
                        .raw_bind_parameter(i, *v)
                        .map_err(DuckDbDatabaseError::DuckDb)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::Real64(v) | DatabaseValue::Real64Opt(Some(v)) => {
                    statement
                        .raw_bind_parameter(i, *v)
                        .map_err(DuckDbDatabaseError::DuckDb)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::Real32(v) | DatabaseValue::Real32Opt(Some(v)) => {
                    statement
                        .raw_bind_parameter(i, f64::from(*v))
                        .map_err(DuckDbDatabaseError::DuckDb)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                #[cfg(feature = "decimal")]
                DatabaseValue::Decimal(v) | DatabaseValue::DecimalOpt(Some(v)) => {
                    statement
                        .raw_bind_parameter(i, v.to_string())
                        .map_err(DuckDbDatabaseError::DuckDb)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                #[cfg(feature = "uuid")]
                DatabaseValue::Uuid(v) | DatabaseValue::UuidOpt(Some(v)) => {
                    statement
                        .raw_bind_parameter(i, v.to_string())
                        .map_err(DuckDbDatabaseError::DuckDb)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::DateTime(v) => {
                    statement
                        .raw_bind_parameter(i, v.to_string())
                        .map_err(DuckDbDatabaseError::DuckDb)?;
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

fn bind_values(
    statement: &mut ::duckdb::Statement<'_>,
    values: Option<&[DuckDbDatabaseValue]>,
    constant_inc: bool,
    offset: usize,
) -> Result<usize, DuckDbDatabaseError> {
    bind_values_inner(statement, values, constant_inc, false, offset)
}

fn bind_values_raw(
    statement: &mut ::duckdb::Statement<'_>,
    values: Option<&[DuckDbDatabaseValue]>,
    offset: usize,
) -> Result<usize, DuckDbDatabaseError> {
    bind_values_inner(statement, values, false, true, offset)
}

// ---------------------------------------------------------------------------
// Value conversion helpers
// ---------------------------------------------------------------------------

fn exprs_to_values(values: &[(&str, Box<dyn Expression>)]) -> Vec<DuckDbDatabaseValue> {
    values
        .iter()
        .flat_map(|value| value.1.values().into_iter())
        .flatten()
        .cloned()
        .map(std::convert::Into::into)
        .collect::<Vec<_>>()
}

fn bexprs_to_values(values: &[Box<dyn BooleanExpression>]) -> Vec<DuckDbDatabaseValue> {
    values
        .iter()
        .flat_map(|value| value.values().into_iter())
        .flatten()
        .cloned()
        .map(std::convert::Into::into)
        .collect::<Vec<_>>()
}

fn bexprs_to_values_opt(
    values: Option<&[Box<dyn BooleanExpression>]>,
) -> Option<Vec<DuckDbDatabaseValue>> {
    values.map(bexprs_to_values)
}

// ---------------------------------------------------------------------------
// Core query operations
// ---------------------------------------------------------------------------

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
) -> Result<Vec<crate::Row>, DuckDbDatabaseError> {
    let query = format!(
        "SELECT {} {} FROM {table_name} {} {} {} {}",
        if distinct { "DISTINCT" } else { "" },
        columns.join(", "),
        build_join_clauses(joins),
        build_where_clause(filters),
        build_sort_clause(sort),
        limit.map_or_else(String::new, |limit| format!("LIMIT {limit}"))
    );

    log::trace!("Running select query: {query}");

    let mut statement = connection
        .prepare_cached(&query)
        .map_err(DuckDbDatabaseError::DuckDb)?;

    bind_values(
        &mut statement,
        bexprs_to_values_opt(filters).as_deref(),
        false,
        0,
    )?;

    statement
        .raw_execute()
        .map_err(DuckDbDatabaseError::DuckDb)?;
    let column_names = statement.column_names();

    to_rows(&column_names, &mut statement.raw_query())
}

fn find_row(
    connection: &Connection,
    table_name: &str,
    distinct: bool,
    columns: &[&str],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    joins: Option<&[Join]>,
    sort: Option<&[Sort]>,
) -> Result<Option<crate::Row>, DuckDbDatabaseError> {
    let query = format!(
        "SELECT {} {} FROM {table_name} {} {} {} LIMIT 1",
        if distinct { "DISTINCT" } else { "" },
        columns.join(", "),
        build_join_clauses(joins),
        build_where_clause(filters),
        build_sort_clause(sort),
    );

    let mut statement = connection
        .prepare_cached(&query)
        .map_err(DuckDbDatabaseError::DuckDb)?;

    bind_values(
        &mut statement,
        bexprs_to_values_opt(filters).as_deref(),
        false,
        0,
    )?;

    statement
        .raw_execute()
        .map_err(DuckDbDatabaseError::DuckDb)?;
    let column_names = statement.column_names();

    let mut query = statement.raw_query();
    query
        .next()
        .map_err(DuckDbDatabaseError::DuckDb)?
        .map(|row| from_row(&column_names, row))
        .transpose()
}

fn insert_and_get_row(
    connection: &Connection,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
) -> Result<crate::Row, DuckDbDatabaseError> {
    let column_names = values
        .iter()
        .map(|(key, _v)| format!("\"{key}\""))
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

    let mut statement = connection
        .prepare_cached(&query)
        .map_err(DuckDbDatabaseError::DuckDb)?;

    bind_values(&mut statement, Some(&exprs_to_values(values)), false, 0)?;

    log::trace!("Running insert_and_get_row query: {query}");

    statement
        .raw_execute()
        .map_err(DuckDbDatabaseError::DuckDb)?;
    let column_names = statement.column_names();

    let mut query = statement.raw_query();
    query
        .next()
        .map_err(DuckDbDatabaseError::DuckDb)?
        .map(|row| from_row(&column_names, row))
        .ok_or(DuckDbDatabaseError::NoRow)?
}

fn update_and_get_row(
    connection: &Connection,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Option<crate::Row>, DuckDbDatabaseError> {
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

    let mut statement = connection
        .prepare_cached(&query)
        .map_err(DuckDbDatabaseError::DuckDb)?;
    bind_values(&mut statement, Some(&all_values), false, 0)?;

    statement
        .raw_execute()
        .map_err(DuckDbDatabaseError::DuckDb)?;
    let column_names = statement.column_names();

    let mut query = statement.raw_query();
    query
        .next()
        .map_err(DuckDbDatabaseError::DuckDb)?
        .map(|row| from_row(&column_names, row))
        .transpose()
}

fn update_and_get_rows(
    connection: &Connection,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, DuckDbDatabaseError> {
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

    let mut statement = connection
        .prepare_cached(&query)
        .map_err(DuckDbDatabaseError::DuckDb)?;
    bind_values(&mut statement, Some(&all_values), false, 0)?;

    statement
        .raw_execute()
        .map_err(DuckDbDatabaseError::DuckDb)?;
    let column_names = statement.column_names();

    to_rows(&column_names, &mut statement.raw_query())
}

fn delete(
    connection: &Connection,
    table_name: &str,
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, DuckDbDatabaseError> {
    let where_clause = build_where_clause(filters);

    let filter_values: Vec<DuckDbDatabaseValue> = filters
        .map(|filters| {
            filters
                .iter()
                .flat_map(|value| value.params().unwrap_or_default().into_iter().cloned())
                .map(std::convert::Into::into)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let limit_clause = limit.map_or_else(String::new, |l| format!(" LIMIT {l}"));

    // DuckDB's RETURNING clause results are consumed by raw_execute() and not
    // retrievable via raw_query(). Work around this by SELECTing the matching
    // rows first, then executing the DELETE.
    //
    // NOTE: This two-step SELECT-then-DELETE is not atomic. It is safe in the
    // current design because each DuckDB connection is behind an Arc<Mutex<>>,
    // so no concurrent operation can modify the table between the SELECT and
    // DELETE on the same connection. For a file-backed DuckDB with multiple
    // connections, callers should wrap this in a transaction to avoid races.
    let returning_query = format!("SELECT * FROM {table_name} {where_clause}{limit_clause}");

    log::trace!("Running delete: selecting rows first with: {returning_query}");

    let mut select_stmt = connection
        .prepare(&returning_query)
        .map_err(DuckDbDatabaseError::DuckDb)?;
    bind_values(&mut select_stmt, Some(&filter_values), false, 0)?;
    select_stmt
        .raw_execute()
        .map_err(DuckDbDatabaseError::DuckDb)?;
    let column_names = select_stmt.column_names();
    let rows_to_return = to_rows(&column_names, &mut select_stmt.raw_query())?;

    // Build the DELETE query using rowid-based subquery for limit, or simple
    // WHERE for unlimited deletes.
    let delete_query = if limit.is_some() {
        format!(
            "DELETE FROM {table_name} WHERE rowid IN (SELECT rowid FROM {table_name} {where_clause}{limit_clause})"
        )
    } else {
        format!("DELETE FROM {table_name} {where_clause}")
    };

    log::trace!("Running delete query: {delete_query}");

    let mut delete_stmt = connection
        .prepare(&delete_query)
        .map_err(DuckDbDatabaseError::DuckDb)?;

    bind_values(&mut delete_stmt, Some(&filter_values), false, 0)?;
    delete_stmt
        .raw_execute()
        .map_err(DuckDbDatabaseError::DuckDb)?;

    Ok(rows_to_return)
}

fn upsert(
    connection: &Connection,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, DuckDbDatabaseError> {
    let rows = update_and_get_rows(connection, table_name, values, filters, limit)?;

    Ok(if rows.is_empty() {
        vec![insert_and_get_row(connection, table_name, values)?]
    } else {
        rows
    })
}

fn upsert_and_get_row(
    connection: &Connection,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<crate::Row, DuckDbDatabaseError> {
    match find_row(connection, table_name, false, &["*"], filters, None, None)? {
        Some(_) => {
            let updated =
                update_and_get_row(connection, table_name, values, filters, limit)?.unwrap();
            Ok(updated)
        }
        None => insert_and_get_row(connection, table_name, values),
    }
}

fn upsert_multi(
    connection: &Connection,
    table_name: &str,
    unique: &[Box<dyn Expression>],
    values: &[Vec<(&str, Box<dyn Expression>)>],
) -> Result<Vec<crate::Row>, DuckDbDatabaseError> {
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
) -> Result<Vec<crate::Row>, DuckDbDatabaseError> {
    let first = values[0].as_slice();
    let expected_value_size = first.len();

    if let Some(bad_row) = values.iter().skip(1).find(|v| {
        v.len() != expected_value_size || v.iter().enumerate().any(|(i, c)| c.0 != first[i].0)
    }) {
        log::error!("Bad row: {bad_row:?}. Expected to match schema of first row: {first:?}");
        return Err(DuckDbDatabaseError::InvalidRequest);
    }

    let set_clause = values[0]
        .iter()
        .map(|(name, _value)| format!("\"{name}\" = EXCLUDED.\"{name}\""))
        .collect::<Vec<_>>()
        .join(", ");

    let column_names = values[0]
        .iter()
        .map(|(key, _v)| format!("\"{key}\""))
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

    let mut statement = connection
        .prepare_cached(&query)
        .map_err(DuckDbDatabaseError::DuckDb)?;

    bind_values(&mut statement, Some(all_values), true, 0)?;

    statement
        .raw_execute()
        .map_err(DuckDbDatabaseError::DuckDb)?;
    let column_names = statement.column_names();

    to_rows(&column_names, &mut statement.raw_query())
}

// ---------------------------------------------------------------------------
// Now/NowPlus parameter transformation
// ---------------------------------------------------------------------------

fn duckdb_transform_query_for_params(
    query: &str,
    params: &[DatabaseValue],
) -> Result<(String, Vec<DatabaseValue>), DatabaseError> {
    transform_query_for_params(query, params, &QuestionMarkHandler, |param| match param {
        DatabaseValue::Now => Some("NOW()::TIMESTAMP".to_string()),
        DatabaseValue::NowPlus(interval) => {
            let interval_str = format_duckdb_interval(interval);
            Some(format!("(NOW()::TIMESTAMP + INTERVAL '{interval_str}')"))
        }
        _ => None,
    })
    .map_err(DatabaseError::QueryFailed)
}

// ---------------------------------------------------------------------------
// Schema helpers
// ---------------------------------------------------------------------------

#[cfg(feature = "schema")]
fn duckdb_data_type_to_sql(data_type: &crate::schema::DataType) -> String {
    use crate::schema::DataType;

    match data_type {
        DataType::Text
        | DataType::Json
        | DataType::Jsonb
        | DataType::Xml
        | DataType::Inet
        | DataType::MacAddr => "TEXT".to_string(),
        DataType::VarChar(n) => format!("VARCHAR({n})"),
        DataType::Char(n) => format!("CHAR({n})"),
        DataType::TinyInt => "TINYINT".to_string(),
        DataType::SmallInt => "SMALLINT".to_string(),
        DataType::Int | DataType::Serial => "INTEGER".to_string(),
        DataType::BigInt | DataType::BigSerial => "BIGINT".to_string(),
        DataType::Real => "REAL".to_string(),
        DataType::Double => "DOUBLE".to_string(),
        DataType::Decimal(p, s) => format!("DECIMAL({p},{s})"),
        DataType::Money => "DECIMAL(19,4)".to_string(),
        DataType::Bool => "BOOLEAN".to_string(),
        DataType::Date => "DATE".to_string(),
        DataType::Time => "TIME".to_string(),
        DataType::DateTime | DataType::Timestamp => "TIMESTAMP".to_string(),
        DataType::Blob | DataType::Binary(None) => "BLOB".to_string(),
        DataType::Binary(Some(n)) => format!("BLOB({n})"),
        DataType::Uuid => "UUID".to_string(),
        DataType::Array(inner) => format!("{}[]", duckdb_data_type_to_sql(inner)),
        DataType::Custom(s) => s.clone(),
    }
}

#[cfg(feature = "schema")]
fn duckdb_default_value_sql(value: &DatabaseValue) -> String {
    match value {
        DatabaseValue::Bool(v) | DatabaseValue::BoolOpt(Some(v)) => {
            if *v { "TRUE" } else { "FALSE" }.to_string()
        }
        DatabaseValue::String(v) | DatabaseValue::StringOpt(Some(v)) => {
            format!("'{}'", v.replace('\'', "''"))
        }
        DatabaseValue::Int8(v) | DatabaseValue::Int8Opt(Some(v)) => v.to_string(),
        DatabaseValue::Int16(v) | DatabaseValue::Int16Opt(Some(v)) => v.to_string(),
        DatabaseValue::Int32(v) | DatabaseValue::Int32Opt(Some(v)) => v.to_string(),
        DatabaseValue::Int64(v) | DatabaseValue::Int64Opt(Some(v)) => v.to_string(),
        DatabaseValue::UInt8(v) | DatabaseValue::UInt8Opt(Some(v)) => v.to_string(),
        DatabaseValue::UInt16(v) | DatabaseValue::UInt16Opt(Some(v)) => v.to_string(),
        DatabaseValue::UInt32(v) | DatabaseValue::UInt32Opt(Some(v)) => v.to_string(),
        DatabaseValue::UInt64(v) | DatabaseValue::UInt64Opt(Some(v)) => v.to_string(),
        DatabaseValue::Real32(v) | DatabaseValue::Real32Opt(Some(v)) => v.to_string(),
        DatabaseValue::Real64(v) | DatabaseValue::Real64Opt(Some(v)) => v.to_string(),
        DatabaseValue::Now => "NOW()::TIMESTAMP".to_string(),
        DatabaseValue::NowPlus(interval) => {
            let interval_str = format_duckdb_interval(interval);
            format!("(NOW()::TIMESTAMP + INTERVAL '{interval_str}')")
        }
        DatabaseValue::DateTime(v) => format!("'{v}'"),
        _ => "NULL".to_string(),
    }
}

#[cfg(feature = "schema")]
fn build_create_table_sql(statement: &crate::schema::CreateTableStatement<'_>) -> String {
    let mut sql = String::new();

    // Create sequences for auto-increment columns first
    for col in &statement.columns {
        if col.auto_increment {
            write!(
                sql,
                "CREATE SEQUENCE IF NOT EXISTS \"{table}_{col}_seq\" START 1; ",
                table = statement.table_name,
                col = col.name
            )
            .unwrap();
        }
    }

    sql.push_str("CREATE TABLE ");

    if statement.if_not_exists {
        sql.push_str("IF NOT EXISTS ");
    }
    write!(sql, "\"{}\" (", statement.table_name).unwrap();

    let mut col_defs = Vec::new();
    for col in &statement.columns {
        let mut def = format!(
            "\"{}\" {}",
            col.name,
            duckdb_data_type_to_sql(&col.data_type)
        );
        if col.auto_increment {
            write!(
                def,
                " DEFAULT nextval('\"{table}_{col}_seq\"')",
                table = statement.table_name,
                col = col.name
            )
            .unwrap();
        }
        if !col.nullable {
            def.push_str(" NOT NULL");
        }
        if !col.auto_increment
            && let Some(default) = &col.default
        {
            write!(def, " DEFAULT {}", duckdb_default_value_sql(default)).unwrap();
        }
        col_defs.push(def);
    }

    if let Some(pk) = statement.primary_key {
        col_defs.push(format!("PRIMARY KEY (\"{pk}\")"));
    }

    for (col, ref_table) in &statement.foreign_keys {
        col_defs.push(format!("FOREIGN KEY (\"{col}\") REFERENCES {ref_table}"));
    }

    sql.push_str(&col_defs.join(", "));
    sql.push(')');
    sql
}

#[cfg(feature = "schema")]
fn build_drop_table_sql(statement: &crate::schema::DropTableStatement<'_>) -> String {
    let mut sql = String::from("DROP TABLE ");
    if statement.if_exists {
        sql.push_str("IF EXISTS ");
    }
    write!(sql, "\"{}\"", statement.table_name).unwrap();

    #[cfg(feature = "cascade")]
    {
        use crate::schema::DropBehavior;
        match statement.behavior {
            DropBehavior::Cascade => sql.push_str(" CASCADE"),
            DropBehavior::Restrict => sql.push_str(" RESTRICT"),
            DropBehavior::Default => {}
        }
    }

    sql
}

#[cfg(feature = "schema")]
fn build_create_index_sql(statement: &crate::schema::CreateIndexStatement<'_>) -> String {
    let mut sql = String::from("CREATE ");
    if statement.unique {
        sql.push_str("UNIQUE ");
    }
    sql.push_str("INDEX ");
    if statement.if_not_exists {
        sql.push_str("IF NOT EXISTS ");
    }
    write!(
        sql,
        "\"{}\" ON \"{}\" ({})",
        statement.index_name,
        statement.table_name,
        statement
            .columns
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ")
    )
    .unwrap();
    sql
}

#[cfg(feature = "schema")]
fn build_drop_index_sql(statement: &crate::schema::DropIndexStatement<'_>) -> String {
    let mut sql = String::from("DROP INDEX ");
    if statement.if_exists {
        sql.push_str("IF EXISTS ");
    }
    write!(sql, "\"{}\"", statement.index_name).unwrap();
    sql
}

#[cfg(feature = "schema")]
fn build_alter_table_sqls(statement: &crate::schema::AlterTableStatement<'_>) -> Vec<String> {
    use crate::schema::AlterOperation;

    let mut sqls = Vec::new();

    for op in &statement.operations {
        let sql = match op {
            AlterOperation::AddColumn {
                name,
                data_type,
                nullable,
                default,
            } => {
                let mut s = format!(
                    "ALTER TABLE \"{}\" ADD COLUMN \"{}\" {}",
                    statement.table_name,
                    name,
                    duckdb_data_type_to_sql(data_type)
                );
                if !nullable {
                    s.push_str(" NOT NULL");
                }
                if let Some(default) = default {
                    write!(s, " DEFAULT {}", duckdb_default_value_sql(default)).unwrap();
                }
                s
            }
            AlterOperation::DropColumn { name, .. } => {
                format!(
                    "ALTER TABLE \"{}\" DROP COLUMN \"{}\"",
                    statement.table_name, name
                )
            }
            AlterOperation::RenameColumn { old_name, new_name } => {
                format!(
                    "ALTER TABLE \"{}\" RENAME COLUMN \"{}\" TO \"{}\"",
                    statement.table_name, old_name, new_name
                )
            }
            AlterOperation::ModifyColumn {
                name,
                new_data_type,
                new_nullable,
                new_default,
            } => {
                let mut parts = vec![format!(
                    "ALTER TABLE \"{}\" ALTER COLUMN \"{}\" SET DATA TYPE {}",
                    statement.table_name,
                    name,
                    duckdb_data_type_to_sql(new_data_type)
                )];
                if let Some(nullable) = new_nullable {
                    if *nullable {
                        parts.push(format!(
                            "ALTER TABLE \"{}\" ALTER COLUMN \"{}\" DROP NOT NULL",
                            statement.table_name, name
                        ));
                    } else {
                        parts.push(format!(
                            "ALTER TABLE \"{}\" ALTER COLUMN \"{}\" SET NOT NULL",
                            statement.table_name, name
                        ));
                    }
                }
                if let Some(default) = new_default {
                    parts.push(format!(
                        "ALTER TABLE \"{}\" ALTER COLUMN \"{}\" SET DEFAULT {}",
                        statement.table_name,
                        name,
                        duckdb_default_value_sql(default)
                    ));
                }
                sqls.extend(parts);
                continue;
            }
        };
        sqls.push(sql);
    }

    sqls
}

#[cfg(feature = "schema")]
#[allow(clippy::cast_possible_truncation)]
fn duckdb_type_str_to_data_type(type_str: &str) -> crate::schema::DataType {
    use crate::schema::DataType;

    let upper = type_str.to_uppercase();
    let upper = upper.trim();

    match upper {
        "BOOLEAN" | "BOOL" => DataType::Bool,
        "TINYINT" | "INT1" => DataType::TinyInt,
        "SMALLINT" | "INT2" | "SHORT" => DataType::SmallInt,
        "INTEGER" | "INT" | "INT4" | "SIGNED" => DataType::Int,
        "BIGINT" | "INT8" | "LONG" => DataType::BigInt,
        "REAL" | "FLOAT" | "FLOAT4" => DataType::Real,
        "DOUBLE" | "FLOAT8" => DataType::Double,
        "TEXT" | "STRING" => DataType::Text,
        "BLOB" | "BYTEA" | "BINARY" | "VARBINARY" => DataType::Blob,
        "DATE" => DataType::Date,
        "TIME" => DataType::Time,
        "TIMESTAMP" | "DATETIME" | "TIMESTAMP WITH TIME ZONE" | "TIMESTAMPTZ" => {
            DataType::Timestamp
        }
        "UUID" => DataType::Uuid,
        "JSON" => DataType::Json,
        _ => {
            if upper.starts_with("VARCHAR") || upper.starts_with("TEXT(") {
                extract_type_param(upper).map_or(DataType::Text, |n| DataType::VarChar(n as u16))
            } else if upper.starts_with("CHAR(") {
                extract_type_param(upper).map_or(DataType::Text, |n| DataType::Char(n as u16))
            } else if upper.starts_with("DECIMAL") || upper.starts_with("NUMERIC") {
                extract_two_type_params(upper).map_or(DataType::Decimal(18, 3), |(p, s)| {
                    DataType::Decimal(p as u8, s as u8)
                })
            } else if upper.ends_with("[]") {
                let inner = upper.trim_end_matches("[]");
                DataType::Array(Box::new(duckdb_type_str_to_data_type(inner)))
            } else {
                DataType::Custom(type_str.to_string())
            }
        }
    }
}

#[cfg(feature = "schema")]
fn extract_type_param(s: &str) -> Option<usize> {
    let start = s.find('(')?;
    let end = s.find(')')?;
    s[start + 1..end].trim().parse().ok()
}

#[cfg(feature = "schema")]
fn extract_two_type_params(s: &str) -> Option<(usize, usize)> {
    let start = s.find('(')?;
    let end = s.find(')')?;
    let inner = &s[start + 1..end];
    let mut parts = inner.split(',');
    let p = parts.next()?.trim().parse().ok()?;
    let s = parts.next()?.trim().parse().ok()?;
    Some((p, s))
}

#[cfg(feature = "schema")]
fn exec_schema_ddl(connection: &Connection, sql: &str) -> Result<(), DuckDbDatabaseError> {
    log::trace!("exec_schema_ddl: {sql}");
    connection
        .execute_batch(sql)
        .map_err(DuckDbDatabaseError::DuckDb)
}

/// Extract column names from a `CREATE INDEX` SQL statement.
///
/// Parses column names from the parenthesised column list in statements like
/// `CREATE INDEX idx ON tbl ("col1", "col2")`.
#[cfg(feature = "schema")]
fn extract_index_columns_from_sql(sql: &str) -> Vec<String> {
    // Find the last '(' ... ')' which contains the column list
    if let (Some(start), Some(end)) = (sql.rfind('('), sql.rfind(')'))
        && start < end
    {
        return sql[start + 1..end]
            .split(',')
            .map(|s| s.trim().trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    Vec::new()
}

// ---------------------------------------------------------------------------
// Shared introspection helpers (used by both DuckDbDatabase and DuckDbTransaction)
// ---------------------------------------------------------------------------

/// Query primary key columns for a table using `duckdb_constraints()`.
#[cfg(feature = "schema")]
fn query_primary_key_columns(
    conn: &Connection,
    table_name: &str,
) -> Result<std::collections::BTreeSet<String>, DuckDbDatabaseError> {
    let mut pk_cols = std::collections::BTreeSet::new();
    // Use unnest() to flatten the constraint_column_names list into individual rows
    let mut stmt = conn
        .prepare(
            "SELECT unnest(constraint_column_names) AS col_name \
             FROM duckdb_constraints() \
             WHERE table_name = ? AND schema_name = 'main' \
             AND constraint_type = 'PRIMARY KEY'",
        )
        .map_err(DuckDbDatabaseError::DuckDb)?;
    let rows = stmt
        .query_map([table_name], |row| row.get::<_, String>(0))
        .map_err(DuckDbDatabaseError::DuckDb)?;
    for row in rows {
        let col_name = row.map_err(DuckDbDatabaseError::DuckDb)?;
        pk_cols.insert(col_name);
    }
    Ok(pk_cols)
}

/// Query foreign key constraints for a table using `duckdb_constraints()`.
#[cfg(feature = "schema")]
fn query_foreign_keys(
    conn: &Connection,
    table_name: &str,
) -> Result<std::collections::BTreeMap<String, crate::schema::ForeignKeyInfo>, DuckDbDatabaseError>
{
    let mut fk_map = std::collections::BTreeMap::new();
    // DuckDB's duckdb_constraints() returns FK info with constraint_column_names
    // and referenced table info encoded in expression. We need to query the
    // information_schema for more structured FK data.
    let mut stmt = conn
        .prepare(
            "SELECT kcu.column_name, \
                    ccu.table_name AS referenced_table, \
                    ccu.column_name AS referenced_column, \
                    rc.update_rule, \
                    rc.delete_rule, \
                    tc.constraint_name \
             FROM information_schema.table_constraints tc \
             JOIN information_schema.key_column_usage kcu \
                 ON tc.constraint_name = kcu.constraint_name \
                 AND tc.table_schema = kcu.table_schema \
             JOIN information_schema.referential_constraints rc \
                 ON tc.constraint_name = rc.constraint_name \
                 AND tc.table_schema = rc.constraint_schema \
             JOIN information_schema.key_column_usage ccu \
                 ON rc.unique_constraint_name = ccu.constraint_name \
                 AND rc.unique_constraint_schema = ccu.constraint_schema \
             WHERE tc.table_name = ? AND tc.table_schema = 'main' \
                 AND tc.constraint_type = 'FOREIGN KEY'",
        )
        .map_err(DuckDbDatabaseError::DuckDb)?;
    let rows = stmt
        .query_map([table_name], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(DuckDbDatabaseError::DuckDb)?;
    for row in rows {
        let (column, referenced_table, referenced_column, on_update, on_delete, constraint_name) =
            row.map_err(DuckDbDatabaseError::DuckDb)?;
        fk_map.insert(
            constraint_name.clone(),
            crate::schema::ForeignKeyInfo {
                name: constraint_name,
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
    Ok(fk_map)
}

/// Get column info with primary key and auto-increment detection.
#[cfg(feature = "schema")]
fn duckdb_get_table_columns(
    conn: &Connection,
    table_name: &str,
) -> Result<Vec<crate::schema::ColumnInfo>, DuckDbDatabaseError> {
    let pk_cols = query_primary_key_columns(conn, table_name)?;

    let mut stmt = conn
        .prepare(
            "SELECT column_name, data_type, is_nullable, column_default, ordinal_position \
             FROM information_schema.columns \
             WHERE table_name = ? AND table_schema = 'main' \
             ORDER BY ordinal_position",
        )
        .map_err(DuckDbDatabaseError::DuckDb)?;
    let rows = stmt
        .query_map([table_name], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i32>(4)?,
            ))
        })
        .map_err(DuckDbDatabaseError::DuckDb)?;

    let mut columns = Vec::new();
    for row in rows {
        let (name, type_str, nullable_str, default_str, ordinal) =
            row.map_err(DuckDbDatabaseError::DuckDb)?;

        // Detect auto-increment from nextval() default pattern
        let auto_increment = default_str.as_ref().is_some_and(|d| d.contains("nextval("));

        columns.push(crate::schema::ColumnInfo {
            name: name.clone(),
            data_type: duckdb_type_str_to_data_type(&type_str),
            nullable: nullable_str == "YES",
            is_primary_key: pk_cols.contains(&name),
            auto_increment,
            default_value: default_str.map(DatabaseValue::String),
            #[allow(clippy::cast_sign_loss)]
            ordinal_position: ordinal as u32,
        });
    }

    Ok(columns)
}

/// Get complete table info including columns, indexes, and foreign keys.
#[cfg(feature = "schema")]
fn duckdb_get_table_info(
    conn: &Connection,
    table_name: &str,
) -> Result<crate::schema::TableInfo, DuckDbDatabaseError> {
    let columns = duckdb_get_table_columns(conn, table_name)?;
    let pk_cols = query_primary_key_columns(conn, table_name)?;
    let mut col_map = std::collections::BTreeMap::new();
    for col in columns {
        col_map.insert(col.name.clone(), col);
    }

    // Indexes via duckdb_indexes()
    let mut idx_map = std::collections::BTreeMap::new();
    let mut stmt = conn
        .prepare(
            "SELECT index_name, is_unique, sql FROM duckdb_indexes() \
             WHERE table_name = ? AND schema_name = 'main'",
        )
        .map_err(DuckDbDatabaseError::DuckDb)?;
    let idx_rows = stmt
        .query_map([table_name], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, bool>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(DuckDbDatabaseError::DuckDb)?;
    for row in idx_rows {
        let (name, unique, sql) = row.map_err(DuckDbDatabaseError::DuckDb)?;
        let idx_columns = extract_index_columns_from_sql(&sql);
        // Detect primary index by checking if its columns exactly match PK columns
        let is_primary = !idx_columns.is_empty()
            && idx_columns.len() == pk_cols.len()
            && idx_columns.iter().all(|c| pk_cols.contains(c));
        idx_map.insert(
            name.clone(),
            crate::schema::IndexInfo {
                name,
                unique,
                columns: idx_columns,
                is_primary,
            },
        );
    }

    // Foreign keys
    let foreign_keys = query_foreign_keys(conn, table_name)?;

    Ok(crate::schema::TableInfo {
        name: table_name.to_string(),
        columns: col_map,
        indexes: idx_map,
        foreign_keys,
    })
}

// ---------------------------------------------------------------------------
// Database trait implementations
// ---------------------------------------------------------------------------

#[async_trait]
impl Database for DuckDbDatabase {
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
                .ok_or(DuckDbDatabaseError::MissingUnique)?,
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
            .map_err(DuckDbDatabaseError::DuckDb)?;
        Ok(())
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn query_raw(&self, query: &str) -> Result<Vec<crate::Row>, DatabaseError> {
        let connection = self.get_connection();
        let connection = connection.lock().await;

        let mut stmt = connection
            .prepare(query)
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let mut rows = stmt
            .query([])
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let column_names: Vec<String> = rows
            .as_ref()
            .map(::duckdb::Statement::column_names)
            .unwrap_or_default();

        to_rows(&column_names, &mut rows).map_err(|e| DatabaseError::QueryFailed(e.to_string()))
    }

    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn crate::DatabaseTransaction>, DatabaseError> {
        let connection = self.get_connection();

        connection
            .lock()
            .await
            .execute_batch("BEGIN TRANSACTION")
            .map_err(DuckDbDatabaseError::DuckDb)?;

        Ok(Box::new(DuckDbTransaction::new(connection)))
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn exec_raw_params(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<u64, DatabaseError> {
        let (transformed_query, filtered_params) =
            duckdb_transform_query_for_params(query, params)?;

        let connection = self.get_connection();
        let connection_guard = connection.lock().await;

        let mut stmt = connection_guard
            .prepare(&transformed_query)
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let duckdb_params: Vec<DuckDbDatabaseValue> =
            filtered_params.iter().map(|p| p.clone().into()).collect();

        log::trace!(
            "\
            exec_raw_params: query:\n\
            '{transformed_query}' (transformed from '{query}')\n\
            params: {params:?}\n\
            filtered: {filtered_params:?}\n\
            raw: {duckdb_params:?}\
            "
        );

        bind_values_raw(&mut stmt, Some(&duckdb_params), 0)
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
        let (transformed_query, filtered_params) =
            duckdb_transform_query_for_params(query, params)?;

        let connection = self.get_connection();
        let connection_guard = connection.lock().await;

        let mut stmt = connection_guard
            .prepare(&transformed_query)
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let duckdb_params: Vec<DuckDbDatabaseValue> =
            filtered_params.iter().map(|p| p.clone().into()).collect();

        log::trace!(
            "\
            query_raw_params: query:\n\
            '{transformed_query}' (transformed from '{query}')\n\
            params: {params:?}\n\
            filtered: {filtered_params:?}\n\
            raw: {duckdb_params:?}\
            "
        );

        bind_values_raw(&mut stmt, Some(&duckdb_params), 0)
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        stmt.raw_execute()
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
        let column_names = stmt.column_names();

        to_rows(&column_names, &mut stmt.raw_query())
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))
    }

    #[cfg(feature = "schema")]
    async fn exec_create_table(
        &self,
        statement: &crate::schema::CreateTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let connection = self.get_connection();
        Ok(exec_schema_ddl(
            &*connection.lock().await,
            &build_create_table_sql(statement),
        )?)
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_table(
        &self,
        statement: &crate::schema::DropTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let connection = self.get_connection();
        Ok(exec_schema_ddl(
            &*connection.lock().await,
            &build_drop_table_sql(statement),
        )?)
    }

    #[cfg(feature = "schema")]
    async fn exec_create_index(
        &self,
        statement: &crate::schema::CreateIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let connection = self.get_connection();
        Ok(exec_schema_ddl(
            &*connection.lock().await,
            &build_create_index_sql(statement),
        )?)
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_index(
        &self,
        statement: &crate::schema::DropIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let connection = self.get_connection();
        Ok(exec_schema_ddl(
            &*connection.lock().await,
            &build_drop_index_sql(statement),
        )?)
    }

    #[cfg(feature = "schema")]
    async fn exec_alter_table(
        &self,
        statement: &crate::schema::AlterTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let connection = self.get_connection();
        let conn = connection.lock().await;
        for sql in build_alter_table_sqls(statement) {
            exec_schema_ddl(&conn, &sql)?;
        }
        Ok(())
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::significant_drop_tightening)]
    async fn table_exists(&self, table_name: &str) -> Result<bool, DatabaseError> {
        let connection = self.get_connection();
        let conn = connection.lock().await;
        let mut stmt = conn
            .prepare(
                "SELECT COUNT(*) FROM information_schema.tables \
                 WHERE table_name = ? AND table_schema = 'main'",
            )
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
        let count: i64 = stmt
            .query_row([table_name], |row| row.get(0))
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
        Ok(count > 0)
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::significant_drop_tightening)]
    async fn list_tables(&self) -> Result<Vec<String>, DatabaseError> {
        let connection = self.get_connection();
        let conn = connection.lock().await;
        let mut stmt = conn
            .prepare(
                "SELECT table_name FROM information_schema.tables \
                 WHERE table_schema = 'main' ORDER BY table_name",
            )
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
        let mut tables = Vec::new();
        for row in rows {
            tables.push(row.map_err(|e| DatabaseError::QueryFailed(e.to_string()))?);
        }
        Ok(tables)
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::significant_drop_tightening)]
    async fn get_table_info(
        &self,
        table_name: &str,
    ) -> Result<Option<crate::schema::TableInfo>, DatabaseError> {
        if !self.table_exists(table_name).await? {
            return Ok(None);
        }
        let connection = self.get_connection();
        let conn = connection.lock().await;
        Ok(Some(duckdb_get_table_info(&conn, table_name)?))
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::significant_drop_tightening)]
    async fn get_table_columns(
        &self,
        table_name: &str,
    ) -> Result<Vec<crate::schema::ColumnInfo>, DatabaseError> {
        let connection = self.get_connection();
        let conn = connection.lock().await;
        Ok(duckdb_get_table_columns(&conn, table_name)?)
    }

    #[cfg(feature = "schema")]
    async fn column_exists(
        &self,
        table_name: &str,
        column_name: &str,
    ) -> Result<bool, DatabaseError> {
        let columns = self.get_table_columns(table_name).await?;
        Ok(columns.iter().any(|c| c.name == column_name))
    }
}

// ---------------------------------------------------------------------------
// Database trait impl for transaction
// ---------------------------------------------------------------------------

#[async_trait]
impl Database for DuckDbTransaction {
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
        Ok(upsert_multi(
            &*self.connection.lock().await,
            statement.table_name,
            statement
                .unique
                .as_ref()
                .ok_or(DuckDbDatabaseError::MissingUnique)?,
            &statement.values,
        )?)
    }

    async fn exec_raw(&self, statement: &str) -> Result<(), DatabaseError> {
        self.connection
            .lock()
            .await
            .execute_batch(statement)
            .map_err(DuckDbDatabaseError::DuckDb)?;
        Ok(())
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn query_raw(&self, query: &str) -> Result<Vec<crate::Row>, DatabaseError> {
        let connection = self.connection.lock().await;

        let mut stmt = connection
            .prepare(query)
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let mut rows = stmt
            .query([])
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let column_names: Vec<String> = rows
            .as_ref()
            .map(::duckdb::Statement::column_names)
            .unwrap_or_default();

        to_rows(&column_names, &mut rows).map_err(|e| DatabaseError::QueryFailed(e.to_string()))
    }

    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn crate::DatabaseTransaction>, DatabaseError> {
        Err(DatabaseError::AlreadyInTransaction)
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn exec_raw_params(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<u64, DatabaseError> {
        let (transformed_query, filtered_params) =
            duckdb_transform_query_for_params(query, params)?;

        let connection_guard = self.connection.lock().await;

        let mut stmt = connection_guard
            .prepare(&transformed_query)
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let duckdb_params: Vec<DuckDbDatabaseValue> =
            filtered_params.iter().map(|p| p.clone().into()).collect();

        bind_values_raw(&mut stmt, Some(&duckdb_params), 0)
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
        let (transformed_query, filtered_params) =
            duckdb_transform_query_for_params(query, params)?;

        let connection_guard = self.connection.lock().await;

        let mut stmt = connection_guard
            .prepare(&transformed_query)
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let duckdb_params: Vec<DuckDbDatabaseValue> =
            filtered_params.iter().map(|p| p.clone().into()).collect();

        bind_values_raw(&mut stmt, Some(&duckdb_params), 0)
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        stmt.raw_execute()
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
        let column_names = stmt.column_names();

        to_rows(&column_names, &mut stmt.raw_query())
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))
    }

    #[cfg(feature = "schema")]
    async fn exec_create_table(
        &self,
        statement: &crate::schema::CreateTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        Ok(exec_schema_ddl(
            &*self.connection.lock().await,
            &build_create_table_sql(statement),
        )?)
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_table(
        &self,
        statement: &crate::schema::DropTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        Ok(exec_schema_ddl(
            &*self.connection.lock().await,
            &build_drop_table_sql(statement),
        )?)
    }

    #[cfg(feature = "schema")]
    async fn exec_create_index(
        &self,
        statement: &crate::schema::CreateIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        Ok(exec_schema_ddl(
            &*self.connection.lock().await,
            &build_create_index_sql(statement),
        )?)
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_index(
        &self,
        statement: &crate::schema::DropIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        Ok(exec_schema_ddl(
            &*self.connection.lock().await,
            &build_drop_index_sql(statement),
        )?)
    }

    #[cfg(feature = "schema")]
    async fn exec_alter_table(
        &self,
        statement: &crate::schema::AlterTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let conn = self.connection.lock().await;
        for sql in build_alter_table_sqls(statement) {
            exec_schema_ddl(&conn, &sql)?;
        }
        Ok(())
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::significant_drop_tightening)]
    async fn table_exists(&self, table_name: &str) -> Result<bool, DatabaseError> {
        let conn = self.connection.lock().await;
        let mut stmt = conn
            .prepare(
                "SELECT COUNT(*) FROM information_schema.tables \
                 WHERE table_name = ? AND table_schema = 'main'",
            )
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
        let count: i64 = stmt
            .query_row([table_name], |row| row.get(0))
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
        Ok(count > 0)
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::significant_drop_tightening)]
    async fn list_tables(&self) -> Result<Vec<String>, DatabaseError> {
        let conn = self.connection.lock().await;
        let mut stmt = conn
            .prepare(
                "SELECT table_name FROM information_schema.tables \
                 WHERE table_schema = 'main' ORDER BY table_name",
            )
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
        let mut tables = Vec::new();
        for row in rows {
            tables.push(row.map_err(|e| DatabaseError::QueryFailed(e.to_string()))?);
        }
        Ok(tables)
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::significant_drop_tightening)]
    async fn get_table_info(
        &self,
        table_name: &str,
    ) -> Result<Option<crate::schema::TableInfo>, DatabaseError> {
        if !self.table_exists(table_name).await? {
            return Ok(None);
        }
        let conn = self.connection.lock().await;
        Ok(Some(duckdb_get_table_info(&conn, table_name)?))
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::significant_drop_tightening)]
    async fn get_table_columns(
        &self,
        table_name: &str,
    ) -> Result<Vec<crate::schema::ColumnInfo>, DatabaseError> {
        let conn = self.connection.lock().await;
        Ok(duckdb_get_table_columns(&conn, table_name)?)
    }

    #[cfg(feature = "schema")]
    async fn column_exists(
        &self,
        table_name: &str,
        column_name: &str,
    ) -> Result<bool, DatabaseError> {
        let columns = self.get_table_columns(table_name).await?;
        Ok(columns.iter().any(|c| c.name == column_name))
    }
}

// ---------------------------------------------------------------------------
// DatabaseTransaction trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl DatabaseTransaction for DuckDbTransaction {
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
            .execute_batch("COMMIT")
            .map_err(DuckDbDatabaseError::DuckDb)?;

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
            .execute_batch("ROLLBACK")
            .map_err(DuckDbDatabaseError::DuckDb)?;

        self.rolled_back.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn savepoint(&self, _name: &str) -> Result<Box<dyn crate::Savepoint>, DatabaseError> {
        Err(DatabaseError::UnsupportedOperation(
            "DuckDB does not support savepoints".to_string(),
        ))
    }

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

            let tables = self.list_tables().await?;
            for check_table in &tables {
                if check_table == &current_table {
                    continue;
                }
                // Check if check_table has a FK referencing current_table
                let conn = self.connection.lock().await;
                let fk_map = query_foreign_keys(&conn, check_table)?;
                drop(conn);

                for fk_info in fk_map.values() {
                    if fk_info.referenced_table == current_table {
                        all_dependents.insert(check_table.clone());
                        to_check.push(check_table.clone());
                        break;
                    }
                }
            }
        }

        let mut drop_order: Vec<String> = all_dependents.into_iter().collect();
        drop_order.push(table_name.to_string());

        Ok(crate::schema::DropPlan::Simple(drop_order))
    }

    #[cfg(feature = "cascade")]
    async fn has_any_dependents(&self, table_name: &str) -> Result<bool, DatabaseError> {
        let tables = self.list_tables().await?;
        for check_table in &tables {
            if check_table == table_name {
                continue;
            }
            let conn = self.connection.lock().await;
            let fk_map = query_foreign_keys(&conn, check_table)?;
            drop(conn);

            for fk_info in fk_map.values() {
                if fk_info.referenced_table == table_name {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    #[cfg(feature = "cascade")]
    async fn get_direct_dependents(
        &self,
        table_name: &str,
    ) -> Result<std::collections::BTreeSet<String>, DatabaseError> {
        let mut dependents = std::collections::BTreeSet::new();
        let tables = self.list_tables().await?;
        for check_table in &tables {
            if check_table == table_name {
                continue;
            }
            let conn = self.connection.lock().await;
            let fk_map = query_foreign_keys(&conn, check_table)?;
            drop(conn);

            for fk_info in fk_map.values() {
                if fk_info.referenced_table == table_name {
                    dependents.insert(check_table.clone());
                    break;
                }
            }
        }
        Ok(dependents)
    }
}

// ---------------------------------------------------------------------------
// DuckDbDatabaseValue wrapper
// ---------------------------------------------------------------------------

/// Wrapper type for converting `DatabaseValue` to `DuckDB`-specific parameter types
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone)]
pub struct DuckDbDatabaseValue(DatabaseValue);

impl From<DatabaseValue> for DuckDbDatabaseValue {
    fn from(value: DatabaseValue) -> Self {
        Self(value)
    }
}

impl Deref for DuckDbDatabaseValue {
    type Target = DatabaseValue;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Expression for DuckDbDatabaseValue {
    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        Some(vec![self])
    }

    fn is_null(&self) -> bool {
        matches!(
            self.0,
            DatabaseValue::Null
                | DatabaseValue::BoolOpt(None)
                | DatabaseValue::Int8Opt(None)
                | DatabaseValue::Int16Opt(None)
                | DatabaseValue::Int32Opt(None)
                | DatabaseValue::Int64Opt(None)
                | DatabaseValue::UInt8Opt(None)
                | DatabaseValue::UInt16Opt(None)
                | DatabaseValue::UInt32Opt(None)
                | DatabaseValue::UInt64Opt(None)
                | DatabaseValue::Real32Opt(None)
                | DatabaseValue::Real64Opt(None)
                | DatabaseValue::StringOpt(None)
        )
    }

    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::DatabaseValue(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DatabaseValue;
    use crate::query::{FilterableQuery, where_eq};
    use std::sync::Arc;
    use switchy_async::sync::Mutex;

    const CONNECTION_POOL_SIZE: u8 = 5;

    fn create_test_db() -> DuckDbDatabase {
        // DuckDB in-memory databases are NOT shared across connections (unlike
        // SQLite with cache=shared). To work around this we open one connection,
        // create the schema, and wrap that single connection for the pool.
        // This means the pool has only 1 real connection, which is fine for
        // tests.
        let conn =
            Connection::open_in_memory().expect("Failed to create in-memory DuckDB database");

        conn.execute_batch(
            "CREATE SEQUENCE test_table_id_seq START 1; \
             CREATE TABLE test_table (id INTEGER PRIMARY KEY DEFAULT nextval('test_table_id_seq'), name TEXT, value INTEGER)",
        )
        .expect("Failed to create test table");

        let shared = Arc::new(Mutex::new(conn));
        let mut connections = Vec::new();
        for _ in 0..CONNECTION_POOL_SIZE {
            connections.push(Arc::clone(&shared));
        }

        DuckDbDatabase::new(connections)
    }

    #[switchy_async::test]
    async fn test_basic_transaction_commit() {
        let db = create_test_db();

        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        let insert_stmt = crate::query::insert("test_table")
            .value("name", DatabaseValue::String("test_name".to_string()))
            .value("value", DatabaseValue::Int64(42));

        insert_stmt
            .execute(&*tx)
            .await
            .expect("Failed to insert in transaction");

        tx.commit().await.expect("Failed to commit transaction");

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
        assert_eq!(rows[0].get("value"), Some(DatabaseValue::Int32(42)));
    }

    #[switchy_async::test]
    async fn test_basic_transaction_rollback() {
        let db = create_test_db();

        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        let insert_stmt = crate::query::insert("test_table")
            .value("name", DatabaseValue::String("rollback_data".to_string()))
            .value("value", DatabaseValue::Int64(999));

        insert_stmt
            .execute(&*tx)
            .await
            .expect("Failed to insert in transaction");

        tx.rollback().await.expect("Failed to rollback transaction");

        let select_stmt = crate::query::select("test_table").filter(Box::new(where_eq(
            "name",
            DatabaseValue::String("rollback_data".to_string()),
        )));

        let rows = select_stmt
            .execute(&db)
            .await
            .expect("Failed to select after rollback");
        assert_eq!(rows.len(), 0, "Rolled back data should not be visible");
    }

    #[switchy_async::test]
    async fn test_nested_transaction_rejected() {
        let db = create_test_db();

        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        let result = tx.begin_transaction().await;
        assert!(
            matches!(result, Err(DatabaseError::AlreadyInTransaction)),
            "Expected AlreadyInTransaction error"
        );

        tx.rollback().await.expect("Failed to rollback transaction");
    }

    #[switchy_async::test]
    async fn test_exec_raw() {
        let db = create_test_db();

        db.exec_raw("INSERT INTO test_table (id, name, value) VALUES (1, 'raw_test', 100)")
            .await
            .expect("Failed to exec_raw");

        let rows = db
            .query_raw("SELECT * FROM test_table WHERE name = 'raw_test'")
            .await
            .expect("Failed to query_raw");

        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].get("name"),
            Some(DatabaseValue::String("raw_test".to_string()))
        );
    }

    #[switchy_async::test]
    async fn test_exec_raw_params() {
        let db = create_test_db();

        db.exec_raw_params(
            "INSERT INTO test_table (id, name, value) VALUES (?, ?, ?)",
            &[
                DatabaseValue::Int32(10),
                DatabaseValue::String("param_test".to_string()),
                DatabaseValue::Int32(200),
            ],
        )
        .await
        .expect("Failed to exec_raw_params");

        let rows = db
            .query_raw_params(
                "SELECT * FROM test_table WHERE name = ?",
                &[DatabaseValue::String("param_test".to_string())],
            )
            .await
            .expect("Failed to query_raw_params");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("value"), Some(DatabaseValue::Int32(200)));
    }

    #[switchy_async::test]
    async fn test_savepoint() {
        let db = create_test_db();

        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        // DuckDB does not support savepoints
        let result = tx.savepoint("sp1").await;
        assert!(
            matches!(result, Err(DatabaseError::UnsupportedOperation(_))),
            "Expected UnsupportedOperation error for savepoints"
        );

        tx.rollback().await.expect("Failed to rollback transaction");
    }

    #[switchy_async::test]
    async fn test_transaction_isolation() {
        // DuckDB uses a single shared connection wrapped in Arc<Mutex>, so
        // true isolation between connections is not testable with in-memory.
        // Instead we verify that uncommitted data from a transaction is visible
        // within the transaction and committed data is visible after commit.
        let db = create_test_db();

        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        let insert_stmt = crate::query::insert("test_table")
            .value("name", DatabaseValue::String("isolated".to_string()))
            .value("value", DatabaseValue::Int64(77));
        insert_stmt
            .execute(&*tx)
            .await
            .expect("Failed to insert in transaction");

        // Data should be visible inside the transaction
        let rows_in_tx = tx
            .query_raw("SELECT * FROM test_table WHERE name = 'isolated'")
            .await
            .expect("Failed to query inside transaction");
        assert_eq!(rows_in_tx.len(), 1, "Data should be visible inside tx");

        tx.commit().await.expect("Failed to commit");

        // Data should be visible after commit
        let rows_after = db
            .query_raw("SELECT * FROM test_table WHERE name = 'isolated'")
            .await
            .expect("Failed to query after commit");
        assert_eq!(
            rows_after.len(),
            1,
            "Committed data should be visible after commit"
        );
    }

    #[switchy_async::test]
    async fn test_query_builder_select() {
        let db = create_test_db();

        db.exec_raw("INSERT INTO test_table (id, name, value) VALUES (1, 'alice', 10)")
            .await
            .expect("Failed to insert");
        db.exec_raw("INSERT INTO test_table (id, name, value) VALUES (2, 'bob', 20)")
            .await
            .expect("Failed to insert");
        db.exec_raw("INSERT INTO test_table (id, name, value) VALUES (3, 'charlie', 30)")
            .await
            .expect("Failed to insert");

        // Select with filter
        let select_stmt = crate::query::select("test_table")
            .columns(&["name", "value"])
            .filter(Box::new(crate::query::where_eq(
                "name",
                DatabaseValue::String("bob".to_string()),
            )));

        let rows = select_stmt.execute(&db).await.expect("Failed to select");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].get("name"),
            Some(DatabaseValue::String("bob".to_string()))
        );
        assert_eq!(rows[0].get("value"), Some(DatabaseValue::Int32(20)));
    }

    #[switchy_async::test]
    async fn test_query_builder_update() {
        let db = create_test_db();

        db.exec_raw("INSERT INTO test_table (id, name, value) VALUES (1, 'alice', 10)")
            .await
            .expect("Failed to insert");

        let update_stmt = crate::query::update("test_table")
            .value("value", DatabaseValue::Int64(99))
            .filter(Box::new(crate::query::where_eq(
                "name",
                DatabaseValue::String("alice".to_string()),
            )));

        let rows = update_stmt.execute(&db).await.expect("Failed to update");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("value"), Some(DatabaseValue::Int32(99)));
    }

    #[switchy_async::test]
    async fn test_query_builder_delete() {
        let db = create_test_db();

        db.exec_raw("INSERT INTO test_table (id, name, value) VALUES (1, 'alice', 10)")
            .await
            .expect("Failed to insert");
        db.exec_raw("INSERT INTO test_table (id, name, value) VALUES (2, 'bob', 20)")
            .await
            .expect("Failed to insert");

        let delete_stmt = crate::query::delete("test_table").filter(Box::new(
            crate::query::where_eq("name", DatabaseValue::String("alice".to_string())),
        ));

        let deleted = delete_stmt.execute(&db).await.expect("Failed to delete");
        assert_eq!(deleted.len(), 1);
        assert_eq!(
            deleted[0].get("name"),
            Some(DatabaseValue::String("alice".to_string()))
        );

        let remaining = db
            .query_raw("SELECT * FROM test_table")
            .await
            .expect("Failed to query");
        assert_eq!(remaining.len(), 1);
        assert_eq!(
            remaining[0].get("name"),
            Some(DatabaseValue::String("bob".to_string()))
        );
    }

    #[switchy_async::test]
    async fn test_query_builder_upsert() {
        let db = create_test_db();

        // Insert via upsert (no existing row)
        let upsert_stmt = crate::query::upsert("test_table")
            .value("name", DatabaseValue::String("dave".to_string()))
            .value("value", DatabaseValue::Int64(50))
            .filter(Box::new(crate::query::where_eq(
                "name",
                DatabaseValue::String("dave".to_string()),
            )));

        let rows = upsert_stmt
            .execute(&db)
            .await
            .expect("Failed to upsert (insert path)");
        assert_eq!(rows.len(), 1);

        // Update via upsert (existing row)
        let upsert_stmt2 = crate::query::upsert("test_table")
            .value("value", DatabaseValue::Int64(99))
            .filter(Box::new(crate::query::where_eq(
                "name",
                DatabaseValue::String("dave".to_string()),
            )));

        let rows2 = upsert_stmt2
            .execute(&db)
            .await
            .expect("Failed to upsert (update path)");
        assert_eq!(rows2.len(), 1);
        assert_eq!(rows2[0].get("value"), Some(DatabaseValue::Int32(99)));
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_schema_table_exists() {
        let db = create_test_db();

        assert!(
            db.table_exists("test_table").await.unwrap(),
            "test_table should exist"
        );
        assert!(
            !db.table_exists("nonexistent").await.unwrap(),
            "nonexistent should not exist"
        );
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_schema_list_tables() {
        let db = create_test_db();

        let tables = db.list_tables().await.unwrap();
        assert!(
            tables.contains(&"test_table".to_string()),
            "list_tables should include test_table, got: {tables:?}"
        );
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_schema_column_exists() {
        let db = create_test_db();

        assert!(db.column_exists("test_table", "id").await.unwrap());
        assert!(db.column_exists("test_table", "name").await.unwrap());
        assert!(db.column_exists("test_table", "value").await.unwrap());
        assert!(!db.column_exists("test_table", "nonexistent").await.unwrap());
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_schema_get_table_columns() {
        let db = create_test_db();

        let columns = db.get_table_columns("test_table").await.unwrap();
        assert!(!columns.is_empty());

        let col_names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();
        assert!(col_names.contains(&"id"), "Should contain 'id'");
        assert!(col_names.contains(&"name"), "Should contain 'name'");
        assert!(col_names.contains(&"value"), "Should contain 'value'");
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_schema_get_table_info() {
        let db = create_test_db();

        let info = db.get_table_info("test_table").await.unwrap();
        assert!(info.is_some(), "test_table info should exist");

        let info = info.unwrap();
        assert_eq!(info.name, "test_table");
        assert!(info.columns.contains_key("id"));
        assert!(info.columns.contains_key("name"));
        assert!(info.columns.contains_key("value"));

        // Non-existent table
        let none_info = db.get_table_info("nonexistent").await.unwrap();
        assert!(none_info.is_none());
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_schema_create_and_drop_table() {
        let db = create_test_db();

        db.exec_raw("CREATE TABLE schema_test (id INTEGER PRIMARY KEY, data TEXT)")
            .await
            .expect("Failed to create table");

        assert!(db.table_exists("schema_test").await.unwrap());

        db.exec_raw("DROP TABLE schema_test")
            .await
            .expect("Failed to drop table");

        assert!(!db.table_exists("schema_test").await.unwrap());
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_schema_list_tables_empty_after_drop() {
        let conn =
            Connection::open_in_memory().expect("Failed to create in-memory DuckDB database");

        let shared = Arc::new(Mutex::new(conn));
        let db = DuckDbDatabase::new(vec![Arc::clone(&shared)]);

        let tables = db.list_tables().await.unwrap();
        assert!(
            tables.is_empty(),
            "Empty database should have no tables, got: {tables:?}"
        );
    }
}
