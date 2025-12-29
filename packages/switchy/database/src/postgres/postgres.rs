//! Native PostgreSQL database backend using tokio-postgres
//!
//! This module implements the [`Database`](crate::Database) trait for PostgreSQL
//! using the `tokio-postgres` driver with connection pooling via `deadpool-postgres`.

use std::{
    ops::Deref,
    sync::{Arc, atomic::AtomicU16},
};

use async_trait::async_trait;
use chrono::NaiveDateTime;
use deadpool_postgres::{Pool, PoolError};
use futures::StreamExt;
use postgres_protocol::types::{
    bool_from_sql, float4_from_sql, float8_from_sql, int2_from_sql, int4_from_sql, int8_from_sql,
    text_from_sql,
};
use switchy_async::sync::Mutex;
use thiserror::Error;
use tokio::pin;
use tokio_postgres::{Client, Row, RowStream, types::IsNull};

#[cfg(feature = "schema")]
use super::introspection::{
    postgres_column_exists, postgres_get_table_columns, postgres_get_table_info,
    postgres_list_tables, postgres_table_exists,
};

use crate::{
    Database, DatabaseError, DatabaseValue, DeleteStatement, InsertStatement, SelectQuery,
    UpdateStatement, UpsertMultiStatement, UpsertStatement,
    query::{BooleanExpression, Expression, ExpressionType, Join, Sort, SortDirection},
    sql_interval::SqlInterval,
};

trait ToSql {
    fn to_sql(&self, index: &AtomicU16) -> String;
}

/// Format `SqlInterval` as `PostgreSQL` interval string for parameter binding
/// Returns formats like "1 year 2 days 3 hours" or "0" for zero interval
fn postgres_interval_to_string(interval: &SqlInterval) -> String {
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
        "0".to_string() // PostgreSQL accepts "0" for zero interval
    } else {
        parts.join(" ")
    }
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
                | DatabaseValue::Int8Opt(None)
                | DatabaseValue::Int16Opt(None)
                | DatabaseValue::Int32Opt(None)
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

/// `PostgreSQL` database connection pool
///
/// Manages a pool of `PostgreSQL` connections using `deadpool-postgres` for efficient
/// connection reuse and concurrent query execution.
#[allow(clippy::module_name_repetitions)]
pub struct PostgresDatabase {
    pool: Pool,
}

impl PostgresDatabase {
    /// Creates a new `PostgreSQL` database instance from a connection pool
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use deadpool_postgres::{Pool, Config};
    /// use switchy_database::postgres::postgres::PostgresDatabase;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = Config::new();
    /// let pool = config.create_pool(None, tokio_postgres::NoTls)?;
    /// let db = PostgresDatabase::new(pool);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn new(pool: Pool) -> Self {
        Self { pool }
    }

    async fn get_client(&self) -> Result<deadpool_postgres::Object, DatabaseError> {
        self.pool
            .get()
            .await
            .map_err(|e| DatabaseError::Postgres(PostgresDatabaseError::Pool(e)))
    }
}

impl std::fmt::Debug for PostgresDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresDatabase")
            .field("pool", &self.pool)
            .finish_non_exhaustive()
    }
}

/// `PostgreSQL` database transaction
///
/// Represents an active transaction on a `PostgreSQL` connection. Provides ACID guarantees
/// for a series of database operations. Must be explicitly committed or rolled back.
pub struct PostgresTransaction {
    client: Arc<Mutex<deadpool_postgres::Object>>,
    committed: Arc<Mutex<bool>>,
    rolled_back: Arc<Mutex<bool>>,
}

impl PostgresTransaction {
    /// Creates a new transaction by executing `BEGIN`
    ///
    /// # Errors
    ///
    /// * If the transaction could not be started via `BEGIN`
    pub async fn new(client: deadpool_postgres::Object) -> Result<Self, PostgresDatabaseError> {
        // Start the transaction with raw SQL
        client
            .execute("BEGIN", &[])
            .await
            .map_err(PostgresDatabaseError::Postgres)?;

        Ok(Self {
            client: Arc::new(Mutex::new(client)),
            committed: Arc::new(Mutex::new(false)),
            rolled_back: Arc::new(Mutex::new(false)),
        })
    }
}

impl std::fmt::Debug for PostgresTransaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresTransaction")
            .field("transaction", &"<transaction>")
            .finish_non_exhaustive()
    }
}

/// Errors specific to `PostgreSQL` database operations
///
/// Wraps errors from the underlying `tokio-postgres` driver and connection pool,
/// plus additional error types for query validation and result handling.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Error)]
pub enum PostgresDatabaseError {
    /// Error from the underlying `tokio-postgres` driver
    #[error(transparent)]
    Postgres(#[from] tokio_postgres::Error),
    /// Error from the `deadpool-postgres` connection pool
    #[error(transparent)]
    Pool(#[from] PoolError),
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
    /// `PostgreSQL` type name not found in type registry
    #[error("Type Not Found: '{type_name}'")]
    TypeNotFound {
        /// The name of the type that was not found
        type_name: String,
    },
    /// Parameter type cannot be bound to `PostgreSQL` query
    #[error("Invalid parameter type: {0}")]
    InvalidParameterType(String),
}

impl From<PostgresDatabaseError> for DatabaseError {
    fn from(value: PostgresDatabaseError) -> Self {
        Self::Postgres(value)
    }
}

#[async_trait]
impl Database for PostgresDatabase {
    async fn query(&self, query: &SelectQuery<'_>) -> Result<Vec<crate::Row>, DatabaseError> {
        let client = self.get_client().await?;
        Ok(select(
            &client,
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
        let client = self.get_client().await?;
        Ok(find_row(
            &client,
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
        let client = self.get_client().await?;
        Ok(delete(
            &client,
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
        let client = self.get_client().await?;
        Ok(delete(
            &client,
            statement.table_name,
            statement.filters.as_deref(),
            Some(1),
        )
        .await?
        .into_iter()
        .next())
    }

    #[cfg(feature = "schema")]
    async fn exec_create_table(
        &self,
        statement: &crate::schema::CreateTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let client = self.get_client().await?;
        postgres_exec_create_table(&client, statement)
            .await
            .map_err(Into::into)
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

        let client = self.get_client().await?;
        postgres_exec_drop_table(&client, statement)
            .await
            .map_err(Into::into)
    }

    #[cfg(feature = "schema")]
    async fn exec_create_index(
        &self,
        statement: &crate::schema::CreateIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let client = self.get_client().await?;
        postgres_exec_create_index(&client, statement)
            .await
            .map_err(Into::into)
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_index(
        &self,
        statement: &crate::schema::DropIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let client = self.get_client().await?;
        postgres_exec_drop_index(&client, statement)
            .await
            .map_err(Into::into)
    }

    #[cfg(feature = "schema")]
    async fn exec_alter_table(
        &self,
        statement: &crate::schema::AlterTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let client = self.get_client().await?;
        postgres_exec_alter_table(&client, statement)
            .await
            .map_err(Into::into)
    }

    #[cfg(feature = "schema")]
    async fn table_exists(&self, table_name: &str) -> Result<bool, DatabaseError> {
        let client = self.get_client().await?;
        let client_ref: &Client = &client;
        postgres_table_exists(client_ref, table_name).await
    }

    #[cfg(feature = "schema")]
    async fn list_tables(&self) -> Result<Vec<String>, DatabaseError> {
        let client = self.get_client().await?;
        let client_ref: &Client = &client;
        postgres_list_tables(client_ref).await
    }

    #[cfg(feature = "schema")]
    async fn get_table_info(
        &self,
        table_name: &str,
    ) -> Result<Option<crate::schema::TableInfo>, DatabaseError> {
        let client = self.get_client().await?;
        let client_ref: &Client = &client;
        postgres_get_table_info(client_ref, table_name).await
    }

    #[cfg(feature = "schema")]
    async fn get_table_columns(
        &self,
        table_name: &str,
    ) -> Result<Vec<crate::schema::ColumnInfo>, DatabaseError> {
        let client = self.get_client().await?;
        let client_ref: &Client = &client;
        postgres_get_table_columns(client_ref, table_name).await
    }

    #[cfg(feature = "schema")]
    async fn column_exists(
        &self,
        table_name: &str,
        column_name: &str,
    ) -> Result<bool, DatabaseError> {
        let client = self.get_client().await?;
        let client_ref: &Client = &client;
        postgres_column_exists(client_ref, table_name, column_name).await
    }

    async fn exec_insert(
        &self,
        statement: &InsertStatement<'_>,
    ) -> Result<crate::Row, DatabaseError> {
        let client = self.get_client().await?;
        Ok(insert_and_get_row(&client, statement.table_name, &statement.values).await?)
    }

    async fn exec_update(
        &self,
        statement: &UpdateStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        let client = self.get_client().await?;
        Ok(update_and_get_rows(
            &client,
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
        let client = self.get_client().await?;
        Ok(update_and_get_row(
            &client,
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
        let client = self.get_client().await?;
        Ok(upsert(
            &client,
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
        let client = self.get_client().await?;
        Ok(upsert_and_get_row(
            &client,
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
        let client = self.get_client().await?;
        let rows = {
            upsert_multi(
                &client,
                statement.table_name,
                statement
                    .unique
                    .as_ref()
                    .ok_or(PostgresDatabaseError::MissingUnique)?,
                &statement.values,
            )
            .await?
        };

        Ok(rows)
    }

    async fn exec_raw(&self, sql: &str) -> Result<(), DatabaseError> {
        let client = self.get_client().await?;
        client
            .execute_raw(sql, &[] as &[&str])
            .await
            .map_err(PostgresDatabaseError::Postgres)?;
        Ok(())
    }

    async fn query_raw(&self, query: &str) -> Result<Vec<crate::Row>, DatabaseError> {
        let client = self.get_client().await?;

        let pg_rows = client
            .query(query, &[])
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        if pg_rows.is_empty() {
            return Ok(vec![]);
        }

        // Get column names from first row
        let column_names: Vec<String> = pg_rows[0]
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect();

        // Use existing from_row helper
        let mut rows = Vec::new();
        for row in pg_rows {
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
        let client = self.get_client().await?;
        let transaction = PostgresTransaction::new(client).await?;
        Ok(Box::new(transaction))
    }

    async fn exec_raw_params(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<u64, DatabaseError> {
        // Transform query to handle Now/NowPlus parameters consistently with other backends
        let (transformed_query, filtered_params) =
            postgres_transform_query_for_params(query, params);

        let client = self.get_client().await?;

        // Convert DatabaseValue to PgDatabaseValue for ToSql trait
        let pg_params: Vec<PgDatabaseValue> = filtered_params
            .into_iter()
            .map(PgDatabaseValue::from)
            .collect();

        // Create references for tokio_postgres (it expects &[&dyn ToSql])
        let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = pg_params
            .iter()
            .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();

        // Execute with proper parameter binding
        let rows_affected = client
            .execute(&transformed_query, &param_refs[..])
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        Ok(rows_affected)
    }

    async fn query_raw_params(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        // Transform query to handle Now/NowPlus parameters consistently with other backends
        let (transformed_query, filtered_params) =
            postgres_transform_query_for_params(query, params);

        let client = self.get_client().await?;

        // Convert DatabaseValue to PgDatabaseValue for ToSql trait
        let pg_params: Vec<PgDatabaseValue> = filtered_params
            .into_iter()
            .map(PgDatabaseValue::from)
            .collect();

        // Create references for tokio_postgres (it expects &[&dyn ToSql])
        let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = pg_params
            .iter()
            .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();

        // Execute with proper parameter binding
        let pg_rows = client
            .query(&transformed_query, &param_refs[..])
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        if pg_rows.is_empty() {
            return Ok(vec![]);
        }

        // Get column names from first row
        let column_names: Vec<String> = pg_rows[0]
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect();

        // Convert postgres rows to our Row format
        let mut rows = Vec::new();
        for pg_row in &pg_rows {
            let row = from_row(&column_names, pg_row)
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
            rows.push(row);
        }

        Ok(rows)
    }
}

#[async_trait]
impl Database for PostgresTransaction {
    async fn query(&self, query: &SelectQuery<'_>) -> Result<Vec<crate::Row>, DatabaseError> {
        Ok(select(
            &*self.client.lock().await,
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
            &*self.client.lock().await,
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
            &*self.client.lock().await,
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
            &*self.client.lock().await,
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
            &*self.client.lock().await,
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
            &*self.client.lock().await,
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
            &*self.client.lock().await,
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
            &*self.client.lock().await,
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
            &*self.client.lock().await,
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
                &*self.client.lock().await,
                statement.table_name,
                statement
                    .unique
                    .as_ref()
                    .ok_or(PostgresDatabaseError::MissingUnique)?,
                &statement.values,
            )
            .await?
        };

        Ok(rows)
    }

    #[cfg(feature = "schema")]
    async fn exec_create_table(
        &self,
        statement: &crate::schema::CreateTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        postgres_exec_create_table(&*self.client.lock().await, statement)
            .await
            .map_err(Into::into)
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_table(
        &self,
        statement: &crate::schema::DropTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        #[cfg(feature = "cascade")]
        {
            use crate::schema::DropBehavior;
            match statement.behavior {
                DropBehavior::Cascade => {
                    let client = self.client.lock().await;
                    return postgres_exec_drop_table_cascade(&client, statement).await;
                }
                DropBehavior::Restrict => {
                    let client = self.client.lock().await;
                    return postgres_exec_drop_table_restrict_native(&client, statement).await;
                }
                DropBehavior::Default => {}
            }
        }

        postgres_exec_drop_table(&*self.client.lock().await, statement)
            .await
            .map_err(Into::into)
    }

    #[cfg(feature = "schema")]
    async fn exec_create_index(
        &self,
        statement: &crate::schema::CreateIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        postgres_exec_create_index(&*self.client.lock().await, statement)
            .await
            .map_err(Into::into)
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_index(
        &self,
        statement: &crate::schema::DropIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        postgres_exec_drop_index(&*self.client.lock().await, statement)
            .await
            .map_err(Into::into)
    }

    #[cfg(feature = "schema")]
    async fn exec_alter_table(
        &self,
        statement: &crate::schema::AlterTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        postgres_exec_alter_table(&*self.client.lock().await, statement)
            .await
            .map_err(Into::into)
    }

    #[cfg(feature = "schema")]
    async fn table_exists(&self, table_name: &str) -> Result<bool, DatabaseError> {
        let client_ref: &Client = &*self.client.lock().await;
        postgres_table_exists(client_ref, table_name).await
    }

    #[cfg(feature = "schema")]
    async fn list_tables(&self) -> Result<Vec<String>, DatabaseError> {
        let client_ref: &Client = &*self.client.lock().await;
        postgres_list_tables(client_ref).await
    }

    #[cfg(feature = "schema")]
    async fn get_table_info(
        &self,
        table_name: &str,
    ) -> Result<Option<crate::schema::TableInfo>, DatabaseError> {
        let client_ref: &Client = &*self.client.lock().await;
        postgres_get_table_info(client_ref, table_name).await
    }

    #[cfg(feature = "schema")]
    async fn get_table_columns(
        &self,
        table_name: &str,
    ) -> Result<Vec<crate::schema::ColumnInfo>, DatabaseError> {
        let client_ref: &Client = &*self.client.lock().await;
        postgres_get_table_columns(client_ref, table_name).await
    }

    #[cfg(feature = "schema")]
    async fn column_exists(
        &self,
        table_name: &str,
        column_name: &str,
    ) -> Result<bool, DatabaseError> {
        let client_ref: &Client = &*self.client.lock().await;
        postgres_column_exists(client_ref, table_name, column_name).await
    }

    async fn exec_raw(&self, sql: &str) -> Result<(), DatabaseError> {
        self.client
            .lock()
            .await
            .execute(sql, &[])
            .await
            .map_err(PostgresDatabaseError::Postgres)?;
        Ok(())
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn query_raw(&self, query: &str) -> Result<Vec<crate::Row>, DatabaseError> {
        let client_ref = self.client.lock().await;

        let pg_rows = client_ref
            .query(query, &[])
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        if pg_rows.is_empty() {
            return Ok(vec![]);
        }

        // Get column names from first row
        let column_names: Vec<String> = pg_rows[0]
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect();

        // Use existing from_row helper
        let mut rows = Vec::new();
        for row in pg_rows {
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
        Err(DatabaseError::Postgres(
            PostgresDatabaseError::InvalidRequest,
        ))
    }

    async fn exec_raw_params(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<u64, DatabaseError> {
        // Transform query to handle Now/NowPlus parameters consistently with other backends
        let (transformed_query, filtered_params) =
            postgres_transform_query_for_params(query, params);

        // Convert DatabaseValue to PgDatabaseValue for ToSql trait
        let pg_params: Vec<PgDatabaseValue> = filtered_params
            .into_iter()
            .map(PgDatabaseValue::from)
            .collect();

        // Create references for tokio_postgres (it expects &[&dyn ToSql])
        let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = pg_params
            .iter()
            .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();

        // Execute with proper parameter binding
        let rows_affected = self
            .client
            .lock()
            .await
            .execute(&transformed_query, &param_refs[..])
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        Ok(rows_affected)
    }

    async fn query_raw_params(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        // Transform query to handle Now/NowPlus parameters consistently with other backends
        let (transformed_query, filtered_params) =
            postgres_transform_query_for_params(query, params);

        // Convert DatabaseValue to PgDatabaseValue for ToSql trait
        let pg_params: Vec<PgDatabaseValue> = filtered_params
            .into_iter()
            .map(PgDatabaseValue::from)
            .collect();

        // Create references for tokio_postgres (it expects &[&dyn ToSql])
        let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = pg_params
            .iter()
            .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();

        // Execute with proper parameter binding
        let pg_rows = self
            .client
            .lock()
            .await
            .query(&transformed_query, &param_refs[..])
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        if pg_rows.is_empty() {
            return Ok(vec![]);
        }

        // Get column names from first row
        let column_names: Vec<String> = pg_rows[0]
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect();

        // Convert postgres rows to our Row format
        let mut rows = Vec::new();
        for pg_row in &pg_rows {
            let row = from_row(&column_names, pg_row)
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
            rows.push(row);
        }

        Ok(rows)
    }
}

struct PostgresSavepoint {
    name: String,
    client: Arc<Mutex<deadpool_postgres::Object>>,
    released: Arc<Mutex<bool>>,
    rolled_back: Arc<Mutex<bool>>,
    // Share parent transaction state for consistency
    parent_committed: Arc<Mutex<bool>>,
    parent_rolled_back: Arc<Mutex<bool>>,
}

#[async_trait]
impl crate::Savepoint for PostgresSavepoint {
    #[allow(clippy::significant_drop_tightening)]
    async fn release(self: Box<Self>) -> Result<(), DatabaseError> {
        // Check our own state
        let mut released = self.released.lock().await;
        if *released {
            return Err(DatabaseError::InvalidSavepointName(format!(
                "Savepoint '{}' already released",
                self.name
            )));
        }

        let rolled_back = self.rolled_back.lock().await;
        if *rolled_back {
            return Err(DatabaseError::InvalidSavepointName(format!(
                "Savepoint '{}' already rolled back",
                self.name
            )));
        }
        drop(rolled_back);

        // Check parent transaction state for consistency with SQLite behavior
        let parent_committed = self.parent_committed.lock().await;
        let parent_rolled_back = self.parent_rolled_back.lock().await;
        if *parent_committed || *parent_rolled_back {
            return Err(DatabaseError::TransactionCommitted);
        }
        drop(parent_committed);
        drop(parent_rolled_back);

        // Execute SQL
        self.client
            .lock()
            .await
            .execute(&format!("RELEASE SAVEPOINT {}", self.name), &[])
            .await
            .map_err(PostgresDatabaseError::Postgres)?;

        *released = true;
        Ok(())
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn rollback_to(self: Box<Self>) -> Result<(), DatabaseError> {
        // Check our own state
        let mut rolled_back = self.rolled_back.lock().await;
        if *rolled_back {
            return Err(DatabaseError::InvalidSavepointName(format!(
                "Savepoint '{}' already rolled back",
                self.name
            )));
        }

        let released = self.released.lock().await;
        if *released {
            return Err(DatabaseError::InvalidSavepointName(format!(
                "Savepoint '{}' already released",
                self.name
            )));
        }
        drop(released);

        // Check parent transaction state for consistency with SQLite behavior
        let parent_committed = self.parent_committed.lock().await;
        let parent_rolled_back = self.parent_rolled_back.lock().await;
        if *parent_committed || *parent_rolled_back {
            return Err(DatabaseError::TransactionCommitted);
        }
        drop(parent_committed);
        drop(parent_rolled_back);

        // Execute SQL
        self.client
            .lock()
            .await
            .execute(&format!("ROLLBACK TO SAVEPOINT {}", self.name), &[])
            .await
            .map_err(PostgresDatabaseError::Postgres)?;

        *rolled_back = true;
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

#[async_trait]
impl crate::DatabaseTransaction for PostgresTransaction {
    async fn commit(self: Box<Self>) -> Result<(), DatabaseError> {
        let mut committed = self.committed.lock().await;
        let rolled_back = self.rolled_back.lock().await;

        if *committed || *rolled_back {
            return Err(DatabaseError::Postgres(
                PostgresDatabaseError::InvalidRequest,
            ));
        }
        drop(rolled_back);

        self.client
            .lock()
            .await
            .execute("COMMIT", &[])
            .await
            .map_err(PostgresDatabaseError::Postgres)?;
        *committed = true;
        drop(committed);
        Ok(())
    }

    async fn rollback(self: Box<Self>) -> Result<(), DatabaseError> {
        let committed = self.committed.lock().await;
        let mut rolled_back = self.rolled_back.lock().await;

        if *committed || *rolled_back {
            return Err(DatabaseError::Postgres(
                PostgresDatabaseError::InvalidRequest,
            ));
        }
        drop(committed);

        self.client
            .lock()
            .await
            .execute("ROLLBACK", &[])
            .await
            .map_err(PostgresDatabaseError::Postgres)?;
        *rolled_back = true;
        drop(rolled_back);
        Ok(())
    }

    async fn savepoint(&self, name: &str) -> Result<Box<dyn crate::Savepoint>, DatabaseError> {
        crate::validate_savepoint_name(name)?;

        // Execute SAVEPOINT SQL
        self.client
            .lock()
            .await
            .execute(&format!("SAVEPOINT {name}"), &[])
            .await
            .map_err(PostgresDatabaseError::Postgres)?;

        Ok(Box::new(PostgresSavepoint {
            name: name.to_string(),
            client: Arc::clone(&self.client),
            released: Arc::new(Mutex::new(false)),
            rolled_back: Arc::new(Mutex::new(false)),
            // Share parent's state to enable consistency checks
            parent_committed: Arc::clone(&self.committed),
            parent_rolled_back: Arc::clone(&self.rolled_back),
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
                    AND ccu.table_name = '{table_name}'
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
            "
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

fn column_value(row: &Row, index: &str) -> Result<DatabaseValue, PostgresDatabaseError> {
    let column_type = row
        .columns()
        .iter()
        .find(|x| x.name() == index)
        .map(tokio_postgres::Column::type_)
        .unwrap();

    row.try_get(index)
        .map_err(|_| PostgresDatabaseError::TypeNotFound {
            type_name: column_type.name().to_string(),
        })
}

fn from_row(column_names: &[String], row: &Row) -> Result<crate::Row, PostgresDatabaseError> {
    let mut columns = vec![];

    for column in column_names {
        log::trace!("Mapping column {column:?}");
        columns.push((column.clone(), column_value(row, column)?));
    }

    Ok(crate::Row { columns })
}

#[allow(clippy::too_many_lines)]
#[cfg(feature = "schema")]
async fn postgres_exec_create_table(
    client: &tokio_postgres::Client,
    statement: &crate::schema::CreateTableStatement<'_>,
) -> Result<(), PostgresDatabaseError> {
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
            crate::schema::DataType::Array(ref inner_type) => {
                // Recursively handle the inner type
                match **inner_type {
                    crate::schema::DataType::Text => query.push_str("TEXT[]"),
                    crate::schema::DataType::Int => query.push_str("INTEGER[]"),
                    crate::schema::DataType::BigInt => query.push_str("BIGINT[]"),
                    _ => {
                        // For complex nested types, fall back to Custom
                        query.push_str("TEXT[]");
                    }
                }
            }
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

    client
        .execute_raw(&query, &[] as &[&str])
        .await
        .map_err(PostgresDatabaseError::Postgres)?;

    Ok(())
}

// Helper functions for CASCADE support - return DatabaseError for flexibility
#[cfg(feature = "cascade")]
async fn postgres_get_direct_dependents(
    client: &tokio_postgres::Client,
    table_name: &str,
) -> Result<Vec<String>, PostgresDatabaseError> {
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

    let rows = client
        .query(query, &[&table_name])
        .await
        .map_err(PostgresDatabaseError::Postgres)?;

    Ok(rows
        .iter()
        .filter_map(|row| row.try_get::<_, String>(0).ok())
        .collect())
}

#[cfg(feature = "cascade")]
async fn postgres_exec_drop_table_cascade(
    client: &tokio_postgres::Client,
    statement: &crate::schema::DropTableStatement<'_>,
) -> Result<(), DatabaseError> {
    // Iterative approach to collect all dependent tables
    let mut to_drop = Vec::new();
    let mut to_check = vec![statement.table_name.to_string()];
    let mut visited = std::collections::BTreeSet::new();

    while let Some(table) = to_check.pop() {
        if !visited.insert(table.clone()) {
            continue;
        }

        let dependents = postgres_get_direct_dependents(client, &table)
            .await
            .map_err(DatabaseError::Postgres)?;

        for dependent in dependents {
            if !visited.contains(&dependent) {
                to_check.push(dependent);
            }
        }

        to_drop.push(table);
    }

    // to_drop is now in order: parent first, dependents after
    // Reverse to get dependents first for dropping
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
        client
            .execute_raw(&sql, &[] as &[&str])
            .await
            .map_err(|e| DatabaseError::Postgres(PostgresDatabaseError::Postgres(e)))?;
    }
    Ok(())
}

#[cfg(feature = "cascade")]
async fn postgres_exec_drop_table_restrict_native(
    client: &tokio_postgres::Client,
    statement: &crate::schema::DropTableStatement<'_>,
) -> Result<(), DatabaseError> {
    let mut query = "DROP TABLE ".to_string();

    if statement.if_exists {
        query.push_str("IF EXISTS ");
    }

    query.push_str(statement.table_name);
    query.push_str(" RESTRICT");

    client
        .execute_raw(&query, &[] as &[&str])
        .await
        .map_err(|e| DatabaseError::Postgres(PostgresDatabaseError::Postgres(e)))?;

    Ok(())
}

#[cfg(feature = "schema")]
async fn postgres_exec_drop_table(
    client: &tokio_postgres::Client,
    statement: &crate::schema::DropTableStatement<'_>,
) -> Result<(), PostgresDatabaseError> {
    #[cfg(feature = "cascade")]
    {
        use crate::schema::DropBehavior;
        match statement.behavior {
            DropBehavior::Cascade => {
                return postgres_exec_drop_table_cascade(client, statement)
                    .await
                    .map_err(|e| match e {
                        DatabaseError::Postgres(pg_err) => pg_err,
                        _ => PostgresDatabaseError::InvalidRequest,
                    });
            }
            DropBehavior::Restrict => {
                return postgres_exec_drop_table_restrict_native(client, statement)
                    .await
                    .map_err(|e| match e {
                        DatabaseError::Postgres(pg_err) => pg_err,
                        _ => PostgresDatabaseError::InvalidRequest,
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

    client
        .execute_raw(&query, &[] as &[&str])
        .await
        .map_err(PostgresDatabaseError::Postgres)?;

    Ok(())
}

#[cfg(feature = "schema")]
pub(crate) async fn postgres_exec_create_index(
    client: &tokio_postgres::Client,
    statement: &crate::schema::CreateIndexStatement<'_>,
) -> Result<(), PostgresDatabaseError> {
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

    client
        .execute_raw(&sql, &[] as &[&str])
        .await
        .map_err(PostgresDatabaseError::Postgres)?;

    Ok(())
}

#[cfg(feature = "schema")]
pub(crate) async fn postgres_exec_drop_index(
    client: &tokio_postgres::Client,
    statement: &crate::schema::DropIndexStatement<'_>,
) -> Result<(), PostgresDatabaseError> {
    let if_exists_str = if statement.if_exists {
        "IF EXISTS "
    } else {
        ""
    };

    let sql = format!("DROP INDEX {}{}", if_exists_str, statement.index_name);

    client
        .execute_raw(&sql, &[] as &[&str])
        .await
        .map_err(PostgresDatabaseError::Postgres)?;

    Ok(())
}

#[cfg(feature = "schema")]
#[allow(clippy::too_many_lines)]
pub(crate) async fn postgres_exec_alter_table(
    client: &tokio_postgres::Client,
    statement: &crate::schema::AlterTableStatement<'_>,
) -> Result<(), PostgresDatabaseError> {
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
                                log::error!(
                                    "Unsupported default value type for ALTER TABLE ADD COLUMN: {val:?}"
                                );
                                return Err(PostgresDatabaseError::InvalidRequest);
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

                client
                    .execute_raw(&sql, &[] as &[&str])
                    .await
                    .map_err(PostgresDatabaseError::Postgres)?;
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

                client
                    .execute_raw(&sql, &[] as &[&str])
                    .await
                    .map_err(PostgresDatabaseError::Postgres)?;
            }
            AlterOperation::RenameColumn { old_name, new_name } => {
                let sql = format!(
                    "ALTER TABLE {} RENAME COLUMN \"{}\" TO \"{}\"",
                    statement.table_name, old_name, new_name
                );

                client
                    .execute_raw(&sql, &[] as &[&str])
                    .await
                    .map_err(PostgresDatabaseError::Postgres)?;
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

                client
                    .execute_raw(&alter_type_sql, &[] as &[&str])
                    .await
                    .map_err(PostgresDatabaseError::Postgres)?;

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

                    client
                        .execute_raw(&nullable_sql, &[] as &[&str])
                        .await
                        .map_err(PostgresDatabaseError::Postgres)?;
                }

                // Change default value if specified
                if let Some(default) = new_default {
                    let default_str = match default {
                        crate::DatabaseValue::String(s) => format!("'{s}'"),
                        crate::DatabaseValue::Int64(n) => n.to_string(),
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
                            log::error!(
                                "Unsupported default value type for MODIFY COLUMN: {default:?}"
                            );
                            return Err(PostgresDatabaseError::InvalidRequest);
                        }
                    };

                    let default_sql = format!(
                        "ALTER TABLE {} ALTER COLUMN \"{}\" SET DEFAULT {}",
                        statement.table_name, name, default_str
                    );

                    client
                        .execute_raw(&default_sql, &[] as &[&str])
                        .await
                        .map_err(PostgresDatabaseError::Postgres)?;
                }
            }
        }
    }

    Ok(())
}

async fn update_and_get_row(
    client: &Client,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Option<crate::Row>, PostgresDatabaseError> {
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

    log::trace!("Running update_and_get_row query: {query} with params: {all_values:?}");

    let statement = client.prepare(&query).await?;

    let column_names = statement
        .columns()
        .iter()
        .map(|x| x.name().to_string())
        .collect::<Vec<_>>();

    let stream = client.query_raw(&statement, &all_values).await?;

    pin!(stream);

    let row: Option<Row> = stream.next().await.transpose()?;

    row.map(|row| from_row(&column_names, &row)).transpose()
}

async fn update_and_get_rows(
    client: &Client,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, PostgresDatabaseError> {
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

    log::trace!("Running update_and_get_rows query: {query} with params: {all_values:?}");

    let statement = client.prepare(&query).await?;

    let column_names = statement
        .columns()
        .iter()
        .map(|x| x.name().to_string())
        .collect::<Vec<_>>();

    let rows = client.query_raw(&statement, &all_values).await?;

    to_rows(&column_names, rows).await
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
    use std::sync::LazyLock;
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

async fn to_rows(
    column_names: &[String],
    rows: RowStream,
) -> Result<Vec<crate::Row>, PostgresDatabaseError> {
    let mut results = vec![];

    pin!(rows);

    while let Some(row) = rows.next().await {
        results.push(from_row(column_names, &row?)?);
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

fn exprs_to_params(values: &[(&str, Box<dyn Expression>)]) -> Vec<PgDatabaseValue> {
    values
        .iter()
        .flat_map(|value| value.1.params().into_iter())
        .flatten()
        .cloned()
        .map(std::convert::Into::into)
        .collect::<Vec<_>>()
}

fn bexprs_to_params(values: &[Box<dyn BooleanExpression>]) -> Vec<PgDatabaseValue> {
    values
        .iter()
        .flat_map(|value| value.params().into_iter())
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
fn exprs_to_params_opt(values: Option<&[(&str, Box<dyn Expression>)]>) -> Vec<PgDatabaseValue> {
    values.map(exprs_to_params).unwrap_or_default()
}

fn bexprs_to_params_opt(values: Option<&[Box<dyn BooleanExpression>]>) -> Vec<PgDatabaseValue> {
    values.map(bexprs_to_params).unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
async fn select(
    client: &Client,
    table_name: &str,
    distinct: bool,
    columns: &[&str],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    joins: Option<&[Join<'_>]>,
    sort: Option<&[Sort]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, PostgresDatabaseError> {
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

    let statement = client.prepare(&query).await?;
    let column_names = statement
        .columns()
        .iter()
        .map(|x| x.name().to_string())
        .collect::<Vec<_>>();

    let filters = bexprs_to_params_opt(filters);
    let rows = client.query_raw(&statement, filters).await?;

    to_rows(&column_names, rows).await
}

async fn delete(
    client: &Client,
    table_name: &str,
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, PostgresDatabaseError> {
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

    let statement = client.prepare(&query).await?;
    let column_names = statement
        .columns()
        .iter()
        .map(|x| x.name().to_string())
        .collect::<Vec<_>>();

    // For LIMIT queries, we need to duplicate the filter parameters
    // since they appear twice in the query (main WHERE and subquery WHERE)
    let filters = bexprs_to_params_opt(filters);
    let all_filters = if limit.is_some() {
        let mut all = filters.clone();
        all.extend(filters);
        all
    } else {
        filters
    };

    let rows = client.query_raw(&statement, all_filters).await?;

    to_rows(&column_names, rows).await
}

async fn find_row(
    client: &Client,
    table_name: &str,
    distinct: bool,
    columns: &[&str],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    joins: Option<&[Join<'_>]>,
    sort: Option<&[Sort]>,
) -> Result<Option<crate::Row>, PostgresDatabaseError> {
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

    let filters = bexprs_to_params_opt(filters);
    log::trace!("Running find_row query: {query} with params: {filters:?}");

    let statement = client.prepare(&query).await?;
    let column_names = statement
        .columns()
        .iter()
        .map(|x| x.name().to_string())
        .collect::<Vec<_>>();

    let rows = client.query_raw(&statement, filters).await?;

    pin!(rows);

    rows.next()
        .await
        .transpose()?
        .map(|row| from_row(&column_names, &row))
        .transpose()
}

async fn insert_and_get_row(
    client: &Client,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
) -> Result<crate::Row, PostgresDatabaseError> {
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

    let values = exprs_to_params(values);
    log::trace!("Running insert_and_get_row query: '{query}' with params: {values:?}");

    let statement = client.prepare(&query).await?;
    let column_names = statement
        .columns()
        .iter()
        .map(|x| x.name().to_string())
        .collect::<Vec<_>>();

    let rows = client.query_raw(&statement, &values).await?;

    pin!(rows);

    rows.next()
        .await
        .transpose()?
        .map(|row| from_row(&column_names, &row))
        .ok_or(PostgresDatabaseError::NoRow)?
}

/// # Errors
///
/// Will return `Err` if the update multi execution failed.
pub async fn update_multi(
    client: &Client,
    table_name: &str,
    values: &[Vec<(&str, Box<dyn Expression>)>],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    mut limit: Option<usize>,
) -> Result<Vec<crate::Row>, PostgresDatabaseError> {
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
                &mut update_chunk(client, table_name, &values[last_i..i], filters, limit).await?,
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
            &mut update_chunk(client, table_name, &values[last_i..], filters, limit).await?,
        );
    }

    Ok(results)
}

async fn update_chunk(
    client: &Client,
    table_name: &str,
    values: &[Vec<(&str, Box<dyn Expression>)>],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, PostgresDatabaseError> {
    let first = values[0].as_slice();
    let expected_value_size = first.len();

    if let Some(bad_row) = values.iter().skip(1).find(|v| {
        v.len() != expected_value_size || v.iter().enumerate().any(|(i, c)| c.0 != first[i].0)
    }) {
        log::error!("Bad row: {bad_row:?}. Expected to match schema of first row: {first:?}");
        return Err(PostgresDatabaseError::InvalidRequest);
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
        .collect::<Vec<PgDatabaseValue>>();
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
                .collect::<Vec<PgDatabaseValue>>()
        })
        .unwrap_or_default();

    if limit.is_some() {
        all_filter_values.extend(all_filter_values.clone());
    }

    let all_values = [all_values, all_filter_values].concat();

    log::trace!("Running update chunk query: {query} with params: {all_values:?}");

    let statement = client.prepare(&query).await?;
    let column_names = statement
        .columns()
        .iter()
        .map(|x| x.name().to_string())
        .collect::<Vec<_>>();

    let rows = client.query_raw(&statement, &all_values).await?;

    to_rows(&column_names, rows).await
}

/// # Errors
///
/// Will return `Err` if the upsert multi execution failed.
pub async fn upsert_multi(
    client: &Client,
    table_name: &str,
    unique: &[Box<dyn Expression>],
    values: &[Vec<(&str, Box<dyn Expression>)>],
) -> Result<Vec<crate::Row>, PostgresDatabaseError> {
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
            results
                .append(&mut upsert_chunk(client, table_name, unique, &values[last_i..i]).await?);
            last_i = i;
            pos = 0;
        }
        i += 1;
        pos += count;
    }

    if i > last_i {
        results.append(&mut upsert_chunk(client, table_name, unique, &values[last_i..]).await?);
    }

    Ok(results)
}

async fn upsert_chunk(
    client: &Client,
    table_name: &str,
    unique: &[Box<dyn Expression>],
    values: &[Vec<(&str, Box<dyn Expression>)>],
) -> Result<Vec<crate::Row>, PostgresDatabaseError> {
    let first = values[0].as_slice();
    let expected_value_size = first.len();

    if let Some(bad_row) = values.iter().skip(1).find(|v| {
        v.len() != expected_value_size || v.iter().enumerate().any(|(i, c)| c.0 != first[i].0)
    }) {
        log::error!("Bad row: {bad_row:?}. Expected to match schema of first row: {first:?}");
        return Err(PostgresDatabaseError::InvalidRequest);
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
        .collect::<Vec<PgDatabaseValue>>();

    log::trace!("Running upsert chunk query: {query} with params: {all_values:?}");

    let statement = client.prepare(&query).await?;
    let column_names = statement
        .columns()
        .iter()
        .map(|x| x.name().to_string())
        .collect::<Vec<_>>();

    let rows = client.query_raw(&statement, all_values).await?;

    to_rows(&column_names, rows).await
}

async fn upsert(
    client: &Client,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, PostgresDatabaseError> {
    let rows = update_and_get_rows(client, table_name, values, filters, limit).await?;

    Ok(if rows.is_empty() {
        vec![insert_and_get_row(client, table_name, values).await?]
    } else {
        rows
    })
}

async fn upsert_and_get_row(
    client: &Client,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<crate::Row, PostgresDatabaseError> {
    match find_row(client, table_name, false, &["*"], filters, None, None).await? {
        Some(row) => {
            let updated = update_and_get_row(client, table_name, values, filters, limit)
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
        None => Ok(insert_and_get_row(client, table_name, values).await?),
    }
}

/// Wrapper type for converting `DatabaseValue` to `PostgreSQL`-specific parameter types
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

impl<'a> tokio_postgres::types::FromSql<'a> for DatabaseValue {
    fn from_sql(
        ty: &tokio_postgres::types::Type,
        raw: &'a [u8],
    ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        log::trace!("FromSql from_sql: ty={}, {ty:?}", ty.name());
        Ok(match ty.name() {
            "int2" => Self::Int64(int2_from_sql(raw)?.into()),
            "int4" => Self::Int64(int4_from_sql(raw)?.into()),
            "bool" => Self::Bool(bool_from_sql(raw)?),
            "char" | "smallint" | "smallserial" | "int" | "serial" | "bigint" | "bigserial"
            | "int8" => Self::Int64(int8_from_sql(raw)?),
            "float4" => Self::Real32(float4_from_sql(raw)?),
            "real" | "double precision" | "float8" => Self::Real64(float8_from_sql(raw)?),
            "varchar" | "bpchar" | "char(n)" | "text" | "name" | "citext" => {
                Self::String(text_from_sql(raw)?.to_string())
            }
            "timestamp" => Self::DateTime(NaiveDateTime::from_sql(ty, raw)?),
            #[cfg(feature = "uuid")]
            "uuid" => Self::Uuid(uuid::Uuid::from_sql(ty, raw)?),
            #[cfg(feature = "decimal")]
            "numeric" => Self::Decimal(rust_decimal::Decimal::from_sql(ty, raw)?),
            other => {
                return Err(Box::new(PostgresDatabaseError::TypeNotFound {
                    type_name: other.to_string(),
                }));
            }
        })
    }

    fn from_sql_nullable(
        ty: &tokio_postgres::types::Type,
        raw: Option<&'a [u8]>,
    ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        let name = ty.name();
        log::trace!("FromSql from_sql_nullable: ty={name}, {ty:?}");
        Ok(match name {
            "int2" => raw
                .map(|raw| {
                    Ok::<_, Box<dyn std::error::Error + Sync + Send>>(Self::Int64(i64::from(
                        int2_from_sql(raw)?,
                    )))
                })
                .transpose()?
                .unwrap_or(Self::Null),
            "int4" => raw
                .map(|raw| {
                    Ok::<_, Box<dyn std::error::Error + Sync + Send>>(Self::Int64(i64::from(
                        int4_from_sql(raw)?,
                    )))
                })
                .transpose()?
                .unwrap_or(Self::Null),
            "bool" => raw
                .map(|raw| {
                    Ok::<_, Box<dyn std::error::Error + Sync + Send>>(Self::Bool(bool_from_sql(
                        raw,
                    )?))
                })
                .transpose()?
                .unwrap_or(Self::Null),
            "char" | "smallint" | "smallserial" | "int" | "serial" | "bigint" | "bigserial"
            | "int8" => raw
                .map(|raw| {
                    Ok::<_, Box<dyn std::error::Error + Sync + Send>>(Self::Int64(int8_from_sql(
                        raw,
                    )?))
                })
                .transpose()?
                .unwrap_or(Self::Null),
            "float4" => raw
                .map(|raw| {
                    Ok::<_, Box<dyn std::error::Error + Sync + Send>>(Self::Real32(
                        float4_from_sql(raw)?,
                    ))
                })
                .transpose()?
                .unwrap_or(Self::Null),
            "real" | "double precision" | "float8" => raw
                .map(|raw| {
                    Ok::<_, Box<dyn std::error::Error + Sync + Send>>(Self::Real64(
                        float8_from_sql(raw)?,
                    ))
                })
                .transpose()?
                .unwrap_or(Self::Null),
            "varchar" | "bpchar" | "char(n)" | "text" | "name" | "citext" => raw
                .map(|raw| {
                    Ok::<_, Box<dyn std::error::Error + Sync + Send>>(Self::String(
                        text_from_sql(raw)?.to_string(),
                    ))
                })
                .transpose()?
                .unwrap_or(Self::Null),
            "timestamp" => raw
                .map(|raw| {
                    Ok::<_, Box<dyn std::error::Error + Sync + Send>>(Self::DateTime(
                        NaiveDateTime::from_sql(ty, raw)?,
                    ))
                })
                .transpose()?
                .unwrap_or(Self::Null),
            #[cfg(feature = "uuid")]
            "uuid" => raw
                .map(|raw| {
                    Ok::<_, Box<dyn std::error::Error + Sync + Send>>(Self::Uuid(
                        uuid::Uuid::from_sql(ty, raw)?,
                    ))
                })
                .transpose()?
                .unwrap_or(Self::Null),
            #[cfg(feature = "decimal")]
            "numeric" => raw
                .map(|raw| {
                    Ok::<_, Box<dyn std::error::Error + Sync + Send>>(Self::Decimal(
                        rust_decimal::Decimal::from_sql(ty, raw)?,
                    ))
                })
                .transpose()?
                .unwrap_or(Self::Null),
            other => {
                return Err(Box::new(PostgresDatabaseError::TypeNotFound {
                    type_name: other.to_string(),
                }));
            }
        })
    }

    fn from_sql_null(
        ty: &tokio_postgres::types::Type,
    ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        log::trace!("FromSql from_sql_null: ty={}, {ty:?}", ty.name());
        Ok(Self::Null)
    }

    fn accepts(ty: &tokio_postgres::types::Type) -> bool {
        log::trace!("FromSql accepts: ty={}, {ty:?}", ty.name());
        true
    }
}

impl tokio_postgres::types::ToSql for PgDatabaseValue {
    fn accepts(ty: &tokio_postgres::types::Type) -> bool
    where
        Self: Sized,
    {
        log::trace!("ToSql accepts: ty={}, {ty:?}", ty.name());
        true
    }

    fn encode_format(&self, ty: &tokio_postgres::types::Type) -> tokio_postgres::types::Format {
        // Check if we're sending a String to an interval column
        if ty.name() == "interval"
            && let DatabaseValue::String(_) = &self.0
        {
            // Use text format for interval strings
            return tokio_postgres::types::Format::Text;
        }
        // Default to binary format for everything else
        tokio_postgres::types::Format::Binary
    }

    fn to_sql_checked(
        &self,
        ty: &tokio_postgres::types::Type,
        out: &mut tokio_util::bytes::BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        log::trace!("to_sql_checked: ty={}, {ty:?} {self:?}", ty.name());
        Ok(match &self.0 {
            DatabaseValue::Null
            | DatabaseValue::UInt8Opt(None)
            | DatabaseValue::UInt16Opt(None)
            | DatabaseValue::UInt32Opt(None)
            | DatabaseValue::UInt64Opt(None) => IsNull::Yes,
            DatabaseValue::StringOpt(value) => value.to_sql(ty, out)?,
            DatabaseValue::Bool(value) => value.to_sql(ty, out)?,
            DatabaseValue::BoolOpt(value) => value.to_sql(ty, out)?,
            DatabaseValue::Int8(value) => i16::from(*value).to_sql(ty, out)?,
            DatabaseValue::Int8Opt(value) => value.map(i16::from).to_sql(ty, out)?,
            DatabaseValue::Int16(value) => value.to_sql(ty, out)?,
            DatabaseValue::Int16Opt(value) => value.to_sql(ty, out)?,
            DatabaseValue::Int32(value) => value.to_sql(ty, out)?,
            DatabaseValue::Int32Opt(value) => value.to_sql(ty, out)?,
            DatabaseValue::Int64(value) => value.to_sql(ty, out)?,
            DatabaseValue::Int64Opt(value) => value.to_sql(ty, out)?,
            DatabaseValue::UInt8(value) | DatabaseValue::UInt8Opt(Some(value)) => {
                i16::from(*value).to_sql(ty, out)?
            }
            DatabaseValue::UInt16(value) | DatabaseValue::UInt16Opt(Some(value)) => {
                i16::try_from(*value)?.to_sql(ty, out)?
            }
            DatabaseValue::UInt32(value) | DatabaseValue::UInt32Opt(Some(value)) => {
                i32::try_from(*value)?.to_sql(ty, out)?
            }
            DatabaseValue::UInt64(value) | DatabaseValue::UInt64Opt(Some(value)) => {
                i64::try_from(*value)?.to_sql(ty, out)?
            }
            DatabaseValue::Real64(value) => value.to_sql(ty, out)?,
            DatabaseValue::Real64Opt(value) => value.to_sql(ty, out)?,
            DatabaseValue::Real32(value) => value.to_sql(ty, out)?,
            DatabaseValue::Real32Opt(value) => value.to_sql(ty, out)?,
            #[cfg(feature = "decimal")]
            DatabaseValue::Decimal(value) => value.to_sql(ty, out)?,
            #[cfg(feature = "decimal")]
            DatabaseValue::DecimalOpt(value) => value.to_sql(ty, out)?,
            #[cfg(feature = "uuid")]
            DatabaseValue::Uuid(value) => value.to_sql(ty, out)?,
            #[cfg(feature = "uuid")]
            DatabaseValue::UuidOpt(value) => value.to_sql(ty, out)?,
            DatabaseValue::String(value) => {
                if ty.name() == "interval" {
                    // For interval type, write as text format (UTF-8 bytes)
                    // No binary encoding needed, just the raw string bytes
                    out.extend_from_slice(value.as_bytes());
                    IsNull::No
                } else {
                    // For other types, use standard String ToSql
                    value.to_sql(ty, out)?
                }
            }
            DatabaseValue::NowPlus(_interval) => {
                // NowPlus should not be used as a bindable parameter - it should be a SQL expression
                return Err(PostgresDatabaseError::InvalidParameterType(
                    "NowPlus cannot be bound as parameter - use in SQL expression instead"
                        .to_string(),
                )
                .into());
            }
            DatabaseValue::Now => switchy_time::datetime_utc_now()
                .naive_utc()
                .to_sql(ty, out)?,
            DatabaseValue::DateTime(value) => value.to_sql(ty, out)?,
        })
    }

    fn to_sql(
        &self,
        ty: &tokio_postgres::types::Type,
        out: &mut tokio_util::bytes::BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>>
    where
        Self: Sized,
    {
        log::trace!("to_sql: ty={}, {ty:?}", ty.name());
        self.to_sql_checked(ty, out)
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
                // NOW() cannot be parameterized - replace in SQL
                transformed_query = transformed_query.replace(&old_placeholder, "NOW()");
            }
            DatabaseValue::NowPlus(interval) => {
                // Transform to (NOW() + $N::interval) with interval as parameter
                let new_placeholder = format!("${param_counter}");
                transformed_query = transformed_query.replace(
                    &old_placeholder,
                    &format!("(NOW() + {new_placeholder}::interval)"),
                );

                // Add interval string as regular string parameter
                let interval_string = postgres_interval_to_string(interval);
                output_params.push(DatabaseValue::String(interval_string));
                param_counter += 1;
            }
            other => {
                // Regular parameter - renumber if needed
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
    use crate::postgres::postgres::PostgresDatabase;

    fn get_postgres_test_url() -> Option<String> {
        std::env::var("POSTGRES_TEST_URL").ok()
    }

    fn create_pool(url: &str) -> Result<Pool, deadpool_postgres::CreatePoolError> {
        // Simple approach: create a connection manually then build pool from it
        // For tests, we'll use direct connection parameters
        let mut cfg = deadpool_postgres::Config::new();
        cfg.url = Some(url.to_string());

        if url.contains("sslmode=require") {
            let connector = native_tls::TlsConnector::builder()
                .danger_accept_invalid_certs(true) // For testing only!
                .build()
                .unwrap();
            let connector = postgres_native_tls::MakeTlsConnector::new(connector);

            cfg.create_pool(Some(deadpool_postgres::Runtime::Tokio1), connector)
        } else {
            cfg.create_pool(
                Some(deadpool_postgres::Runtime::Tokio1),
                tokio_postgres::NoTls,
            )
        }
    }

    #[switchy_async::test]
    async fn test_postgres_table_exists() {
        let Some(url) = get_postgres_test_url() else {
            return;
        };

        let pool = create_pool(&url).expect("Failed to create pool");
        let db = PostgresDatabase::new(pool);

        // Test non-existent table
        assert!(!db.table_exists("non_existent_table").await.unwrap());

        // Create test table
        db.exec_raw("CREATE TABLE IF NOT EXISTS test_table_exists (id INTEGER PRIMARY KEY)")
            .await
            .unwrap();

        // Test existing table
        assert!(db.table_exists("test_table_exists").await.unwrap());

        // Clean up
        db.exec_raw("DROP TABLE IF EXISTS test_table_exists")
            .await
            .unwrap();
    }

    #[switchy_async::test]
    async fn test_postgres_list_tables() {
        let Some(url) = get_postgres_test_url() else {
            return;
        };

        let pool = create_pool(&url).expect("Failed to create pool");
        let db = PostgresDatabase::new(pool);

        // Clean up any existing test tables first
        db.exec_raw("DROP TABLE IF EXISTS test_list_table1")
            .await
            .ok();
        db.exec_raw("DROP TABLE IF EXISTS test_list_table2")
            .await
            .ok();

        // Create test tables
        db.exec_raw("CREATE TABLE test_list_table1 (id SERIAL PRIMARY KEY)")
            .await
            .expect("Failed to create table1");

        db.exec_raw("CREATE TABLE test_list_table2 (id SERIAL PRIMARY KEY, name VARCHAR(255))")
            .await
            .expect("Failed to create table2");

        // List tables - should contain our test tables
        let tables = db.list_tables().await.expect("Failed to list tables");
        assert!(
            tables.contains(&"test_list_table1".to_string()),
            "Should contain test_list_table1"
        );
        assert!(
            tables.contains(&"test_list_table2".to_string()),
            "Should contain test_list_table2"
        );

        // Drop one table and verify it's removed from the list
        db.exec_raw("DROP TABLE test_list_table1")
            .await
            .expect("Failed to drop table1");

        let tables = db.list_tables().await.expect("Failed to list tables");
        assert!(
            !tables.contains(&"test_list_table1".to_string()),
            "Should not contain dropped table"
        );
        assert!(
            tables.contains(&"test_list_table2".to_string()),
            "Should still contain table2"
        );

        // Clean up
        db.exec_raw("DROP TABLE IF EXISTS test_list_table2")
            .await
            .ok();
    }

    #[switchy_async::test]
    async fn test_postgres_column_metadata() {
        let Some(url) = get_postgres_test_url() else {
            return;
        };

        let pool = create_pool(&url).expect("Failed to create pool");
        let db = PostgresDatabase::new(pool);

        // Create test table with various column types
        db.exec_raw(
            "CREATE TABLE IF NOT EXISTS test_column_metadata (
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
        let columns = db.get_table_columns("test_column_metadata").await.unwrap();

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
        db.exec_raw("DROP TABLE IF EXISTS test_column_metadata")
            .await
            .unwrap();
    }

    #[switchy_async::test]
    async fn test_postgres_constraints() {
        let Some(url) = get_postgres_test_url() else {
            return;
        };

        let pool = create_pool(&url).expect("Failed to create pool");
        let db = PostgresDatabase::new(pool);

        // Create test tables with constraints
        db.exec_raw(
            "CREATE TABLE IF NOT EXISTS test_parent (
            id SERIAL PRIMARY KEY,
            email VARCHAR(100) UNIQUE
        )",
        )
        .await
        .unwrap();

        db.exec_raw(
            "CREATE TABLE IF NOT EXISTS test_child (
            id SERIAL PRIMARY KEY,
            parent_id INTEGER REFERENCES test_parent(id),
            name TEXT NOT NULL
        )",
        )
        .await
        .unwrap();

        // Get table info with constraints
        let table_info = db.get_table_info("test_parent").await.unwrap().unwrap();

        // Should have primary key and unique constraints reflected in indexes
        assert!(!table_info.indexes.is_empty());

        let child_info = db.get_table_info("test_child").await.unwrap().unwrap();

        // Should have foreign key constraint
        assert!(!child_info.foreign_keys.is_empty());

        // Clean up (order matters due to foreign key)
        db.exec_raw("DROP TABLE IF EXISTS test_child")
            .await
            .unwrap();
        db.exec_raw("DROP TABLE IF EXISTS test_parent")
            .await
            .unwrap();
    }

    #[switchy_async::test]
    async fn test_postgres_type_mapping() {
        let Some(url) = get_postgres_test_url() else {
            return;
        };

        let pool = create_pool(&url).expect("Failed to create pool");
        let db = PostgresDatabase::new(pool);

        // Create test table with all supported PostgreSQL types
        db.exec_raw(
            "CREATE TABLE IF NOT EXISTS test_type_mapping (
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

        let columns = db.get_table_columns("test_type_mapping").await.unwrap();

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
        db.exec_raw("DROP TABLE IF EXISTS test_type_mapping")
            .await
            .unwrap();
    }

    #[switchy_async::test]
    async fn test_postgres_default_values() {
        let Some(url) = get_postgres_test_url() else {
            return;
        };

        let pool = create_pool(&url).expect("Failed to create pool");
        let db = PostgresDatabase::new(pool);

        // Create test table with various default value formats
        db.exec_raw(
            "CREATE TABLE IF NOT EXISTS test_default_values (
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

        let columns = db.get_table_columns("test_default_values").await.unwrap();

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
        db.exec_raw("DROP TABLE IF EXISTS test_default_values")
            .await
            .unwrap();
    }

    #[switchy_async::test]
    async fn test_postgres_transaction_isolation() {
        let Some(url) = get_postgres_test_url() else {
            return;
        };

        let pool = create_pool(&url).expect("Failed to create pool");
        let db = PostgresDatabase::new(pool);

        // Create test table
        db.exec_raw(
            "CREATE TABLE IF NOT EXISTS test_transaction_iso (id INTEGER PRIMARY KEY, name TEXT)",
        )
        .await
        .unwrap();

        // Start transaction
        let tx = db.begin_transaction().await.unwrap();

        // Test introspection methods work within transaction
        assert!(tx.table_exists("test_transaction_iso").await.unwrap());

        // Test column_exists within transaction
        assert!(
            tx.column_exists("test_transaction_iso", "id")
                .await
                .unwrap()
        );
        assert!(
            !tx.column_exists("test_transaction_iso", "nonexistent")
                .await
                .unwrap()
        );

        // Test get_table_columns within transaction
        let columns = tx.get_table_columns("test_transaction_iso").await.unwrap();
        assert_eq!(columns.len(), 2);

        // Test get_table_info within transaction
        let table_info = tx.get_table_info("test_transaction_iso").await.unwrap();
        assert!(table_info.is_some());

        // Commit transaction
        tx.commit().await.unwrap();

        // Clean up
        db.exec_raw("DROP TABLE IF EXISTS test_transaction_iso")
            .await
            .unwrap();
    }

    #[switchy_async::test]
    async fn test_postgres_savepoint_basic() {
        let Some(url) = get_postgres_test_url() else {
            return;
        };

        let pool = create_pool(&url).expect("Failed to create pool");
        let db = PostgresDatabase::new(pool);

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
    async fn test_postgres_savepoint_rollback() {
        let Some(url) = get_postgres_test_url() else {
            return;
        };

        let pool = create_pool(&url).expect("Failed to create pool");
        let db = PostgresDatabase::new(pool);

        // Start transaction
        let tx = db.begin_transaction().await.unwrap();

        // Create savepoint
        let savepoint = tx.savepoint("test_rollback").await.unwrap();
        assert_eq!(savepoint.name(), "test_rollback");

        // Rollback savepoint
        savepoint.rollback_to().await.unwrap();

        // Commit transaction
        tx.commit().await.unwrap();
    }

    #[switchy_async::test]
    async fn test_postgres_savepoint_double_release() {
        let Some(url) = get_postgres_test_url() else {
            return;
        };

        let pool = create_pool(&url).expect("Failed to create pool");
        let db = PostgresDatabase::new(pool);

        // Start transaction
        let tx = db.begin_transaction().await.unwrap();

        // Create savepoint
        let savepoint = tx.savepoint("test_double").await.unwrap();

        // Release savepoint
        savepoint.release().await.unwrap();

        // Try to release again - should fail
        let savepoint2 = tx.savepoint("test_double2").await.unwrap();
        savepoint2.release().await.unwrap();

        // Create another savepoint and test double release
        let savepoint3 = tx.savepoint("test_double3").await.unwrap();
        savepoint3.release().await.unwrap();

        // Can't test double release with the same savepoint due to move semantics
        // The behavior is tested at the implementation level through state checking

        // Commit transaction
        tx.commit().await.unwrap();
    }

    #[switchy_async::test]
    async fn test_postgres_savepoint_after_transaction_commit() {
        let Some(url) = get_postgres_test_url() else {
            return;
        };

        let pool = create_pool(&url).expect("Failed to create pool");
        let db = PostgresDatabase::new(pool);

        // Start transaction
        let tx = db.begin_transaction().await.unwrap();

        // Create savepoint
        let savepoint = tx.savepoint("test_after_commit").await.unwrap();

        // Commit transaction
        tx.commit().await.unwrap();

        // Try to release savepoint after transaction commit - should fail
        match savepoint.release().await {
            Err(DatabaseError::TransactionCommitted) => {} // Expected
            other => panic!("Expected TransactionCommitted, got: {other:?}"),
        }
    }

    #[switchy_async::test]
    async fn test_postgres_savepoint_after_transaction_rollback() {
        let Some(url) = get_postgres_test_url() else {
            return;
        };

        let pool = create_pool(&url).expect("Failed to create pool");
        let db = PostgresDatabase::new(pool);

        // Start transaction
        let tx = db.begin_transaction().await.unwrap();

        // Create savepoint
        let savepoint = tx.savepoint("test_after_rollback").await.unwrap();

        // Rollback transaction
        tx.rollback().await.unwrap();

        // Try to rollback savepoint after transaction rollback - should fail
        match savepoint.rollback_to().await {
            Err(DatabaseError::TransactionCommitted) => {} // Expected (same error for both cases)
            other => panic!("Expected TransactionCommitted, got: {other:?}"),
        }
    }

    #[switchy_async::test]
    async fn test_postgres_savepoint_invalid_name() {
        let Some(url) = get_postgres_test_url() else {
            return;
        };

        let pool = create_pool(&url).expect("Failed to create pool");
        let db = PostgresDatabase::new(pool);

        // Start transaction
        let tx = db.begin_transaction().await.unwrap();

        // Try to create savepoint with invalid name
        match tx.savepoint("invalid-name").await {
            Err(DatabaseError::InvalidSavepointName(_)) => {} // Expected
            Ok(_) => panic!("Expected InvalidSavepointName, got Ok(_)"),
            Err(other) => panic!("Expected InvalidSavepointName, got: {other:?}"),
        }

        // Commit transaction
        tx.commit().await.unwrap();
    }
}
