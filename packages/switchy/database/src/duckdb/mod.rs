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
//! - Configurable routing/consistency policies via `DuckDbConfig`
//!
//! # Transaction Support
//!
//! Transactions get dedicated connections from the pool to ensure isolation.
//! Each transaction uses SQL commands: `BEGIN`, `COMMIT`, `ROLLBACK`.
//! In strict pooled mode, operations are serialized through a global gate to
//! avoid inconsistent cross-connection visibility.

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
use switchy_async::runtime::Handle;
use switchy_async::sync::Mutex;
use switchy_async::task::JoinError;
use thiserror::Error;
use tokio::sync::OwnedMutexGuard;

#[cfg(feature = "raw-sql")]
use crate::query_transform::{QuestionMarkHandler, transform_query_for_params};
use crate::{
    Database, DatabaseError, DatabaseTransaction, DatabaseValue, DeleteStatement, InsertStatement,
    SelectQuery, UpdateStatement, UpsertMultiStatement, UpsertStatement,
    query::{BooleanExpression, Expression, ExpressionType, Join, Sort, SortDirection},
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuckDbMode {
    /// Uses one shared connection across the configured pool slots.
    Deterministic,
    /// Uses independent connections and round-robin routing.
    Pooled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Visibility consistency model for pooled `DuckDB` operations.
pub enum DuckDbConsistency {
    /// Serializes operations through a global gate.
    Strict,
    /// Allows operations to proceed independently.
    Relaxed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Runtime configuration for `DuckDbDatabase` routing and consistency behavior.
pub struct DuckDbConfig {
    /// Connection routing mode.
    pub mode: DuckDbMode,
    /// Consistency model used when multiple pooled connections are active.
    pub consistency: DuckDbConsistency,
}

impl Default for DuckDbConfig {
    fn default() -> Self {
        Self {
            mode: DuckDbMode::Deterministic,
            consistency: DuckDbConsistency::Strict,
        }
    }
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
/// `DuckDB` database implementation for the crate `Database` abstraction.
pub struct DuckDbDatabase {
    connections: Vec<Arc<Mutex<Connection>>>,
    next_connection: AtomicUsize,
    config: DuckDbConfig,
    operation_gate: Arc<Mutex<()>>,
}

impl DuckDbDatabase {
    /// Creates a new `DuckDB` database instance from a vector of connections
    ///
    /// The connections are used in round-robin fashion to distribute load.
    #[must_use]
    pub fn new(connections: Vec<Arc<Mutex<Connection>>>) -> Self {
        Self::new_with_config(
            connections,
            DuckDbConfig {
                mode: DuckDbMode::Pooled,
                consistency: DuckDbConsistency::Relaxed,
            },
        )
    }

    /// Creates a new `DuckDB` database instance with explicit routing and consistency config.
    #[must_use]
    pub fn new_with_config(connections: Vec<Arc<Mutex<Connection>>>, config: DuckDbConfig) -> Self {
        Self {
            connections,
            next_connection: AtomicUsize::new(0),
            config,
            operation_gate: Arc::new(Mutex::new(())),
        }
    }

    fn get_connection(&self) -> Arc<Mutex<Connection>> {
        let index = self.next_connection.fetch_add(1, Ordering::Relaxed) % self.connections.len();
        self.connections[index].clone()
    }

    /// Returns the active runtime config for this database instance.
    #[must_use]
    pub const fn config(&self) -> DuckDbConfig {
        self.config
    }

    fn should_gate_operations(&self) -> bool {
        self.config.mode == DuckDbMode::Pooled
            && self.config.consistency == DuckDbConsistency::Strict
    }

    async fn lock_operation_gate(&self) -> Option<OwnedMutexGuard<()>> {
        if self.should_gate_operations() {
            Some(self.operation_gate.clone().lock_owned().await)
        } else {
            None
        }
    }

    async fn begin_transaction_blocking(
        &self,
        connection: Arc<Mutex<Connection>>,
    ) -> Result<(), DatabaseError> {
        run_duckdb_blocking("duckdb_begin_transaction", move || {
            connection
                .blocking_lock()
                .execute_batch("BEGIN TRANSACTION")
                .map_err(DuckDbDatabaseError::DuckDb)?;
            Ok(())
        })
        .await
    }
}

fn join_error_to_database_error(task_name: &str, error: &JoinError) -> DatabaseError {
    DatabaseError::QueryFailed(format!(
        "DuckDB blocking task '{task_name}' failed: {error}"
    ))
}

/// Runs synchronous `DuckDB` work on the blocking executor.
///
/// This helper is used to keep synchronous `DuckDB` operations off async
/// runtime worker threads while preserving `DatabaseError` mapping.
#[allow(dead_code)]
async fn run_duckdb_blocking<T>(
    task_name: &'static str,
    task: impl FnOnce() -> Result<T, DatabaseError> + Send + 'static,
) -> Result<T, DatabaseError>
where
    T: Send + Unpin + 'static,
{
    Handle::current()
        .spawn_blocking_with_name(task_name, task)
        .await
        .map_err(|e| join_error_to_database_error(task_name, &e))?
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
    _operation_gate_guard: Option<OwnedMutexGuard<()>>,
}

impl DuckDbTransaction {
    /// Creates a new `DuckDB` transaction from a connection
    #[must_use]
    pub const fn new(connection: Arc<Mutex<Connection>>) -> Self {
        Self::new_with_guard(connection, None)
    }

    /// Creates a transaction from a connection plus an optional operation gate guard.
    #[must_use]
    pub const fn new_with_guard(
        connection: Arc<Mutex<Connection>>,
        operation_gate_guard: Option<OwnedMutexGuard<()>>,
    ) -> Self {
        Self {
            connection,
            committed: AtomicBool::new(false),
            rolled_back: AtomicBool::new(false),
            _operation_gate_guard: operation_gate_guard,
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
            #[cfg(feature = "schema")]
            ExpressionType::SchemaPredicate(value) => value.sqlite.clone(),
            ExpressionType::JsonTypeIs(value) => format!(
                "(json_type({}, {}) = '{}')",
                value.expression.to_sql(),
                value.path.sqlite(),
                value.kind.sqlite()
            ),
            ExpressionType::StorageType(value) => format!("typeof({})", value.expression.to_sql()),
            ExpressionType::Add(value) => {
                format!("({} + {})", value.left.to_sql(), value.right.to_sql())
            }
            ExpressionType::ZeroBlob(value) => format!("zeroblob({})", value.length),
            ExpressionType::StorageCast(value) => format!(
                "CAST({} AS {})",
                value.expression.to_sql(),
                match value.target {
                    crate::query::StorageCastTarget::Blob => "BLOB",
                    crate::query::StorageCastTarget::Text => "TEXT",
                }
            ),
            ExpressionType::BoundedText(value) => format!(
                "CASE WHEN typeof({}) = 'text' AND length(CAST({} AS BLOB)) <= {} THEN {} ELSE {} END",
                value.expression.to_sql(),
                value.expression.to_sql(),
                value.max_bytes,
                value.expression.to_sql(),
                value.fallback.to_sql()
            ),
            ExpressionType::GtExpression(value) => {
                format!("({} > {})", value.left.to_sql(), value.right.to_sql())
            }
            ExpressionType::EqExpression(value) => format!(
                "({} {} {})",
                value.left.to_sql(),
                if value.right.is_null() { "IS" } else { "=" },
                value.right.to_sql()
            ),
            ExpressionType::NotEqExpression(value) => format!(
                "({} {} {})",
                value.left.to_sql(),
                if value.right.is_null() {
                    "IS NOT"
                } else {
                    "!="
                },
                value.right.to_sql()
            ),
            ExpressionType::Eq(value) => {
                if value.right.is_null() {
                    format!("({} IS {})", value.left.to_sql(), value.right.to_sql())
                } else {
                    format!("({} = {})", value.left.to_sql(), value.right.to_sql())
                }
            }
            ExpressionType::Gt(value) => {
                assert!(!value.right.is_null(), "Invalid > comparison with NULL");
                format!("({} > {})", value.left.to_sql(), value.right.to_sql())
            }
            ExpressionType::In(value) => {
                format!("{} IN ({})", value.left.to_sql(), value.values.to_sql())
            }
            ExpressionType::NotIn(value) => {
                format!("{} NOT IN ({})", value.left.to_sql(), value.values.to_sql())
            }
            ExpressionType::Lt(value) => {
                assert!(!value.right.is_null(), "Invalid < comparison with NULL");
                format!("({} < {})", value.left.to_sql(), value.right.to_sql())
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
                assert!(!value.right.is_null(), "Invalid >= comparison with NULL");
                format!("({} >= {})", value.left.to_sql(), value.right.to_sql())
            }
            ExpressionType::Lte(value) => {
                assert!(!value.right.is_null(), "Invalid <= comparison with NULL");
                format!("({} <= {})", value.left.to_sql(), value.right.to_sql())
            }
            ExpressionType::Join(value) => format!(
                "{} JOIN {} ON {}",
                if value.left { "LEFT" } else { "" },
                value.render_table('"'),
                value.on.render('"')
            ),
            ExpressionType::Sort(value) => format!(
                "({}) {}",
                value.expression.to_sql(),
                match value.direction {
                    SortDirection::Asc => "ASC",
                    SortDirection::Desc => "DESC",
                }
            ),
            ExpressionType::NotLike(value) => format!(
                "({} NOT LIKE {})",
                value.left.to_sql(),
                value.right.to_sql()
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
            #[cfg(feature = "raw-sql")]
            ExpressionType::Literal(value) => value.value.clone(),
            ExpressionType::Max(value) => format!("MAX({})", value.expression.to_sql()),
            ExpressionType::Count(value) => format!(
                "COUNT({})",
                value
                    .expression
                    .as_ref()
                    .map_or_else(|| "*".to_owned(), |expression| expression.to_sql())
            ),
            ExpressionType::Min(value) => format!("MIN({})", value.expression.to_sql()),
            ExpressionType::ByteLength(value) => {
                format!("octet_length(encode({}))", value.expression.to_sql())
            }
            ExpressionType::QualifiedColumn(value) => value.render('"'),
            ExpressionType::Identifier(value) => crate::query::render_identifier(&value.value, '"'),
            ExpressionType::SelectQuery(value) => {
                let columns = if value.projections.is_empty() {
                    crate::query::render_columns(value.columns, '"')
                } else {
                    value
                        .projections
                        .iter()
                        .map(|projection| {
                            let expression = projection.expression.to_sql();
                            projection.alias.as_ref().map_or_else(
                                || expression.clone(),
                                |alias| {
                                    let alias = alias.replace('"', "\"\"");
                                    format!("{expression} AS \"{alias}\"")
                                },
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let joins = value
                    .joins
                    .iter()
                    .flatten()
                    .map(ToSql::to_sql)
                    .collect::<Vec<_>>()
                    .join(" ");
                let filters = value
                    .filters
                    .iter()
                    .flatten()
                    .map(|x| x.to_sql())
                    .collect::<Vec<_>>()
                    .join(" AND ");
                let where_clause = if filters.is_empty() {
                    String::new()
                } else {
                    format!("WHERE {filters}")
                };
                let groups = value
                    .groups
                    .iter()
                    .map(|x| x.to_sql())
                    .collect::<Vec<_>>()
                    .join(", ");
                let group_clause = if groups.is_empty() {
                    String::new()
                } else {
                    format!("GROUP BY {groups}")
                };
                let sorts = value
                    .sorts
                    .iter()
                    .flatten()
                    .map(ToSql::to_sql)
                    .collect::<Vec<_>>()
                    .join(", ");
                let sort_clause = if sorts.is_empty() {
                    String::new()
                } else {
                    format!("ORDER BY {sorts}")
                };
                let limit = value
                    .limit
                    .map_or_else(String::new, |limit| format!("LIMIT {limit}"));
                format!(
                    "SELECT {} {columns} FROM {} {joins} {where_clause} {group_clause} {sort_clause} {limit}",
                    if value.distinct { "DISTINCT" } else { "" },
                    value.render_table('"')
                )
            }
            ExpressionType::DatabaseValue(value) => match value {
                null if null.is_null() => "NULL".to_string(),
                // Convert the instant to UTC before dropping its timezone.
                DatabaseValue::Now => "timezone('UTC', NOW())".to_string(),
                DatabaseValue::NowPlus(interval) => {
                    let interval_str = format_duckdb_interval(interval);
                    format!("(timezone('UTC', NOW()) + INTERVAL '{interval_str}')")
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

/// Convert a `DuckDB` [`Value`] into a [`DatabaseValue`] without loss where possible.
///
/// * `BLOB` becomes [`DatabaseValue::Bytes`], `DATE` [`DatabaseValue::Date`] and
///   `TIME` [`DatabaseValue::Time`].
/// * `TIMESTAMP` and `TIMESTAMP WITH TIME ZONE` both decode to
///   [`DatabaseValue::DateTime`] in UTC (the driver reports `TIMESTAMPTZ` as a UTC
///   instant without an offset), preserving the historical behavior of this backend.
/// * `DECIMAL` becomes [`DatabaseValue::Decimal`] with the `decimal` feature.
/// * Types without a dedicated variant (`INTERVAL`, `LIST`, `STRUCT`, `MAP`, `ENUM`,
///   out-of-range `HUGEINT`, `DECIMAL` without the `decimal` feature, ...) decode to
///   [`DatabaseValue::Other`] with `DuckDB`-style text.
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
            Value::Text(v) => Self::String(v),
            Value::HugeInt(v) => i64::try_from(v).map_or_else(
                |_| Self::Other {
                    type_name: "HUGEINT".to_string(),
                    text: v.to_string(),
                },
                Self::Int64,
            ),
            #[cfg(feature = "decimal")]
            Value::Decimal(v) => Self::Decimal(v),
            #[cfg(not(feature = "decimal"))]
            Value::Decimal(v) => Self::Other {
                type_name: "DECIMAL".to_string(),
                text: v.to_string(),
            },
            Value::Timestamp(unit, value) => {
                duckdb_timestamp(unit, value).map_or(Self::Null, Self::DateTime)
            }
            Value::Date32(days) => duckdb_date(days).map_or(Self::Null, Self::Date),
            Value::Time64(unit, value) => duckdb_time(unit, value).map_or(Self::Null, Self::Time),
            Value::Blob(v) => Self::Bytes(v),
            Value::Union(inner) => (*inner).into(),
            other @ (Value::Interval { .. }
            | Value::List(_)
            | Value::Array(_)
            | Value::Struct(_)
            | Value::Map(_)
            | Value::Enum(_)) => Self::Other {
                type_name: duckdb_value_kind_name(&other).to_string(),
                text: duckdb_value_text(&other),
            },
        }
    }
}

/// Days since the Unix epoch to a calendar date.
fn duckdb_date(days: i32) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::from_num_days_from_ce_opt(days.checked_add(719_163)?)
}

/// A `TIME` value in `unit`s since midnight to a time of day.
fn duckdb_time(unit: ::duckdb::types::TimeUnit, value: i64) -> Option<chrono::NaiveTime> {
    let nanos = duckdb_unit_to_nanos(unit, value);
    let secs = u32::try_from(nanos.div_euclid(1_000_000_000)).ok()?;
    let frac = u32::try_from(nanos.rem_euclid(1_000_000_000)).ok()?;
    chrono::NaiveTime::from_num_seconds_from_midnight_opt(secs, frac)
}

/// A `TIMESTAMP` value in `unit`s since the Unix epoch to a UTC date/time.
fn duckdb_timestamp(unit: ::duckdb::types::TimeUnit, value: i64) -> Option<chrono::NaiveDateTime> {
    let nanos = duckdb_unit_to_nanos(unit, value);
    let secs = i64::try_from(nanos.div_euclid(1_000_000_000)).ok()?;
    let frac = u32::try_from(nanos.rem_euclid(1_000_000_000)).ok()?;
    chrono::DateTime::from_timestamp(secs, frac).map(|dt| dt.naive_utc())
}

const fn duckdb_unit_to_nanos(unit: ::duckdb::types::TimeUnit, value: i64) -> i128 {
    use ::duckdb::types::TimeUnit;
    let value = value as i128;
    match unit {
        TimeUnit::Second => value * 1_000_000_000,
        TimeUnit::Millisecond => value * 1_000_000,
        TimeUnit::Microsecond => value * 1000,
        TimeUnit::Nanosecond => value,
    }
}

/// Generic `DuckDB` type name for a [`Value`] variant without a dedicated mapping.
const fn duckdb_value_kind_name(value: &Value) -> &'static str {
    match value {
        Value::Interval { .. } => "INTERVAL",
        Value::List(_) => "LIST",
        Value::Array(_) => "ARRAY",
        Value::Struct(_) => "STRUCT",
        Value::Map(_) => "MAP",
        Value::Enum(_) => "ENUM",
        Value::Union(_) => "UNION",
        Value::HugeInt(_) => "HUGEINT",
        Value::Decimal(_) => "DECIMAL",
        Value::Blob(_) => "BLOB",
        _ => "UNKNOWN",
    }
}

/// Render an `INTERVAL` the way `DuckDB` prints it (e.g. `1 year 2 months 3 days 04:05:06`).
fn duckdb_interval_text(months: i32, days: i32, nanos: i64) -> String {
    let mut parts = Vec::new();
    let years = months / 12;
    let months = months % 12;
    let plural = |n: i64, unit: &str| {
        if n.abs() == 1 {
            format!("{n} {unit}")
        } else {
            format!("{n} {unit}s")
        }
    };
    if years != 0 {
        parts.push(plural(i64::from(years), "year"));
    }
    if months != 0 {
        parts.push(plural(i64::from(months), "month"));
    }
    if days != 0 {
        parts.push(plural(i64::from(days), "day"));
    }
    if nanos != 0 || parts.is_empty() {
        let sign = if nanos < 0 { "-" } else { "" };
        let total_micros = (nanos / 1000).unsigned_abs();
        let micros = total_micros % 1_000_000;
        let total_secs = total_micros / 1_000_000;
        let mut time = format!(
            "{sign}{:02}:{:02}:{:02}",
            total_secs / 3600,
            (total_secs / 60) % 60,
            total_secs % 60
        );
        if micros != 0 {
            write!(time, ".{micros:06}").unwrap();
        }
        parts.push(time);
    }
    parts.join(" ")
}

/// Render a [`Value`] as text, mirroring `DuckDB`'s own display format.
fn duckdb_value_text(value: &Value) -> String {
    fn nested(value: &Value) -> String {
        match value {
            Value::Text(s) | Value::Enum(s) => format!("'{}'", s.replace('\'', "''")),
            other => duckdb_value_text(other),
        }
    }
    match value {
        Value::Null => "NULL".to_string(),
        Value::Boolean(v) => v.to_string(),
        Value::TinyInt(v) => v.to_string(),
        Value::SmallInt(v) => v.to_string(),
        Value::Int(v) => v.to_string(),
        Value::BigInt(v) => v.to_string(),
        Value::HugeInt(v) => v.to_string(),
        Value::UTinyInt(v) => v.to_string(),
        Value::USmallInt(v) => v.to_string(),
        Value::UInt(v) => v.to_string(),
        Value::UBigInt(v) => v.to_string(),
        Value::Float(v) => v.to_string(),
        Value::Double(v) => v.to_string(),
        Value::Decimal(v) => v.to_string(),
        Value::Text(v) | Value::Enum(v) => v.clone(),
        Value::Blob(bytes) => {
            let mut out = String::with_capacity(bytes.len() * 4);
            for b in bytes {
                if b.is_ascii_graphic() && *b != b'\\' {
                    out.push(char::from(*b));
                } else {
                    write!(out, "\\x{b:02X}").unwrap();
                }
            }
            out
        }
        Value::Timestamp(unit, v) => {
            duckdb_timestamp(*unit, *v).map_or_else(|| v.to_string(), |dt| dt.to_string())
        }
        Value::Date32(days) => {
            duckdb_date(*days).map_or_else(|| days.to_string(), |d| d.to_string())
        }
        Value::Time64(unit, v) => {
            duckdb_time(*unit, *v).map_or_else(|| v.to_string(), |t| t.to_string())
        }
        Value::Interval {
            months,
            days,
            nanos,
        } => duckdb_interval_text(*months, *days, *nanos),
        Value::List(values) | Value::Array(values) => format!(
            "[{}]",
            values.iter().map(nested).collect::<Vec<_>>().join(", ")
        ),
        Value::Struct(map) => format!(
            "{{{}}}",
            map.iter()
                .map(|(k, v)| format!("'{k}': {}", duckdb_value_text(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Map(map) => format!(
            "{{{}}}",
            map.iter()
                .map(|(k, v)| format!("{}={}", duckdb_value_text(k), duckdb_value_text(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Union(inner) => duckdb_value_text(inner),
    }
}

/// Native `DuckDB`-style type name for a result column's Arrow type.
#[cfg(feature = "raw-sql")]
fn duckdb_arrow_type_name(data_type: &::duckdb::arrow::datatypes::DataType) -> String {
    use ::duckdb::arrow::datatypes::{DataType as Arrow, TimeUnit};
    match data_type {
        Arrow::Null => "NULL".to_string(),
        Arrow::Boolean => "BOOLEAN".to_string(),
        Arrow::Int8 => "TINYINT".to_string(),
        Arrow::Int16 => "SMALLINT".to_string(),
        Arrow::Int32 => "INTEGER".to_string(),
        Arrow::Int64 => "BIGINT".to_string(),
        Arrow::UInt8 => "UTINYINT".to_string(),
        Arrow::UInt16 => "USMALLINT".to_string(),
        Arrow::UInt32 => "UINTEGER".to_string(),
        Arrow::UInt64 => "UBIGINT".to_string(),
        Arrow::Float16 | Arrow::Float32 => "FLOAT".to_string(),
        Arrow::Float64 => "DOUBLE".to_string(),
        Arrow::Utf8 | Arrow::LargeUtf8 | Arrow::Utf8View => "VARCHAR".to_string(),
        Arrow::Binary | Arrow::LargeBinary | Arrow::BinaryView | Arrow::FixedSizeBinary(_) => {
            "BLOB".to_string()
        }
        Arrow::Date32 | Arrow::Date64 => "DATE".to_string(),
        Arrow::Time32(_) | Arrow::Time64(_) => "TIME".to_string(),
        Arrow::Timestamp(_, Some(_)) => "TIMESTAMP WITH TIME ZONE".to_string(),
        Arrow::Timestamp(TimeUnit::Second, None) => "TIMESTAMP_S".to_string(),
        Arrow::Timestamp(TimeUnit::Millisecond, None) => "TIMESTAMP_MS".to_string(),
        Arrow::Timestamp(TimeUnit::Nanosecond, None) => "TIMESTAMP_NS".to_string(),
        Arrow::Timestamp(TimeUnit::Microsecond, None) => "TIMESTAMP".to_string(),
        Arrow::Decimal32(p, s)
        | Arrow::Decimal64(p, s)
        | Arrow::Decimal128(p, s)
        | Arrow::Decimal256(p, s) => format!("DECIMAL({p},{s})"),
        Arrow::Interval(_) | Arrow::Duration(_) => "INTERVAL".to_string(),
        Arrow::List(field)
        | Arrow::LargeList(field)
        | Arrow::ListView(field)
        | Arrow::LargeListView(field) => format!("{}[]", duckdb_arrow_type_name(field.data_type())),
        Arrow::FixedSizeList(field, size) => {
            format!("{}[{size}]", duckdb_arrow_type_name(field.data_type()))
        }
        Arrow::Struct(fields) => format!(
            "STRUCT({})",
            fields
                .iter()
                .map(|f| format!("{} {}", f.name(), duckdb_arrow_type_name(f.data_type())))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Arrow::Map(entries, _) => match entries.data_type() {
            Arrow::Struct(kv) if kv.len() == 2 => format!(
                "MAP({}, {})",
                duckdb_arrow_type_name(kv[0].data_type()),
                duckdb_arrow_type_name(kv[1].data_type())
            ),
            _ => "MAP".to_string(),
        },
        Arrow::Dictionary(_, _) => "ENUM".to_string(),
        Arrow::Union(_, _) => "UNION".to_string(),
        other => format!("{other:?}"),
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
                    join.render_table('"'),
                    join.on.render('"')
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
        .map(|(name, value)| {
            format!(
                "{}=({})",
                crate::query::render_identifier(name, '"'),
                value.deref().to_sql()
            )
        })
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
                null if null.is_null() => {
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
                DatabaseValue::Bytes(v) | DatabaseValue::BytesOpt(Some(v)) => {
                    statement
                        .raw_bind_parameter(i, v.as_slice())
                        .map_err(DuckDbDatabaseError::DuckDb)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::Date(_)
                | DatabaseValue::DateOpt(Some(_))
                | DatabaseValue::Time(_)
                | DatabaseValue::TimeOpt(Some(_))
                | DatabaseValue::DateTimeTz(_)
                | DatabaseValue::DateTimeTzOpt(Some(_))
                | DatabaseValue::Other { .. } => {
                    // Bound as ISO-8601 / native text; DuckDB casts it to the
                    // target column type.
                    let text = duckdb_text_param(value).unwrap_or_default();
                    statement
                        .raw_bind_parameter(i, text)
                        .map_err(DuckDbDatabaseError::DuckDb)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                #[cfg(feature = "json")]
                DatabaseValue::Json(v) | DatabaseValue::JsonOpt(Some(v)) => {
                    statement
                        .raw_bind_parameter(i, v.to_string())
                        .map_err(DuckDbDatabaseError::DuckDb)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                // Remaining `*Opt(None)` variants are covered by the `is_null` arm
                _ => {}
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

/// Text form used to bind or inline values that `DuckDB` accepts as casts from `VARCHAR`.
fn duckdb_text_param(value: &DatabaseValue) -> Option<String> {
    match value {
        DatabaseValue::Date(v) | DatabaseValue::DateOpt(Some(v)) => {
            Some(v.format("%Y-%m-%d").to_string())
        }
        DatabaseValue::Time(v) | DatabaseValue::TimeOpt(Some(v)) => {
            Some(v.format("%H:%M:%S%.f").to_string())
        }
        DatabaseValue::DateTime(v) => Some(v.format("%Y-%m-%d %H:%M:%S%.f").to_string()),
        DatabaseValue::DateTimeTz(v) | DatabaseValue::DateTimeTzOpt(Some(v)) => {
            Some(v.format("%Y-%m-%d %H:%M:%S%.f%:z").to_string())
        }
        #[cfg(feature = "json")]
        DatabaseValue::Json(v) | DatabaseValue::JsonOpt(Some(v)) => Some(v.to_string()),
        DatabaseValue::Other { text, .. } => Some(text.clone()),
        _ => None,
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

#[cfg(feature = "raw-sql")]
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

fn select_values(query: &SelectQuery<'_>) -> Vec<DuckDbDatabaseValue> {
    query
        .params()
        .unwrap_or_default()
        .into_iter()
        .cloned()
        .map(DuckDbDatabaseValue::from)
        .collect()
}

fn build_insert_plan(
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
) -> (String, Vec<DuckDbDatabaseValue>) {
    let column_names = values
        .iter()
        .map(|(key, _)| format!("\"{key}\""))
        .collect::<Vec<_>>()
        .join(", ");

    let insert_columns = if values.is_empty() {
        String::new()
    } else {
        format!("({column_names})")
    };

    (
        format!(
            "INSERT INTO {table_name} {insert_columns} {} RETURNING *",
            build_values_clause(values),
        ),
        exprs_to_values(values),
    )
}

fn build_update_plan(
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> (String, Vec<DuckDbDatabaseValue>) {
    let select_query = limit.map(|_| {
        format!(
            "SELECT rowid FROM {table_name} {}",
            build_where_clause(filters)
        )
    });
    let query = format!(
        "UPDATE {table_name} {} {} RETURNING *",
        build_set_clause(values),
        build_update_where_clause(filters, limit, select_query.as_deref()),
    );

    let update_values = values
        .iter()
        .flat_map(|(_, value)| value.params().unwrap_or_default().into_iter().cloned())
        .map(std::convert::Into::into)
        .collect::<Vec<DuckDbDatabaseValue>>();
    let mut filter_values = filters.map(bexprs_to_values).unwrap_or_default();
    if limit.is_some() {
        filter_values.extend(filter_values.clone());
    }

    (query, [update_values, filter_values].concat())
}

fn build_upsert_multi_chunk_plans(
    table_name: &str,
    unique: &[Box<dyn Expression>],
    values: &[Vec<(&str, Box<dyn Expression>)>],
) -> Result<Vec<(String, Vec<DuckDbDatabaseValue>)>, DuckDbDatabaseError> {
    if values.is_empty() {
        return Ok(Vec::new());
    }

    let mut plans = Vec::new();
    let mut pos = 0usize;
    let mut i = 0usize;
    let mut last_i = i;

    for value in values {
        let count = value.len();
        if pos + count >= (i16::MAX - 1) as usize {
            let chunk = &values[last_i..i];
            if !chunk.is_empty() {
                plans.push(build_upsert_chunk_plan(table_name, unique, chunk)?);
            }
            last_i = i;
            pos = 0;
        }
        i += 1;
        pos += count;
    }

    if i > last_i {
        plans.push(build_upsert_chunk_plan(
            table_name,
            unique,
            &values[last_i..],
        )?);
    }

    Ok(plans)
}

fn build_upsert_chunk_plan(
    table_name: &str,
    unique: &[Box<dyn Expression>],
    values: &[Vec<(&str, Box<dyn Expression>)>],
) -> Result<(String, Vec<DuckDbDatabaseValue>), DuckDbDatabaseError> {
    let first = values[0].as_slice();
    let expected_value_size = first.len();

    if let Some(bad_row) = values.iter().skip(1).find(|v| {
        v.len() != expected_value_size || v.iter().enumerate().any(|(i, c)| c.0 != first[i].0)
    }) {
        log::error!("Bad row: {bad_row:?}. Expected to match schema of first row: {first:?}");
        return Err(DuckDbDatabaseError::InvalidRequest);
    }

    let set_clause = first
        .iter()
        .map(|(name, _)| format!("\"{name}\" = EXCLUDED.\"{name}\""))
        .collect::<Vec<_>>()
        .join(", ");

    let column_names = first
        .iter()
        .map(|(key, _)| format!("\"{key}\""))
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
        "\
        INSERT INTO {table_name} {insert_columns} {values_str}
        ON CONFLICT({unique_conflict}) DO UPDATE
            SET {set_clause}
        RETURNING *"
    );

    let params = values
        .iter()
        .flat_map(std::iter::IntoIterator::into_iter)
        .flat_map(|(_, value)| value.params().unwrap_or_default().into_iter().cloned())
        .map(std::convert::Into::into)
        .collect::<Vec<DuckDbDatabaseValue>>();

    Ok((query, params))
}

fn execute_returning_rows(
    connection: &Connection,
    sql: &str,
    params: &[DuckDbDatabaseValue],
    constant_inc: bool,
) -> Result<Vec<crate::Row>, DuckDbDatabaseError> {
    let mut stmt = connection
        .prepare_cached(sql)
        .map_err(DuckDbDatabaseError::DuckDb)?;
    bind_values(&mut stmt, Some(params), constant_inc, 0)?;
    stmt.raw_execute().map_err(DuckDbDatabaseError::DuckDb)?;
    let column_names = stmt.column_names();
    to_rows(&column_names, &mut stmt.raw_query())
}

fn execute_returning_first(
    connection: &Connection,
    sql: &str,
    params: &[DuckDbDatabaseValue],
    constant_inc: bool,
) -> Result<crate::Row, DuckDbDatabaseError> {
    let mut stmt = connection
        .prepare_cached(sql)
        .map_err(DuckDbDatabaseError::DuckDb)?;
    bind_values(&mut stmt, Some(params), constant_inc, 0)?;
    stmt.raw_execute().map_err(DuckDbDatabaseError::DuckDb)?;
    let column_names = stmt.column_names();
    let mut query = stmt.raw_query();
    query
        .next()
        .map_err(DuckDbDatabaseError::DuckDb)?
        .map(|row| from_row(&column_names, row))
        .transpose()?
        .ok_or(DuckDbDatabaseError::NoRow)
}

fn execute_upsert_plan(
    connection: &Connection,
    update_sql: &str,
    update_params: &[DuckDbDatabaseValue],
    insert_sql: &str,
    insert_params: &[DuckDbDatabaseValue],
) -> Result<Vec<crate::Row>, DuckDbDatabaseError> {
    let updated_rows = execute_returning_rows(connection, update_sql, update_params, false)?;
    if updated_rows.is_empty() {
        Ok(vec![execute_returning_first(
            connection,
            insert_sql,
            insert_params,
            false,
        )?])
    } else {
        Ok(updated_rows)
    }
}

fn execute_upsert_first_plan(
    connection: &Connection,
    update_sql: &str,
    update_params: &[DuckDbDatabaseValue],
    insert_sql: &str,
    insert_params: &[DuckDbDatabaseValue],
) -> Result<crate::Row, DuckDbDatabaseError> {
    let updated_rows = execute_returning_rows(connection, update_sql, update_params, false)?;
    updated_rows.into_iter().next().map_or_else(
        || execute_returning_first(connection, insert_sql, insert_params, false),
        Ok,
    )
}

fn execute_upsert_multi_plans(
    connection: &Connection,
    plans: &[(String, Vec<DuckDbDatabaseValue>)],
) -> Result<Vec<crate::Row>, DuckDbDatabaseError> {
    let mut results = Vec::new();
    for (sql, params) in plans {
        results.append(&mut execute_returning_rows(connection, sql, params, true)?);
    }
    Ok(results)
}

// ---------------------------------------------------------------------------
// Core query operations
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
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

fn delete(
    connection: &Connection,
    table_name: &str,
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, DuckDbDatabaseError> {
    let table_name = crate::query::render_identifier(table_name, '"');
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

// ---------------------------------------------------------------------------
// Now/NowPlus parameter transformation
// ---------------------------------------------------------------------------

#[cfg(feature = "raw-sql")]
fn duckdb_transform_query_for_params(
    query: &str,
    params: &[DatabaseValue],
) -> Result<(String, Vec<DatabaseValue>), DatabaseError> {
    transform_query_for_params(query, params, &QuestionMarkHandler, |param| match param {
        DatabaseValue::Now => Some("timezone('UTC', NOW())".to_string()),
        DatabaseValue::NowPlus(interval) => {
            let interval_str = format_duckdb_interval(interval);
            Some(format!(
                "(timezone('UTC', NOW()) + INTERVAL '{interval_str}')"
            ))
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
        DatabaseValue::Now => "timezone('UTC', NOW())".to_string(),
        DatabaseValue::NowPlus(interval) => {
            let interval_str = format_duckdb_interval(interval);
            format!("(timezone('UTC', NOW()) + INTERVAL '{interval_str}')")
        }
        DatabaseValue::Bytes(v) | DatabaseValue::BytesOpt(Some(v)) => {
            let mut hex = String::with_capacity(v.len() * 2);
            for b in v {
                write!(hex, "{b:02X}").unwrap();
            }
            format!("unhex('{hex}')")
        }
        #[cfg(feature = "decimal")]
        DatabaseValue::Decimal(v) | DatabaseValue::DecimalOpt(Some(v)) => v.to_string(),
        #[cfg(feature = "uuid")]
        DatabaseValue::Uuid(v) | DatabaseValue::UuidOpt(Some(v)) => format!("'{v}'"),
        other => duckdb_text_param(other).map_or_else(
            || "NULL".to_string(),
            |text| format!("'{}'", text.replace('\'', "''")),
        ),
    }
}

#[cfg(feature = "schema")]
fn build_create_table_sql(
    statement: &crate::schema::CreateTableStatement<'_>,
) -> Result<String, DatabaseError> {
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
        let pk = if cfg!(feature = "raw-sql") {
            format!("\"{pk}\"")
        } else {
            crate::query::render_identifier(pk, '"')
        };
        col_defs.push(format!("PRIMARY KEY ({pk})"));
    }

    col_defs.extend(
        statement
            .constraints
            .iter()
            .map(|constraint| constraint.render('"'))
            .collect::<Result<Vec<_>, _>>()?,
    );

    for (col, ref_table) in &statement.foreign_keys {
        if cfg!(feature = "raw-sql") {
            col_defs.push(format!("FOREIGN KEY (\"{col}\") REFERENCES {ref_table}"));
        } else {
            col_defs.push(format!(
                "FOREIGN KEY ({}) REFERENCES {}",
                crate::query::render_identifier(col, '"'),
                crate::query::render_identifier(ref_table, '"')
            ));
        }
    }

    sql.push_str(&col_defs.join(", "));
    sql.push(')');
    Ok(sql)
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
        statement.index_name.replace('"', "\"\""),
        statement.table_name.replace('"', "\"\""),
        statement
            .columns
            .iter()
            .map(|c| format!("\"{}\"", c.replace('"', "\"\"")))
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
    write!(sql, "\"{}\"", statement.index_name.replace('"', "\"\"")).unwrap();
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

/// Map a `DuckDB` type name (as reported by `information_schema.columns.data_type`)
/// to a [`crate::schema::DataType`].
///
/// Never fails: types without a matching variant map to `DataType::Custom` with
/// the native name preserved.
#[cfg(feature = "schema")]
fn duckdb_type_str_to_data_type(type_str: &str) -> crate::schema::DataType {
    use crate::schema::DataType;

    let upper = type_str.trim().to_uppercase();
    let upper = upper.as_str();

    // Arrays: `T[]` (list) and `T[n]` (fixed-size array)
    if upper.ends_with(']')
        && let Some(open) = upper.rfind('[')
    {
        let size = &upper[open + 1..upper.len() - 1];
        if size.is_empty() || size.chars().all(|c| c.is_ascii_digit()) {
            return DataType::Array(Box::new(duckdb_type_str_to_data_type(
                &type_str.trim()[..open],
            )));
        }
    }

    match upper {
        "BOOLEAN" | "BOOL" | "LOGICAL" => DataType::Bool,
        "TINYINT" | "INT1" => DataType::TinyInt,
        "SMALLINT" | "INT2" | "SHORT" => DataType::SmallInt,
        "INTEGER" | "INT" | "INT4" | "SIGNED" => DataType::Int,
        "BIGINT" | "INT8" | "LONG" => DataType::BigInt,
        "REAL" | "FLOAT" | "FLOAT4" => DataType::Real,
        "DOUBLE" | "FLOAT8" => DataType::Double,
        "TEXT" | "STRING" | "VARCHAR" | "BPCHAR" | "CHAR" => DataType::Text,
        "BLOB" | "BYTEA" | "BINARY" | "VARBINARY" => DataType::Blob,
        "DATE" => DataType::Date,
        "TIME" => DataType::Time,
        "TIMESTAMP"
        | "DATETIME"
        | "TIMESTAMP WITH TIME ZONE"
        | "TIMESTAMPTZ"
        | "TIMESTAMP_S"
        | "TIMESTAMP_MS"
        | "TIMESTAMP_NS" => DataType::Timestamp,
        "UUID" => DataType::Uuid,
        "JSON" => DataType::Json,
        _ => {
            if upper.starts_with("VARCHAR(") || upper.starts_with("TEXT(") {
                extract_type_param(upper)
                    .and_then(|n| u16::try_from(n).ok())
                    .map_or(DataType::Text, DataType::VarChar)
            } else if upper.starts_with("CHAR(") || upper.starts_with("BPCHAR(") {
                extract_type_param(upper)
                    .and_then(|n| u16::try_from(n).ok())
                    .map_or(DataType::Text, DataType::Char)
            } else if upper == "DECIMAL" || upper == "NUMERIC" {
                DataType::Decimal(18, 3)
            } else if upper.starts_with("DECIMAL(") || upper.starts_with("NUMERIC(") {
                extract_two_type_params(upper)
                    .and_then(|(p, s)| Some((u8::try_from(p).ok()?, u8::try_from(s).ok()?)))
                    .map_or_else(
                        || DataType::Custom(type_str.to_string()),
                        |(p, s)| DataType::Decimal(p, s),
                    )
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

/// Resolve an optional schema name to the effective schema (`current_schema()` when `None`).
#[cfg(feature = "schema")]
fn duckdb_resolve_schema(
    conn: &Connection,
    schema: Option<&str>,
) -> Result<String, DuckDbDatabaseError> {
    if let Some(schema) = schema {
        return Ok(schema.to_string());
    }
    Ok(conn.query_row("SELECT current_schema()", [], |row| row.get::<_, String>(0))?)
}

/// List the schemas of the current database plus the built-in catalog schemas.
#[cfg(feature = "schema")]
fn duckdb_list_schemas(
    conn: &Connection,
) -> Result<Vec<crate::schema::SchemaInfo>, DuckDbDatabaseError> {
    let default_schema = duckdb_resolve_schema(conn, None)?;
    let mut stmt = conn.prepare(
        "SELECT DISTINCT schema_name FROM duckdb_schemas() \
         WHERE database_name = current_database() \
            OR (database_name = 'system' \
                AND schema_name IN ('information_schema', 'pg_catalog')) \
         ORDER BY schema_name",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut schemas = Vec::new();
    for row in rows {
        let name = row?;
        schemas.push(crate::schema::SchemaInfo {
            is_default: name == default_schema,
            is_system: matches!(name.as_str(), "information_schema" | "pg_catalog"),
            name,
        });
    }
    Ok(schemas)
}

/// SQL selecting `name`, `kind`, `comment`, `estimated_rows` for tables and views
/// in schema `?1`; the optional `extra` predicate may reference `name`.
#[cfg(feature = "schema")]
fn duckdb_relations_sql(extra: &str) -> String {
    format!(
        "SELECT name, kind, comment, estimated_rows FROM ( \
             SELECT table_name AS name, 'table' AS kind, comment, \
                    estimated_size AS estimated_rows \
             FROM duckdb_tables() \
             WHERE database_name = current_database() AND schema_name = $1 AND NOT internal \
             UNION ALL \
             SELECT view_name AS name, 'view' AS kind, comment, NULL AS estimated_rows \
             FROM duckdb_views() \
             WHERE database_name = current_database() AND schema_name = $1 AND NOT internal \
         ) relations {extra} ORDER BY name"
    )
}

#[cfg(feature = "schema")]
fn duckdb_relation_kind(kind: &str) -> crate::schema::RelationKind {
    match kind {
        "table" => crate::schema::RelationKind::Table,
        "view" => crate::schema::RelationKind::View,
        other => crate::schema::RelationKind::Other(other.to_string()),
    }
}

/// List tables and views in `schema` with comments and estimated row counts.
#[cfg(feature = "schema")]
fn duckdb_list_relations(
    conn: &Connection,
    schema: &str,
) -> Result<Vec<crate::schema::RelationSummary>, DuckDbDatabaseError> {
    let mut stmt = conn.prepare(&duckdb_relations_sql(""))?;
    let rows = stmt.query_map([schema], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<i64>>(3)?,
        ))
    })?;
    let mut relations = Vec::new();
    for row in rows {
        let (name, kind, comment, estimated_rows) = row?;
        relations.push(crate::schema::RelationSummary {
            schema: Some(schema.to_string()),
            name,
            kind: duckdb_relation_kind(&kind),
            comment,
            estimated_rows: estimated_rows.and_then(|n| u64::try_from(n).ok()),
        });
    }
    Ok(relations)
}

/// Look up the kind and comment of a single relation.
#[cfg(feature = "schema")]
fn duckdb_relation_meta(
    conn: &Connection,
    schema: &str,
    name: &str,
) -> Result<Option<(crate::schema::RelationKind, Option<String>)>, DuckDbDatabaseError> {
    let mut stmt = conn.prepare(&duckdb_relations_sql("WHERE name = $2"))?;
    let mut rows = stmt.query([schema, name])?;
    if let Some(row) = rows.next()? {
        let kind: String = row.get(1)?;
        let comment: Option<String> = row.get(2)?;
        Ok(Some((duckdb_relation_kind(&kind), comment)))
    } else {
        Ok(None)
    }
}

/// Get the `CREATE VIEW` SQL of a view.
#[cfg(feature = "schema")]
fn duckdb_view_definition(
    conn: &Connection,
    schema: &str,
    name: &str,
) -> Result<Option<String>, DuckDbDatabaseError> {
    let mut stmt = conn.prepare(
        "SELECT sql FROM duckdb_views() \
         WHERE database_name = current_database() AND schema_name = ? AND view_name = ?",
    )?;
    let mut rows = stmt.query([schema, name])?;
    if let Some(row) = rows.next()? {
        Ok(row.get::<_, Option<String>>(0)?)
    } else {
        Ok(None)
    }
}

/// Whether a table or view exists in `schema` (`information_schema.tables`).
#[cfg(feature = "schema")]
fn duckdb_table_exists(
    conn: &Connection,
    schema: &str,
    table_name: &str,
) -> Result<bool, DuckDbDatabaseError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM information_schema.tables \
         WHERE table_catalog = current_database() AND table_schema = ? AND table_name = ?",
        [schema, table_name],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Names from `information_schema.tables` in `schema` (tables and views).
#[cfg(feature = "schema")]
fn duckdb_list_table_names(
    conn: &Connection,
    schema: &str,
) -> Result<Vec<String>, DuckDbDatabaseError> {
    let mut stmt = conn.prepare(
        "SELECT table_name FROM information_schema.tables \
         WHERE table_catalog = current_database() AND table_schema = ? ORDER BY table_name",
    )?;
    let rows = stmt.query_map([schema], |row| row.get::<_, String>(0))?;
    let mut tables = Vec::new();
    for row in rows {
        tables.push(row?);
    }
    Ok(tables)
}

/// Query primary key columns for a table using `duckdb_constraints()`.
#[cfg(feature = "schema")]
fn query_primary_key_columns(
    conn: &Connection,
    schema: &str,
    table_name: &str,
) -> Result<std::collections::BTreeSet<String>, DuckDbDatabaseError> {
    let mut pk_cols = std::collections::BTreeSet::new();
    // Use unnest() to flatten the constraint_column_names list into individual rows
    let mut stmt = conn.prepare(
        "SELECT unnest(constraint_column_names) AS col_name \
         FROM duckdb_constraints() \
         WHERE database_name = current_database() AND schema_name = ? AND table_name = ? \
         AND constraint_type = 'PRIMARY KEY'",
    )?;
    let rows = stmt.query_map([schema, table_name], |row| row.get::<_, String>(0))?;
    for row in rows {
        pk_cols.insert(row?);
    }
    Ok(pk_cols)
}

/// Query foreign key constraints for a table using `information_schema`.
#[cfg(feature = "schema")]
fn query_foreign_keys(
    conn: &Connection,
    schema: &str,
    table_name: &str,
) -> Result<std::collections::BTreeMap<String, crate::schema::ForeignKeyInfo>, DuckDbDatabaseError>
{
    let mut fk_map = std::collections::BTreeMap::new();
    let mut stmt = conn.prepare(
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
         WHERE tc.table_catalog = current_database() \
             AND tc.table_schema = ? AND tc.table_name = ? \
             AND tc.constraint_type = 'FOREIGN KEY'",
    )?;
    let rows = stmt.query_map([schema, table_name], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;
    for row in rows {
        let (column, referenced_table, referenced_column, on_update, on_delete, constraint_name) =
            row?;
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

/// Get column info (tables and views) with primary key and auto-increment detection.
#[cfg(feature = "schema")]
fn duckdb_get_table_columns(
    conn: &Connection,
    schema: &str,
    table_name: &str,
) -> Result<Vec<crate::schema::ColumnInfo>, DuckDbDatabaseError> {
    let pk_cols = query_primary_key_columns(conn, schema, table_name)?;

    let mut stmt = conn.prepare(
        "SELECT column_name, data_type, is_nullable, column_default, ordinal_position \
         FROM information_schema.columns \
         WHERE table_catalog = current_database() AND table_schema = ? AND table_name = ? \
         ORDER BY ordinal_position",
    )?;
    let rows = stmt.query_map([schema, table_name], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, i32>(4)?,
        ))
    })?;

    let mut columns = Vec::new();
    for row in rows {
        let (name, type_str, nullable_str, default_str, ordinal) = row?;

        // Detect auto-increment from nextval() default pattern
        let auto_increment = default_str.as_ref().is_some_and(|d| d.contains("nextval("));

        columns.push(crate::schema::ColumnInfo {
            is_primary_key: pk_cols.contains(&name),
            name,
            data_type: duckdb_type_str_to_data_type(&type_str),
            nullable: nullable_str == "YES",
            auto_increment,
            default_value: default_str.map(DatabaseValue::String),
            ordinal_position: u32::try_from(ordinal).unwrap_or_default(),
            native_type: type_str,
        });
    }

    Ok(columns)
}

/// Get complete relation info including columns, indexes, and foreign keys.
///
/// Returns `None` if no table or view named `name` exists in `schema`.
#[cfg(feature = "schema")]
fn duckdb_get_relation_info(
    conn: &Connection,
    schema: &str,
    name: &str,
) -> Result<Option<crate::schema::TableInfo>, DuckDbDatabaseError> {
    let Some((kind, comment)) = duckdb_relation_meta(conn, schema, name)? else {
        return Ok(None);
    };

    let columns = duckdb_get_table_columns(conn, schema, name)?;
    let col_map = columns
        .into_iter()
        .map(|col| (col.name.clone(), col))
        .collect::<std::collections::BTreeMap<_, _>>();

    let mut idx_map = std::collections::BTreeMap::new();

    let foreign_keys = if kind == crate::schema::RelationKind::Table {
        let pk_cols = query_primary_key_columns(conn, schema, name)?;
        let mut stmt = conn.prepare(
            "SELECT index_name, is_unique, sql FROM duckdb_indexes() \
             WHERE database_name = current_database() AND schema_name = ? AND table_name = ?",
        )?;
        let idx_rows = stmt.query_map([schema, name], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, bool>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;
        for row in idx_rows {
            let (index_name, unique, sql) = row?;
            let idx_columns = sql
                .as_deref()
                .map(extract_index_columns_from_sql)
                .unwrap_or_default();
            // Detect primary index by checking if its columns exactly match PK columns
            let is_primary = !idx_columns.is_empty()
                && idx_columns.len() == pk_cols.len()
                && idx_columns.iter().all(|c| pk_cols.contains(c));
            idx_map.insert(
                index_name.clone(),
                crate::schema::IndexInfo {
                    name: index_name,
                    unique,
                    columns: idx_columns,
                    is_primary,
                },
            );
        }

        query_foreign_keys(conn, schema, name)?
    } else {
        std::collections::BTreeMap::new()
    };

    Ok(Some(crate::schema::TableInfo {
        name: name.to_string(),
        schema: Some(schema.to_string()),
        kind,
        comment,
        columns: col_map,
        indexes: idx_map,
        foreign_keys,
    }))
}

/// Execute a raw query and collect a positional [`crate::QueryResult`].
#[cfg(feature = "raw-sql")]
fn duckdb_query_result(
    conn: &Connection,
    query: &str,
    params: &[DuckDbDatabaseValue],
) -> Result<crate::QueryResult, DuckDbDatabaseError> {
    let mut stmt = conn.prepare(query)?;
    bind_values_raw(&mut stmt, Some(params), 0)?;
    stmt.raw_execute()?;

    let names = stmt.column_names();
    let columns: Vec<crate::ResultColumn> = names
        .into_iter()
        .enumerate()
        .map(|(index, name)| crate::ResultColumn {
            name,
            type_name: Some(duckdb_arrow_type_name(&stmt.column_type(index))),
        })
        .collect();

    let mut result_rows = Vec::new();
    let mut rows = stmt.raw_query();
    while let Some(row) = rows.next()? {
        let mut values = Vec::with_capacity(columns.len());
        for (index, column) in columns.iter().enumerate() {
            let mut value = DatabaseValue::from(row.get::<_, Value>(index)?);
            if let (DatabaseValue::Other { type_name, .. }, Some(native)) =
                (&mut value, &column.type_name)
            {
                type_name.clone_from(native);
            }
            values.push(value);
        }
        result_rows.push(values);
    }

    Ok(crate::QueryResult {
        columns,
        rows: result_rows,
    })
}

impl DuckDbDatabase {
    /// Run synchronous work against one pooled connection on the blocking executor.
    #[allow(dead_code)]
    async fn with_connection<T: Send + Unpin + 'static>(
        &self,
        task_name: &'static str,
        task: impl FnOnce(&Connection) -> Result<T, DuckDbDatabaseError> + Send + 'static,
    ) -> Result<T, DatabaseError> {
        let _operation_guard = self.lock_operation_gate().await;
        let connection = self.get_connection();
        run_duckdb_blocking(task_name, move || {
            let conn = connection.blocking_lock();
            task(&conn).map_err(Into::into)
        })
        .await
    }
}

impl DuckDbTransaction {
    /// Run synchronous work against the transaction's connection on the blocking executor.
    #[allow(dead_code)]
    async fn with_connection<T: Send + Unpin + 'static>(
        &self,
        task_name: &'static str,
        task: impl FnOnce(&Connection) -> Result<T, DuckDbDatabaseError> + Send + 'static,
    ) -> Result<T, DatabaseError> {
        let connection = Arc::clone(&self.connection);
        run_duckdb_blocking(task_name, move || {
            let conn = connection.blocking_lock();
            task(&conn).map_err(Into::into)
        })
        .await
    }
}

// ---------------------------------------------------------------------------
// Database trait implementations
// ---------------------------------------------------------------------------

#[async_trait]
#[allow(clippy::significant_drop_tightening)]
impl Database for DuckDbDatabase {
    async fn query(&self, query: &SelectQuery<'_>) -> Result<Vec<crate::Row>, DatabaseError> {
        if query
            .filters
            .iter()
            .flatten()
            .any(|filter| matches!(filter.expression_type(), ExpressionType::JsonTypeIs(_)))
        {
            return Err(DatabaseError::UnsupportedOperation(
                "SQLite JSON type predicate".into(),
            ));
        }
        if query.table_alias.is_some()
            || query.indexed_by.is_some()
            || query.projections.iter().any(|projection| {
                matches!(
                    projection.expression.expression_type(),
                    ExpressionType::BoundedText(_)
                )
            })
        {
            return Err(DatabaseError::UnsupportedOperation(
                "SQLite-only selection policy".into(),
            ));
        }

        let _operation_guard = self.lock_operation_gate().await;
        let connection = self.get_connection();
        let sql = query.to_sql();
        let params = select_values(query);

        run_duckdb_blocking("duckdb_select", move || {
            let conn = connection.blocking_lock();
            let mut statement = conn
                .prepare_cached(&sql)
                .map_err(DuckDbDatabaseError::DuckDb)?;
            bind_values(&mut statement, Some(&params), false, 0)?;
            statement
                .raw_execute()
                .map_err(DuckDbDatabaseError::DuckDb)?;
            let column_names = statement.column_names();
            to_rows(&column_names, &mut statement.raw_query()).map_err(Into::into)
        })
        .await
    }

    async fn query_first(
        &self,
        query: &SelectQuery<'_>,
    ) -> Result<Option<crate::Row>, DatabaseError> {
        if query
            .filters
            .iter()
            .flatten()
            .any(|filter| matches!(filter.expression_type(), ExpressionType::JsonTypeIs(_)))
        {
            return Err(DatabaseError::UnsupportedOperation(
                "SQLite JSON type predicate".into(),
            ));
        }
        if query.table_alias.is_some() {
            return Err(DatabaseError::UnsupportedOperation(
                "Table alias on this execution path".into(),
            ));
        }
        let _operation_guard = self.lock_operation_gate().await;
        let connection = self.get_connection();
        let sql = format!(
            "SELECT * FROM ({}) AS switchy_first LIMIT 1",
            query.to_sql()
        );
        let params = select_values(query);

        run_duckdb_blocking("duckdb_select_first", move || {
            let conn = connection.blocking_lock();
            let mut statement = conn
                .prepare_cached(&sql)
                .map_err(DuckDbDatabaseError::DuckDb)?;
            bind_values(&mut statement, Some(&params), false, 0)?;
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
                .map_err(Into::into)
        })
        .await
    }

    async fn exec_delete(
        &self,
        statement: &DeleteStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        let _operation_guard = self.lock_operation_gate().await;
        let connection = self.get_connection();
        let where_clause = build_where_clause(statement.filters.as_deref());
        let filter_values = statement
            .filters
            .as_deref()
            .map(bexprs_to_values)
            .unwrap_or_default();
        let limit_clause = statement
            .limit
            .map_or_else(String::new, |l| format!(" LIMIT {l}"));
        let returning_query = format!(
            "SELECT * FROM {} {where_clause}{limit_clause}",
            statement.table_name
        );
        let delete_query = if statement.limit.is_some() {
            format!(
                "DELETE FROM {} WHERE rowid IN (SELECT rowid FROM {} {where_clause}{limit_clause})",
                statement.table_name, statement.table_name
            )
        } else {
            format!("DELETE FROM {} {where_clause}", statement.table_name)
        };

        run_duckdb_blocking("duckdb_delete", move || {
            let conn = connection.blocking_lock();

            let mut select_stmt = conn
                .prepare(&returning_query)
                .map_err(DuckDbDatabaseError::DuckDb)?;
            bind_values(&mut select_stmt, Some(&filter_values), false, 0)?;
            select_stmt
                .raw_execute()
                .map_err(DuckDbDatabaseError::DuckDb)?;
            let column_names = select_stmt.column_names();
            let rows_to_return = to_rows(&column_names, &mut select_stmt.raw_query())?;

            let mut delete_stmt = conn
                .prepare(&delete_query)
                .map_err(DuckDbDatabaseError::DuckDb)?;
            bind_values(&mut delete_stmt, Some(&filter_values), false, 0)?;
            delete_stmt
                .raw_execute()
                .map_err(DuckDbDatabaseError::DuckDb)?;

            Ok(rows_to_return)
        })
        .await
    }

    async fn exec_delete_first(
        &self,
        statement: &DeleteStatement<'_>,
    ) -> Result<Option<crate::Row>, DatabaseError> {
        let _operation_guard = self.lock_operation_gate().await;
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
        let _operation_guard = self.lock_operation_gate().await;
        let connection = self.get_connection();
        let column_names = statement
            .values
            .iter()
            .map(|(key, _)| format!("\"{key}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let insert_columns = if statement.values.is_empty() {
            String::new()
        } else {
            format!("({column_names})")
        };
        let sql = format!(
            "INSERT INTO {} {insert_columns} {} RETURNING *",
            statement.table_name,
            build_values_clause(&statement.values)
        );
        let params = exprs_to_values(&statement.values);

        run_duckdb_blocking("duckdb_insert", move || {
            let conn = connection.blocking_lock();
            let mut stmt = conn
                .prepare_cached(&sql)
                .map_err(DuckDbDatabaseError::DuckDb)?;
            bind_values(&mut stmt, Some(&params), false, 0)?;
            stmt.raw_execute().map_err(DuckDbDatabaseError::DuckDb)?;
            let column_names = stmt.column_names();
            let mut query = stmt.raw_query();
            let row = query
                .next()
                .map_err(DuckDbDatabaseError::DuckDb)?
                .map(|row| from_row(&column_names, row))
                .transpose()?
                .ok_or(DuckDbDatabaseError::NoRow)?;
            Ok(row)
        })
        .await
    }

    async fn exec_update(
        &self,
        statement: &UpdateStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        let _operation_guard = self.lock_operation_gate().await;
        let connection = self.get_connection();
        let select_query = statement.limit.map(|_| {
            format!(
                "SELECT rowid FROM {} {}",
                statement.table_name,
                build_where_clause(statement.filters.as_deref())
            )
        });
        let sql = format!(
            "UPDATE {} {} {} RETURNING *",
            statement.table_name,
            build_set_clause(&statement.values),
            build_update_where_clause(
                statement.filters.as_deref(),
                statement.limit,
                select_query.as_deref()
            ),
        );

        let update_values = statement
            .values
            .iter()
            .flat_map(|(_, value)| value.params().unwrap_or_default().into_iter().cloned())
            .map(Into::into)
            .collect::<Vec<DuckDbDatabaseValue>>();
        let mut filter_values = statement
            .filters
            .as_deref()
            .map(bexprs_to_values)
            .unwrap_or_default();
        if statement.limit.is_some() {
            filter_values.extend(filter_values.clone());
        }
        let params = [update_values, filter_values].concat();

        run_duckdb_blocking("duckdb_update", move || {
            let conn = connection.blocking_lock();
            let mut stmt = conn
                .prepare_cached(&sql)
                .map_err(DuckDbDatabaseError::DuckDb)?;
            bind_values(&mut stmt, Some(&params), false, 0)?;
            stmt.raw_execute().map_err(DuckDbDatabaseError::DuckDb)?;
            let column_names = stmt.column_names();
            to_rows(&column_names, &mut stmt.raw_query()).map_err(Into::into)
        })
        .await
    }

    async fn exec_update_first(
        &self,
        statement: &UpdateStatement<'_>,
    ) -> Result<Option<crate::Row>, DatabaseError> {
        let _operation_guard = self.lock_operation_gate().await;
        let connection = self.get_connection();
        let select_query = statement.limit.map(|_| {
            format!(
                "SELECT rowid FROM {} {}",
                statement.table_name,
                build_where_clause(statement.filters.as_deref())
            )
        });
        let sql = format!(
            "UPDATE {} {} {} RETURNING *",
            statement.table_name,
            build_set_clause(&statement.values),
            build_update_where_clause(
                statement.filters.as_deref(),
                statement.limit,
                select_query.as_deref()
            ),
        );

        let update_values = statement
            .values
            .iter()
            .flat_map(|(_, value)| value.params().unwrap_or_default().into_iter().cloned())
            .map(Into::into)
            .collect::<Vec<DuckDbDatabaseValue>>();
        let mut filter_values = statement
            .filters
            .as_deref()
            .map(bexprs_to_values)
            .unwrap_or_default();
        if statement.limit.is_some() {
            filter_values.extend(filter_values.clone());
        }
        let params = [update_values, filter_values].concat();

        run_duckdb_blocking("duckdb_update_first", move || {
            let conn = connection.blocking_lock();
            let mut stmt = conn
                .prepare_cached(&sql)
                .map_err(DuckDbDatabaseError::DuckDb)?;
            bind_values(&mut stmt, Some(&params), false, 0)?;
            stmt.raw_execute().map_err(DuckDbDatabaseError::DuckDb)?;
            let column_names = stmt.column_names();
            let mut query = stmt.raw_query();
            query
                .next()
                .map_err(DuckDbDatabaseError::DuckDb)?
                .map(|row| from_row(&column_names, row))
                .transpose()
                .map_err(Into::into)
        })
        .await
    }

    async fn exec_upsert(
        &self,
        statement: &UpsertStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        let _operation_guard = self.lock_operation_gate().await;
        let connection = self.get_connection();
        let (update_sql, update_params) = build_update_plan(
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            statement.limit,
        );
        let (insert_sql, insert_params) =
            build_insert_plan(statement.table_name, &statement.values);

        run_duckdb_blocking("duckdb_upsert", move || {
            let conn = connection.blocking_lock();
            execute_upsert_plan(
                &conn,
                &update_sql,
                &update_params,
                &insert_sql,
                &insert_params,
            )
            .map_err(Into::into)
        })
        .await
    }

    async fn exec_upsert_first(
        &self,
        statement: &UpsertStatement<'_>,
    ) -> Result<crate::Row, DatabaseError> {
        let _operation_guard = self.lock_operation_gate().await;
        let connection = self.get_connection();
        let (update_sql, update_params) = build_update_plan(
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            statement.limit,
        );
        let (insert_sql, insert_params) =
            build_insert_plan(statement.table_name, &statement.values);

        run_duckdb_blocking("duckdb_upsert_first", move || {
            let conn = connection.blocking_lock();
            execute_upsert_first_plan(
                &conn,
                &update_sql,
                &update_params,
                &insert_sql,
                &insert_params,
            )
            .map_err(Into::into)
        })
        .await
    }

    async fn exec_upsert_multi(
        &self,
        statement: &UpsertMultiStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        let _operation_guard = self.lock_operation_gate().await;
        let connection = self.get_connection();
        let plans = build_upsert_multi_chunk_plans(
            statement.table_name,
            statement
                .unique
                .as_ref()
                .ok_or(DuckDbDatabaseError::MissingUnique)?,
            &statement.values,
        )?;

        run_duckdb_blocking("duckdb_upsert_multi", move || {
            let conn = connection.blocking_lock();
            execute_upsert_multi_plans(&conn, &plans).map_err(Into::into)
        })
        .await
    }

    #[cfg(feature = "raw-sql")]
    async fn exec_raw(&self, statement: &str) -> Result<(), DatabaseError> {
        self.exec_raw_internal(statement).await
    }

    #[allow(clippy::significant_drop_tightening)]
    #[cfg(feature = "raw-sql")]
    async fn query_raw(&self, query: &str) -> Result<Vec<crate::Row>, DatabaseError> {
        self.query_raw_internal(query).await
    }

    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn crate::DatabaseTransaction>, DatabaseError> {
        let operation_gate_guard = self.lock_operation_gate().await;
        let connection = self.get_connection();
        self.begin_transaction_blocking(Arc::clone(&connection))
            .await?;

        Ok(Box::new(DuckDbTransaction::new_with_guard(
            connection,
            operation_gate_guard,
        )))
    }

    #[allow(clippy::significant_drop_tightening)]
    #[cfg(feature = "raw-sql")]
    async fn exec_raw_params(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<u64, DatabaseError> {
        self.exec_raw_params_internal(query, params).await
    }

    #[allow(clippy::significant_drop_tightening)]
    #[cfg(feature = "raw-sql")]
    async fn query_raw_params(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        self.query_raw_params_internal(query, params).await
    }

    #[cfg(feature = "schema")]
    async fn exec_create_table(
        &self,
        statement: &crate::schema::CreateTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let _operation_guard = self.lock_operation_gate().await;
        let connection = self.get_connection();
        let sql = build_create_table_sql(statement)?;
        run_duckdb_blocking("duckdb_exec_create_table", move || {
            exec_schema_ddl(&connection.blocking_lock(), &sql).map_err(Into::into)
        })
        .await
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_table(
        &self,
        statement: &crate::schema::DropTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let _operation_guard = self.lock_operation_gate().await;
        let connection = self.get_connection();
        let sql = build_drop_table_sql(statement);
        run_duckdb_blocking("duckdb_exec_drop_table", move || {
            exec_schema_ddl(&connection.blocking_lock(), &sql).map_err(Into::into)
        })
        .await
    }

    #[cfg(feature = "schema")]
    async fn exec_create_index(
        &self,
        statement: &crate::schema::CreateIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let _operation_guard = self.lock_operation_gate().await;
        let connection = self.get_connection();
        if statement.predicate.is_some() || !statement.ordered_columns.is_empty() {
            return Err(DatabaseError::UnsupportedOperation(
                "Partial indexes on this backend".into(),
            ));
        }
        let sql = build_create_index_sql(statement);
        run_duckdb_blocking("duckdb_exec_create_index", move || {
            exec_schema_ddl(&connection.blocking_lock(), &sql).map_err(Into::into)
        })
        .await
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_index(
        &self,
        statement: &crate::schema::DropIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let _operation_guard = self.lock_operation_gate().await;
        let connection = self.get_connection();
        let sql = build_drop_index_sql(statement);
        run_duckdb_blocking("duckdb_exec_drop_index", move || {
            exec_schema_ddl(&connection.blocking_lock(), &sql).map_err(Into::into)
        })
        .await
    }

    #[cfg(feature = "schema")]
    async fn exec_alter_table(
        &self,
        statement: &crate::schema::AlterTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        if !statement.column_checks.is_empty() {
            return Err(DatabaseError::UnsupportedOperation(
                "Add-column CHECK on this backend".into(),
            ));
        }
        let _operation_guard = self.lock_operation_gate().await;
        let connection = self.get_connection();
        let sqls = build_alter_table_sqls(statement);
        run_duckdb_blocking("duckdb_exec_alter_table", move || {
            let conn = connection.blocking_lock();
            for sql in sqls {
                exec_schema_ddl(&conn, &sql)?;
            }
            Ok(())
        })
        .await
    }

    #[cfg(feature = "schema")]
    async fn table_exists(&self, table_name: &str) -> Result<bool, DatabaseError> {
        let table_name = table_name.to_string();
        self.with_connection("duckdb_table_exists", move |conn| {
            let schema = duckdb_resolve_schema(conn, None)?;
            duckdb_table_exists(conn, &schema, &table_name)
        })
        .await
    }

    #[cfg(feature = "schema")]
    async fn list_tables(&self) -> Result<Vec<String>, DatabaseError> {
        self.with_connection("duckdb_list_tables", move |conn| {
            let schema = duckdb_resolve_schema(conn, None)?;
            duckdb_list_table_names(conn, &schema)
        })
        .await
    }

    #[cfg(feature = "schema")]
    async fn get_table_info(
        &self,
        table_name: &str,
    ) -> Result<Option<crate::schema::TableInfo>, DatabaseError> {
        self.get_relation_info(&crate::schema::RelationRef::new(table_name))
            .await
    }

    #[cfg(feature = "schema")]
    async fn get_table_columns(
        &self,
        table_name: &str,
    ) -> Result<Vec<crate::schema::ColumnInfo>, DatabaseError> {
        let table_name = table_name.to_string();
        self.with_connection("duckdb_get_table_columns", move |conn| {
            let schema = duckdb_resolve_schema(conn, None)?;
            duckdb_get_table_columns(conn, &schema, &table_name)
        })
        .await
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

    #[cfg(feature = "schema")]
    async fn list_schemas(&self) -> Result<Vec<crate::schema::SchemaInfo>, DatabaseError> {
        self.with_connection("duckdb_list_schemas", duckdb_list_schemas)
            .await
    }

    #[cfg(feature = "schema")]
    async fn list_relations(
        &self,
        schema: Option<&str>,
    ) -> Result<Vec<crate::schema::RelationSummary>, DatabaseError> {
        let schema = schema.map(ToString::to_string);
        self.with_connection("duckdb_list_relations", move |conn| {
            let schema = duckdb_resolve_schema(conn, schema.as_deref())?;
            duckdb_list_relations(conn, &schema)
        })
        .await
    }

    #[cfg(feature = "schema")]
    async fn get_relation_info(
        &self,
        relation: &crate::schema::RelationRef,
    ) -> Result<Option<crate::schema::TableInfo>, DatabaseError> {
        let relation = relation.clone();
        self.with_connection("duckdb_get_relation_info", move |conn| {
            let schema = duckdb_resolve_schema(conn, relation.schema.as_deref())?;
            duckdb_get_relation_info(conn, &schema, &relation.name)
        })
        .await
    }

    #[cfg(feature = "schema")]
    async fn get_view_definition(
        &self,
        relation: &crate::schema::RelationRef,
    ) -> Result<Option<String>, DatabaseError> {
        let relation = relation.clone();
        self.with_connection("duckdb_get_view_definition", move |conn| {
            let schema = duckdb_resolve_schema(conn, relation.schema.as_deref())?;
            duckdb_view_definition(conn, &schema, &relation.name)
        })
        .await
    }

    #[cfg(feature = "raw-sql")]
    async fn query_raw_result(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<crate::QueryResult, DatabaseError> {
        let (transformed_query, filtered_params) =
            duckdb_transform_query_for_params(query, params)?;
        let duckdb_params: Vec<DuckDbDatabaseValue> =
            filtered_params.into_iter().map(Into::into).collect();
        self.with_connection("duckdb_query_raw_result", move |conn| {
            duckdb_query_result(conn, &transformed_query, &duckdb_params)
        })
        .await
    }
}

// ---------------------------------------------------------------------------
// Database trait impl for transaction
// ---------------------------------------------------------------------------

#[async_trait]
#[allow(clippy::significant_drop_tightening)]
impl Database for DuckDbTransaction {
    async fn query(&self, query: &SelectQuery<'_>) -> Result<Vec<crate::Row>, DatabaseError> {
        if query
            .filters
            .iter()
            .flatten()
            .any(|filter| matches!(filter.expression_type(), ExpressionType::JsonTypeIs(_)))
        {
            return Err(DatabaseError::UnsupportedOperation(
                "SQLite JSON type predicate".into(),
            ));
        }
        if query.table_alias.is_some()
            || query.indexed_by.is_some()
            || query.projections.iter().any(|projection| {
                matches!(
                    projection.expression.expression_type(),
                    ExpressionType::BoundedText(_)
                )
            })
        {
            return Err(DatabaseError::UnsupportedOperation(
                "SQLite-only selection policy".into(),
            ));
        }

        let connection = Arc::clone(&self.connection);
        let sql = query.to_sql();
        let params = select_values(query);

        run_duckdb_blocking("duckdb_tx_select", move || {
            let conn = connection.blocking_lock();
            let mut statement = conn
                .prepare_cached(&sql)
                .map_err(DuckDbDatabaseError::DuckDb)?;
            bind_values(&mut statement, Some(&params), false, 0)?;
            statement
                .raw_execute()
                .map_err(DuckDbDatabaseError::DuckDb)?;
            let column_names = statement.column_names();
            to_rows(&column_names, &mut statement.raw_query()).map_err(Into::into)
        })
        .await
    }

    async fn query_first(
        &self,
        query: &SelectQuery<'_>,
    ) -> Result<Option<crate::Row>, DatabaseError> {
        if query
            .filters
            .iter()
            .flatten()
            .any(|filter| matches!(filter.expression_type(), ExpressionType::JsonTypeIs(_)))
        {
            return Err(DatabaseError::UnsupportedOperation(
                "SQLite JSON type predicate".into(),
            ));
        }
        if query.table_alias.is_some() {
            return Err(DatabaseError::UnsupportedOperation(
                "Table alias on this execution path".into(),
            ));
        }
        let connection = Arc::clone(&self.connection);
        let sql = format!(
            "SELECT * FROM ({}) AS switchy_first LIMIT 1",
            query.to_sql()
        );
        let params = select_values(query);

        run_duckdb_blocking("duckdb_tx_select_first", move || {
            let conn = connection.blocking_lock();
            let mut statement = conn
                .prepare_cached(&sql)
                .map_err(DuckDbDatabaseError::DuckDb)?;
            bind_values(&mut statement, Some(&params), false, 0)?;
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
                .map_err(Into::into)
        })
        .await
    }

    async fn exec_delete(
        &self,
        statement: &DeleteStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        let connection = Arc::clone(&self.connection);
        let where_clause = build_where_clause(statement.filters.as_deref());
        let filter_values = statement
            .filters
            .as_deref()
            .map(bexprs_to_values)
            .unwrap_or_default();
        let limit_clause = statement
            .limit
            .map_or_else(String::new, |l| format!(" LIMIT {l}"));
        let returning_query = format!(
            "SELECT * FROM {} {where_clause}{limit_clause}",
            statement.table_name
        );
        let delete_query = if statement.limit.is_some() {
            format!(
                "DELETE FROM {} WHERE rowid IN (SELECT rowid FROM {} {where_clause}{limit_clause})",
                statement.table_name, statement.table_name
            )
        } else {
            format!("DELETE FROM {} {where_clause}", statement.table_name)
        };

        run_duckdb_blocking("duckdb_tx_delete", move || {
            let conn = connection.blocking_lock();
            let mut select_stmt = conn
                .prepare(&returning_query)
                .map_err(DuckDbDatabaseError::DuckDb)?;
            bind_values(&mut select_stmt, Some(&filter_values), false, 0)?;
            select_stmt
                .raw_execute()
                .map_err(DuckDbDatabaseError::DuckDb)?;
            let column_names = select_stmt.column_names();
            let rows_to_return = to_rows(&column_names, &mut select_stmt.raw_query())?;

            let mut delete_stmt = conn
                .prepare(&delete_query)
                .map_err(DuckDbDatabaseError::DuckDb)?;
            bind_values(&mut delete_stmt, Some(&filter_values), false, 0)?;
            delete_stmt
                .raw_execute()
                .map_err(DuckDbDatabaseError::DuckDb)?;

            Ok(rows_to_return)
        })
        .await
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
        let connection = Arc::clone(&self.connection);
        let column_names = statement
            .values
            .iter()
            .map(|(key, _)| format!("\"{key}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let insert_columns = if statement.values.is_empty() {
            String::new()
        } else {
            format!("({column_names})")
        };
        let sql = format!(
            "INSERT INTO {} {insert_columns} {} RETURNING *",
            statement.table_name,
            build_values_clause(&statement.values)
        );
        let params = exprs_to_values(&statement.values);

        run_duckdb_blocking("duckdb_tx_insert", move || {
            let conn = connection.blocking_lock();
            let mut stmt = conn
                .prepare_cached(&sql)
                .map_err(DuckDbDatabaseError::DuckDb)?;
            bind_values(&mut stmt, Some(&params), false, 0)?;
            stmt.raw_execute().map_err(DuckDbDatabaseError::DuckDb)?;
            let column_names = stmt.column_names();
            let mut query = stmt.raw_query();
            let row = query
                .next()
                .map_err(DuckDbDatabaseError::DuckDb)?
                .map(|row| from_row(&column_names, row))
                .transpose()?
                .ok_or(DuckDbDatabaseError::NoRow)?;
            Ok(row)
        })
        .await
    }

    async fn exec_update(
        &self,
        statement: &UpdateStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        let connection = Arc::clone(&self.connection);
        let select_query = statement.limit.map(|_| {
            format!(
                "SELECT rowid FROM {} {}",
                statement.table_name,
                build_where_clause(statement.filters.as_deref())
            )
        });
        let sql = format!(
            "UPDATE {} {} {} RETURNING *",
            statement.table_name,
            build_set_clause(&statement.values),
            build_update_where_clause(
                statement.filters.as_deref(),
                statement.limit,
                select_query.as_deref()
            ),
        );

        let update_values = statement
            .values
            .iter()
            .flat_map(|(_, value)| value.params().unwrap_or_default().into_iter().cloned())
            .map(Into::into)
            .collect::<Vec<DuckDbDatabaseValue>>();
        let mut filter_values = statement
            .filters
            .as_deref()
            .map(bexprs_to_values)
            .unwrap_or_default();
        if statement.limit.is_some() {
            filter_values.extend(filter_values.clone());
        }
        let params = [update_values, filter_values].concat();

        run_duckdb_blocking("duckdb_tx_update", move || {
            let conn = connection.blocking_lock();
            let mut stmt = conn
                .prepare_cached(&sql)
                .map_err(DuckDbDatabaseError::DuckDb)?;
            bind_values(&mut stmt, Some(&params), false, 0)?;
            stmt.raw_execute().map_err(DuckDbDatabaseError::DuckDb)?;
            let column_names = stmt.column_names();
            to_rows(&column_names, &mut stmt.raw_query()).map_err(Into::into)
        })
        .await
    }

    async fn exec_update_first(
        &self,
        statement: &UpdateStatement<'_>,
    ) -> Result<Option<crate::Row>, DatabaseError> {
        let connection = Arc::clone(&self.connection);
        let select_query = statement.limit.map(|_| {
            format!(
                "SELECT rowid FROM {} {}",
                statement.table_name,
                build_where_clause(statement.filters.as_deref())
            )
        });
        let sql = format!(
            "UPDATE {} {} {} RETURNING *",
            statement.table_name,
            build_set_clause(&statement.values),
            build_update_where_clause(
                statement.filters.as_deref(),
                statement.limit,
                select_query.as_deref()
            ),
        );

        let update_values = statement
            .values
            .iter()
            .flat_map(|(_, value)| value.params().unwrap_or_default().into_iter().cloned())
            .map(Into::into)
            .collect::<Vec<DuckDbDatabaseValue>>();
        let mut filter_values = statement
            .filters
            .as_deref()
            .map(bexprs_to_values)
            .unwrap_or_default();
        if statement.limit.is_some() {
            filter_values.extend(filter_values.clone());
        }
        let params = [update_values, filter_values].concat();

        run_duckdb_blocking("duckdb_tx_update_first", move || {
            let conn = connection.blocking_lock();
            let mut stmt = conn
                .prepare_cached(&sql)
                .map_err(DuckDbDatabaseError::DuckDb)?;
            bind_values(&mut stmt, Some(&params), false, 0)?;
            stmt.raw_execute().map_err(DuckDbDatabaseError::DuckDb)?;
            let column_names = stmt.column_names();
            let mut query = stmt.raw_query();
            query
                .next()
                .map_err(DuckDbDatabaseError::DuckDb)?
                .map(|row| from_row(&column_names, row))
                .transpose()
                .map_err(Into::into)
        })
        .await
    }

    async fn exec_upsert(
        &self,
        statement: &UpsertStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        let connection = Arc::clone(&self.connection);
        let (update_sql, update_params) = build_update_plan(
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            statement.limit,
        );
        let (insert_sql, insert_params) =
            build_insert_plan(statement.table_name, &statement.values);

        run_duckdb_blocking("duckdb_tx_upsert", move || {
            let conn = connection.blocking_lock();
            execute_upsert_plan(
                &conn,
                &update_sql,
                &update_params,
                &insert_sql,
                &insert_params,
            )
            .map_err(Into::into)
        })
        .await
    }

    async fn exec_upsert_first(
        &self,
        statement: &UpsertStatement<'_>,
    ) -> Result<crate::Row, DatabaseError> {
        let connection = Arc::clone(&self.connection);
        let (update_sql, update_params) = build_update_plan(
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            statement.limit,
        );
        let (insert_sql, insert_params) =
            build_insert_plan(statement.table_name, &statement.values);

        run_duckdb_blocking("duckdb_tx_upsert_first", move || {
            let conn = connection.blocking_lock();
            execute_upsert_first_plan(
                &conn,
                &update_sql,
                &update_params,
                &insert_sql,
                &insert_params,
            )
            .map_err(Into::into)
        })
        .await
    }

    async fn exec_upsert_multi(
        &self,
        statement: &UpsertMultiStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        let connection = Arc::clone(&self.connection);
        let plans = build_upsert_multi_chunk_plans(
            statement.table_name,
            statement
                .unique
                .as_ref()
                .ok_or(DuckDbDatabaseError::MissingUnique)?,
            &statement.values,
        )?;

        run_duckdb_blocking("duckdb_tx_upsert_multi", move || {
            let conn = connection.blocking_lock();
            execute_upsert_multi_plans(&conn, &plans).map_err(Into::into)
        })
        .await
    }

    #[cfg(feature = "raw-sql")]
    async fn exec_raw(&self, statement: &str) -> Result<(), DatabaseError> {
        self.exec_raw_internal(statement).await
    }

    #[allow(clippy::significant_drop_tightening)]
    #[cfg(feature = "raw-sql")]
    async fn query_raw(&self, query: &str) -> Result<Vec<crate::Row>, DatabaseError> {
        self.query_raw_internal(query).await
    }

    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn crate::DatabaseTransaction>, DatabaseError> {
        Err(DatabaseError::AlreadyInTransaction)
    }

    #[allow(clippy::significant_drop_tightening)]
    #[cfg(feature = "raw-sql")]
    async fn exec_raw_params(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<u64, DatabaseError> {
        self.exec_raw_params_internal(query, params).await
    }

    #[allow(clippy::significant_drop_tightening)]
    #[cfg(feature = "raw-sql")]
    async fn query_raw_params(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        self.query_raw_params_internal(query, params).await
    }

    #[cfg(feature = "schema")]
    async fn exec_create_table(
        &self,
        statement: &crate::schema::CreateTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let connection = Arc::clone(&self.connection);
        let sql = build_create_table_sql(statement)?;
        run_duckdb_blocking("duckdb_tx_exec_create_table", move || {
            exec_schema_ddl(&connection.blocking_lock(), &sql).map_err(Into::into)
        })
        .await
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_table(
        &self,
        statement: &crate::schema::DropTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let connection = Arc::clone(&self.connection);
        let sql = build_drop_table_sql(statement);
        run_duckdb_blocking("duckdb_tx_exec_drop_table", move || {
            exec_schema_ddl(&connection.blocking_lock(), &sql).map_err(Into::into)
        })
        .await
    }

    #[cfg(feature = "schema")]
    async fn exec_create_index(
        &self,
        statement: &crate::schema::CreateIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let connection = Arc::clone(&self.connection);
        if statement.predicate.is_some() || !statement.ordered_columns.is_empty() {
            return Err(DatabaseError::UnsupportedOperation(
                "Partial indexes on this backend".into(),
            ));
        }
        let sql = build_create_index_sql(statement);
        run_duckdb_blocking("duckdb_tx_exec_create_index", move || {
            exec_schema_ddl(&connection.blocking_lock(), &sql).map_err(Into::into)
        })
        .await
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_index(
        &self,
        statement: &crate::schema::DropIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let connection = Arc::clone(&self.connection);
        let sql = build_drop_index_sql(statement);
        run_duckdb_blocking("duckdb_tx_exec_drop_index", move || {
            exec_schema_ddl(&connection.blocking_lock(), &sql).map_err(Into::into)
        })
        .await
    }

    #[cfg(feature = "schema")]
    async fn exec_alter_table(
        &self,
        statement: &crate::schema::AlterTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        if !statement.column_checks.is_empty() {
            return Err(DatabaseError::UnsupportedOperation(
                "Add-column CHECK on this backend".into(),
            ));
        }
        let connection = Arc::clone(&self.connection);
        let sqls = build_alter_table_sqls(statement);
        run_duckdb_blocking("duckdb_tx_exec_alter_table", move || {
            let conn = connection.blocking_lock();
            for sql in sqls {
                exec_schema_ddl(&conn, &sql)?;
            }
            Ok(())
        })
        .await
    }

    #[cfg(feature = "schema")]
    async fn table_exists(&self, table_name: &str) -> Result<bool, DatabaseError> {
        let table_name = table_name.to_string();
        self.with_connection("duckdb_tx_table_exists", move |conn| {
            let schema = duckdb_resolve_schema(conn, None)?;
            duckdb_table_exists(conn, &schema, &table_name)
        })
        .await
    }

    #[cfg(feature = "schema")]
    async fn list_tables(&self) -> Result<Vec<String>, DatabaseError> {
        self.with_connection("duckdb_tx_list_tables", move |conn| {
            let schema = duckdb_resolve_schema(conn, None)?;
            duckdb_list_table_names(conn, &schema)
        })
        .await
    }

    #[cfg(feature = "schema")]
    async fn get_table_info(
        &self,
        table_name: &str,
    ) -> Result<Option<crate::schema::TableInfo>, DatabaseError> {
        self.get_relation_info(&crate::schema::RelationRef::new(table_name))
            .await
    }

    #[cfg(feature = "schema")]
    async fn get_table_columns(
        &self,
        table_name: &str,
    ) -> Result<Vec<crate::schema::ColumnInfo>, DatabaseError> {
        let table_name = table_name.to_string();
        self.with_connection("duckdb_tx_get_table_columns", move |conn| {
            let schema = duckdb_resolve_schema(conn, None)?;
            duckdb_get_table_columns(conn, &schema, &table_name)
        })
        .await
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

    #[cfg(feature = "schema")]
    async fn list_schemas(&self) -> Result<Vec<crate::schema::SchemaInfo>, DatabaseError> {
        self.with_connection("duckdb_tx_list_schemas", duckdb_list_schemas)
            .await
    }

    #[cfg(feature = "schema")]
    async fn list_relations(
        &self,
        schema: Option<&str>,
    ) -> Result<Vec<crate::schema::RelationSummary>, DatabaseError> {
        let schema = schema.map(ToString::to_string);
        self.with_connection("duckdb_tx_list_relations", move |conn| {
            let schema = duckdb_resolve_schema(conn, schema.as_deref())?;
            duckdb_list_relations(conn, &schema)
        })
        .await
    }

    #[cfg(feature = "schema")]
    async fn get_relation_info(
        &self,
        relation: &crate::schema::RelationRef,
    ) -> Result<Option<crate::schema::TableInfo>, DatabaseError> {
        let relation = relation.clone();
        self.with_connection("duckdb_tx_get_relation_info", move |conn| {
            let schema = duckdb_resolve_schema(conn, relation.schema.as_deref())?;
            duckdb_get_relation_info(conn, &schema, &relation.name)
        })
        .await
    }

    #[cfg(feature = "schema")]
    async fn get_view_definition(
        &self,
        relation: &crate::schema::RelationRef,
    ) -> Result<Option<String>, DatabaseError> {
        let relation = relation.clone();
        self.with_connection("duckdb_tx_get_view_definition", move |conn| {
            let schema = duckdb_resolve_schema(conn, relation.schema.as_deref())?;
            duckdb_view_definition(conn, &schema, &relation.name)
        })
        .await
    }

    #[cfg(feature = "raw-sql")]
    async fn query_raw_result(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<crate::QueryResult, DatabaseError> {
        let (transformed_query, filtered_params) =
            duckdb_transform_query_for_params(query, params)?;
        let duckdb_params: Vec<DuckDbDatabaseValue> =
            filtered_params.into_iter().map(Into::into).collect();
        self.with_connection("duckdb_tx_query_raw_result", move |conn| {
            duckdb_query_result(conn, &transformed_query, &duckdb_params)
        })
        .await
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

        let connection = Arc::clone(&self.connection);
        run_duckdb_blocking("duckdb_commit_transaction", move || {
            connection
                .blocking_lock()
                .execute_batch("COMMIT")
                .map_err(DuckDbDatabaseError::DuckDb)?;
            Ok(())
        })
        .await?;

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

        let connection = Arc::clone(&self.connection);
        run_duckdb_blocking("duckdb_rollback_transaction", move || {
            connection
                .blocking_lock()
                .execute_batch("ROLLBACK")
                .map_err(DuckDbDatabaseError::DuckDb)?;
            Ok(())
        })
        .await?;

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
                let fk_map =
                    query_foreign_keys(&conn, &duckdb_resolve_schema(&conn, None)?, check_table)?;
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
            let fk_map =
                query_foreign_keys(&conn, &duckdb_resolve_schema(&conn, None)?, check_table)?;
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
            let fk_map =
                query_foreign_keys(&conn, &duckdb_resolve_schema(&conn, None)?, check_table)?;
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
        self.0.is_null()
    }

    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::DatabaseValue(self)
    }
}

#[cfg(feature = "raw-sql")]
impl DuckDbDatabase {
    pub(crate) async fn exec_raw_internal(&self, statement: &str) -> Result<(), DatabaseError> {
        let _operation_guard = self.lock_operation_gate().await;
        let connection = self.get_connection();
        let statement = statement.to_string();

        run_duckdb_blocking("duckdb_exec_raw", move || {
            log::trace!("exec_raw: query:\n{statement}");
            connection
                .blocking_lock()
                .execute_batch(&statement)
                .map_err(DuckDbDatabaseError::DuckDb)?;
            Ok(())
        })
        .await
    }
}

#[cfg(feature = "raw-sql")]
impl DuckDbDatabase {
    pub(crate) async fn query_raw_internal(
        &self,
        query: &str,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        let _operation_guard = self.lock_operation_gate().await;
        let connection = self.get_connection();
        let query = query.to_string();

        run_duckdb_blocking("duckdb_query_raw", move || {
            let connection = connection.blocking_lock();

            let mut stmt = connection
                .prepare(&query)
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

            let mut rows = stmt
                .query([])
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

            let column_names: Vec<String> = rows
                .as_ref()
                .map(::duckdb::Statement::column_names)
                .unwrap_or_default();

            let result = to_rows(&column_names, &mut rows)
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()));
            drop(rows);
            drop(stmt);
            drop(connection);
            result
        })
        .await
    }
}

#[cfg(feature = "raw-sql")]
impl DuckDbDatabase {
    pub(crate) async fn exec_raw_params_internal(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<u64, DatabaseError> {
        let _operation_guard = self.lock_operation_gate().await;
        let (transformed_query, filtered_params) =
            duckdb_transform_query_for_params(query, params)?;

        let connection = self.get_connection();
        let duckdb_params: Vec<DuckDbDatabaseValue> =
            filtered_params.iter().map(|p| p.clone().into()).collect();

        let original_query = query.to_string();

        run_duckdb_blocking("duckdb_exec_raw_params", move || {
            let connection_guard = connection.blocking_lock();

            let mut stmt = connection_guard
                .prepare(&transformed_query)
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

            log::trace!(
                "\
                exec_raw_params: query:\n\
                '{transformed_query}' (transformed from '{original_query}')\n\
                raw: {duckdb_params:?}\
                "
            );

            bind_values_raw(&mut stmt, Some(&duckdb_params), 0)
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

            let rows_affected = stmt
                .raw_execute()
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

            drop(stmt);
            drop(connection_guard);
            Ok(rows_affected as u64)
        })
        .await
    }
}

#[cfg(feature = "raw-sql")]
impl DuckDbDatabase {
    pub(crate) async fn query_raw_params_internal(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        let _operation_guard = self.lock_operation_gate().await;
        let (transformed_query, filtered_params) =
            duckdb_transform_query_for_params(query, params)?;

        let connection = self.get_connection();
        let duckdb_params: Vec<DuckDbDatabaseValue> =
            filtered_params.iter().map(|p| p.clone().into()).collect();

        let original_query = query.to_string();

        run_duckdb_blocking("duckdb_query_raw_params", move || {
            let connection_guard = connection.blocking_lock();

            let mut stmt = connection_guard
                .prepare(&transformed_query)
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

            log::trace!(
                "\
                query_raw_params: query:\n\
                '{transformed_query}' (transformed from '{original_query}')\n\
                raw: {duckdb_params:?}\
                "
            );

            bind_values_raw(&mut stmt, Some(&duckdb_params), 0)
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

            stmt.raw_execute()
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
            let column_names = stmt.column_names();

            let result = to_rows(&column_names, &mut stmt.raw_query())
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()));
            drop(stmt);
            drop(connection_guard);
            result
        })
        .await
    }
}

#[cfg(feature = "raw-sql")]
impl DuckDbTransaction {
    pub(crate) async fn exec_raw_internal(&self, statement: &str) -> Result<(), DatabaseError> {
        let connection = Arc::clone(&self.connection);
        let statement = statement.to_string();
        run_duckdb_blocking("duckdb_tx_exec_raw", move || {
            connection
                .blocking_lock()
                .execute_batch(&statement)
                .map_err(DuckDbDatabaseError::DuckDb)?;
            Ok(())
        })
        .await
    }
}

#[cfg(feature = "raw-sql")]
impl DuckDbTransaction {
    pub(crate) async fn query_raw_internal(
        &self,
        query: &str,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        let connection = Arc::clone(&self.connection);
        let query = query.to_string();

        run_duckdb_blocking("duckdb_tx_query_raw", move || {
            let connection = connection.blocking_lock();

            let mut stmt = connection
                .prepare(&query)
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

            let mut rows = stmt
                .query([])
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

            let column_names: Vec<String> = rows
                .as_ref()
                .map(::duckdb::Statement::column_names)
                .unwrap_or_default();

            let result = to_rows(&column_names, &mut rows)
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()));
            drop(rows);
            drop(stmt);
            drop(connection);
            result
        })
        .await
    }
}

#[cfg(feature = "raw-sql")]
impl DuckDbTransaction {
    pub(crate) async fn exec_raw_params_internal(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<u64, DatabaseError> {
        let (transformed_query, filtered_params) =
            duckdb_transform_query_for_params(query, params)?;

        let connection = Arc::clone(&self.connection);

        let duckdb_params: Vec<DuckDbDatabaseValue> =
            filtered_params.iter().map(|p| p.clone().into()).collect();

        run_duckdb_blocking("duckdb_tx_exec_raw_params", move || {
            let connection_guard = connection.blocking_lock();

            let mut stmt = connection_guard
                .prepare(&transformed_query)
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

            bind_values_raw(&mut stmt, Some(&duckdb_params), 0)
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

            let rows_affected = stmt
                .raw_execute()
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

            drop(stmt);
            drop(connection_guard);
            Ok(rows_affected as u64)
        })
        .await
    }
}

#[cfg(feature = "raw-sql")]
impl DuckDbTransaction {
    pub(crate) async fn query_raw_params_internal(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        let (transformed_query, filtered_params) =
            duckdb_transform_query_for_params(query, params)?;

        let connection = Arc::clone(&self.connection);

        let duckdb_params: Vec<DuckDbDatabaseValue> =
            filtered_params.iter().map(|p| p.clone().into()).collect();

        run_duckdb_blocking("duckdb_tx_query_raw_params", move || {
            let connection_guard = connection.blocking_lock();

            let mut stmt = connection_guard
                .prepare(&transformed_query)
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

            bind_values_raw(&mut stmt, Some(&duckdb_params), 0)
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

            stmt.raw_execute()
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
            let column_names = stmt.column_names();

            let result = to_rows(&column_names, &mut stmt.raw_query())
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()));
            drop(stmt);
            drop(connection_guard);
            result
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DatabaseValue;
    use crate::query::{FilterableQuery, where_eq};
    use std::sync::Arc;
    use switchy_async::sync::Mutex;

    #[test]
    fn now_uses_utc_in_non_utc_sessions() {
        let conn = Connection::open_in_memory().unwrap();
        for timezone in ["UTC", "America/New_York"] {
            conn.execute_batch(&format!("SET TimeZone = '{timezone}'"))
                .unwrap();
            let expression = DatabaseValue::Now.to_sql();
            let difference: f64 = conn
                .query_row(
                    &format!("SELECT epoch({expression}) - epoch(NOW())"),
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(difference.abs() < 0.001, "{timezone}: {difference}");
        }
    }

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

    async fn assert_select_sort_bindings(db: &dyn Database) {
        for first_only in [false, true] {
            let query = db.select("test_table").where_gte("value", 1).sorts(vec![
                Sort {
                    expression: Box::new(where_eq(
                        crate::query::identifier("value"),
                        DatabaseValue::Int64(2),
                    )),
                    direction: crate::query::SortDirection::Desc,
                },
                crate::query::sort(
                    crate::query::identifier("value"),
                    crate::query::SortDirection::Asc,
                ),
            ]);
            let row = if first_only {
                query.execute_first(db).await.unwrap().unwrap()
            } else {
                query.execute(db).await.unwrap().remove(0)
            };
            assert_eq!(
                row.get("name"),
                Some(DatabaseValue::String("second".into()))
            );
        }
    }

    #[switchy_async::test]
    async fn select_binds_sort_values_after_filters_in_database_and_transaction() {
        let db = create_test_db();
        for (name, value) in [("first", 1), ("second", 2)] {
            db.insert("test_table")
                .value("name", name)
                .value("value", value)
                .execute(&db)
                .await
                .unwrap();
        }
        assert_select_sort_bindings(&db).await;
        let tx = db.begin_transaction().await.unwrap();
        assert_select_sort_bindings(&*tx).await;
        tx.rollback().await.unwrap();
        db.close().await.unwrap();
    }

    #[cfg(not(feature = "raw-sql"))]
    #[switchy_async::test]
    async fn select_rejects_fragment_names_in_database_and_transaction() {
        let db = create_test_db();
        assert!(
            crate::query::select("test_table")
                .execute(&db)
                .await
                .is_ok()
        );
        for name in ["test_table WHERE false", "test_table AS other"] {
            assert!(crate::query::select(name).execute(&db).await.is_err());
            assert!(crate::query::select(name).execute_first(&db).await.is_err());
        }
        for name in ["id + 1", "id AS renamed"] {
            let columns = [name];
            let query = crate::query::select("test_table").columns(&columns);
            assert!(query.execute(&db).await.is_err());
            let query = crate::query::select("test_table").columns(&columns);
            assert!(query.execute_first(&db).await.is_err());
        }
        let tx = db.begin_transaction().await.unwrap();
        assert!(
            crate::query::select("test_table")
                .execute(&*tx)
                .await
                .is_ok()
        );
        for name in ["test_table WHERE false", "test_table AS other"] {
            assert!(crate::query::select(name).execute(&*tx).await.is_err());
            assert!(
                crate::query::select(name)
                    .execute_first(&*tx)
                    .await
                    .is_err()
            );
        }
        for name in ["id + 1", "id AS renamed"] {
            let columns = [name];
            let query = crate::query::select("test_table").columns(&columns);
            assert!(query.execute(&*tx).await.is_err());
            let query = crate::query::select("test_table").columns(&columns);
            assert!(query.execute_first(&*tx).await.is_err());
        }
        tx.rollback().await.unwrap();
        db.close().await.unwrap();
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

    #[cfg(feature = "raw-sql")]
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

    #[cfg(feature = "raw-sql")]
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

    #[cfg(feature = "raw-sql")]
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

    #[cfg(feature = "raw-sql")]
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

    #[cfg(feature = "raw-sql")]
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

    #[cfg(feature = "raw-sql")]
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
        assert_ne!(columns.len(), 0);

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

    #[cfg(feature = "raw-sql")]
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
