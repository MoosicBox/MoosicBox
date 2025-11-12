use std::{
    ops::Deref,
    pin::Pin,
    sync::{
        Arc, LazyLock,
        atomic::{AtomicBool, AtomicU16, Ordering},
    },
};

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use sqlx::{
    Column, Executor, PgPool, Postgres, Row, Statement, Transaction, TypeInfo, Value, ValueRef,
    pool::PoolConnection,
    postgres::{PgArguments, PgRow, PgValueRef},
    query::Query,
};
use sqlx_postgres::PgConnection;
use thiserror::Error;
use tokio::sync::Mutex;

#[cfg(feature = "schema")]
use super::postgres_introspection::{
    postgres_sqlx_column_exists, postgres_sqlx_get_table_columns, postgres_sqlx_get_table_info,
    postgres_sqlx_list_tables, postgres_sqlx_table_exists,
};

use crate::{
    Database, DatabaseError, DatabaseValue, DeleteStatement, InsertStatement, SelectQuery,
    UpdateStatement, UpsertMultiStatement, UpsertStatement,
    query::{BooleanExpression, Expression, ExpressionType, Join, Sort, SortDirection},
    sql_interval::SqlInterval,
};

/// Format `SqlInterval` as `PostgreSQL` interval string for parameter binding
fn postgres_interval_to_string_sqlx(interval: &SqlInterval) -> String {
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

    // Handle seconds with nanoseconds
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
        "0".to_string()
    } else {
        parts.join(" ")
    }
}

trait ToSql {
    fn to_sql(&self, index: &AtomicU16) -> String;
}

impl<T: Expression + ?Sized> ToSql for T {
    #[allow(clippy::too_many_lines)]
    fn to_sql(&self, index: &AtomicU16) -> String {
        match self.expression_type() {
            ExpressionType::Eq(value) => {
                if value.right.is_null() {
                    format!(
                        "({} IS {})",
                        value.left.to_sql(index),
                        value.right.to_sql(index)
                    )
                } else {
                    format!(
                        "({} = {})",
                        value.left.to_sql(index),
                        value.right.to_sql(index)
                    )
                }
            }
            ExpressionType::Gt(value) => {
                if value.right.is_null() {
                    panic!("Invalid > comparison with NULL");
                } else {
                    format!(
                        "({} > {})",
                        value.left.to_sql(index),
                        value.right.to_sql(index)
                    )
                }
            }
            ExpressionType::In(value) => {
                format!(
                    "{} IN ({})",
                    value.left.to_sql(index),
                    value.values.to_sql(index)
                )
            }
            ExpressionType::NotIn(value) => {
                format!(
                    "{} NOT IN ({})",
                    value.left.to_sql(index),
                    value.values.to_sql(index)
                )
            }
            ExpressionType::Lt(value) => {
                if value.right.is_null() {
                    panic!("Invalid < comparison with NULL");
                } else {
                    format!(
                        "({} < {})",
                        value.left.to_sql(index),
                        value.right.to_sql(index)
                    )
                }
            }
            ExpressionType::Or(value) => format!(
                "({})",
                value
                    .conditions
                    .iter()
                    .map(|x| x.to_sql(index))
                    .collect::<Vec<_>>()
                    .join(" OR ")
            ),
            ExpressionType::And(value) => format!(
                "({})",
                value
                    .conditions
                    .iter()
                    .map(|x| x.to_sql(index))
                    .collect::<Vec<_>>()
                    .join(" AND ")
            ),
            ExpressionType::Gte(value) => {
                if value.right.is_null() {
                    panic!("Invalid >= comparison with NULL");
                } else {
                    format!(
                        "({} >= {})",
                        value.left.to_sql(index),
                        value.right.to_sql(index)
                    )
                }
            }
            ExpressionType::Lte(value) => {
                if value.right.is_null() {
                    panic!("Invalid <= comparison with NULL");
                } else {
                    format!(
                        "({} <= {})",
                        value.left.to_sql(index),
                        value.right.to_sql(index)
                    )
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
                value.expression.to_sql(index),
                match value.direction {
                    SortDirection::Asc => "ASC",
                    SortDirection::Desc => "DESC",
                }
            ),
            ExpressionType::NotEq(value) => {
                if value.right.is_null() {
                    format!(
                        "({} IS NOT {})",
                        value.left.to_sql(index),
                        value.right.to_sql(index)
                    )
                } else {
                    format!(
                        "({} != {})",
                        value.left.to_sql(index),
                        value.right.to_sql(index)
                    )
                }
            }
            ExpressionType::InList(value) => value
                .values
                .iter()
                .map(|value| value.to_sql(index))
                .collect::<Vec<_>>()
                .join(","),
            ExpressionType::Coalesce(value) => format!(
                "COALESCE({})",
                value
                    .values
                    .iter()
                    .map(|value| value.to_sql(index))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            ExpressionType::Literal(value) => value.value.clone(),
            ExpressionType::Identifier(value) => format_identifier(&value.value),
            ExpressionType::SelectQuery(value) => {
                let joins = value.joins.as_ref().map_or_else(String::new, |joins| {
                    joins
                        .iter()
                        .map(|x| x.to_sql(index))
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
                                .map(|x| format!("({})", x.to_sql(index)))
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
                                .map(|x| x.to_sql(index))
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
                    value
                        .columns
                        .iter()
                        .map(|x| format_identifier(x))
                        .collect::<Vec<_>>()
                        .join(", "),
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
                | DatabaseValue::Int64Opt(None)
                | DatabaseValue::UInt64Opt(None)
                | DatabaseValue::Real64Opt(None)
                | DatabaseValue::Real32Opt(None) => "NULL".to_string(),
                #[cfg(feature = "decimal")]
                DatabaseValue::DecimalOpt(None) => "NULL".to_string(),
                #[cfg(feature = "uuid")]
                DatabaseValue::UuidOpt(None) => "NULL".to_string(),
                DatabaseValue::Now => "NOW()".to_string(),
                DatabaseValue::NowPlus(_) => {
                    // This should never be reached - NowPlus is transformed to (NOW() + $N::interval)
                    unreachable!(
                        "NowPlus must be transformed to (NOW() + $N::interval), not used as direct parameter"
                    )
                }
                _ => {
                    let pos = index.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                    format!("${pos}")
                }
            },
        }
    }
}

/// `PostgreSQL` database transaction using `SQLx`
///
/// Represents an active transaction on a `PostgreSQL` connection. Provides ACID guarantees
/// for a series of database operations. Must be explicitly committed or rolled back.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct PostgresSqlxTransaction {
    transaction: Arc<Mutex<Option<Transaction<'static, Postgres>>>>,
}

impl PostgresSqlxTransaction {
    /// Creates a new `PostgreSQL` transaction from an `SQLx` transaction
    #[must_use]
    pub fn new(transaction: Transaction<'static, Postgres>) -> Self {
        Self {
            transaction: Arc::new(Mutex::new(Some(transaction))),
        }
    }
}

/// `PostgreSQL` database connection pool using `SQLx`
///
/// Manages a pool of `PostgreSQL` connections for efficient connection reuse
/// and concurrent query execution.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct PostgresSqlxDatabase {
    pool: Arc<Mutex<PgPool>>,
    #[allow(clippy::type_complexity)]
    connection: Arc<Mutex<Option<Arc<Mutex<PoolConnection<Postgres>>>>>>,
}

impl PostgresSqlxDatabase {
    /// Creates a new `PostgreSQL` database instance from an `SQLx` connection pool
    pub fn new(pool: Arc<Mutex<PgPool>>) -> Self {
        Self {
            pool,
            connection: Arc::new(Mutex::new(None)),
        }
    }

    /// Gets a connection from the pool, reusing existing connection if available
    ///
    /// # Errors
    ///
    /// Will return `Err` if cannot get a connection
    pub async fn get_connection(
        &self,
    ) -> Result<Arc<Mutex<PoolConnection<Postgres>>>, SqlxDatabaseError> {
        let connection = { self.connection.lock().await.clone() };

        if let Some(connection) = connection {
            log::trace!("Returning existing connection from postgres db pool");
            return Ok(connection);
        }

        log::debug!("Fetching new connection from postgres db pool");
        let connection = Arc::new(Mutex::new(self.pool.lock().await.acquire().await?));
        self.connection.lock().await.replace(connection.clone());
        Ok(connection)
    }
}

/// Errors specific to `PostgreSQL` database operations using `SQLx`
///
/// Wraps errors from the underlying `SQLx` `PostgreSQL` driver plus additional error types
/// for query validation and result handling.
#[derive(Debug, Error)]
pub enum SqlxDatabaseError {
    /// Error from the underlying `SQLx` `PostgreSQL` driver
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
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

impl From<SqlxDatabaseError> for DatabaseError {
    fn from(value: SqlxDatabaseError) -> Self {
        Self::PostgresSqlx(value)
    }
}

impl From<sqlx::Error> for DatabaseError {
    fn from(value: sqlx::Error) -> Self {
        Self::PostgresSqlx(SqlxDatabaseError::Sqlx(value))
    }
}

#[async_trait]
impl Database for PostgresSqlxDatabase {
    async fn query(&self, query: &SelectQuery<'_>) -> Result<Vec<crate::Row>, DatabaseError> {
        Ok(select(
            self.get_connection().await?.lock().await.as_mut(),
            query.table_name,
            query.distinct,
            query.columns,
            query.filters.as_deref(),
            query.joins.as_deref(),
            query.sorts.as_deref(),
            query.limit,
        )
        .await?)
    }

    async fn query_first(
        &self,
        query: &SelectQuery<'_>,
    ) -> Result<Option<crate::Row>, DatabaseError> {
        Ok(find_row(
            self.get_connection().await?.lock().await.as_mut(),
            query.table_name,
            query.distinct,
            query.columns,
            query.filters.as_deref(),
            query.joins.as_deref(),
            query.sorts.as_deref(),
        )
        .await?)
    }

    async fn exec_delete(
        &self,
        statement: &DeleteStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        Ok(delete(
            self.get_connection().await?.lock().await.as_mut(),
            statement.table_name,
            statement.filters.as_deref(),
            statement.limit,
        )
        .await?)
    }

    async fn exec_delete_first(
        &self,
        statement: &DeleteStatement<'_>,
    ) -> Result<Option<crate::Row>, DatabaseError> {
        Ok(delete(
            self.get_connection().await?.lock().await.as_mut(),
            statement.table_name,
            statement.filters.as_deref(),
            Some(1),
        )
        .await?
        .into_iter()
        .next())
    }

    async fn exec_insert(
        &self,
        statement: &InsertStatement<'_>,
    ) -> Result<crate::Row, DatabaseError> {
        Ok(insert_and_get_row(
            self.get_connection().await?.lock().await.as_mut(),
            statement.table_name,
            &statement.values,
        )
        .await?)
    }

    async fn exec_update(
        &self,
        statement: &UpdateStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        Ok(update_and_get_rows(
            self.get_connection().await?.lock().await.as_mut(),
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            statement.limit,
        )
        .await?)
    }

    async fn exec_update_first(
        &self,
        statement: &UpdateStatement<'_>,
    ) -> Result<Option<crate::Row>, DatabaseError> {
        Ok(update_and_get_row(
            self.get_connection().await?.lock().await.as_mut(),
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            statement.limit,
        )
        .await?)
    }

    async fn exec_upsert(
        &self,
        statement: &UpsertStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        Ok(upsert(
            self.get_connection().await?.lock().await.as_mut(),
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            statement.limit,
        )
        .await?)
    }

    async fn exec_upsert_first(
        &self,
        statement: &UpsertStatement<'_>,
    ) -> Result<crate::Row, DatabaseError> {
        Ok(upsert_and_get_row(
            self.get_connection().await?.lock().await.as_mut(),
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            statement.limit,
        )
        .await?)
    }

    async fn exec_upsert_multi(
        &self,
        statement: &UpsertMultiStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        let rows = {
            upsert_multi(
                self.get_connection().await?.lock().await.as_mut(),
                statement.table_name,
                statement
                    .unique
                    .as_ref()
                    .ok_or(SqlxDatabaseError::MissingUnique)?,
                &statement.values,
            )
            .await?
        };
        Ok(rows)
    }

    async fn exec_raw(&self, statement: &str) -> Result<(), DatabaseError> {
        log::trace!("exec_raw: query:\n{statement}");

        let connection = self.get_connection().await?;
        let mut connection = connection.lock().await;

        connection
            .execute(statement)
            .await
            .map_err(SqlxDatabaseError::Sqlx)?;

        drop(connection);

        Ok(())
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::too_many_lines)]
    async fn exec_create_table(
        &self,
        statement: &crate::schema::CreateTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        postgres_sqlx_exec_create_table(
            self.get_connection().await?.lock().await.as_mut(),
            statement,
        )
        .await?;

        Ok(())
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_table(
        &self,
        statement: &crate::schema::DropTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        #[cfg(feature = "cascade")]
        {
            use crate::schema::DropBehavior;
            if matches!(
                statement.behavior,
                DropBehavior::Cascade | DropBehavior::Restrict
            ) {
                let tx = self.begin_transaction().await?;
                let result = tx.exec_drop_table(statement).await;
                return match result {
                    Ok(()) => tx.commit().await,
                    Err(e) => {
                        let _ = tx.rollback().await;
                        Err(e)
                    }
                };
            }
        }

        postgres_sqlx_exec_drop_table(
            self.get_connection().await?.lock().await.as_mut(),
            statement,
        )
        .await?;

        Ok(())
    }

    #[cfg(feature = "schema")]
    async fn exec_create_index(
        &self,
        statement: &crate::schema::CreateIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        postgres_sqlx_exec_create_index(
            self.get_connection().await?.lock().await.as_mut(),
            statement,
        )
        .await?;

        Ok(())
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_index(
        &self,
        statement: &crate::schema::DropIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        postgres_sqlx_exec_drop_index(
            self.get_connection().await?.lock().await.as_mut(),
            statement,
        )
        .await?;

        Ok(())
    }

    #[cfg(feature = "schema")]
    async fn exec_alter_table(
        &self,
        statement: &crate::schema::AlterTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        postgres_sqlx_exec_alter_table(
            self.get_connection().await?.lock().await.as_mut(),
            statement,
        )
        .await?;

        Ok(())
    }

    #[cfg(feature = "schema")]
    async fn table_exists(&self, table_name: &str) -> Result<bool, DatabaseError> {
        let mut conn = self.pool.lock().await.acquire().await.map_err(|e| {
            DatabaseError::PostgresSqlx(crate::sqlx::postgres::SqlxDatabaseError::from(e))
        })?;
        postgres_sqlx_table_exists(&mut conn, table_name).await
    }

    #[cfg(feature = "schema")]
    async fn list_tables(&self) -> Result<Vec<String>, DatabaseError> {
        let mut conn = self.pool.lock().await.acquire().await.map_err(|e| {
            DatabaseError::PostgresSqlx(crate::sqlx::postgres::SqlxDatabaseError::from(e))
        })?;
        postgres_sqlx_list_tables(&mut conn).await
    }

    #[cfg(feature = "schema")]
    async fn get_table_info(
        &self,
        table_name: &str,
    ) -> Result<Option<crate::schema::TableInfo>, DatabaseError> {
        let mut conn = self.pool.lock().await.acquire().await.map_err(|e| {
            DatabaseError::PostgresSqlx(crate::sqlx::postgres::SqlxDatabaseError::from(e))
        })?;
        postgres_sqlx_get_table_info(&mut conn, table_name).await
    }

    #[cfg(feature = "schema")]
    async fn get_table_columns(
        &self,
        table_name: &str,
    ) -> Result<Vec<crate::schema::ColumnInfo>, DatabaseError> {
        let mut conn = self.pool.lock().await.acquire().await.map_err(|e| {
            DatabaseError::PostgresSqlx(crate::sqlx::postgres::SqlxDatabaseError::from(e))
        })?;
        postgres_sqlx_get_table_columns(&mut conn, table_name).await
    }

    #[cfg(feature = "schema")]
    async fn column_exists(
        &self,
        table_name: &str,
        column_name: &str,
    ) -> Result<bool, DatabaseError> {
        let mut conn = self.pool.lock().await.acquire().await.map_err(|e| {
            DatabaseError::PostgresSqlx(crate::sqlx::postgres::SqlxDatabaseError::from(e))
        })?;
        postgres_sqlx_column_exists(&mut conn, table_name, column_name).await
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn query_raw(&self, query: &str) -> Result<Vec<crate::Row>, DatabaseError> {
        let pool = self.pool.lock().await;
        let mut connection = pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?;

        let result = sqlx::query(query)
            .fetch_all(&mut *connection)
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        if result.is_empty() {
            return Ok(vec![]);
        }

        // Get column names from first row
        let column_names: Vec<String> = result[0]
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect();

        // Use existing from_row helper
        let mut rows = Vec::new();
        for row in result {
            rows.push(
                from_row(&column_names, &row)
                    .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?,
            );
        }

        Ok(rows)
    }

    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn crate::DatabaseTransaction>, DatabaseError> {
        let tx = {
            let pool = self.pool.lock().await;
            pool.begin().await.map_err(SqlxDatabaseError::Sqlx)?
        };

        Ok(Box::new(PostgresSqlxTransaction::new(tx)))
    }

    async fn exec_raw_params(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<u64, DatabaseError> {
        // Transform query to handle Now/NowPlus parameters with proper $N renumbering
        let (transformed_query, filtered_params) =
            postgres_transform_query_for_params(query, params);

        let mut connection = {
            let pool = self.pool.lock().await;
            pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?
        };

        let mut query_builder: sqlx::query::Query<'_, sqlx::Postgres, sqlx::postgres::PgArguments> =
            sqlx::query(&transformed_query);

        // Add only filtered parameters - Now/NowPlus are already in the SQL
        for param in &filtered_params {
            query_builder = match param {
                crate::DatabaseValue::Int8(n) => query_builder.bind(i16::from(*n)),
                crate::DatabaseValue::Int8Opt(n) => query_builder.bind(n.map(i16::from)),
                crate::DatabaseValue::Int16(n) => query_builder.bind(*n),
                crate::DatabaseValue::Int16Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::Int32(n) => query_builder.bind(*n),
                crate::DatabaseValue::Int32Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::String(s) => query_builder.bind(s),
                crate::DatabaseValue::StringOpt(s) => query_builder.bind(s),
                crate::DatabaseValue::Int64(n) => query_builder.bind(*n),
                crate::DatabaseValue::Int64Opt(n) => query_builder.bind(n),
                // DatabaseValue::UInt8(value) | DatabaseValue::UInt8Opt(Some(value)) => {
                //     let signed = i16::from(*value);
                //     query = query.bind(signed);
                // }
                // DatabaseValue::UInt16(value) | DatabaseValue::UInt16Opt(Some(value)) => {
                //     let signed =
                //         i16::try_from(*value).map_err(|_| DatabaseError::UInt16Overflow(*value))?;
                //     query = query.bind(signed);
                // }
                // DatabaseValue::UInt32(value) | DatabaseValue::UInt32Opt(Some(value)) => {
                //     let signed =
                //         i32::try_from(*value).map_err(|_| DatabaseError::UInt32Overflow(*value))?;
                //     query = query.bind(signed);
                // }
                crate::DatabaseValue::UInt8(n) => {
                    let signed = i16::from(*n);
                    query_builder.bind(signed)
                }
                crate::DatabaseValue::UInt8Opt(n) => {
                    let signed = n.map(i16::from);
                    query_builder.bind(signed)
                }
                crate::DatabaseValue::UInt16(n) => {
                    let signed =
                        i16::try_from(*n).map_err(|_| DatabaseError::UInt16Overflow(*n))?;
                    query_builder.bind(signed)
                }
                crate::DatabaseValue::UInt16Opt(n) => {
                    let signed = n.and_then(|v| i16::try_from(v).ok());
                    query_builder.bind(signed)
                }
                crate::DatabaseValue::UInt32(n) => {
                    let signed =
                        i32::try_from(*n).map_err(|_| DatabaseError::UInt32Overflow(*n))?;
                    query_builder.bind(signed)
                }
                crate::DatabaseValue::UInt32Opt(n) => {
                    let signed = n.and_then(|v| i32::try_from(v).ok());
                    query_builder.bind(signed)
                }
                crate::DatabaseValue::UInt64(n) => {
                    query_builder.bind(i64::try_from(*n).unwrap_or(i64::MAX))
                }
                crate::DatabaseValue::UInt64Opt(n) => {
                    query_builder.bind(n.map(|x| i64::try_from(x).unwrap_or(i64::MAX)))
                }
                crate::DatabaseValue::Real64(r) => query_builder.bind(*r),
                crate::DatabaseValue::Real64Opt(r) => query_builder.bind(r),
                crate::DatabaseValue::Real32(r) => query_builder.bind(*r),
                crate::DatabaseValue::Real32Opt(r) => query_builder.bind(r),
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::Decimal(d) => query_builder.bind(*d),
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::DecimalOpt(d) => query_builder.bind(d),
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::Uuid(u) => query_builder.bind(u),
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::UuidOpt(u) => query_builder.bind(u),
                crate::DatabaseValue::Bool(b) => query_builder.bind(*b),
                crate::DatabaseValue::BoolOpt(b) => query_builder.bind(b),
                crate::DatabaseValue::DateTime(dt) => query_builder.bind(*dt),
                crate::DatabaseValue::Null => query_builder.bind(Option::<String>::None),
                crate::DatabaseValue::Now | crate::DatabaseValue::NowPlus(_) => {
                    // These should never reach here due to query transformation
                    return Err(DatabaseError::QueryFailed(
                        "Now/NowPlus parameters should be handled by query transformation"
                            .to_string(),
                    ));
                }
            };
        }

        let result = query_builder
            .execute(&mut *connection)
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        Ok(result.rows_affected())
    }

    async fn query_raw_params(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        // Transform query to handle Now/NowPlus parameters with proper $N renumbering
        let (transformed_query, filtered_params) =
            postgres_transform_query_for_params(query, params);

        let mut connection = {
            let pool = self.pool.lock().await;
            pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?
        };

        let mut query_builder: sqlx::query::Query<'_, sqlx::Postgres, sqlx::postgres::PgArguments> =
            sqlx::query(&transformed_query);

        // Add only filtered parameters - Now/NowPlus are already in the SQL
        for param in &filtered_params {
            query_builder = match param {
                crate::DatabaseValue::Int8(n) => query_builder.bind(*n),
                crate::DatabaseValue::Int8Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::Int16(n) => query_builder.bind(*n),
                crate::DatabaseValue::Int16Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::Int32(n) => query_builder.bind(*n),
                crate::DatabaseValue::Int32Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::String(s) => query_builder.bind(s),
                crate::DatabaseValue::StringOpt(s) => query_builder.bind(s),
                crate::DatabaseValue::Int64(n) => query_builder.bind(*n),
                crate::DatabaseValue::Int64Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::UInt8(n) => {
                    let signed = i16::from(*n);
                    query_builder.bind(signed)
                }
                crate::DatabaseValue::UInt8Opt(n) => {
                    let signed = n.map(i16::from);
                    query_builder.bind(signed)
                }
                crate::DatabaseValue::UInt16(n) => {
                    let signed =
                        i16::try_from(*n).map_err(|_| DatabaseError::UInt16Overflow(*n))?;
                    query_builder.bind(signed)
                }
                crate::DatabaseValue::UInt16Opt(n) => {
                    let signed = n.and_then(|v| i16::try_from(v).ok());
                    query_builder.bind(signed)
                }
                crate::DatabaseValue::UInt32(n) => {
                    let signed =
                        i32::try_from(*n).map_err(|_| DatabaseError::UInt32Overflow(*n))?;
                    query_builder.bind(signed)
                }
                crate::DatabaseValue::UInt32Opt(n) => {
                    let signed = n.and_then(|v| i32::try_from(v).ok());
                    query_builder.bind(signed)
                }
                crate::DatabaseValue::UInt64(n) => {
                    query_builder.bind(i64::try_from(*n).unwrap_or(i64::MAX))
                }
                crate::DatabaseValue::UInt64Opt(n) => {
                    query_builder.bind(n.map(|x| i64::try_from(x).unwrap_or(i64::MAX)))
                }
                crate::DatabaseValue::Real64(r) => query_builder.bind(*r),
                crate::DatabaseValue::Real64Opt(r) => query_builder.bind(r),
                crate::DatabaseValue::Real32(r) => query_builder.bind(*r),
                crate::DatabaseValue::Real32Opt(r) => query_builder.bind(r),
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::Decimal(d) => query_builder.bind(*d),
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::DecimalOpt(d) => query_builder.bind(d),
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::Uuid(u) => query_builder.bind(u),
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::UuidOpt(u) => query_builder.bind(u),
                crate::DatabaseValue::Bool(b) => query_builder.bind(*b),
                crate::DatabaseValue::BoolOpt(b) => query_builder.bind(b),
                crate::DatabaseValue::DateTime(dt) => query_builder.bind(*dt),
                crate::DatabaseValue::Null => query_builder.bind(Option::<String>::None),
                crate::DatabaseValue::Now | crate::DatabaseValue::NowPlus(_) => {
                    // These should never reach here due to query transformation
                    return Err(DatabaseError::QueryFailed(
                        "Now/NowPlus parameters should be handled by query transformation"
                            .to_string(),
                    ));
                }
            };
        }

        let result = query_builder
            .fetch_all(&mut *connection)
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        if result.is_empty() {
            return Ok(vec![]);
        }

        // Get column names from first row
        let column_names: Vec<String> = result[0]
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect();

        // Convert sqlx rows to our Row format
        let mut rows = Vec::new();
        for sqlx_row in result {
            let row = from_row(&column_names, &sqlx_row)
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
            rows.push(row);
        }

        Ok(rows)
    }
}

#[async_trait]
impl Database for PostgresSqlxTransaction {
    #[allow(clippy::significant_drop_tightening)]
    async fn query(&self, query: &SelectQuery<'_>) -> Result<Vec<crate::Row>, DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        Ok(select(
            &mut *tx,
            query.table_name,
            query.distinct,
            query.columns,
            query.filters.as_deref(),
            query.joins.as_deref(),
            query.sorts.as_deref(),
            query.limit,
        )
        .await?)
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn query_first(
        &self,
        query: &SelectQuery<'_>,
    ) -> Result<Option<crate::Row>, DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        Ok(find_row(
            &mut *tx,
            query.table_name,
            query.distinct,
            query.columns,
            query.filters.as_deref(),
            query.joins.as_deref(),
            query.sorts.as_deref(),
        )
        .await?)
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn exec_delete(
        &self,
        statement: &DeleteStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        Ok(delete(
            &mut *tx,
            statement.table_name,
            statement.filters.as_deref(),
            statement.limit,
        )
        .await?)
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn exec_delete_first(
        &self,
        statement: &DeleteStatement<'_>,
    ) -> Result<Option<crate::Row>, DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        Ok(delete(
            &mut *tx,
            statement.table_name,
            statement.filters.as_deref(),
            Some(1),
        )
        .await?
        .into_iter()
        .next())
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn exec_insert(
        &self,
        statement: &InsertStatement<'_>,
    ) -> Result<crate::Row, DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        Ok(insert_and_get_row(&mut *tx, statement.table_name, &statement.values).await?)
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn exec_update(
        &self,
        statement: &UpdateStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        Ok(update_and_get_rows(
            &mut *tx,
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            statement.limit,
        )
        .await?)
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn exec_update_first(
        &self,
        statement: &UpdateStatement<'_>,
    ) -> Result<Option<crate::Row>, DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        Ok(update_and_get_row(
            &mut *tx,
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            statement.limit,
        )
        .await?)
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn exec_upsert(
        &self,
        statement: &UpsertStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        Ok(upsert(
            &mut *tx,
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            statement.limit,
        )
        .await?)
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn exec_upsert_first(
        &self,
        statement: &UpsertStatement<'_>,
    ) -> Result<crate::Row, DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        Ok(upsert_and_get_row(
            &mut *tx,
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            statement.limit,
        )
        .await?)
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn exec_upsert_multi(
        &self,
        statement: &UpsertMultiStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        let rows = {
            upsert_multi(
                &mut *tx,
                statement.table_name,
                statement
                    .unique
                    .as_ref()
                    .ok_or(SqlxDatabaseError::MissingUnique)?,
                &statement.values,
            )
            .await?
        };
        Ok(rows)
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn exec_raw(&self, statement: &str) -> Result<(), DatabaseError> {
        log::trace!("exec_raw: query:\n{statement}");

        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        tx.execute(statement)
            .await
            .map_err(SqlxDatabaseError::Sqlx)?;

        Ok(())
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::significant_drop_tightening)]
    async fn exec_create_table(
        &self,
        statement: &crate::schema::CreateTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        postgres_sqlx_exec_create_table(&mut *tx, statement).await?;

        Ok(())
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::significant_drop_tightening)]
    async fn exec_drop_table(
        &self,
        statement: &crate::schema::DropTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        #[cfg(feature = "cascade")]
        {
            use crate::schema::DropBehavior;
            match statement.behavior {
                DropBehavior::Cascade => {
                    return postgres_sqlx_exec_drop_table_cascade(&mut *tx, statement).await;
                }
                DropBehavior::Restrict => {
                    return postgres_sqlx_exec_drop_table_restrict_native(&mut *tx, statement)
                        .await;
                }
                DropBehavior::Default => {}
            }
        }

        postgres_sqlx_exec_drop_table(&mut *tx, statement).await?;

        Ok(())
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::significant_drop_tightening)]
    async fn exec_create_index(
        &self,
        statement: &crate::schema::CreateIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        postgres_sqlx_exec_create_index(&mut *tx, statement).await?;

        Ok(())
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::significant_drop_tightening)]
    async fn exec_drop_index(
        &self,
        statement: &crate::schema::DropIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        postgres_sqlx_exec_drop_index(&mut *tx, statement).await?;

        Ok(())
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::significant_drop_tightening)]
    async fn exec_alter_table(
        &self,
        statement: &crate::schema::AlterTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        postgres_sqlx_exec_alter_table(&mut *tx, statement).await?;

        Ok(())
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::significant_drop_tightening)]
    async fn table_exists(&self, table_name: &str) -> Result<bool, DatabaseError> {
        let mut lock = self.transaction.lock().await;
        let tx = lock.as_mut().ok_or(DatabaseError::TransactionCommitted)?;
        postgres_sqlx_table_exists(tx, table_name).await
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::significant_drop_tightening)]
    async fn list_tables(&self) -> Result<Vec<String>, DatabaseError> {
        let mut lock = self.transaction.lock().await;
        let tx = lock.as_mut().ok_or(DatabaseError::TransactionCommitted)?;
        postgres_sqlx_list_tables(tx).await
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::significant_drop_tightening)]
    async fn get_table_info(
        &self,
        table_name: &str,
    ) -> Result<Option<crate::schema::TableInfo>, DatabaseError> {
        let mut lock = self.transaction.lock().await;
        let tx = lock.as_mut().ok_or(DatabaseError::TransactionCommitted)?;
        postgres_sqlx_get_table_info(tx, table_name).await
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::significant_drop_tightening)]
    async fn get_table_columns(
        &self,
        table_name: &str,
    ) -> Result<Vec<crate::schema::ColumnInfo>, DatabaseError> {
        let mut lock = self.transaction.lock().await;
        let tx = lock.as_mut().ok_or(DatabaseError::TransactionCommitted)?;
        postgres_sqlx_get_table_columns(tx, table_name).await
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::significant_drop_tightening)]
    async fn column_exists(
        &self,
        table_name: &str,
        column_name: &str,
    ) -> Result<bool, DatabaseError> {
        let mut lock = self.transaction.lock().await;
        let tx = lock.as_mut().ok_or(DatabaseError::TransactionCommitted)?;
        postgres_sqlx_column_exists(tx, table_name, column_name).await
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn query_raw(&self, query: &str) -> Result<Vec<crate::Row>, DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        let result = sqlx::query(query)
            .fetch_all(&mut **tx)
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        if result.is_empty() {
            return Ok(vec![]);
        }

        // Get column names from first row
        let column_names: Vec<String> = result[0]
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect();

        // Use existing from_row helper
        let mut rows = Vec::new();
        for row in result {
            rows.push(
                from_row(&column_names, &row)
                    .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?,
            );
        }

        Ok(rows)
    }

    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn crate::DatabaseTransaction>, DatabaseError> {
        Err(DatabaseError::AlreadyInTransaction)
    }

    async fn exec_raw_params(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<u64, DatabaseError> {
        let mut query_builder: sqlx::query::Query<'_, sqlx::Postgres, sqlx::postgres::PgArguments> =
            sqlx::query(query);

        // Add parameters in order - PostgreSQL uses $1, $2 placeholders
        for param in params {
            query_builder = match param {
                crate::DatabaseValue::Int8(n) => query_builder.bind(*n),
                crate::DatabaseValue::Int8Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::Int16(n) => query_builder.bind(*n),
                crate::DatabaseValue::Int16Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::Int32(n) => query_builder.bind(*n),
                crate::DatabaseValue::Int32Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::String(s) => query_builder.bind(s),
                crate::DatabaseValue::StringOpt(s) => query_builder.bind(s),
                crate::DatabaseValue::Int64(n) => query_builder.bind(*n),
                crate::DatabaseValue::Int64Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::UInt8(n) => {
                    let signed = i8::try_from(*n).map_err(|_| DatabaseError::UInt8Overflow(*n))?;
                    query_builder.bind(signed)
                }
                crate::DatabaseValue::UInt8Opt(n) => {
                    let signed = n.and_then(|v| i8::try_from(v).ok());
                    query_builder.bind(signed)
                }
                crate::DatabaseValue::UInt16(n) => {
                    let signed =
                        i16::try_from(*n).map_err(|_| DatabaseError::UInt16Overflow(*n))?;
                    query_builder.bind(signed)
                }
                crate::DatabaseValue::UInt16Opt(n) => {
                    let signed = n.and_then(|v| i16::try_from(v).ok());
                    query_builder.bind(signed)
                }
                crate::DatabaseValue::UInt32(n) => {
                    let signed =
                        i32::try_from(*n).map_err(|_| DatabaseError::UInt32Overflow(*n))?;
                    query_builder.bind(signed)
                }
                crate::DatabaseValue::UInt32Opt(n) => {
                    let signed = n.and_then(|v| i32::try_from(v).ok());
                    query_builder.bind(signed)
                }
                crate::DatabaseValue::UInt64(n) => {
                    query_builder.bind(i64::try_from(*n).unwrap_or(i64::MAX))
                }
                crate::DatabaseValue::UInt64Opt(n) => {
                    query_builder.bind(n.map(|x| i64::try_from(x).unwrap_or(i64::MAX)))
                }
                crate::DatabaseValue::Real64(r) => query_builder.bind(*r),
                crate::DatabaseValue::Real64Opt(r) => query_builder.bind(r),
                crate::DatabaseValue::Real32(r) => query_builder.bind(*r),
                crate::DatabaseValue::Real32Opt(r) => query_builder.bind(r),
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::Decimal(d) => query_builder.bind(*d),
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::DecimalOpt(d) => query_builder.bind(d),
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::Uuid(u) => query_builder.bind(u),
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::UuidOpt(u) => query_builder.bind(u),
                crate::DatabaseValue::Bool(b) => query_builder.bind(*b),
                crate::DatabaseValue::BoolOpt(b) => query_builder.bind(b),
                crate::DatabaseValue::DateTime(dt) => query_builder.bind(*dt),
                crate::DatabaseValue::Null => query_builder.bind(Option::<String>::None),
                crate::DatabaseValue::Now => query_builder.bind("NOW()"),
                crate::DatabaseValue::NowPlus(_interval) => {
                    // NowPlus should not be bound as parameter - it should be a SQL expression
                    panic!("NowPlus cannot be bound as parameter - use in SQL expression instead");
                }
            };
        }

        let result = {
            let mut transaction_guard = self.transaction.lock().await;
            query_builder
                .execute(
                    &mut **transaction_guard
                        .as_mut()
                        .ok_or(DatabaseError::TransactionCommitted)?,
                )
                .await
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?
        };

        Ok(result.rows_affected())
    }

    async fn query_raw_params(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        let mut query_builder: sqlx::query::Query<'_, sqlx::Postgres, sqlx::postgres::PgArguments> =
            sqlx::query(query);

        // Add parameters in order - PostgreSQL uses $1, $2 placeholders
        for param in params {
            query_builder = match param {
                crate::DatabaseValue::Int8(n) => query_builder.bind(*n),
                crate::DatabaseValue::Int8Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::Int16(n) => query_builder.bind(*n),
                crate::DatabaseValue::Int16Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::Int32(n) => query_builder.bind(*n),
                crate::DatabaseValue::Int32Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::String(s) => query_builder.bind(s),
                crate::DatabaseValue::StringOpt(s) => query_builder.bind(s),
                crate::DatabaseValue::Int64(n) => query_builder.bind(*n),
                crate::DatabaseValue::Int64Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::UInt8(n) => {
                    let signed = i8::try_from(*n).map_err(|_| DatabaseError::UInt8Overflow(*n))?;
                    query_builder.bind(signed)
                }
                crate::DatabaseValue::UInt8Opt(n) => {
                    let signed = n.and_then(|v| i8::try_from(v).ok());
                    query_builder.bind(signed)
                }
                crate::DatabaseValue::UInt16(n) => {
                    let signed =
                        i16::try_from(*n).map_err(|_| DatabaseError::UInt16Overflow(*n))?;
                    query_builder.bind(signed)
                }
                crate::DatabaseValue::UInt16Opt(n) => {
                    let signed = n.and_then(|v| i16::try_from(v).ok());
                    query_builder.bind(signed)
                }
                crate::DatabaseValue::UInt32(n) => {
                    let signed =
                        i32::try_from(*n).map_err(|_| DatabaseError::UInt32Overflow(*n))?;
                    query_builder.bind(signed)
                }
                crate::DatabaseValue::UInt32Opt(n) => {
                    let signed = n.and_then(|v| i32::try_from(v).ok());
                    query_builder.bind(signed)
                }
                crate::DatabaseValue::UInt64(n) => {
                    query_builder.bind(i64::try_from(*n).unwrap_or(i64::MAX))
                }
                crate::DatabaseValue::UInt64Opt(n) => {
                    query_builder.bind(n.map(|x| i64::try_from(x).unwrap_or(i64::MAX)))
                }
                crate::DatabaseValue::Real64(r) => query_builder.bind(*r),
                crate::DatabaseValue::Real64Opt(r) => query_builder.bind(r),
                crate::DatabaseValue::Real32(r) => query_builder.bind(*r),
                crate::DatabaseValue::Real32Opt(r) => query_builder.bind(r),
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::Decimal(d) => query_builder.bind(*d),
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::DecimalOpt(d) => query_builder.bind(d),
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::Uuid(u) => query_builder.bind(u),
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::UuidOpt(u) => query_builder.bind(u),
                crate::DatabaseValue::Bool(b) => query_builder.bind(*b),
                crate::DatabaseValue::BoolOpt(b) => query_builder.bind(b),
                crate::DatabaseValue::DateTime(dt) => query_builder.bind(*dt),
                crate::DatabaseValue::Null => query_builder.bind(Option::<String>::None),
                crate::DatabaseValue::Now => query_builder.bind("NOW()"),
                crate::DatabaseValue::NowPlus(_interval) => {
                    // NowPlus should not be bound as parameter - it should be a SQL expression
                    panic!("NowPlus cannot be bound as parameter - use in SQL expression instead");
                }
            };
        }

        let result = {
            let mut transaction_guard = self.transaction.lock().await;
            query_builder
                .fetch_all(
                    &mut **transaction_guard
                        .as_mut()
                        .ok_or(DatabaseError::TransactionCommitted)?,
                )
                .await
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?
        };

        if result.is_empty() {
            return Ok(vec![]);
        }

        // Get column names from first row
        let column_names: Vec<String> = result[0]
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect();

        // Convert sqlx rows to our Row format
        let mut rows = Vec::new();
        for sqlx_row in result {
            let row = from_row(&column_names, &sqlx_row)
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
            rows.push(row);
        }

        Ok(rows)
    }
}

struct PostgresSqlxSavepoint {
    name: String,
    transaction: Arc<Mutex<Option<Transaction<'static, Postgres>>>>,
    released: AtomicBool,
    rolled_back: AtomicBool,
}

#[async_trait]
impl crate::Savepoint for PostgresSqlxSavepoint {
    #[allow(clippy::significant_drop_tightening)]
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

        let mut transaction_guard = self.transaction.lock().await;
        if let Some(tx) = transaction_guard.as_mut() {
            sqlx::query(&format!("RELEASE SAVEPOINT {}", self.name))
                .execute(&mut **tx)
                .await
                .map_err(SqlxDatabaseError::Sqlx)?;
        } else {
            return Err(DatabaseError::TransactionCommitted);
        }

        Ok(())
    }

    #[allow(clippy::significant_drop_tightening)]
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

        let mut transaction_guard = self.transaction.lock().await;
        if let Some(tx) = transaction_guard.as_mut() {
            sqlx::query(&format!("ROLLBACK TO SAVEPOINT {}", self.name))
                .execute(&mut **tx)
                .await
                .map_err(SqlxDatabaseError::Sqlx)?;
        } else {
            return Err(DatabaseError::TransactionCommitted);
        }

        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

#[async_trait]
impl crate::DatabaseTransaction for PostgresSqlxTransaction {
    #[allow(clippy::significant_drop_tightening)]
    async fn commit(self: Box<Self>) -> Result<(), DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .take()
            .ok_or(DatabaseError::TransactionCommitted)?;

        tx.commit().await.map_err(SqlxDatabaseError::Sqlx)?;

        Ok(())
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn rollback(self: Box<Self>) -> Result<(), DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .take()
            .ok_or(DatabaseError::TransactionCommitted)?;

        tx.rollback().await.map_err(SqlxDatabaseError::Sqlx)?;

        Ok(())
    }

    async fn savepoint(&self, name: &str) -> Result<Box<dyn crate::Savepoint>, DatabaseError> {
        crate::validate_savepoint_name(name)?;

        // Execute SAVEPOINT SQL
        if let Some(tx) = self.transaction.lock().await.as_mut() {
            sqlx::query(&format!("SAVEPOINT {name}"))
                .execute(&mut **tx)
                .await
                .map_err(SqlxDatabaseError::Sqlx)?;
        } else {
            return Err(DatabaseError::TransactionCommitted);
        }

        Ok(Box::new(PostgresSqlxSavepoint {
            name: name.to_string(),
            transaction: Arc::clone(&self.transaction),
            released: AtomicBool::new(false),
            rolled_back: AtomicBool::new(false),
        }))
    }

    /// PostgreSQL-optimized CASCADE discovery using recursive CTE
    #[cfg(feature = "cascade")]
    async fn find_cascade_targets(
        &self,
        table_name: &str,
    ) -> Result<crate::schema::DropPlan, DatabaseError> {
        let query = format!(
            r"
            WITH RECURSIVE dependent_tables AS (
                -- Base case: direct dependents
                SELECT DISTINCT
                    tc.table_name as dependent_table,
                    1 as level
                FROM information_schema.table_constraints tc
                JOIN information_schema.key_column_usage kcu
                    ON tc.constraint_name = kcu.constraint_name
                    AND tc.table_schema = kcu.table_schema
                JOIN information_schema.constraint_column_usage ccu
                    ON ccu.constraint_name = tc.constraint_name
                    AND ccu.table_schema = tc.table_schema
                WHERE tc.constraint_type = 'FOREIGN KEY'
                    AND ccu.table_name = '{}'
                    AND tc.table_schema = current_schema()

                UNION

                -- Recursive case: indirect dependents
                SELECT DISTINCT
                    tc.table_name as dependent_table,
                    dt.level + 1 as level
                FROM information_schema.table_constraints tc
                JOIN information_schema.key_column_usage kcu
                    ON tc.constraint_name = kcu.constraint_name
                    AND tc.table_schema = kcu.table_schema
                JOIN information_schema.constraint_column_usage ccu
                    ON ccu.constraint_name = tc.constraint_name
                    AND ccu.table_schema = tc.table_schema
                JOIN dependent_tables dt ON ccu.table_name = dt.dependent_table
                WHERE tc.constraint_type = 'FOREIGN KEY'
                    AND tc.table_schema = current_schema()
            )
            SELECT dependent_table
            FROM dependent_tables
            ORDER BY level DESC, dependent_table
            ",
            sanitize_value(table_name)
        );

        let rows = self.query_raw(&query).await?;

        let mut result = Vec::new();
        for row in rows {
            if let Some((_, crate::DatabaseValue::String(table))) = row.columns.first() {
                result.push(table.clone());
            }
        }

        // Add the original table at the end (dropped last)
        result.push(table_name.to_string());

        // Simplified cycle detection for Phase 15.1.4 - real implementation would track properly
        Ok(crate::schema::DropPlan::Simple(result))
    }

    /// PostgreSQL-optimized dependency check using EXISTS
    #[cfg(feature = "cascade")]
    async fn has_any_dependents(&self, table_name: &str) -> Result<bool, DatabaseError> {
        let query = format!(
            r"
            SELECT EXISTS (
                SELECT 1
                FROM information_schema.table_constraints tc
                JOIN information_schema.key_column_usage kcu
                    ON tc.constraint_name = kcu.constraint_name
                    AND tc.table_schema = kcu.table_schema
                JOIN information_schema.constraint_column_usage ccu
                    ON ccu.constraint_name = tc.constraint_name
                    AND ccu.table_schema = tc.table_schema
                WHERE tc.constraint_type = 'FOREIGN KEY'
                    AND ccu.table_name = '{}'
                    AND tc.table_schema = current_schema()
                LIMIT 1
            ) as has_dependents
            ",
            sanitize_value(table_name)
        );

        let rows = self.query_raw(&query).await?;

        if let Some(row) = rows.first()
            && let Some((_, crate::DatabaseValue::Bool(has_deps))) = row.columns.first()
        {
            return Ok(*has_deps);
        }

        Ok(false)
    }

    /// Get direct dependents of a table (PostgreSQL-optimized)
    #[cfg(feature = "cascade")]
    async fn get_direct_dependents(
        &self,
        table_name: &str,
    ) -> Result<std::collections::BTreeSet<String>, DatabaseError> {
        let query = format!(
            r"
            SELECT DISTINCT tc.table_name
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu
                ON tc.constraint_name = kcu.constraint_name
                AND tc.table_schema = kcu.table_schema
            JOIN information_schema.constraint_column_usage ccu
                ON ccu.constraint_name = tc.constraint_name
                AND ccu.table_schema = tc.table_schema
                WHERE tc.constraint_type = 'FOREIGN KEY'
                    AND ccu.table_name = '{}'
                    AND tc.table_schema = current_schema()
            ",
            sanitize_value(table_name)
        );

        let rows = self.query_raw(&query).await?;

        let mut dependents = std::collections::BTreeSet::new();
        for row in rows {
            if let Some((_, crate::DatabaseValue::String(table))) = row.columns.first() {
                dependents.insert(table.clone());
            }
        }

        Ok(dependents)
    }
}

#[cfg(feature = "schema")]
#[allow(clippy::too_many_lines)]
async fn postgres_sqlx_exec_create_table(
    connection: &mut PgConnection,
    statement: &crate::schema::CreateTableStatement<'_>,
) -> Result<(), SqlxDatabaseError> {
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

        query.push_str(&column.name);
        query.push(' ');

        match column.data_type {
            crate::schema::DataType::VarChar(size) => {
                query.push_str("VARCHAR(");
                query.push_str(&size.to_string());
                query.push(')');
            }
            crate::schema::DataType::Text => query.push_str("TEXT"),
            crate::schema::DataType::Char(size) => {
                query.push_str("CHAR(");
                query.push_str(&size.to_string());
                query.push(')');
            }
            crate::schema::DataType::Bool => query.push_str("BOOLEAN"),
            crate::schema::DataType::TinyInt | crate::schema::DataType::SmallInt => {
                if column.auto_increment {
                    query.push_str("SMALLSERIAL");
                } else {
                    query.push_str("SMALLINT");
                }
            }
            crate::schema::DataType::Int => {
                if column.auto_increment {
                    query.push_str("SERIAL");
                } else {
                    query.push_str("INTEGER");
                }
            }
            crate::schema::DataType::BigInt => {
                if column.auto_increment {
                    query.push_str("BIGSERIAL");
                } else {
                    query.push_str("BIGINT");
                }
            }
            crate::schema::DataType::Serial => query.push_str("SERIAL"),
            crate::schema::DataType::BigSerial => query.push_str("BIGSERIAL"),
            crate::schema::DataType::Real => query.push_str("REAL"),
            crate::schema::DataType::Double => query.push_str("DOUBLE PRECISION"),
            crate::schema::DataType::Decimal(precision, scale) => {
                query.push_str("DECIMAL(");
                query.push_str(&precision.to_string());
                query.push(',');
                query.push_str(&scale.to_string());
                query.push(')');
            }
            crate::schema::DataType::Money => query.push_str("MONEY"),
            crate::schema::DataType::Date => query.push_str("DATE"),
            crate::schema::DataType::Time => query.push_str("TIME"),
            crate::schema::DataType::DateTime => query.push_str("TIMESTAMP WITH TIME ZONE"),
            crate::schema::DataType::Timestamp => query.push_str("TIMESTAMP"),
            crate::schema::DataType::Blob => query.push_str("BYTEA"),
            crate::schema::DataType::Binary(_) => {
                query.push_str("BYTEA"); // PostgreSQL doesn't have fixed-size binary
            }
            crate::schema::DataType::Json => query.push_str("JSON"),
            crate::schema::DataType::Jsonb => query.push_str("JSONB"),
            crate::schema::DataType::Uuid => query.push_str("UUID"),
            crate::schema::DataType::Xml => query.push_str("XML"),
            crate::schema::DataType::Array(ref inner_type) => match **inner_type {
                crate::schema::DataType::Int => query.push_str("INTEGER[]"),
                crate::schema::DataType::BigInt => query.push_str("BIGINT[]"),
                _ => query.push_str("TEXT[]"),
            },
            crate::schema::DataType::Inet => query.push_str("INET"),
            crate::schema::DataType::MacAddr => query.push_str("MACADDR"),
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
                    query.push_str(if *x { "TRUE" } else { "FALSE" });
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
                    query.push_str(&x.to_string());
                }
                #[cfg(feature = "uuid")]
                DatabaseValue::Uuid(u) | DatabaseValue::UuidOpt(Some(u)) => {
                    query.push('\'');
                    query.push_str(&u.to_string());
                    query.push('\'');
                }
                DatabaseValue::NowPlus(_) => {
                    // This should never be reached - NowPlus is transformed to (NOW() + $N::interval)
                    unreachable!(
                        "NowPlus must be transformed to (NOW() + $N::interval), not used as direct parameter"
                    )
                }
                DatabaseValue::Now => {
                    query.push_str("NOW()");
                }
                DatabaseValue::DateTime(x) => {
                    query.push_str("timestamp '");
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

    log::trace!("exec_create_table: query:\n{query}");

    connection
        .execute(query.as_str())
        .await
        .map_err(SqlxDatabaseError::Sqlx)?;

    Ok(())
}

#[cfg(feature = "schema")]
// Helper functions for CASCADE support using iterative approach
#[cfg(feature = "cascade")]
async fn postgres_sqlx_get_direct_dependents(
    connection: &mut PgConnection,
    table_name: &str,
) -> Result<Vec<String>, SqlxDatabaseError> {
    let query = r"
        SELECT DISTINCT tc.table_name
        FROM information_schema.table_constraints AS tc
        JOIN information_schema.key_column_usage AS kcu
            ON tc.constraint_name = kcu.constraint_name
            AND tc.table_schema = kcu.table_schema
        JOIN information_schema.constraint_column_usage AS ccu
            ON ccu.constraint_name = tc.constraint_name
            AND ccu.table_schema = tc.table_schema
        WHERE tc.constraint_type = 'FOREIGN KEY'
            AND ccu.table_name = $1
            AND tc.table_schema = 'public'
    ";

    let rows: Vec<(String,)> = sqlx::query_as(query)
        .bind(table_name)
        .fetch_all(&mut *connection)
        .await
        .map_err(SqlxDatabaseError::Sqlx)?;

    Ok(rows.into_iter().map(|(name,)| name).collect())
}

#[cfg(feature = "cascade")]
async fn postgres_sqlx_exec_drop_table_cascade(
    connection: &mut PgConnection,
    statement: &crate::schema::DropTableStatement<'_>,
) -> Result<(), DatabaseError> {
    // Iterative collection of dependent tables
    let mut to_drop = Vec::new();
    let mut to_check = vec![statement.table_name.to_string()];
    let mut visited = std::collections::BTreeSet::new();

    while let Some(table) = to_check.pop() {
        if !visited.insert(table.clone()) {
            continue;
        }

        let dependents = postgres_sqlx_get_direct_dependents(connection, &table)
            .await
            .map_err(DatabaseError::PostgresSqlx)?;

        for dependent in dependents {
            if !visited.contains(&dependent) {
                to_check.push(dependent);
            }
        }

        to_drop.push(table);
    }

    // Reverse to get dependents first
    to_drop.reverse();

    for table in to_drop {
        let sql = format!(
            "DROP TABLE {}{}",
            if statement.if_exists {
                "IF EXISTS "
            } else {
                ""
            },
            table
        );
        connection
            .execute(sql.as_str())
            .await
            .map_err(|e| DatabaseError::PostgresSqlx(SqlxDatabaseError::Sqlx(e)))?;
    }
    Ok(())
}

#[cfg(feature = "cascade")]
async fn postgres_sqlx_exec_drop_table_restrict_native(
    connection: &mut PgConnection,
    statement: &crate::schema::DropTableStatement<'_>,
) -> Result<(), DatabaseError> {
    let mut query = "DROP TABLE ".to_string();

    if statement.if_exists {
        query.push_str("IF EXISTS ");
    }

    query.push_str(statement.table_name);
    query.push_str(" RESTRICT");

    log::trace!("exec_drop_table_restrict_native: query:\n{query}");

    connection
        .execute(query.as_str())
        .await
        .map_err(|e| DatabaseError::PostgresSqlx(SqlxDatabaseError::Sqlx(e)))?;

    Ok(())
}

#[cfg(feature = "schema")]
async fn postgres_sqlx_exec_drop_table(
    connection: &mut PgConnection,
    statement: &crate::schema::DropTableStatement<'_>,
) -> Result<(), SqlxDatabaseError> {
    #[cfg(feature = "cascade")]
    {
        use crate::schema::DropBehavior;
        match statement.behavior {
            DropBehavior::Cascade => {
                return postgres_sqlx_exec_drop_table_cascade(connection, statement)
                    .await
                    .map_err(|e| match e {
                        DatabaseError::PostgresSqlx(pg_err) => pg_err,
                        _ => SqlxDatabaseError::Sqlx(sqlx::Error::Protocol(format!(
                            "CASCADE operation failed: {e}"
                        ))),
                    });
            }
            DropBehavior::Restrict => {
                return postgres_sqlx_exec_drop_table_restrict_native(connection, statement)
                    .await
                    .map_err(|e| match e {
                        DatabaseError::PostgresSqlx(pg_err) => pg_err,
                        _ => SqlxDatabaseError::Sqlx(sqlx::Error::Protocol(format!(
                            "RESTRICT operation failed: {e}"
                        ))),
                    });
            }
            DropBehavior::Default => {}
        }
    }

    let mut query = "DROP TABLE ".to_string();

    if statement.if_exists {
        query.push_str("IF EXISTS ");
    }

    query.push_str(statement.table_name);

    log::trace!("exec_drop_table: query:\n{query}");

    connection
        .execute(query.as_str())
        .await
        .map_err(SqlxDatabaseError::Sqlx)?;

    Ok(())
}

#[cfg(feature = "schema")]
pub(crate) async fn postgres_sqlx_exec_create_index(
    connection: &mut PgConnection,
    statement: &crate::schema::CreateIndexStatement<'_>,
) -> Result<(), SqlxDatabaseError> {
    let unique_str = if statement.unique { "UNIQUE " } else { "" };
    let if_not_exists_str = if statement.if_not_exists {
        "IF NOT EXISTS "
    } else {
        ""
    };

    let columns_str = statement
        .columns
        .iter()
        .map(|col| format!("\"{col}\"")) // PostgreSQL uses double quotes
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!(
        "CREATE {}INDEX {}{} ON {} ({})",
        unique_str, if_not_exists_str, statement.index_name, statement.table_name, columns_str
    );

    log::trace!("exec_create_index: query:\n{sql}");

    connection
        .execute(sql.as_str())
        .await
        .map_err(SqlxDatabaseError::Sqlx)?;

    Ok(())
}

#[cfg(feature = "schema")]
pub(crate) async fn postgres_sqlx_exec_drop_index(
    connection: &mut PgConnection,
    statement: &crate::schema::DropIndexStatement<'_>,
) -> Result<(), SqlxDatabaseError> {
    let if_exists_str = if statement.if_exists {
        "IF EXISTS "
    } else {
        ""
    };

    let sql = format!("DROP INDEX {}{}", if_exists_str, statement.index_name);

    log::trace!("exec_drop_index: query:\n{sql}");

    connection
        .execute(sql.as_str())
        .await
        .map_err(SqlxDatabaseError::Sqlx)?;

    Ok(())
}

#[cfg(feature = "schema")]
#[allow(clippy::too_many_lines)]
pub(crate) async fn postgres_sqlx_exec_alter_table(
    connection: &mut PgConnection,
    statement: &crate::schema::AlterTableStatement<'_>,
) -> Result<(), SqlxDatabaseError> {
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
                    crate::schema::DataType::Text => "TEXT".to_string(),
                    crate::schema::DataType::Char(len) => format!("CHAR({len})"),
                    crate::schema::DataType::Bool => "BOOLEAN".to_string(),
                    crate::schema::DataType::TinyInt | crate::schema::DataType::SmallInt => {
                        "SMALLINT".to_string()
                    }
                    crate::schema::DataType::Int => "INTEGER".to_string(),
                    crate::schema::DataType::BigInt => "BIGINT".to_string(),
                    crate::schema::DataType::Serial => "SERIAL".to_string(),
                    crate::schema::DataType::BigSerial => "BIGSERIAL".to_string(),
                    crate::schema::DataType::Real => "REAL".to_string(),
                    crate::schema::DataType::Double => "DOUBLE PRECISION".to_string(),
                    crate::schema::DataType::Decimal(precision, scale) => {
                        format!("DECIMAL({precision}, {scale})")
                    }
                    crate::schema::DataType::Money => "MONEY".to_string(),
                    crate::schema::DataType::Date => "DATE".to_string(),
                    crate::schema::DataType::Time => "TIME".to_string(),
                    crate::schema::DataType::DateTime => "TIMESTAMP WITH TIME ZONE".to_string(),
                    crate::schema::DataType::Timestamp => "TIMESTAMP".to_string(),
                    crate::schema::DataType::Blob | crate::schema::DataType::Binary(_) => {
                        "BYTEA".to_string()
                    }
                    crate::schema::DataType::Json => "JSON".to_string(),
                    crate::schema::DataType::Jsonb => "JSONB".to_string(),
                    crate::schema::DataType::Uuid => "UUID".to_string(),
                    crate::schema::DataType::Xml => "XML".to_string(),
                    crate::schema::DataType::Array(inner_type) => match **inner_type {
                        crate::schema::DataType::Int => "INTEGER[]".to_string(),
                        crate::schema::DataType::BigInt => "BIGINT[]".to_string(),
                        _ => "TEXT[]".to_string(),
                    },
                    crate::schema::DataType::Inet => "INET".to_string(),
                    crate::schema::DataType::MacAddr => "MACADDR".to_string(),
                    crate::schema::DataType::Custom(type_name) => type_name.clone(),
                };

                let nullable_str = if *nullable { "" } else { " NOT NULL" };
                let default_str = match default {
                    Some(val) => {
                        let val_str = match val {
                            crate::DatabaseValue::String(s) => format!("'{s}'"),
                            crate::DatabaseValue::Int64(n) => n.to_string(),
                            crate::DatabaseValue::UInt8(n) => n.to_string(),
                            crate::DatabaseValue::UInt16(n) => n.to_string(),
                            crate::DatabaseValue::UInt32(n) => n.to_string(),
                            crate::DatabaseValue::UInt64(n) => n.to_string(),
                            crate::DatabaseValue::Bool(b) => b.to_string(),
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
                                return Err(SqlxDatabaseError::Sqlx(sqlx::Error::TypeNotFound {
                                    type_name:
                                        "Unsupported default value type for ALTER TABLE ADD COLUMN"
                                            .to_string(),
                                }));
                            }
                        };
                        format!(" DEFAULT {val_str}")
                    }
                    None => String::new(),
                };

                let sql = format!(
                    "ALTER TABLE {} ADD COLUMN \"{}\" {}{}{}",
                    statement.table_name, name, type_str, nullable_str, default_str
                );

                log::trace!("exec_alter_table ADD COLUMN: query:\n{sql}");

                connection
                    .execute(sql.as_str())
                    .await
                    .map_err(SqlxDatabaseError::Sqlx)?;
            }
            AlterOperation::DropColumn {
                name,
                #[cfg(feature = "cascade")]
                behavior,
            } => {
                #[allow(unused_mut)]
                let mut sql = format!(
                    "ALTER TABLE {} DROP COLUMN \"{}\"",
                    statement.table_name, name
                );

                #[cfg(feature = "cascade")]
                {
                    use crate::schema::DropBehavior;
                    match behavior {
                        DropBehavior::Cascade => sql.push_str(" CASCADE"),
                        DropBehavior::Restrict => sql.push_str(" RESTRICT"),
                        DropBehavior::Default => {} // PostgreSQL defaults to RESTRICT
                    }
                }

                log::trace!("exec_alter_table DROP COLUMN: query:\n{sql}");

                connection
                    .execute(sql.as_str())
                    .await
                    .map_err(SqlxDatabaseError::Sqlx)?;
            }
            AlterOperation::RenameColumn { old_name, new_name } => {
                let sql = format!(
                    "ALTER TABLE {} RENAME COLUMN \"{}\" TO \"{}\"",
                    statement.table_name, old_name, new_name
                );

                log::trace!("exec_alter_table RENAME COLUMN: query:\n{sql}");

                connection
                    .execute(sql.as_str())
                    .await
                    .map_err(SqlxDatabaseError::Sqlx)?;
            }
            AlterOperation::ModifyColumn {
                name,
                new_data_type,
                new_nullable,
                new_default,
            } => {
                // PostgreSQL supports native ALTER COLUMN for type changes
                let type_str = match new_data_type {
                    crate::schema::DataType::VarChar(len) => format!("VARCHAR({len})"),
                    crate::schema::DataType::Text => "TEXT".to_string(),
                    crate::schema::DataType::Char(len) => format!("CHAR({len})"),
                    crate::schema::DataType::Bool => "BOOLEAN".to_string(),
                    crate::schema::DataType::TinyInt | crate::schema::DataType::SmallInt => {
                        "SMALLINT".to_string()
                    }
                    crate::schema::DataType::Int => "INTEGER".to_string(),
                    crate::schema::DataType::BigInt => "BIGINT".to_string(),
                    crate::schema::DataType::Serial => "SERIAL".to_string(),
                    crate::schema::DataType::BigSerial => "BIGSERIAL".to_string(),
                    crate::schema::DataType::Real => "REAL".to_string(),
                    crate::schema::DataType::Double => "DOUBLE PRECISION".to_string(),
                    crate::schema::DataType::Decimal(precision, scale) => {
                        format!("DECIMAL({precision}, {scale})")
                    }
                    crate::schema::DataType::Money => "MONEY".to_string(),
                    crate::schema::DataType::Date => "DATE".to_string(),
                    crate::schema::DataType::Time => "TIME".to_string(),
                    crate::schema::DataType::DateTime => "TIMESTAMP WITH TIME ZONE".to_string(),
                    crate::schema::DataType::Timestamp => "TIMESTAMP".to_string(),
                    crate::schema::DataType::Blob | crate::schema::DataType::Binary(_) => {
                        "BYTEA".to_string()
                    }
                    crate::schema::DataType::Json => "JSON".to_string(),
                    crate::schema::DataType::Jsonb => "JSONB".to_string(),
                    crate::schema::DataType::Uuid => "UUID".to_string(),
                    crate::schema::DataType::Xml => "XML".to_string(),
                    crate::schema::DataType::Array(inner_type) => match **inner_type {
                        crate::schema::DataType::Int => "INTEGER[]".to_string(),
                        crate::schema::DataType::BigInt => "BIGINT[]".to_string(),
                        _ => "TEXT[]".to_string(),
                    },
                    crate::schema::DataType::Inet => "INET".to_string(),
                    crate::schema::DataType::MacAddr => "MACADDR".to_string(),
                    crate::schema::DataType::Custom(type_name) => type_name.clone(),
                };

                // Change data type
                let alter_type_sql = format!(
                    "ALTER TABLE {} ALTER COLUMN \"{}\" TYPE {} USING \"{}\"::{}",
                    statement.table_name, name, type_str, name, type_str
                );

                log::trace!("exec_alter_table MODIFY COLUMN (type): query:\n{alter_type_sql}");

                connection
                    .execute(alter_type_sql.as_str())
                    .await
                    .map_err(SqlxDatabaseError::Sqlx)?;

                // Change nullable constraint if specified
                if let Some(nullable) = new_nullable {
                    let nullable_sql = if *nullable {
                        format!(
                            "ALTER TABLE {} ALTER COLUMN \"{}\" DROP NOT NULL",
                            statement.table_name, name
                        )
                    } else {
                        format!(
                            "ALTER TABLE {} ALTER COLUMN \"{}\" SET NOT NULL",
                            statement.table_name, name
                        )
                    };

                    log::trace!(
                        "exec_alter_table MODIFY COLUMN (nullable): query:\n{nullable_sql}"
                    );

                    connection
                        .execute(nullable_sql.as_str())
                        .await
                        .map_err(SqlxDatabaseError::Sqlx)?;
                }

                // Change default value if specified
                if let Some(default) = new_default {
                    let default_str = match default {
                        crate::DatabaseValue::String(s) => format!("'{s}'"),
                        crate::DatabaseValue::Int64(n) => n.to_string(),
                        crate::DatabaseValue::UInt8(n) => n.to_string(),
                        crate::DatabaseValue::UInt16(n) => n.to_string(),
                        crate::DatabaseValue::UInt32(n) => n.to_string(),
                        crate::DatabaseValue::UInt64(n) => n.to_string(),
                        crate::DatabaseValue::Bool(b) => b.to_string(),
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
                        crate::DatabaseValue::Uuid(u) | crate::DatabaseValue::UuidOpt(Some(u)) => {
                            format!("'{u}'")
                        }
                        _ => {
                            return Err(SqlxDatabaseError::Sqlx(sqlx::Error::TypeNotFound {
                                type_name: "Unsupported default value type for MODIFY COLUMN"
                                    .to_string(),
                            }));
                        }
                    };

                    let default_sql = format!(
                        "ALTER TABLE {} ALTER COLUMN \"{}\" SET DEFAULT {}",
                        statement.table_name, name, default_str
                    );

                    log::trace!("exec_alter_table MODIFY COLUMN (default): query:\n{default_sql}");

                    connection
                        .execute(default_sql.as_str())
                        .await
                        .map_err(SqlxDatabaseError::Sqlx)?;
                }
            }
        }
    }

    Ok(())
}

fn column_value(value: &PgValueRef<'_>) -> Result<DatabaseValue, sqlx::Error> {
    if value.is_null() {
        return Ok(DatabaseValue::Null);
    }
    let owned = sqlx::ValueRef::to_owned(value);
    match value.type_info().name() {
        "BOOL" => Ok(DatabaseValue::Bool(owned.try_decode()?)),
        "\"CHAR\"" => Ok(DatabaseValue::Int8(owned.try_decode()?)),
        "SMALLINT" | "SMALLSERIAL" | "INT2" => Ok(DatabaseValue::Int16(owned.try_decode()?)),
        "INT" | "SERIAL" | "INT4" => Ok(DatabaseValue::Int32(owned.try_decode()?)),
        "BIGINT" | "BIGSERIAL" | "INT8" => Ok(DatabaseValue::Int64(owned.try_decode()?)),
        "REAL" | "FLOAT4" => Ok(DatabaseValue::Real32(owned.try_decode()?)),
        #[cfg(feature = "decimal")]
        "DECIMAL" | "NUMERIC" => Ok(DatabaseValue::Decimal(owned.try_decode()?)),
        "DOUBLE PRECISION" | "FLOAT8" => Ok(DatabaseValue::Real64(owned.try_decode()?)),
        "CHAR" | "VARCHAR" | "CHAR(N)" | "TEXT" | "NAME" | "CITEXT" | "BPCHAR" => {
            Ok(DatabaseValue::String(owned.try_decode()?))
        }
        "TIMESTAMP" => Ok(DatabaseValue::DateTime(owned.try_decode()?)),
        "TIMESTAMPTZ" => {
            let dt: chrono::DateTime<chrono::Utc> = owned.try_decode()?;
            Ok(DatabaseValue::DateTime(dt.naive_utc()))
        }
        #[cfg(feature = "uuid")]
        "UUID" => Ok(DatabaseValue::Uuid(owned.try_decode()?)),
        _ => Err(sqlx::Error::TypeNotFound {
            type_name: value.type_info().name().to_string(),
        }),
    }
}

fn from_row(column_names: &[String], row: &PgRow) -> Result<crate::Row, SqlxDatabaseError> {
    let mut columns = vec![];

    for column in column_names {
        columns.push((
            column.clone(),
            column_value(&row.try_get_raw(column.as_str())?)?,
        ));
    }

    Ok(crate::Row { columns })
}

async fn update_and_get_row(
    connection: &mut PgConnection,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Option<crate::Row>, DatabaseError> {
    let index = AtomicU16::new(0);
    let query = format!(
        "UPDATE {table_name} {} {} RETURNING *",
        build_set_clause(values, &index),
        build_update_where_clause(
            filters,
            limit,
            limit
                .map(|_| {
                    format!(
                        "SELECT CTID FROM {table_name} {}",
                        build_where_clause(filters, &index),
                    )
                })
                .as_deref(),
            &index
        ),
    );

    let all_values = values
        .iter()
        .flat_map(|(_, value)| {
            value
                .params()
                .unwrap_or(vec![])
                .into_iter()
                .cloned()
                .map(std::convert::Into::into)
        })
        .collect::<Vec<_>>();
    let mut all_filter_values = filters
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
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if limit.is_some() {
        all_filter_values.extend(all_filter_values.clone());
    }

    let all_values = [all_values, all_filter_values].concat();

    log::trace!("Running update query: {query} with params: {all_values:?}");

    let statement = connection.prepare(&query).await?;

    let column_names = statement
        .columns()
        .iter()
        .map(|x| x.name().to_string())
        .collect::<Vec<_>>();

    let query = bind_values(statement.query(), Some(&all_values))?;

    let mut stream = query.fetch(connection);
    let pg_row: Option<PgRow> = stream.next().await.transpose()?;

    Ok(pg_row
        .map(|row| from_row(&column_names, &row))
        .transpose()?)
}

async fn update_and_get_rows(
    connection: &mut PgConnection,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, DatabaseError> {
    let index = AtomicU16::new(0);
    let query = format!(
        "UPDATE {table_name} {} {} RETURNING *",
        build_set_clause(values, &index),
        build_update_where_clause(
            filters,
            limit,
            limit
                .map(|_| {
                    format!(
                        "SELECT CTID FROM {table_name} {}",
                        build_where_clause(filters, &index),
                    )
                })
                .as_deref(),
            &index
        ),
    );

    let all_values = values
        .iter()
        .flat_map(|(_, value)| {
            value
                .params()
                .unwrap_or(vec![])
                .into_iter()
                .cloned()
                .map(std::convert::Into::into)
        })
        .collect::<Vec<PgDatabaseValue>>();
    let mut all_filter_values = filters
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
                })
                .collect::<Vec<PgDatabaseValue>>()
        })
        .unwrap_or_default();

    if limit.is_some() {
        all_filter_values.extend(all_filter_values.clone());
    }

    let all_values = [all_values, all_filter_values].concat();

    log::trace!("Running update query: {query} with params: {all_values:?}");

    let statement = connection.prepare(&query).await?;

    let column_names = statement
        .columns()
        .iter()
        .map(|x| x.name().to_string())
        .collect::<Vec<_>>();

    let query = bind_values(statement.query(), Some(&all_values))?;

    Ok(to_rows(&column_names, query.fetch(connection)).await?)
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

fn build_where_clause(filters: Option<&[Box<dyn BooleanExpression>]>, index: &AtomicU16) -> String {
    filters.map_or_else(String::new, |filters| {
        if filters.is_empty() {
            String::new()
        } else {
            let filters = build_where_props(filters, index);
            format!("WHERE {}", filters.join(" AND "))
        }
    })
}

fn build_where_props(filters: &[Box<dyn BooleanExpression>], index: &AtomicU16) -> Vec<String> {
    filters
        .iter()
        .map(|filter| filter.deref().to_sql(index))
        .collect()
}

fn build_sort_clause(sorts: Option<&[Sort]>, index: &AtomicU16) -> String {
    sorts.map_or_else(String::new, |sorts| {
        if sorts.is_empty() {
            String::new()
        } else {
            format!("ORDER BY {}", build_sort_props(sorts, index).join(", "))
        }
    })
}

fn build_sort_props(sorts: &[Sort], index: &AtomicU16) -> Vec<String> {
    sorts.iter().map(|sort| sort.to_sql(index)).collect()
}

fn build_update_where_clause(
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
    query: Option<&str>,
    index: &AtomicU16,
) -> String {
    let clause = build_where_clause(filters, index);
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
            format!("CTID IN ({query} LIMIT {limit})")
        })
    })
}

fn build_set_clause(values: &[(&str, Box<dyn Expression>)], index: &AtomicU16) -> String {
    if values.is_empty() {
        String::new()
    } else {
        format!("SET {}", build_set_props(values, index).join(", "))
    }
}

fn build_set_props(values: &[(&str, Box<dyn Expression>)], index: &AtomicU16) -> Vec<String> {
    values
        .iter()
        .map(|(name, value)| {
            format!(
                "{}={}",
                format_identifier(name),
                value.deref().to_sql(index)
            )
        })
        .collect()
}

fn build_values_clause(values: &[(&str, Box<dyn Expression>)], index: &AtomicU16) -> String {
    if values.is_empty() {
        "DEFAULT VALUES".to_string()
    } else {
        let filters = build_values_props(values, index).join(", ");

        format!("VALUES({filters})")
    }
}

fn build_values_props(values: &[(&str, Box<dyn Expression>)], index: &AtomicU16) -> Vec<String> {
    values
        .iter()
        .map(|(_, value)| value.deref().to_sql(index))
        .collect()
}

#[cfg(feature = "cascade")]
fn sanitize_value(identifier: &str) -> String {
    identifier.replace('\'', "''")
}

fn format_identifier(identifier: &str) -> String {
    static NON_ALPHA_NUMERIC_REGEX: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"[^A-Za-z0-9_]").expect("Invalid Regex"));

    if identifier == "*" {
        identifier.to_string()
    } else if NON_ALPHA_NUMERIC_REGEX.is_match(identifier) {
        format!("\"{identifier}\"")
    } else {
        identifier.to_string()
    }
}

fn bind_values<'a, 'b>(
    mut query: Query<'a, Postgres, PgArguments>,
    values: Option<&'b [PgDatabaseValue]>,
) -> Result<Query<'a, Postgres, PgArguments>, DatabaseError>
where
    'b: 'a,
{
    if let Some(values) = values {
        for value in values {
            match &**value {
                DatabaseValue::String(value) | DatabaseValue::StringOpt(Some(value)) => {
                    query = query.bind(value);
                }
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
                | DatabaseValue::Now => (),
                #[cfg(feature = "decimal")]
                DatabaseValue::DecimalOpt(None) => (),
                #[cfg(feature = "uuid")]
                DatabaseValue::UuidOpt(None) => (),
                DatabaseValue::Bool(value) | DatabaseValue::BoolOpt(Some(value)) => {
                    query = query.bind(value);
                }
                DatabaseValue::Int8(value) | DatabaseValue::Int8Opt(Some(value)) => {
                    query = query.bind(i16::from(*value));
                }
                DatabaseValue::Int16(value) | DatabaseValue::Int16Opt(Some(value)) => {
                    query = query.bind(*value);
                }
                DatabaseValue::Int32(value) | DatabaseValue::Int32Opt(Some(value)) => {
                    query = query.bind(*value);
                }
                DatabaseValue::Int64(value) | DatabaseValue::Int64Opt(Some(value)) => {
                    query = query.bind(*value);
                }
                DatabaseValue::UInt8(value) | DatabaseValue::UInt8Opt(Some(value)) => {
                    let signed = i16::from(*value);
                    query = query.bind(signed);
                }
                DatabaseValue::UInt16(value) | DatabaseValue::UInt16Opt(Some(value)) => {
                    let signed =
                        i16::try_from(*value).map_err(|_| DatabaseError::UInt16Overflow(*value))?;
                    query = query.bind(signed);
                }
                DatabaseValue::UInt32(value) | DatabaseValue::UInt32Opt(Some(value)) => {
                    let signed =
                        i32::try_from(*value).map_err(|_| DatabaseError::UInt32Overflow(*value))?;
                    query = query.bind(signed);
                }
                DatabaseValue::UInt64(value) | DatabaseValue::UInt64Opt(Some(value)) => {
                    query = query.bind(
                        i64::try_from(*value).map_err(|_| SqlxDatabaseError::InvalidRequest)?,
                    );
                }
                DatabaseValue::Real64(value) | DatabaseValue::Real64Opt(Some(value)) => {
                    query = query.bind(*value);
                }
                DatabaseValue::Real32(value) | DatabaseValue::Real32Opt(Some(value)) => {
                    query = query.bind(*value);
                }
                #[cfg(feature = "decimal")]
                DatabaseValue::Decimal(value) | DatabaseValue::DecimalOpt(Some(value)) => {
                    query = query.bind(*value);
                }
                #[cfg(feature = "uuid")]
                DatabaseValue::Uuid(value) | DatabaseValue::UuidOpt(Some(value)) => {
                    query = query.bind(value);
                }
                DatabaseValue::NowPlus(_interval) => (),
                DatabaseValue::DateTime(value) => {
                    query = query.bind(value);
                }
            }
        }
    }
    Ok(query)
}

async fn to_rows<'a>(
    column_names: &[String],
    mut rows: Pin<Box<dyn Stream<Item = Result<PgRow, sqlx::Error>> + Send + 'a>>,
) -> Result<Vec<crate::Row>, SqlxDatabaseError> {
    let mut results = vec![];

    while let Some(row) = rows.next().await.transpose()? {
        results.push(from_row(column_names, &row)?);
    }

    log::trace!(
        "Got {} row{}",
        results.len(),
        if results.len() == 1 { "" } else { "s" }
    );

    Ok(results)
}

fn to_values(values: &[(&str, DatabaseValue)]) -> Vec<PgDatabaseValue> {
    values
        .iter()
        .map(|(_key, value)| value.clone().into())
        .collect::<Vec<_>>()
}

fn exprs_to_values(values: &[(&str, Box<dyn Expression>)]) -> Vec<PgDatabaseValue> {
    values
        .iter()
        .flat_map(|value| value.1.values().into_iter())
        .flatten()
        .cloned()
        .map(std::convert::Into::into)
        .collect::<Vec<_>>()
}

fn bexprs_to_values(values: &[Box<dyn BooleanExpression>]) -> Vec<PgDatabaseValue> {
    values
        .iter()
        .flat_map(|value| value.values().into_iter())
        .flatten()
        .cloned()
        .map(std::convert::Into::into)
        .collect::<Vec<_>>()
}

#[allow(unused)]
fn to_values_opt(values: Option<&[(&str, DatabaseValue)]>) -> Option<Vec<PgDatabaseValue>> {
    values.map(to_values)
}

#[allow(unused)]
fn exprs_to_values_opt(
    values: Option<&[(&str, Box<dyn Expression>)]>,
) -> Option<Vec<PgDatabaseValue>> {
    values.map(exprs_to_values)
}

fn bexprs_to_values_opt(
    values: Option<&[Box<dyn BooleanExpression>]>,
) -> Option<Vec<PgDatabaseValue>> {
    values.map(bexprs_to_values)
}

#[allow(clippy::too_many_arguments)]
async fn select(
    connection: &mut PgConnection,
    table_name: &str,
    distinct: bool,
    columns: &[&str],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    joins: Option<&[Join<'_>]>,
    sort: Option<&[Sort]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, DatabaseError> {
    let index = AtomicU16::new(0);
    let query = format!(
        "SELECT {} {} FROM {table_name} {} {} {} {}",
        if distinct { "DISTINCT" } else { "" },
        columns
            .iter()
            .map(|x| format_identifier(x))
            .collect::<Vec<_>>()
            .join(", "),
        build_join_clauses(joins),
        build_where_clause(filters, &index),
        build_sort_clause(sort, &index),
        limit.map_or_else(String::new, |limit| format!("LIMIT {limit}"))
    );

    log::trace!(
        "Running select query: {query} with params: {:?}",
        filters.map(|f| f.iter().filter_map(|x| x.params()).collect::<Vec<_>>())
    );

    let statement = connection.prepare(&query).await?;
    let column_names = statement
        .columns()
        .iter()
        .map(|x| x.name().to_string())
        .collect::<Vec<_>>();

    let filters = bexprs_to_values_opt(filters);
    let query = bind_values(statement.query(), filters.as_deref())?;

    Ok(to_rows(&column_names, query.fetch(connection)).await?)
}

async fn delete(
    connection: &mut PgConnection,
    table_name: &str,
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, DatabaseError> {
    let index = AtomicU16::new(0);

    // PostgreSQL doesn't support LIMIT directly in DELETE statements
    // Use subquery with ctid for LIMIT support
    let query = limit.map_or_else(|| format!(
            "DELETE FROM {table_name} {} RETURNING *",
            build_where_clause(filters, &index)
        ), |limit| format!(
            "DELETE FROM {table_name} WHERE ctid IN (SELECT ctid FROM {table_name} {} LIMIT {}) RETURNING *",
            build_where_clause(filters, &index),
            limit
        ));

    log::trace!(
        "Running delete query: {query} with params: {:?}",
        filters.map(|f| f.iter().filter_map(|x| x.params()).collect::<Vec<_>>())
    );

    let statement = connection.prepare(&query).await?;
    let column_names = statement
        .columns()
        .iter()
        .map(|x| x.name().to_string())
        .collect::<Vec<_>>();

    // For LIMIT queries, we need to duplicate the filter parameters
    // since they appear twice in the query (main WHERE and subquery WHERE)
    let filters = bexprs_to_values_opt(filters);
    let all_filters = if let Some(filters) = filters.clone()
        && limit.is_some()
    {
        let mut all = filters.clone();
        all.extend(filters);
        Some(all)
    } else {
        filters
    };

    let query = bind_values(statement.query(), all_filters.as_deref())?;

    Ok(to_rows(&column_names, query.fetch(connection)).await?)
}

async fn find_row(
    connection: &mut PgConnection,
    table_name: &str,
    distinct: bool,
    columns: &[&str],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    joins: Option<&[Join<'_>]>,
    sort: Option<&[Sort]>,
) -> Result<Option<crate::Row>, DatabaseError> {
    let index = AtomicU16::new(0);
    let query = format!(
        "SELECT {} {} FROM {table_name} {} {} {} LIMIT 1",
        if distinct { "DISTINCT" } else { "" },
        columns
            .iter()
            .map(|x| format_identifier(x))
            .collect::<Vec<_>>()
            .join(", "),
        build_join_clauses(joins),
        build_where_clause(filters, &index),
        build_sort_clause(sort, &index),
    );

    log::trace!(
        "Running find_row query: {query} with params: {:?}",
        filters.map(|f| f.iter().filter_map(|x| x.params()).collect::<Vec<_>>())
    );

    let statement = connection.prepare(&query).await?;
    let column_names = statement
        .columns()
        .iter()
        .map(|x| x.name().to_string())
        .collect::<Vec<_>>();

    let filters = bexprs_to_values_opt(filters);
    let query = bind_values(statement.query(), filters.as_deref())?;

    let mut query = query.fetch(connection);

    Ok(query
        .next()
        .await
        .transpose()?
        .map(|row| from_row(&column_names, &row))
        .transpose()?)
}

async fn insert_and_get_row(
    connection: &mut PgConnection,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
) -> Result<crate::Row, DatabaseError> {
    let column_names = values
        .iter()
        .map(|(key, _v)| format_identifier(key))
        .collect::<Vec<_>>()
        .join(", ");

    let index = AtomicU16::new(0);
    let insert_columns = if values.is_empty() {
        String::new()
    } else {
        format!("({column_names})")
    };
    let query = format!(
        "INSERT INTO {table_name} {insert_columns} {} RETURNING *",
        build_values_clause(values, &index),
    );

    log::trace!(
        "Running insert_and_get_row query: '{query}' with params: {:?}",
        values
            .iter()
            .filter_map(|(_, x)| x.params())
            .collect::<Vec<_>>()
    );

    let statement = connection.prepare(&query).await?;
    let column_names = statement
        .columns()
        .iter()
        .map(|x| x.name().to_string())
        .collect::<Vec<_>>();

    let values = exprs_to_values(values);
    let query = bind_values(statement.query(), Some(&values))?;

    let mut stream = query.fetch(connection);

    Ok(stream
        .next()
        .await
        .transpose()?
        .map(|row| from_row(&column_names, &row))
        .ok_or(SqlxDatabaseError::NoRow)??)
}

/// # Errors
///
/// Will return `Err` if the update multi execution failed.
pub async fn update_multi(
    connection: &mut PgConnection,
    table_name: &str,
    values: &[Vec<(&str, Box<dyn Expression>)>],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    mut limit: Option<usize>,
) -> Result<Vec<crate::Row>, DatabaseError> {
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
            results.append(
                &mut update_chunk(connection, table_name, &values[last_i..i], filters, limit)
                    .await?,
            );
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
        results.append(
            &mut update_chunk(connection, table_name, &values[last_i..], filters, limit).await?,
        );
    }

    Ok(results)
}

async fn update_chunk(
    connection: &mut PgConnection,
    table_name: &str,
    values: &[Vec<(&str, Box<dyn Expression>)>],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, DatabaseError> {
    let first = values[0].as_slice();
    let expected_value_size = first.len();

    if let Some(bad_row) = values.iter().skip(1).find(|v| {
        v.len() != expected_value_size || v.iter().enumerate().any(|(i, c)| c.0 != first[i].0)
    }) {
        log::error!("Bad row: {bad_row:?}. Expected to match schema of first row: {first:?}");
        return Err(SqlxDatabaseError::InvalidRequest.into());
    }

    let set_clause = values[0]
        .iter()
        .map(|(name, _value)| {
            format!(
                "{} = EXCLUDED.{}",
                format_identifier(name),
                format_identifier(name)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    let column_names = values[0]
        .iter()
        .map(|(key, _v)| format_identifier(key))
        .collect::<Vec<_>>()
        .join(", ");

    let index = AtomicU16::new(0);
    let query = format!(
        "
        UPDATE {table_name} ({column_names})
        {}
        SET {set_clause}
        RETURNING *",
        build_update_where_clause(
            filters,
            limit,
            limit
                .map(|_| {
                    format!(
                        "SELECT CTID FROM {table_name} {}",
                        build_where_clause(filters, &index),
                    )
                })
                .as_deref(),
            &index
        ),
    );

    let all_values = values
        .iter()
        .flat_map(std::iter::IntoIterator::into_iter)
        .flat_map(|(_, value)| {
            value
                .params()
                .unwrap_or(vec![])
                .into_iter()
                .cloned()
                .map(std::convert::Into::into)
        })
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

    let statement = connection.prepare(&query).await?;
    let column_names = statement
        .columns()
        .iter()
        .map(|x| x.name().to_string())
        .collect::<Vec<_>>();

    let query = bind_values(statement.query(), Some(&all_values))?;

    Ok(to_rows(&column_names, query.fetch(connection)).await?)
}

/// # Errors
///
/// Will return `Err` if the upsert multi execution failed.
pub async fn upsert_multi(
    connection: &mut PgConnection,
    table_name: &str,
    unique: &[Box<dyn Expression>],
    values: &[Vec<(&str, Box<dyn Expression>)>],
) -> Result<Vec<crate::Row>, DatabaseError> {
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
            results.append(
                &mut upsert_chunk(connection, table_name, unique, &values[last_i..i]).await?,
            );
            last_i = i;
            pos = 0;
        }
        i += 1;
        pos += count;
    }

    if i > last_i {
        results.append(&mut upsert_chunk(connection, table_name, unique, &values[last_i..]).await?);
    }

    Ok(results)
}

async fn upsert_chunk(
    connection: &mut PgConnection,
    table_name: &str,
    unique: &[Box<dyn Expression>],
    values: &[Vec<(&str, Box<dyn Expression>)>],
) -> Result<Vec<crate::Row>, DatabaseError> {
    let first = values[0].as_slice();
    let expected_value_size = first.len();

    if let Some(bad_row) = values.iter().skip(1).find(|v| {
        v.len() != expected_value_size || v.iter().enumerate().any(|(i, c)| c.0 != first[i].0)
    }) {
        log::error!("Bad row: {bad_row:?}. Expected to match schema of first row: {first:?}");
        return Err(SqlxDatabaseError::InvalidRequest.into());
    }

    let set_clause = values[0]
        .iter()
        .map(|(name, _value)| {
            format!(
                "{} = EXCLUDED.{}",
                format_identifier(name),
                format_identifier(name)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    let column_names = values[0]
        .iter()
        .map(|(key, _v)| format_identifier(key))
        .collect::<Vec<_>>()
        .join(", ");

    let index = AtomicU16::new(0);
    let values_str_list = values
        .iter()
        .map(|v| format!("({})", build_values_props(v, &index).join(", ")))
        .collect::<Vec<_>>();

    let values_str = values_str_list.join(", ");

    let unique_conflict = unique
        .iter()
        .map(|x| x.to_sql(&index))
        .collect::<Vec<_>>()
        .join(", ");

    let query = format!(
        "
        INSERT INTO {table_name} ({column_names})
        VALUES {values_str}
        ON CONFLICT({unique_conflict}) DO UPDATE
            SET {set_clause}
        RETURNING *"
    );

    let all_values = &values
        .iter()
        .flat_map(std::iter::IntoIterator::into_iter)
        .flat_map(|(_, value)| {
            value
                .params()
                .unwrap_or(vec![])
                .into_iter()
                .cloned()
                .map(std::convert::Into::into)
        })
        .collect::<Vec<_>>();

    log::trace!("Running upsert chunk query: {query} with params: {all_values:?}");

    let statement = connection.prepare(&query).await?;
    let column_names = statement
        .columns()
        .iter()
        .map(|x| x.name().to_string())
        .collect::<Vec<_>>();

    let query = bind_values(statement.query(), Some(all_values))?;

    Ok(to_rows(&column_names, query.fetch(connection)).await?)
}

async fn upsert(
    connection: &mut PgConnection,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, DatabaseError> {
    let rows = update_and_get_rows(connection, table_name, values, filters, limit).await?;

    Ok(if rows.is_empty() {
        vec![insert_and_get_row(connection, table_name, values).await?]
    } else {
        rows
    })
}

async fn upsert_and_get_row(
    connection: &mut PgConnection,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<crate::Row, DatabaseError> {
    match find_row(connection, table_name, false, &["*"], filters, None, None).await? {
        Some(row) => {
            let updated = update_and_get_row(connection, table_name, values, filters, limit)
                .await?
                .unwrap();

            let str1 = format!("{row:?}");
            let str2 = format!("{updated:?}");

            if str1 == str2 {
                log::trace!("No updates to {table_name}");
            } else {
                log::debug!("Changed {table_name} from {str1} to {str2}");
            }

            Ok(updated)
        }
        None => Ok(insert_and_get_row(connection, table_name, values).await?),
    }
}

/// Wrapper type for converting `DatabaseValue` to `PostgreSQL` `SQLx`-specific parameter types
#[derive(Debug, Clone)]
pub struct PgDatabaseValue(DatabaseValue);

impl From<DatabaseValue> for PgDatabaseValue {
    fn from(value: DatabaseValue) -> Self {
        Self(value)
    }
}

impl Deref for PgDatabaseValue {
    type Target = DatabaseValue;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Expression for PgDatabaseValue {
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

fn postgres_transform_query_for_params(
    query: &str,
    params: &[DatabaseValue],
) -> (String, Vec<DatabaseValue>) {
    let mut transformed_query = query.to_string();
    let mut output_params = Vec::new();
    let mut param_counter = 1;

    for (i, param) in params.iter().enumerate() {
        let old_placeholder = format!("${}", i + 1);

        match param {
            DatabaseValue::Now => {
                transformed_query = transformed_query.replace(&old_placeholder, "NOW()");
            }
            DatabaseValue::NowPlus(interval) => {
                let new_placeholder = format!("${param_counter}");
                transformed_query = transformed_query.replace(
                    &old_placeholder,
                    &format!("(NOW() + {new_placeholder}::interval)"),
                );

                let interval_string = postgres_interval_to_string_sqlx(interval);
                output_params.push(DatabaseValue::String(interval_string));
                param_counter += 1;
            }
            other => {
                if param_counter != i + 1 {
                    let new_placeholder = format!("${param_counter}");
                    transformed_query =
                        transformed_query.replace(&old_placeholder, &new_placeholder);
                }
                output_params.push(other.clone());
                param_counter += 1;
            }
        }
    }

    (transformed_query, output_params)
}

#[cfg(all(test, feature = "schema"))]
mod tests {
    use super::*;
    use crate::sqlx::postgres::PostgresSqlxDatabase;

    fn get_postgres_test_url() -> Option<String> {
        std::env::var("POSTGRES_TEST_URL").ok()
    }

    async fn create_pool(url: &str) -> Result<Arc<Mutex<PgPool>>, sqlx::Error> {
        let pool = PgPool::connect(url).await?;
        Ok(Arc::new(Mutex::new(pool)))
    }

    #[switchy_async::test]
    async fn test_postgres_sqlx_table_exists() {
        let Some(url) = get_postgres_test_url() else {
            return;
        };

        let pool = create_pool(&url).await.expect("Failed to create pool");
        let db = PostgresSqlxDatabase::new(pool);

        // Test non-existent table
        assert!(!db.table_exists("non_existent_table").await.unwrap());

        // Create test table
        db.exec_raw("CREATE TABLE IF NOT EXISTS test_table_exists_sqlx (id INTEGER PRIMARY KEY)")
            .await
            .unwrap();

        // Test existing table
        assert!(db.table_exists("test_table_exists_sqlx").await.unwrap());

        // Clean up
        db.exec_raw("DROP TABLE IF EXISTS test_table_exists_sqlx")
            .await
            .unwrap();
    }

    #[switchy_async::test]
    async fn test_postgres_sqlx_column_metadata() {
        let Some(url) = get_postgres_test_url() else {
            return;
        };

        let pool = create_pool(&url).await.expect("Failed to create pool");
        let db = PostgresSqlxDatabase::new(pool);

        // Create test table with various column types
        db.exec_raw(
            "CREATE TABLE IF NOT EXISTS test_column_metadata_sqlx (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            age INTEGER DEFAULT 0,
            height REAL,
            is_active BOOLEAN DEFAULT true,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )",
        )
        .await
        .unwrap();

        // Get column metadata
        let columns = db
            .get_table_columns("test_column_metadata_sqlx")
            .await
            .unwrap();

        // Verify we have the expected columns
        assert_eq!(columns.len(), 6);

        // Check ID column (should be primary key)
        let id_column = columns.iter().find(|c| c.name == "id").unwrap();
        assert!(id_column.is_primary_key);
        assert!(!id_column.nullable);

        // Check name column (not null, text)
        let name_column = columns.iter().find(|c| c.name == "name").unwrap();
        assert!(!name_column.nullable);
        assert!(!name_column.is_primary_key);

        // Check age column (has default)
        let age_column = columns.iter().find(|c| c.name == "age").unwrap();
        assert!(age_column.nullable); // PostgreSQL allows NULL even with DEFAULT
        assert!(age_column.default_value.is_some());

        // Clean up
        db.exec_raw("DROP TABLE IF EXISTS test_column_metadata_sqlx")
            .await
            .unwrap();
    }

    #[switchy_async::test]
    async fn test_postgres_sqlx_constraints() {
        let Some(url) = get_postgres_test_url() else {
            return;
        };

        let pool = create_pool(&url).await.expect("Failed to create pool");
        let db = PostgresSqlxDatabase::new(pool);

        // Create test tables with constraints
        db.exec_raw(
            "CREATE TABLE IF NOT EXISTS test_parent_sqlx (
            id SERIAL PRIMARY KEY,
            email VARCHAR(100) UNIQUE
        )",
        )
        .await
        .unwrap();

        db.exec_raw(
            "CREATE TABLE IF NOT EXISTS test_child_sqlx (
            id SERIAL PRIMARY KEY,
            parent_id INTEGER REFERENCES test_parent_sqlx(id),
            name TEXT NOT NULL
        )",
        )
        .await
        .unwrap();

        // Get table info with constraints
        let table_info = db
            .get_table_info("test_parent_sqlx")
            .await
            .unwrap()
            .unwrap();

        // Should have primary key and unique constraints reflected in indexes
        assert!(!table_info.indexes.is_empty());

        let child_info = db.get_table_info("test_child_sqlx").await.unwrap().unwrap();

        // Should have foreign key constraint
        assert!(!child_info.foreign_keys.is_empty());

        // Clean up (order matters due to foreign key)
        db.exec_raw("DROP TABLE IF EXISTS test_child_sqlx")
            .await
            .unwrap();
        db.exec_raw("DROP TABLE IF EXISTS test_parent_sqlx")
            .await
            .unwrap();
    }

    #[switchy_async::test]
    async fn test_postgres_sqlx_type_mapping() {
        let Some(url) = get_postgres_test_url() else {
            return;
        };

        let pool = create_pool(&url).await.expect("Failed to create pool");
        let db = PostgresSqlxDatabase::new(pool);

        // Create test table with all supported PostgreSQL types
        db.exec_raw(
            "CREATE TABLE IF NOT EXISTS test_type_mapping_sqlx (
            small_col SMALLINT,
            int_col INTEGER,
            big_col BIGINT,
            real_col REAL,
            double_col DOUBLE PRECISION,
            decimal_col DECIMAL(10,2),
            text_col TEXT,
            varchar_col VARCHAR(50),
            bool_col BOOLEAN,
            timestamp_col TIMESTAMP
        )",
        )
        .await
        .unwrap();

        let columns = db
            .get_table_columns("test_type_mapping_sqlx")
            .await
            .unwrap();

        // Verify type mapping
        let small_col = columns.iter().find(|c| c.name == "small_col").unwrap();
        assert!(matches!(
            small_col.data_type,
            crate::schema::DataType::SmallInt
        ));

        let int_col = columns.iter().find(|c| c.name == "int_col").unwrap();
        assert!(matches!(int_col.data_type, crate::schema::DataType::Int));

        let big_col = columns.iter().find(|c| c.name == "big_col").unwrap();
        assert!(matches!(big_col.data_type, crate::schema::DataType::BigInt));

        let text_col = columns.iter().find(|c| c.name == "text_col").unwrap();
        assert!(matches!(text_col.data_type, crate::schema::DataType::Text));

        let varchar_col = columns.iter().find(|c| c.name == "varchar_col").unwrap();
        assert!(matches!(
            varchar_col.data_type,
            crate::schema::DataType::VarChar(50)
        ));

        let bool_col = columns.iter().find(|c| c.name == "bool_col").unwrap();
        assert!(matches!(bool_col.data_type, crate::schema::DataType::Bool));

        // Clean up
        db.exec_raw("DROP TABLE IF EXISTS test_type_mapping_sqlx")
            .await
            .unwrap();
    }

    #[switchy_async::test]
    async fn test_postgres_sqlx_default_values() {
        let Some(url) = get_postgres_test_url() else {
            return;
        };

        let pool = create_pool(&url).await.expect("Failed to create pool");
        let db = PostgresSqlxDatabase::new(pool);

        // Create test table with various default value formats
        db.exec_raw(
            "CREATE TABLE IF NOT EXISTS test_default_values_sqlx (
            id SERIAL PRIMARY KEY,
            name TEXT DEFAULT 'unknown',
            age INTEGER DEFAULT 18,
            is_active BOOLEAN DEFAULT true,
            score REAL DEFAULT 0.0,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )",
        )
        .await
        .unwrap();

        let columns = db
            .get_table_columns("test_default_values_sqlx")
            .await
            .unwrap();

        // Check string default
        let name_col = columns.iter().find(|c| c.name == "name").unwrap();
        assert!(name_col.default_value.is_some());

        // Check numeric default
        let age_col = columns.iter().find(|c| c.name == "age").unwrap();
        assert!(age_col.default_value.is_some());

        // Check boolean default
        let active_col = columns.iter().find(|c| c.name == "is_active").unwrap();
        assert!(active_col.default_value.is_some());

        // Check real default
        let score_col = columns.iter().find(|c| c.name == "score").unwrap();
        assert!(score_col.default_value.is_some());

        // Function defaults like CURRENT_TIMESTAMP may not be parsed as simple values
        let _created_col = columns.iter().find(|c| c.name == "created_at").unwrap();
        // This might be None due to complexity of CURRENT_TIMESTAMP

        // Clean up
        db.exec_raw("DROP TABLE IF EXISTS test_default_values_sqlx")
            .await
            .unwrap();
    }

    #[switchy_async::test]
    async fn test_postgres_sqlx_transaction_isolation() {
        let Some(url) = get_postgres_test_url() else {
            return;
        };

        let pool = create_pool(&url).await.expect("Failed to create pool");
        let db = PostgresSqlxDatabase::new(pool);

        // Create test table
        db.exec_raw("CREATE TABLE IF NOT EXISTS test_transaction_iso_sqlx (id INTEGER PRIMARY KEY, name TEXT)").await.unwrap();

        // Start transaction
        let tx = db.begin_transaction().await.unwrap();

        // Test introspection methods work within transaction
        assert!(tx.table_exists("test_transaction_iso_sqlx").await.unwrap());

        // Test column_exists within transaction
        assert!(
            tx.column_exists("test_transaction_iso_sqlx", "id")
                .await
                .unwrap()
        );
        assert!(
            !tx.column_exists("test_transaction_iso_sqlx", "nonexistent")
                .await
                .unwrap()
        );

        // Test get_table_columns within transaction
        let columns = tx
            .get_table_columns("test_transaction_iso_sqlx")
            .await
            .unwrap();
        assert_eq!(columns.len(), 2);

        // Test get_table_info within transaction
        let table_info = tx
            .get_table_info("test_transaction_iso_sqlx")
            .await
            .unwrap();
        assert!(table_info.is_some());

        // Commit transaction
        tx.commit().await.unwrap();

        // Clean up
        db.exec_raw("DROP TABLE IF EXISTS test_transaction_iso_sqlx")
            .await
            .unwrap();
    }

    #[switchy_async::test]
    async fn test_postgres_sqlx_savepoint_basic() {
        let Some(url) = get_postgres_test_url() else {
            return;
        };

        let pool = create_pool(&url).await.expect("Failed to create pool");
        let db = PostgresSqlxDatabase::new(pool);

        // Start transaction
        let tx = db.begin_transaction().await.unwrap();

        // Create savepoint
        let savepoint = tx.savepoint("test_sp").await.unwrap();
        assert_eq!(savepoint.name(), "test_sp");

        // Release savepoint
        savepoint.release().await.unwrap();

        // Commit transaction
        tx.commit().await.unwrap();
    }

    #[switchy_async::test]
    async fn test_postgres_sqlx_savepoint_release() {
        let Some(url) = get_postgres_test_url() else {
            return;
        };

        let pool = create_pool(&url).await.expect("Failed to create pool");
        let db = PostgresSqlxDatabase::new(pool);

        // Start transaction
        let tx = db.begin_transaction().await.unwrap();

        // Create savepoint
        let savepoint = tx.savepoint("test_sp").await.unwrap();

        // Release savepoint
        savepoint.release().await.unwrap();

        // Commit transaction
        tx.commit().await.unwrap();
    }

    #[switchy_async::test]
    async fn test_postgres_sqlx_savepoint_rollback() {
        let Some(url) = get_postgres_test_url() else {
            return;
        };

        let pool = create_pool(&url).await.expect("Failed to create pool");
        let db = PostgresSqlxDatabase::new(pool);

        // Start transaction
        let tx = db.begin_transaction().await.unwrap();

        // Create savepoint
        let savepoint = tx.savepoint("test_sp").await.unwrap();

        // Rollback to savepoint
        savepoint.rollback_to().await.unwrap();

        // Commit transaction
        tx.commit().await.unwrap();
    }

    #[switchy_async::test]
    async fn test_postgres_sqlx_savepoint_after_transaction_commit() {
        let Some(url) = get_postgres_test_url() else {
            return;
        };

        let pool = create_pool(&url).await.expect("Failed to create pool");
        let db = PostgresSqlxDatabase::new(pool);

        // Start transaction
        let tx = db.begin_transaction().await.unwrap();

        // Create savepoint
        let savepoint = tx.savepoint("test_sp").await.unwrap();

        // Commit transaction (this makes savepoint operations fail)
        tx.commit().await.unwrap();

        // Try to release savepoint after commit - should fail
        let result = savepoint.release().await;
        assert!(matches!(result, Err(DatabaseError::TransactionCommitted)));
    }

    #[switchy_async::test]
    async fn test_postgres_sqlx_savepoint_after_transaction_rollback() {
        let Some(url) = get_postgres_test_url() else {
            return;
        };

        let pool = create_pool(&url).await.expect("Failed to create pool");
        let db = PostgresSqlxDatabase::new(pool);

        // Start transaction
        let tx = db.begin_transaction().await.unwrap();

        // Create savepoint
        let savepoint = tx.savepoint("test_sp").await.unwrap();

        // Rollback transaction (this makes savepoint operations fail)
        tx.rollback().await.unwrap();

        // Try to release savepoint after rollback - should fail
        let result = savepoint.rollback_to().await;
        assert!(matches!(result, Err(DatabaseError::TransactionCommitted)));
    }
}
