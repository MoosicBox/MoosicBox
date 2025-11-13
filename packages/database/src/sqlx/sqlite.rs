use std::{
    ops::Deref,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU16, Ordering},
    },
};

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use sqlx::{
    Column, Executor, Row, Sqlite, SqliteConnection, SqlitePool, Statement, Transaction, TypeInfo,
    Value, ValueRef,
    pool::PoolConnection,
    query::Query,
    sqlite::{SqliteArguments, SqliteRow, SqliteValueRef},
};
use thiserror::Error;
use tokio::sync::Mutex;

use crate::{
    Database, DatabaseError, DatabaseTransaction, DatabaseValue, DeleteStatement, InsertStatement,
    SelectQuery, UpdateStatement, UpsertMultiStatement, UpsertStatement,
    query::{BooleanExpression, Expression, ExpressionType, Join, Sort, SortDirection},
    query_transform::{QuestionMarkHandler, transform_query_for_params},
    sql_interval::SqlInterval,
};

/// Format `SqlInterval` as `SQLite` datetime modifiers (reuse from rusqlite)
fn format_sqlite_interval_sqlx(interval: &SqlInterval) -> Vec<String> {
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

/// `SQLite` database transaction using `SQLx`
#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct SqliteSqlxTransaction {
    transaction: Arc<Mutex<Option<Transaction<'static, Sqlite>>>>,
}

impl SqliteSqlxTransaction {
    /// Creates a new transaction wrapper from an `SQLx` transaction
    #[must_use]
    pub fn new(transaction: Transaction<'static, Sqlite>) -> Self {
        Self {
            transaction: Arc::new(Mutex::new(Some(transaction))),
        }
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
                "ifnull({})",
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
                    let modifiers = format_sqlite_interval_sqlx(interval);
                    let modifier_str = modifiers
                        .iter()
                        .map(|m| format!("'{m}'"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("strftime('%Y-%m-%dT%H:%M:%f', datetime('now', {modifier_str}))")
                }
                _ => {
                    let pos = index.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                    format!("${pos}")
                }
            },
        }
    }
}

/// `SQLite` database implementation using `SQLx`
#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct SqliteSqlxDatabase {
    pool: Arc<Mutex<SqlitePool>>,
    #[allow(clippy::type_complexity)]
    connection: Arc<Mutex<Option<Arc<Mutex<PoolConnection<Sqlite>>>>>>,
}

impl SqliteSqlxDatabase {
    /// Creates a new database instance from an `SQLx` connection pool
    #[must_use]
    pub fn new(pool: Arc<Mutex<SqlitePool>>) -> Self {
        Self {
            pool,
            connection: Arc::new(Mutex::new(None)),
        }
    }

    /// # Errors
    ///
    /// Will return `Err` if cannot get a connection
    pub async fn get_connection(
        &self,
    ) -> Result<Arc<Mutex<PoolConnection<Sqlite>>>, SqlxDatabaseError> {
        let connection = { self.connection.lock().await.clone() };

        if let Some(connection) = connection {
            log::trace!("Returning existing connection from sqlite db pool");
            return Ok(connection);
        }

        log::debug!("Fetching new connection from sqlite db pool");
        let connection = Arc::new(Mutex::new(self.pool.lock().await.acquire().await?));
        self.connection.lock().await.replace(connection.clone());
        Ok(connection)
    }
}

/// Errors that can occur during `SQLite` `SQLx` database operations
#[derive(Debug, Error)]
pub enum SqlxDatabaseError {
    /// Underlying `SQLx` error
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    /// INSERT operation did not return an ID
    #[error("No ID")]
    NoId,
    /// Query returned no rows
    #[error("No row")]
    NoRow,
    /// Invalid request format or parameters
    #[error("Invalid request")]
    InvalidRequest,
    /// UPSERT operation missing required unique constraint columns
    #[error("Missing unique")]
    MissingUnique,
    /// Unsupported or unknown database data type encountered
    #[error("Unsupported data type: {0}")]
    UnsupportedDataType(String),
}

impl From<SqlxDatabaseError> for DatabaseError {
    fn from(value: SqlxDatabaseError) -> Self {
        match value {
            SqlxDatabaseError::UnsupportedDataType(type_name) => {
                Self::UnsupportedDataType(type_name)
            }
            other => Self::SqliteSqlx(other),
        }
    }
}

/// Get column dependencies (indexes and foreign keys) for a specific column in `SQLite`
#[cfg(feature = "cascade")]
async fn sqlite_get_column_dependencies(
    connection: &mut SqliteConnection,
    table_name: &str,
    column_name: &str,
) -> Result<(Vec<String>, Vec<String>), SqlxDatabaseError> {
    let mut indexes = Vec::new();
    let mut foreign_keys = Vec::new();

    // Find indexes that use this column
    let index_list_query = format!("PRAGMA index_list({table_name})");
    let index_rows = sqlx::query(&index_list_query)
        .fetch_all(&mut *connection)
        .await
        .map_err(SqlxDatabaseError::Sqlx)?;

    for row in index_rows {
        let index_name: String = row.try_get("name").map_err(SqlxDatabaseError::Sqlx)?;

        // Check if this index uses the column we're interested in
        let index_info_query = format!("PRAGMA index_info({index_name})");
        let column_rows = sqlx::query(&index_info_query)
            .fetch_all(&mut *connection)
            .await
            .map_err(SqlxDatabaseError::Sqlx)?;

        for col_row in column_rows {
            let col_name: String = col_row.try_get("name").map_err(SqlxDatabaseError::Sqlx)?;
            if col_name == column_name {
                indexes.push(index_name.clone());
                break;
            }
        }
    }

    // Find foreign key constraints that use this column
    let fk_list_query = format!("PRAGMA foreign_key_list({table_name})");
    let fk_rows = sqlx::query(&fk_list_query)
        .fetch_all(&mut *connection)
        .await
        .map_err(SqlxDatabaseError::Sqlx)?;

    for row in fk_rows {
        let from_column: String = row.try_get("from").map_err(SqlxDatabaseError::Sqlx)?;
        if from_column == column_name {
            let id: i64 = row.try_get("id").map_err(SqlxDatabaseError::Sqlx)?;
            let to_table: String = row.try_get("table").map_err(SqlxDatabaseError::Sqlx)?;
            let to_column: String = row.try_get("to").map_err(SqlxDatabaseError::Sqlx)?;
            foreign_keys.push(format!("FK_{id}_{table_name}_{to_table}_{to_column}"));
        }
    }

    Ok((indexes, foreign_keys))
}

#[async_trait]
impl Database for SqliteSqlxDatabase {
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
        let mut binding = connection.lock().await;

        binding
            .execute(sqlx::raw_sql(statement))
            .await
            .map_err(SqlxDatabaseError::Sqlx)?;

        drop(binding);

        Ok(())
    }

    #[cfg(feature = "schema")]
    async fn exec_create_table(
        &self,
        statement: &crate::schema::CreateTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        sqlite_sqlx_exec_create_table(
            self.get_connection().await?.lock().await.as_mut(),
            statement,
        )
        .await
        .map_err(Into::into)
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_table(
        &self,
        statement: &crate::schema::DropTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        sqlite_sqlx_exec_drop_table(
            self.get_connection().await?.lock().await.as_mut(),
            statement,
        )
        .await
        .map_err(Into::into)
    }

    #[cfg(feature = "schema")]
    async fn exec_create_index(
        &self,
        statement: &crate::schema::CreateIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        sqlite_sqlx_exec_create_index(
            self.get_connection().await?.lock().await.as_mut(),
            statement,
        )
        .await
        .map_err(Into::into)
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_index(
        &self,
        statement: &crate::schema::DropIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        sqlite_sqlx_exec_drop_index(
            self.get_connection().await?.lock().await.as_mut(),
            statement,
        )
        .await
        .map_err(Into::into)
    }

    #[cfg(feature = "schema")]
    async fn exec_alter_table(
        &self,
        statement: &crate::schema::AlterTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        sqlite_sqlx_exec_alter_table(
            self.get_connection().await?.lock().await.as_mut(),
            statement,
        )
        .await
        .map_err(Into::into)
    }

    #[cfg(feature = "schema")]
    async fn table_exists(&self, table_name: &str) -> Result<bool, DatabaseError> {
        sqlx_sqlite_table_exists(
            self.get_connection().await?.lock().await.as_mut(),
            table_name,
        )
        .await
        .map_err(Into::into)
    }

    #[cfg(feature = "schema")]
    async fn list_tables(&self) -> Result<Vec<String>, DatabaseError> {
        sqlx_sqlite_list_tables(self.get_connection().await?.lock().await.as_mut())
            .await
            .map_err(Into::into)
    }

    #[cfg(feature = "schema")]
    async fn get_table_info(
        &self,
        table_name: &str,
    ) -> Result<Option<crate::schema::TableInfo>, DatabaseError> {
        sqlx_sqlite_get_table_info(
            self.get_connection().await?.lock().await.as_mut(),
            table_name,
        )
        .await
        .map_err(Into::into)
    }

    #[cfg(feature = "schema")]
    async fn get_table_columns(
        &self,
        table_name: &str,
    ) -> Result<Vec<crate::schema::ColumnInfo>, DatabaseError> {
        sqlx_sqlite_get_table_columns(
            self.get_connection().await?.lock().await.as_mut(),
            table_name,
        )
        .await
        .map_err(Into::into)
    }

    #[cfg(feature = "schema")]
    async fn column_exists(
        &self,
        table_name: &str,
        column_name: &str,
    ) -> Result<bool, DatabaseError> {
        let columns = self.get_table_columns(table_name).await?;
        Ok(columns.iter().any(|col| col.name == column_name))
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

        // Use existing from_row helper for each row
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
        // Get connection from pool and begin transaction
        let tx = {
            let pool = self.pool.lock().await;
            pool.begin().await.map_err(SqlxDatabaseError::Sqlx)?
        };

        Ok(Box::new(SqliteSqlxTransaction::new(tx)))
    }

    async fn exec_raw_params(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<u64, DatabaseError> {
        // Transform query to handle Now/NowPlus parameters
        let (transformed_query, filtered_params) =
            sqlite_transform_query_for_params(query, params)?;

        let mut connection = {
            let pool = self.pool.lock().await;
            pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?
        };

        let mut query_builder: Query<'_, Sqlite, SqliteArguments> = sqlx::query(&transformed_query);

        // Add only filtered parameters - Now/NowPlus are already in the SQL
        for param in &filtered_params {
            query_builder = match param {
                crate::DatabaseValue::Int8(n) => query_builder.bind(i64::from(*n)),
                crate::DatabaseValue::Int8Opt(n) => query_builder.bind(n.map(i64::from)),
                crate::DatabaseValue::Int16(n) => query_builder.bind(i64::from(*n)),
                crate::DatabaseValue::Int16Opt(n) => query_builder.bind(n.map(i64::from)),
                crate::DatabaseValue::Int32(n) => query_builder.bind(i64::from(*n)),
                crate::DatabaseValue::Int32Opt(n) => query_builder.bind(n.map(i64::from)),
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
                crate::DatabaseValue::Real32(r) => query_builder.bind(f64::from(*r)),
                crate::DatabaseValue::Real32Opt(r) => query_builder.bind(r.map(f64::from)),
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::Decimal(d) => query_builder.bind(d.to_string()),
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::DecimalOpt(d) => {
                    query_builder.bind(d.as_ref().map(ToString::to_string))
                }
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::Uuid(u) => query_builder.bind(u.to_string()),
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::UuidOpt(u) => {
                    query_builder.bind(u.as_ref().map(ToString::to_string))
                }
                crate::DatabaseValue::Bool(b) => query_builder.bind(*b),
                crate::DatabaseValue::BoolOpt(b) => query_builder.bind(b),
                crate::DatabaseValue::DateTime(dt) => query_builder.bind(dt.to_string()),
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
        // Transform query to handle Now/NowPlus parameters
        let (transformed_query, filtered_params) =
            sqlite_transform_query_for_params(query, params)?;

        let mut connection = {
            let pool = self.pool.lock().await;
            pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?
        };

        let mut query_builder: Query<'_, Sqlite, SqliteArguments> = sqlx::query(&transformed_query);

        // Add only filtered parameters - Now/NowPlus are already in the SQL
        for param in &filtered_params {
            query_builder = match param {
                crate::DatabaseValue::Int8(n) => query_builder.bind(i64::from(*n)),
                crate::DatabaseValue::Int8Opt(n) => query_builder.bind(n.map(i64::from)),
                crate::DatabaseValue::Int16(n) => query_builder.bind(i64::from(*n)),
                crate::DatabaseValue::Int16Opt(n) => query_builder.bind(n.map(i64::from)),
                crate::DatabaseValue::Int32(n) => query_builder.bind(i64::from(*n)),
                crate::DatabaseValue::Int32Opt(n) => query_builder.bind(n.map(i64::from)),
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
                crate::DatabaseValue::Real32(r) => query_builder.bind(f64::from(*r)),
                crate::DatabaseValue::Real32Opt(r) => query_builder.bind(r.map(f64::from)),
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::Decimal(d) => query_builder.bind(d.to_string()),
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::DecimalOpt(d) => {
                    query_builder.bind(d.as_ref().map(ToString::to_string))
                }
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::Uuid(u) => query_builder.bind(u.to_string()),
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::UuidOpt(u) => {
                    query_builder.bind(u.as_ref().map(ToString::to_string))
                }
                crate::DatabaseValue::Bool(b) => query_builder.bind(*b),
                crate::DatabaseValue::BoolOpt(b) => query_builder.bind(b),
                crate::DatabaseValue::DateTime(dt) => query_builder.bind(dt.to_string()),
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

/// # Errors
///
/// Will return `Err` if column type was invalid.
pub fn column_value(value: &SqliteValueRef<'_>) -> Result<DatabaseValue, sqlx::Error> {
    if value.is_null() {
        return Ok(DatabaseValue::Null);
    }
    let owned = sqlx::ValueRef::to_owned(value);
    match value.type_info().name() {
        "BOOL" => Ok(DatabaseValue::Bool(owned.try_decode()?)),
        "CHAR" | "SMALLINT" | "SMALLSERIAL" | "INT2" | "INTEGER" | "INT" | "SERIAL" | "INT4"
        | "BIGINT" | "BIGSERIAL" | "INT8" => Ok(DatabaseValue::Int64(owned.try_decode()?)),
        "REAL" | "FLOAT4" | "DOUBLE PRECISION" | "FLOAT8" => {
            Ok(DatabaseValue::Real64(owned.try_decode()?))
        }
        "VARCHAR" | "CHAR(N)" | "TEXT" | "NAME" | "CITEXT" => {
            Ok(DatabaseValue::String(owned.try_decode()?))
        }
        "TIMESTAMP" => Ok(DatabaseValue::DateTime(owned.try_decode()?)),
        _ => Err(sqlx::Error::TypeNotFound {
            type_name: value.type_info().name().to_string(),
        }),
    }
}

fn from_row(column_names: &[String], row: &SqliteRow) -> Result<crate::Row, SqlxDatabaseError> {
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
    connection: &mut SqliteConnection,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Option<crate::Row>, SqlxDatabaseError> {
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
                        "SELECT rowid FROM {table_name} {}",
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
    let pg_row: Option<SqliteRow> = stream.next().await.transpose()?;

    pg_row.map(|row| from_row(&column_names, &row)).transpose()
}

async fn update_and_get_rows(
    connection: &mut SqliteConnection,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, SqlxDatabaseError> {
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
                        "SELECT rowid FROM {table_name} {}",
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
        .collect::<Vec<SqliteDatabaseValue>>();
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
                .collect::<Vec<SqliteDatabaseValue>>()
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

    to_rows(&column_names, query.fetch(connection)).await
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
            format!("rowid IN ({query} LIMIT {limit})")
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
                "{}=({})",
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

fn format_identifier(identifier: &str) -> String {
    identifier.to_string()
}

fn bind_values<'a, 'b>(
    mut query: Query<'a, Sqlite, SqliteArguments<'a>>,
    values: Option<&'b [SqliteDatabaseValue]>,
) -> Result<Query<'a, Sqlite, SqliteArguments<'a>>, SqlxDatabaseError>
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
                    query = query.bind(i64::from(*value));
                }
                DatabaseValue::Int16(value) | DatabaseValue::Int16Opt(Some(value)) => {
                    query = query.bind(i64::from(*value));
                }
                DatabaseValue::Int32(value) | DatabaseValue::Int32Opt(Some(value)) => {
                    query = query.bind(i64::from(*value));
                }
                DatabaseValue::Int64(value) | DatabaseValue::Int64Opt(Some(value)) => {
                    query = query.bind(*value);
                }
                DatabaseValue::UInt8(value) | DatabaseValue::UInt8Opt(Some(value)) => {
                    let signed = i8::try_from(*value).ok();
                    query = query.bind(signed);
                }
                DatabaseValue::UInt16(value) | DatabaseValue::UInt16Opt(Some(value)) => {
                    let signed = i16::try_from(*value).ok();
                    query = query.bind(signed);
                }
                DatabaseValue::UInt32(value) | DatabaseValue::UInt32Opt(Some(value)) => {
                    let signed = i32::try_from(*value).ok();
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
                    query = query.bind(f64::from(*value));
                }
                #[cfg(feature = "decimal")]
                DatabaseValue::Decimal(value) | DatabaseValue::DecimalOpt(Some(value)) => {
                    query = query.bind(value.to_string());
                }
                #[cfg(feature = "uuid")]
                DatabaseValue::Uuid(value) | DatabaseValue::UuidOpt(Some(value)) => {
                    query = query.bind(value.to_string());
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
    mut rows: Pin<Box<dyn Stream<Item = Result<SqliteRow, sqlx::Error>> + Send + 'a>>,
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

#[cfg(feature = "schema")]
#[allow(clippy::too_many_lines)]
async fn sqlite_sqlx_exec_create_table(
    connection: &mut SqliteConnection,
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

        if column.auto_increment && statement.primary_key.is_none_or(|x| x != column.name) {
            return Err(SqlxDatabaseError::InvalidRequest);
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
            | crate::schema::DataType::Decimal(..) => query.push_str("TEXT"), // SQLite stores many types as text
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
            | crate::schema::DataType::BigSerial => query.push_str("INTEGER"), // SQLite uses INTEGER for many numeric types
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
                    let modifiers = format_sqlite_interval_sqlx(interval);
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
        .execute(sqlx::raw_sql(&query))
        .await
        .map_err(SqlxDatabaseError::Sqlx)?;

    Ok(())
}

#[cfg(feature = "schema")]
async fn sqlite_sqlx_exec_drop_table(
    connection: &mut SqliteConnection,
    statement: &crate::schema::DropTableStatement<'_>,
) -> Result<(), SqlxDatabaseError> {
    #[cfg(feature = "cascade")]
    {
        use crate::schema::DropBehavior;
        match statement.behavior {
            DropBehavior::Cascade => {
                return sqlite_sqlx_exec_drop_table_cascade(connection, statement).await;
            }
            DropBehavior::Restrict => {
                return sqlite_sqlx_exec_drop_table_restrict(connection, statement).await;
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
        .execute(sqlx::raw_sql(&query))
        .await
        .map_err(SqlxDatabaseError::Sqlx)?;

    Ok(())
}

/// Implement manual CASCADE for `SQLite` using `SQLx` with internal FK helpers
#[cfg(all(feature = "schema", feature = "cascade"))]
async fn sqlite_sqlx_exec_drop_table_cascade(
    connection: &mut SqliteConnection,
    statement: &crate::schema::DropTableStatement<'_>,
) -> Result<(), SqlxDatabaseError> {
    // Get all tables that need to be dropped (dependents first, then target)
    let drop_order = sqlite_sqlx_find_cascade_dependents(connection, statement.table_name).await?;

    // Enable foreign key enforcement temporarily for consistency
    let fk_enabled = sqlite_sqlx_get_foreign_key_state(connection).await?;
    sqlite_sqlx_set_foreign_key_state(connection, true).await?;

    let result = async {
        // Drop all dependent tables first, then the target table
        for table_to_drop in &drop_order {
            let mut query = "DROP TABLE ".to_string();
            if statement.if_exists {
                query.push_str("IF EXISTS ");
            }
            query.push_str(table_to_drop);

            connection
                .execute(sqlx::raw_sql(&query))
                .await
                .map_err(SqlxDatabaseError::Sqlx)?;
        }
        Ok::<_, SqlxDatabaseError>(())
    }
    .await;

    // Restore original foreign key state
    sqlite_sqlx_set_foreign_key_state(connection, fk_enabled).await?;

    result
}

/// Implement manual RESTRICT for `SQLite` using `SQLx` with internal FK helpers
#[cfg(all(feature = "schema", feature = "cascade"))]
async fn sqlite_sqlx_exec_drop_table_restrict(
    connection: &mut SqliteConnection,
    statement: &crate::schema::DropTableStatement<'_>,
) -> Result<(), SqlxDatabaseError> {
    // Check if table has any dependents - if so, fail
    if sqlite_sqlx_has_dependents(connection, statement.table_name).await? {
        return Err(SqlxDatabaseError::InvalidRequest);
    }

    // No dependents, proceed with normal drop
    let mut query = "DROP TABLE ".to_string();
    if statement.if_exists {
        query.push_str("IF EXISTS ");
    }
    query.push_str(statement.table_name);

    connection
        .execute(sqlx::raw_sql(&query))
        .await
        .map_err(SqlxDatabaseError::Sqlx)?;

    Ok(())
}

/// Find all tables that depend on the given table (for CASCADE)
#[cfg(all(feature = "schema", feature = "cascade"))]
async fn sqlite_sqlx_find_cascade_dependents(
    connection: &mut SqliteConnection,
    table_name: &str,
) -> Result<Vec<String>, SqlxDatabaseError> {
    let mut all_dependents = std::collections::BTreeSet::new();
    let mut to_check = vec![table_name.to_string()];
    let mut checked = std::collections::BTreeSet::new();

    while let Some(current_table) = to_check.pop() {
        if !checked.insert(current_table.clone()) {
            continue;
        }

        // Get all tables using query
        let table_rows = sqlx::query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        )
        .fetch_all(&mut *connection)
        .await
        .map_err(SqlxDatabaseError::Sqlx)?;

        for table_row in table_rows {
            let check_table: String = table_row.get(0);

            if check_table == current_table {
                continue;
            }

            // Validate table name for PRAGMA (cannot be parameterized)
            crate::schema::dependencies::validate_table_name_for_pragma(&check_table)
                .map_err(|_| SqlxDatabaseError::InvalidRequest)?;

            let fk_query = format!("PRAGMA foreign_key_list({check_table})");
            let fk_rows = sqlx::query(&fk_query)
                .fetch_all(&mut *connection)
                .await
                .map_err(SqlxDatabaseError::Sqlx)?;

            for fk_row in fk_rows {
                let ref_table: String = fk_row.get(2); // Column 2 is referenced table
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
async fn sqlite_sqlx_has_dependents(
    connection: &mut SqliteConnection,
    table_name: &str,
) -> Result<bool, SqlxDatabaseError> {
    let table_rows = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_all(&mut *connection)
    .await
    .map_err(SqlxDatabaseError::Sqlx)?;

    for table_row in table_rows {
        let check_table: String = table_row.get(0);

        if check_table == table_name {
            continue;
        }

        crate::schema::dependencies::validate_table_name_for_pragma(&check_table)
            .map_err(|_| SqlxDatabaseError::InvalidRequest)?;

        let fk_query = format!("PRAGMA foreign_key_list({check_table})");
        let fk_rows = sqlx::query(&fk_query)
            .fetch_all(&mut *connection)
            .await
            .map_err(SqlxDatabaseError::Sqlx)?;

        for fk_row in fk_rows {
            let ref_table: String = fk_row.get(2); // Column 2 is referenced table
            if ref_table == table_name {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

/// Get current foreign key enforcement state
#[cfg(all(feature = "schema", feature = "cascade"))]
async fn sqlite_sqlx_get_foreign_key_state(
    connection: &mut SqliteConnection,
) -> Result<bool, SqlxDatabaseError> {
    let row = sqlx::query("PRAGMA foreign_keys")
        .fetch_one(&mut *connection)
        .await
        .map_err(SqlxDatabaseError::Sqlx)?;

    let enabled: i64 = row.get(0);
    Ok(enabled != 0)
}

/// Set foreign key enforcement state
#[cfg(all(feature = "schema", feature = "cascade"))]
async fn sqlite_sqlx_set_foreign_key_state(
    connection: &mut SqliteConnection,
    enabled: bool,
) -> Result<(), SqlxDatabaseError> {
    let pragma = if enabled {
        "PRAGMA foreign_keys = ON"
    } else {
        "PRAGMA foreign_keys = OFF"
    };

    sqlx::query(pragma)
        .execute(&mut *connection)
        .await
        .map_err(SqlxDatabaseError::Sqlx)?;

    Ok(())
}

#[cfg(feature = "schema")]
pub(crate) async fn sqlite_sqlx_exec_create_index(
    connection: &mut SqliteConnection,
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
        .map(|col| format!("`{col}`"))
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!(
        "CREATE {}INDEX {}{} ON {} ({})",
        unique_str, if_not_exists_str, statement.index_name, statement.table_name, columns_str
    );

    connection
        .execute(sqlx::raw_sql(&sql))
        .await
        .map_err(SqlxDatabaseError::Sqlx)?;

    Ok(())
}

#[cfg(feature = "schema")]
pub(crate) async fn sqlite_sqlx_exec_drop_index(
    connection: &mut SqliteConnection,
    statement: &crate::schema::DropIndexStatement<'_>,
) -> Result<(), SqlxDatabaseError> {
    let if_exists_str = if statement.if_exists {
        "IF EXISTS "
    } else {
        ""
    };

    let sql = format!("DROP INDEX {}{}", if_exists_str, statement.index_name);

    connection
        .execute(sqlx::raw_sql(&sql))
        .await
        .map_err(SqlxDatabaseError::Sqlx)?;

    Ok(())
}

#[cfg(feature = "schema")]
#[allow(clippy::too_many_lines)]
pub(crate) async fn sqlite_sqlx_exec_alter_table(
    connection: &mut SqliteConnection,
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
                            crate::DatabaseValue::UInt8(n) => n.to_string(),
                            crate::DatabaseValue::UInt16(n) => n.to_string(),
                            crate::DatabaseValue::UInt32(n) => n.to_string(),
                            crate::DatabaseValue::UInt64(n) => n.to_string(),
                            crate::DatabaseValue::Bool(b) => if *b { "1" } else { "0" }.to_string(),
                            crate::DatabaseValue::Real64(r) => r.to_string(),
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
                    "ALTER TABLE {} ADD COLUMN `{}` {}{}{}",
                    statement.table_name, name, type_str, nullable_str, default_str
                );

                connection
                    .execute(sqlx::raw_sql(&sql))
                    .await
                    .map_err(SqlxDatabaseError::Sqlx)?;
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
                            let (indexes, foreign_keys) = sqlite_get_column_dependencies(
                                connection,
                                statement.table_name,
                                name,
                            )
                            .await?;

                            // Drop indexes (SQLite can drop indexes individually)
                            for index_name in indexes {
                                let drop_index_sql = format!("DROP INDEX IF EXISTS `{index_name}`");
                                log::trace!("SQLite CASCADE dropping index: {drop_index_sql}");
                                connection
                                    .execute(sqlx::raw_sql(&drop_index_sql))
                                    .await
                                    .map_err(SqlxDatabaseError::Sqlx)?;
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
                            let (indexes, foreign_keys) = sqlite_get_column_dependencies(
                                connection,
                                statement.table_name,
                                name,
                            )
                            .await?;

                            if !indexes.is_empty() || !foreign_keys.is_empty() {
                                return Err(SqlxDatabaseError::Sqlx(sqlx::Error::Protocol(
                                    format!(
                                        "Cannot drop column {}.{}: has {} index(es) and {} foreign key(s)",
                                        statement.table_name,
                                        name,
                                        indexes.len(),
                                        foreign_keys.len()
                                    ),
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
                    .execute(sqlx::raw_sql(&sql))
                    .await
                    .map_err(SqlxDatabaseError::Sqlx)?;
            }
            AlterOperation::RenameColumn { old_name, new_name } => {
                let sql = format!(
                    "ALTER TABLE {} RENAME COLUMN `{}` TO `{}`",
                    statement.table_name, old_name, new_name
                );

                connection
                    .execute(sqlx::raw_sql(&sql))
                    .await
                    .map_err(SqlxDatabaseError::Sqlx)?;
            }
            AlterOperation::ModifyColumn {
                name,
                new_data_type,
                new_nullable,
                new_default,
            } => {
                // Use decision tree to determine correct workaround approach
                if column_requires_table_recreation(connection, statement.table_name, name).await? {
                    // Use table recreation for complex columns (PRIMARY KEY, UNIQUE, CHECK, GENERATED)
                    sqlite_sqlx_exec_table_recreation_workaround(
                        connection,
                        statement.table_name,
                        name,
                        new_data_type.clone(),
                        *new_nullable,
                        new_default.as_ref(),
                    )
                    .await?;
                } else {
                    // Use column-based workaround for simple columns
                    sqlite_sqlx_exec_modify_column_workaround(
                        connection,
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
#[allow(clippy::too_many_lines)]
async fn sqlite_sqlx_exec_modify_column_workaround(
    connection: &mut SqliteConnection,
    table_name: &str,
    column_name: &str,
    new_data_type: crate::schema::DataType,
    new_nullable: Option<bool>,
    new_default: Option<&crate::DatabaseValue>,
) -> Result<(), SqlxDatabaseError> {
    use sqlx::Connection;

    // Implementation of the column-based workaround for MODIFY COLUMN

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
        crate::schema::DataType::Custom(type_name) => type_name.clone(),
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
    let mut tx = connection.begin().await.map_err(SqlxDatabaseError::Sqlx)?;

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
                crate::DatabaseValue::UInt8(n) => n.to_string(),
                crate::DatabaseValue::UInt16(n) => n.to_string(),
                crate::DatabaseValue::UInt32(n) => n.to_string(),
                crate::DatabaseValue::UInt64(n) => n.to_string(),
                crate::DatabaseValue::Bool(b) => if *b { "1" } else { "0" }.to_string(),
                crate::DatabaseValue::Real64(r) => r.to_string(),
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
                    return Err(SqlxDatabaseError::Sqlx(sqlx::Error::TypeNotFound {
                        type_name: "Unsupported default value type for MODIFY COLUMN".to_string(),
                    }));
                }
            };
            format!(" DEFAULT {val_str}")
        }
        None => String::new(),
    };

    tx.execute(sqlx::raw_sql(&format!(
        "ALTER TABLE {table_name} ADD COLUMN `{temp_column}` {type_str}{nullable_str}{default_str}"
    )))
    .await
    .map_err(SqlxDatabaseError::Sqlx)?;

    // Step 2: Copy and convert data
    tx.execute(sqlx::raw_sql(&format!(
        "UPDATE {table_name} SET `{temp_column}` = CAST(`{column_name}` AS {type_str})"
    )))
    .await
    .map_err(SqlxDatabaseError::Sqlx)?;

    // Step 3: Drop original column
    tx.execute(sqlx::raw_sql(&format!(
        "ALTER TABLE {table_name} DROP COLUMN `{column_name}`"
    )))
    .await
    .map_err(SqlxDatabaseError::Sqlx)?;

    // Step 4: Add column with original name and new type
    tx.execute(sqlx::raw_sql(&format!(
        "ALTER TABLE {table_name} ADD COLUMN `{column_name}` {type_str}{nullable_str}{default_str}"
    )))
    .await
    .map_err(SqlxDatabaseError::Sqlx)?;

    // Step 5: Copy data from temp to final column
    tx.execute(sqlx::raw_sql(&format!(
        "UPDATE {table_name} SET `{column_name}` = `{temp_column}`"
    )))
    .await
    .map_err(SqlxDatabaseError::Sqlx)?;

    // Step 6: Drop temporary column
    tx.execute(sqlx::raw_sql(&format!(
        "ALTER TABLE {table_name} DROP COLUMN `{temp_column}`"
    )))
    .await
    .map_err(SqlxDatabaseError::Sqlx)?;

    tx.commit().await.map_err(SqlxDatabaseError::Sqlx)?;

    Ok(())
}

#[cfg(feature = "schema")]
async fn column_requires_table_recreation(
    connection: &mut SqliteConnection,
    table_name: &str,
    column_name: &str,
) -> Result<bool, SqlxDatabaseError> {
    // Check if column is PRIMARY KEY
    let table_sql: String =
        sqlx::query_scalar("SELECT sql FROM sqlite_master WHERE type='table' AND name=?")
            .bind(table_name)
            .fetch_one(&mut *connection)
            .await
            .map_err(SqlxDatabaseError::Sqlx)?;

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
    let index_sqls: Vec<String> = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type='index' AND tbl_name=? AND sql IS NOT NULL",
    )
    .bind(table_name)
    .fetch_all(&mut *connection)
    .await
    .map_err(SqlxDatabaseError::Sqlx)?;

    for index_sql in index_sqls {
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
fn sqlite_modify_create_table_sql(
    original_sql: &str,
    original_table_name: &str,
    new_table_name: &str,
    column_name: &str,
    new_data_type: &crate::schema::DataType,
    new_nullable: Option<bool>,
    new_default: Option<&crate::DatabaseValue>,
) -> Result<String, SqlxDatabaseError> {
    // Simple regex-based approach to modify column definition
    // This handles most common cases but could be enhanced with a proper SQL parser

    let data_type_str = match new_data_type {
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
        crate::schema::DataType::Date
        | crate::schema::DataType::Time
        | crate::schema::DataType::DateTime
        | crate::schema::DataType::Text
        | crate::schema::DataType::VarChar(..)
        | crate::schema::DataType::Char(..)
        | crate::schema::DataType::Timestamp
        | crate::schema::DataType::Json
        | crate::schema::DataType::Jsonb
        | crate::schema::DataType::Uuid
        | crate::schema::DataType::Xml
        | crate::schema::DataType::Array(..)
        | crate::schema::DataType::Inet
        | crate::schema::DataType::MacAddr
        | crate::schema::DataType::Custom(..)
        | crate::schema::DataType::Decimal(..) => "TEXT",
        crate::schema::DataType::Blob | crate::schema::DataType::Binary(..) => "BLOB",
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
                f64::try_from(*d).unwrap_or(0.0).to_string()
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
            crate::DatabaseValue::NowPlus(_) => return Err(SqlxDatabaseError::InvalidRequest),
        };

        write!(new_column_def, " DEFAULT {default_str}").unwrap();
    }

    // Find and replace the column definition using regex
    // Pattern matches: column_name followed by type and optional constraints
    let column_pattern = format!(
        r"`?{}`?\s+\w+(\s+(NOT\s+NULL|PRIMARY\s+KEY|UNIQUE|CHECK\s*\([^)]+\)|DEFAULT\s+[^,\s)]+|GENERATED\s+[^,)]+))*",
        regex::escape(column_name)
    );

    let re = regex::Regex::new(&column_pattern).map_err(|_| SqlxDatabaseError::InvalidRequest)?;

    let modified_sql = re.replace(original_sql, new_column_def.as_str());

    // Replace table name
    let final_sql = modified_sql.replace(original_table_name, new_table_name);

    Ok(final_sql)
}

#[cfg(feature = "schema")]
#[allow(clippy::too_many_lines)]
async fn sqlite_sqlx_exec_table_recreation_workaround(
    connection: &mut SqliteConnection,
    table_name: &str,
    column_name: &str,
    new_data_type: crate::schema::DataType,
    new_nullable: Option<bool>,
    new_default: Option<&crate::DatabaseValue>,
) -> Result<(), SqlxDatabaseError> {
    use sqlx::Connection;

    // Execute the table recreation workaround in a transaction
    let mut tx = connection.begin().await.map_err(SqlxDatabaseError::Sqlx)?;

    let result = async {
        // Step 1: Check and disable foreign keys if enabled
        let foreign_keys_enabled: i32 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(&mut *tx)
            .await
            .map_err(SqlxDatabaseError::Sqlx)?;

        if foreign_keys_enabled == 1 {
            sqlx::query("PRAGMA foreign_keys=OFF")
                .execute(&mut *tx)
                .await
                .map_err(SqlxDatabaseError::Sqlx)?;
        }

        // Step 2: Save existing schema objects (indexes, triggers, views)
        let schema_objects: Vec<String> = sqlx::query_scalar(
            "SELECT sql FROM sqlite_master WHERE tbl_name=? AND type IN ('index','trigger','view') AND sql IS NOT NULL"
        )
        .bind(table_name)
        .fetch_all(&mut *tx)
        .await
        .map_err(SqlxDatabaseError::Sqlx)?;

        // Step 3: Get original table schema
        let original_sql: String = sqlx::query_scalar("SELECT sql FROM sqlite_master WHERE type='table' AND name=?")
            .bind(table_name)
            .fetch_one(&mut *tx)
            .await
            .map_err(SqlxDatabaseError::Sqlx)?;

        // Step 4: Create temporary table name
        let temp_table = format!("{}_temp_{}", table_name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );

        // Step 5: Parse and modify the CREATE TABLE SQL to update the column definition
        let new_table_sql = sqlite_modify_create_table_sql(
            &original_sql,
            table_name,
            &temp_table,
            column_name,
            &new_data_type,
            new_nullable,
            new_default,
        )?;

        sqlx::query(&new_table_sql)
            .execute(&mut *tx)
            .await
            .map_err(SqlxDatabaseError::Sqlx)?;

        // Step 6: Get column list for INSERT SELECT
        let columns: Vec<String> = sqlx::query(&format!("PRAGMA table_info({table_name})"))
            .fetch_all(&mut *tx)
            .await
            .map_err(SqlxDatabaseError::Sqlx)?
            .into_iter()
            .map(|row| row.get::<String, _>(1)) // Column 1 is the name
            .collect();

        // Step 7: Copy data with potential type conversion for the modified column
        let column_list = columns
            .iter()
            .map(|col| {
                if col == column_name {
                    // Apply CAST for the modified column to ensure proper type conversion
                    let cast_type = match new_data_type {
                        crate::schema::DataType::Bool |
                        crate::schema::DataType::TinyInt |
                        crate::schema::DataType::SmallInt |
                        crate::schema::DataType::Int |
                        crate::schema::DataType::BigInt |
                        crate::schema::DataType::Serial |
                        crate::schema::DataType::BigSerial => "INTEGER",
                        crate::schema::DataType::Real |
                        crate::schema::DataType::Double |
                        crate::schema::DataType::Decimal(_, _) |
                        crate::schema::DataType::Money => "REAL",
                        crate::schema::DataType::Text |
                        crate::schema::DataType::VarChar(_) |
                        crate::schema::DataType::Char(_) |
                        crate::schema::DataType::Date |
                        crate::schema::DataType::Time |
                        crate::schema::DataType::DateTime |
                        crate::schema::DataType::Timestamp |
                        crate::schema::DataType::Json |
                        crate::schema::DataType::Jsonb |
                        crate::schema::DataType::Uuid |
                        crate::schema::DataType::Xml |
                        crate::schema::DataType::Array(_) |
                        crate::schema::DataType::Inet |
                        crate::schema::DataType::MacAddr |
                        crate::schema::DataType::Custom(_) => "TEXT",
                        crate::schema::DataType::Blob |
                        crate::schema::DataType::Binary(_) => "BLOB",
                    };
                    format!("CAST(`{col}` AS {cast_type}) AS `{col}`")
                } else {
                    format!("`{col}`")
                }
            })
            .collect::<Vec<_>>()
            .join(", ");

        sqlx::query(&format!("INSERT INTO {temp_table} SELECT {column_list} FROM {table_name}"))
            .execute(&mut *tx)
            .await
            .map_err(SqlxDatabaseError::Sqlx)?;

        // Step 8: Drop old table
        sqlx::query(&format!("DROP TABLE {table_name}"))
            .execute(&mut *tx)
            .await
            .map_err(SqlxDatabaseError::Sqlx)?;

        // Step 9: Rename temp table to original name
        sqlx::query(&format!("ALTER TABLE {temp_table} RENAME TO {table_name}"))
            .execute(&mut *tx)
            .await
            .map_err(SqlxDatabaseError::Sqlx)?;

        // Step 10: Recreate schema objects
        for schema_sql in schema_objects {
            // Skip auto-indexes and internal indexes
            if !schema_sql.to_uppercase().contains("AUTOINDEX") {
                sqlx::query(&schema_sql)
                    .execute(&mut *tx)
                    .await
                    .map_err(SqlxDatabaseError::Sqlx)?;
            }
        }

        // Step 11: Re-enable foreign keys if they were enabled
        if foreign_keys_enabled == 1 {
            sqlx::query("PRAGMA foreign_keys=ON")
                .execute(&mut *tx)
                .await
                .map_err(SqlxDatabaseError::Sqlx)?;

            // Step 12: Check foreign key integrity
            let fk_violations: Vec<String> = sqlx::query_scalar("PRAGMA foreign_key_check")
                .fetch_all(&mut *tx)
                .await
                .map_err(SqlxDatabaseError::Sqlx)?;

            if !fk_violations.is_empty() {
                return Err(SqlxDatabaseError::Sqlx(sqlx::Error::TypeNotFound {
                    type_name: "Foreign key violations detected after table recreation".to_string(),
                }));
            }
        }

        Ok::<(), SqlxDatabaseError>(())
    }.await;

    match result {
        Ok(()) => {
            tx.commit().await.map_err(SqlxDatabaseError::Sqlx)?;
            Ok(())
        }
        Err(e) => {
            let _ = tx.rollback().await;
            Err(e)
        }
    }
}

fn to_values(values: &[(&str, DatabaseValue)]) -> Vec<SqliteDatabaseValue> {
    values
        .iter()
        .map(|(_key, value)| value.clone().into())
        .collect::<Vec<_>>()
}

fn exprs_to_values(values: &[(&str, Box<dyn Expression>)]) -> Vec<SqliteDatabaseValue> {
    values
        .iter()
        .flat_map(|value| value.1.values().into_iter())
        .flatten()
        .cloned()
        .map(std::convert::Into::into)
        .collect::<Vec<_>>()
}

fn bexprs_to_values(values: &[Box<dyn BooleanExpression>]) -> Vec<SqliteDatabaseValue> {
    values
        .iter()
        .flat_map(|value| value.values().into_iter())
        .flatten()
        .cloned()
        .map(std::convert::Into::into)
        .collect::<Vec<_>>()
}

#[allow(unused)]
fn to_values_opt(values: Option<&[(&str, DatabaseValue)]>) -> Option<Vec<SqliteDatabaseValue>> {
    values.map(to_values)
}

#[allow(unused)]
fn exprs_to_values_opt(
    values: Option<&[(&str, Box<dyn Expression>)]>,
) -> Option<Vec<SqliteDatabaseValue>> {
    values.map(exprs_to_values)
}

fn bexprs_to_values_opt(
    values: Option<&[Box<dyn BooleanExpression>]>,
) -> Option<Vec<SqliteDatabaseValue>> {
    values.map(bexprs_to_values)
}

#[allow(clippy::too_many_arguments)]
async fn select(
    connection: &mut SqliteConnection,
    table_name: &str,
    distinct: bool,
    columns: &[&str],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    joins: Option<&[Join<'_>]>,
    sort: Option<&[Sort]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, SqlxDatabaseError> {
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

    to_rows(&column_names, query.fetch(connection)).await
}

async fn delete(
    connection: &mut SqliteConnection,
    table_name: &str,
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, SqlxDatabaseError> {
    let index = AtomicU16::new(0);

    let select_query = limit.map(|_| {
        let where_clause = build_where_clause(filters, &index);
        format!("SELECT rowid FROM {table_name} {where_clause}")
    });

    let query = format!(
        "DELETE FROM {table_name} {} RETURNING *",
        build_update_where_clause(filters, limit, select_query.as_deref(), &index),
    );

    let mut all_filter_values: Vec<SqliteDatabaseValue> = filters
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
            .filter_map(crate::query::Expression::params)
            .collect::<Vec<_>>()
    );

    let statement = connection.prepare(&query).await?;
    let column_names = statement
        .columns()
        .iter()
        .map(|x| x.name().to_string())
        .collect::<Vec<_>>();

    let query = bind_values(statement.query(), Some(&all_filter_values))?;

    to_rows(&column_names, query.fetch(connection)).await
}

async fn find_row(
    connection: &mut SqliteConnection,
    table_name: &str,
    distinct: bool,
    columns: &[&str],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    joins: Option<&[Join<'_>]>,
    sort: Option<&[Sort]>,
) -> Result<Option<crate::Row>, SqlxDatabaseError> {
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

    query
        .next()
        .await
        .transpose()?
        .map(|row| from_row(&column_names, &row))
        .transpose()
}

async fn insert_and_get_row(
    connection: &mut SqliteConnection,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
) -> Result<crate::Row, SqlxDatabaseError> {
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

    stream
        .next()
        .await
        .transpose()?
        .map(|row| from_row(&column_names, &row))
        .ok_or(SqlxDatabaseError::NoRow)?
}

/// # Errors
///
/// Will return `Err` if the update multi execution failed.
pub async fn update_multi(
    connection: &mut SqliteConnection,
    table_name: &str,
    values: &[Vec<(&str, Box<dyn Expression>)>],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    mut limit: Option<usize>,
) -> Result<Vec<crate::Row>, SqlxDatabaseError> {
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
    connection: &mut SqliteConnection,
    table_name: &str,
    values: &[Vec<(&str, Box<dyn Expression>)>],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, SqlxDatabaseError> {
    let first = values[0].as_slice();
    let expected_value_size = first.len();

    if let Some(bad_row) = values.iter().skip(1).find(|v| {
        v.len() != expected_value_size || v.iter().enumerate().any(|(i, c)| c.0 != first[i].0)
    }) {
        log::error!("Bad row: {bad_row:?}. Expected to match schema of first row: {first:?}");
        return Err(SqlxDatabaseError::InvalidRequest);
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
                        "SELECT rowid FROM {table_name} {}",
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

    to_rows(&column_names, query.fetch(connection)).await
}

/// # Errors
///
/// Will return `Err` if the upsert multi execution failed.
pub async fn upsert_multi(
    connection: &mut SqliteConnection,
    table_name: &str,
    unique: &[Box<dyn Expression>],
    values: &[Vec<(&str, Box<dyn Expression>)>],
) -> Result<Vec<crate::Row>, SqlxDatabaseError> {
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
    connection: &mut SqliteConnection,
    table_name: &str,
    unique: &[Box<dyn Expression>],
    values: &[Vec<(&str, Box<dyn Expression>)>],
) -> Result<Vec<crate::Row>, SqlxDatabaseError> {
    let first = values[0].as_slice();
    let expected_value_size = first.len();

    if let Some(bad_row) = values.iter().skip(1).find(|v| {
        v.len() != expected_value_size || v.iter().enumerate().any(|(i, c)| c.0 != first[i].0)
    }) {
        log::error!("Bad row: {bad_row:?}. Expected to match schema of first row: {first:?}");
        return Err(SqlxDatabaseError::InvalidRequest);
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

    to_rows(&column_names, query.fetch(connection)).await
}

async fn upsert(
    connection: &mut SqliteConnection,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, SqlxDatabaseError> {
    let rows = update_and_get_rows(connection, table_name, values, filters, limit).await?;

    Ok(if rows.is_empty() {
        vec![insert_and_get_row(connection, table_name, values).await?]
    } else {
        rows
    })
}

async fn upsert_and_get_row(
    connection: &mut SqliteConnection,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<crate::Row, SqlxDatabaseError> {
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

/// Wrapper type for converting `DatabaseValue` to `SQLite` `SQLx`-specific parameter types
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone)]
pub struct SqliteDatabaseValue(DatabaseValue);

impl From<DatabaseValue> for SqliteDatabaseValue {
    fn from(value: DatabaseValue) -> Self {
        Self(value)
    }
}

impl Deref for SqliteDatabaseValue {
    type Target = DatabaseValue;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Expression for SqliteDatabaseValue {
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

#[async_trait]
impl Database for SqliteSqlxTransaction {
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

        Ok(upsert_multi(
            &mut *tx,
            statement.table_name,
            statement.unique.as_ref().ok_or(DatabaseError::NoRow)?,
            &statement.values,
        )
        .await?)
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn exec_raw(&self, sql: &str) -> Result<(), DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        tx.execute(sqlx::raw_sql(sql))
            .await
            .map_err(SqlxDatabaseError::Sqlx)?;

        Ok(())
    }

    #[allow(clippy::significant_drop_tightening)]
    #[cfg(feature = "schema")]
    async fn exec_create_table(
        &self,
        statement: &crate::schema::CreateTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        sqlite_sqlx_exec_create_table(&mut *tx, statement)
            .await
            .map_err(Into::into)
    }

    #[allow(clippy::significant_drop_tightening)]
    #[cfg(feature = "schema")]
    async fn exec_drop_table(
        &self,
        statement: &crate::schema::DropTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        sqlite_sqlx_exec_drop_table(&mut *tx, statement)
            .await
            .map_err(Into::into)
    }

    #[allow(clippy::significant_drop_tightening)]
    #[cfg(feature = "schema")]
    async fn exec_create_index(
        &self,
        statement: &crate::schema::CreateIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        sqlite_sqlx_exec_create_index(&mut *tx, statement)
            .await
            .map_err(Into::into)
    }

    #[allow(clippy::significant_drop_tightening)]
    #[cfg(feature = "schema")]
    async fn exec_drop_index(
        &self,
        statement: &crate::schema::DropIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        sqlite_sqlx_exec_drop_index(&mut *tx, statement)
            .await
            .map_err(Into::into)
    }

    #[allow(clippy::significant_drop_tightening)]
    #[cfg(feature = "schema")]
    async fn exec_alter_table(
        &self,
        statement: &crate::schema::AlterTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        sqlite_sqlx_exec_alter_table(&mut *tx, statement)
            .await
            .map_err(Into::into)
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::significant_drop_tightening)]
    async fn table_exists(&self, table_name: &str) -> Result<bool, DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        sqlx_sqlite_table_exists(&mut *tx, table_name)
            .await
            .map_err(Into::into)
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::significant_drop_tightening)]
    async fn list_tables(&self) -> Result<Vec<String>, DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        sqlx_sqlite_list_tables(&mut *tx).await.map_err(Into::into)
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::significant_drop_tightening)]
    async fn get_table_info(
        &self,
        table_name: &str,
    ) -> Result<Option<crate::schema::TableInfo>, DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        sqlx_sqlite_get_table_info(&mut *tx, table_name)
            .await
            .map_err(Into::into)
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::significant_drop_tightening)]
    async fn get_table_columns(
        &self,
        table_name: &str,
    ) -> Result<Vec<crate::schema::ColumnInfo>, DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;

        sqlx_sqlite_get_table_columns(&mut *tx, table_name)
            .await
            .map_err(Into::into)
    }

    #[cfg(feature = "schema")]
    async fn column_exists(
        &self,
        table_name: &str,
        column_name: &str,
    ) -> Result<bool, DatabaseError> {
        let columns = self.get_table_columns(table_name).await?;
        Ok(columns.iter().any(|col| col.name == column_name))
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

        // Use existing from_row helper for each row
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
        let mut query_builder: Query<'_, Sqlite, SqliteArguments> = sqlx::query(query);

        // Add parameters in order - SQLite uses ? placeholders
        for param in params {
            query_builder = match param {
                crate::DatabaseValue::Int8(n) => query_builder.bind(i64::from(*n)),
                crate::DatabaseValue::Int8Opt(n) => query_builder.bind(n.map(i64::from)),
                crate::DatabaseValue::Int16(n) => query_builder.bind(i64::from(*n)),
                crate::DatabaseValue::Int16Opt(n) => query_builder.bind(n.map(i64::from)),
                crate::DatabaseValue::Int32(n) => query_builder.bind(i64::from(*n)),
                crate::DatabaseValue::Int32Opt(n) => query_builder.bind(n.map(i64::from)),
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
                crate::DatabaseValue::Real32(r) => query_builder.bind(f64::from(*r)),
                crate::DatabaseValue::Real32Opt(r) => query_builder.bind(r.map(f64::from)),
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::Decimal(d) => query_builder.bind(d.to_string()),
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::DecimalOpt(d) => {
                    query_builder.bind(d.as_ref().map(ToString::to_string))
                }
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::Uuid(u) => query_builder.bind(u.to_string()),
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::UuidOpt(u) => {
                    query_builder.bind(u.as_ref().map(ToString::to_string))
                }
                crate::DatabaseValue::Bool(b) => query_builder.bind(*b),
                crate::DatabaseValue::BoolOpt(b) => query_builder.bind(b),
                crate::DatabaseValue::DateTime(dt) => query_builder.bind(dt.to_string()),
                crate::DatabaseValue::Null => query_builder.bind(Option::<String>::None),
                crate::DatabaseValue::Now => query_builder.bind("datetime('now')"),
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
        let mut query_builder: Query<'_, Sqlite, SqliteArguments> = sqlx::query(query);

        // Add parameters in order - SQLite uses ? placeholders
        for param in params {
            query_builder = match param {
                crate::DatabaseValue::Int8(n) => query_builder.bind(i64::from(*n)),
                crate::DatabaseValue::Int8Opt(n) => query_builder.bind(n.map(i64::from)),
                crate::DatabaseValue::Int16(n) => query_builder.bind(i64::from(*n)),
                crate::DatabaseValue::Int16Opt(n) => query_builder.bind(n.map(i64::from)),
                crate::DatabaseValue::Int32(n) => query_builder.bind(i64::from(*n)),
                crate::DatabaseValue::Int32Opt(n) => query_builder.bind(n.map(i64::from)),
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
                crate::DatabaseValue::Real32(r) => query_builder.bind(f64::from(*r)),
                crate::DatabaseValue::Real32Opt(r) => query_builder.bind(r.map(f64::from)),
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::Decimal(d) => query_builder.bind(d.to_string()),
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::DecimalOpt(d) => {
                    query_builder.bind(d.as_ref().map(ToString::to_string))
                }
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::Uuid(u) => query_builder.bind(u.to_string()),
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::UuidOpt(u) => {
                    query_builder.bind(u.as_ref().map(ToString::to_string))
                }
                crate::DatabaseValue::Bool(b) => query_builder.bind(*b),
                crate::DatabaseValue::BoolOpt(b) => query_builder.bind(b),
                crate::DatabaseValue::DateTime(dt) => query_builder.bind(dt.to_string()),
                crate::DatabaseValue::Null => query_builder.bind(Option::<String>::None),
                crate::DatabaseValue::Now => query_builder.bind("datetime('now')"),
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

struct SqliteSqlxSavepoint {
    name: String,
    transaction: Arc<Mutex<Option<Transaction<'static, Sqlite>>>>,
    released: AtomicBool,
    rolled_back: AtomicBool,
}

#[async_trait]
impl crate::Savepoint for SqliteSqlxSavepoint {
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
impl DatabaseTransaction for SqliteSqlxTransaction {
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
            return Err(DatabaseError::TransactionRolledBack);
        }

        Ok(Box::new(SqliteSqlxSavepoint {
            name: name.to_string(),
            transaction: Arc::clone(&self.transaction),
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
// SQLite introspection helper functions (Phase 16.4)

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
async fn sqlx_sqlite_table_exists(
    executor: &mut SqliteConnection,
    table_name: &str,
) -> Result<bool, SqlxDatabaseError> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?")
            .bind(table_name)
            .fetch_one(executor)
            .await?;

    Ok(count > 0)
}

#[cfg(feature = "schema")]
async fn sqlx_sqlite_list_tables(
    executor: &mut SqliteConnection,
) -> Result<Vec<String>, SqlxDatabaseError> {
    let rows = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_all(executor)
    .await?;

    let mut tables = Vec::new();
    for row in rows {
        let table_name: String = row.get("name");
        tables.push(table_name);
    }

    Ok(tables)
}

#[cfg(feature = "schema")]
async fn sqlx_sqlite_get_table_columns(
    executor: &mut SqliteConnection,
    table_name: &str,
) -> Result<Vec<crate::schema::ColumnInfo>, SqlxDatabaseError> {
    let mut columns = Vec::new();

    let pragma_query = format!("PRAGMA table_info({table_name})");
    let rows = sqlx::query(&pragma_query).fetch_all(&mut *executor).await?;

    for row in rows {
        let cid: i32 = row.get(0);
        let name: String = row.get(1);
        let type_str: String = row.get(2);
        let notnull: i32 = row.get(3);
        let dflt_value: Option<String> = row.get(4);
        let pk: i32 = row.get(5);

        let data_type = sqlite_type_to_data_type(&type_str);

        let default_value = parse_default_value(dflt_value);

        let auto_increment = if pk > 0 {
            // Check if this column has AUTOINCREMENT in the CREATE TABLE statement
            check_sqlite_sqlx_autoincrement(executor, table_name, &name).await?
        } else {
            false
        };

        columns.push(crate::schema::ColumnInfo {
            name,
            data_type,
            nullable: notnull == 0,
            is_primary_key: pk > 0,
            auto_increment,
            default_value,
            ordinal_position: u32::try_from(cid + 1).unwrap_or(1), // Convert 0-based cid to 1-based ordinal
        });
    }

    Ok(columns)
}

#[cfg(feature = "schema")]
async fn check_sqlite_sqlx_autoincrement(
    executor: &mut SqliteConnection,
    table_name: &str,
    column_name: &str,
) -> Result<bool, SqlxDatabaseError> {
    // Query the CREATE TABLE statement from sqlite_master
    let sql: Option<String> =
        sqlx::query_scalar("SELECT sql FROM sqlite_master WHERE type='table' AND name=?")
            .bind(table_name)
            .fetch_optional(executor)
            .await
            .map_err(SqlxDatabaseError::Sqlx)?;

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
async fn sqlx_sqlite_get_table_info(
    executor: &mut SqliteConnection,
    table_name: &str,
) -> Result<Option<crate::schema::TableInfo>, SqlxDatabaseError> {
    use std::collections::BTreeMap;

    // First check if table exists
    if !sqlx_sqlite_table_exists(executor, table_name).await? {
        return Ok(None);
    }

    // Get columns
    let columns = sqlx_sqlite_get_table_columns(executor, table_name).await?;
    let mut column_map = BTreeMap::new();
    for column in columns {
        column_map.insert(column.name.clone(), column);
    }

    // Get indexes
    let mut indexes = BTreeMap::new();
    let index_query = format!("PRAGMA index_list({table_name})");
    let index_rows = sqlx::query(&index_query).fetch_all(&mut *executor).await?;

    for row in index_rows {
        let index_name: String = row.get(1);
        let unique: i32 = row.get(2);
        let origin: String = row.get(3);

        // Get index columns
        let index_info_query = format!("PRAGMA index_info({index_name})");
        let column_rows = sqlx::query(&index_info_query)
            .fetch_all(&mut *executor)
            .await?;

        let mut index_columns = Vec::new();
        for col_row in column_rows {
            let column_name: String = col_row.get(2);
            index_columns.push(column_name);
        }

        indexes.insert(
            index_name.clone(),
            crate::schema::IndexInfo {
                name: index_name,
                unique: unique == 1,
                columns: index_columns,
                is_primary: origin == "pk",
            },
        );
    }

    // Get foreign keys
    let mut foreign_keys = BTreeMap::new();
    let fk_query = format!("PRAGMA foreign_key_list({table_name})");
    let fk_rows = sqlx::query(&fk_query).fetch_all(&mut *executor).await?;

    for row in fk_rows {
        let id: i32 = row.get(0);
        let _seq: i32 = row.get(1);
        let table: String = row.get(2);
        let from: String = row.get(3);
        let to: String = row.get(4);
        let on_update: String = row.get(5);
        let on_delete: String = row.get(6);

        let constraint_name = format!("fk_{table_name}_{id}");
        foreign_keys.insert(
            constraint_name.clone(),
            crate::schema::ForeignKeyInfo {
                name: constraint_name,
                column: from,
                referenced_table: table,
                referenced_column: to,
                on_update: Some(on_update),
                on_delete: Some(on_delete),
            },
        );
    }

    Ok(Some(crate::schema::TableInfo {
        name: table_name.to_string(),
        columns: column_map,
        indexes,
        foreign_keys,
    }))
}

#[cfg(all(test, feature = "schema"))]
mod introspection_tests {
    use super::*;
    use crate::schema::DataType;
    use std::sync::Arc;

    async fn create_sqlx_introspection_test_db() -> SqliteSqlxDatabase {
        let db_url = "sqlite::memory:".to_string();

        let pool = SqlitePool::connect(&db_url)
            .await
            .expect("Failed to create SQLite pool");

        // Create a comprehensive test schema matching rusqlite tests
        sqlx::query(
            "CREATE TABLE users (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                email TEXT UNIQUE,
                age INTEGER,
                is_active BOOLEAN DEFAULT 1,
                balance REAL DEFAULT 0.0,
                created_at TEXT
            )",
        )
        .execute(&pool)
        .await
        .expect("Failed to create users table");

        sqlx::query(
            "CREATE TABLE posts (
                id INTEGER PRIMARY KEY,
                user_id INTEGER NOT NULL,
                title TEXT NOT NULL,
                content TEXT,
                FOREIGN KEY (user_id) REFERENCES users (id)
            )",
        )
        .execute(&pool)
        .await
        .expect("Failed to create posts table");

        sqlx::query("CREATE INDEX idx_users_email ON users (email)")
            .execute(&pool)
            .await
            .expect("Failed to create index");

        sqlx::query("CREATE UNIQUE INDEX idx_users_name_unique ON users (name)")
            .execute(&pool)
            .await
            .expect("Failed to create unique index");

        // Add table with unsupported types for testing
        sqlx::query(
            "CREATE TABLE unsupported_types (
                id INTEGER PRIMARY KEY,
                blob_data BLOB,
                other_data CUSTOM_TYPE
            )",
        )
        .execute(&pool)
        .await
        .expect("Failed to create unsupported_types table");

        SqliteSqlxDatabase::new(Arc::new(Mutex::new(pool)))
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test(no_simulator)]
    async fn test_sqlx_sqlite_table_exists() {
        let db = create_sqlx_introspection_test_db().await;

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
    #[switchy_async::test(no_simulator)]
    async fn test_sqlx_sqlite_list_tables() {
        let db = create_sqlx_introspection_test_db().await;

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
        assert!(
            tables.contains(&"unsupported_types".to_string()),
            "Should contain unsupported_types table"
        );

        // Should not contain SQLite internal tables
        for table in &tables {
            assert!(
                !table.starts_with("sqlite_"),
                "Should not contain SQLite internal table: {table}"
            );
        }

        // Should have exactly 3 tables (users, posts, unsupported_types)
        assert_eq!(tables.len(), 3, "Should have exactly 3 tables");

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

        // Should now contain 4 tables
        assert_eq!(tables_in_tx.len(), 4, "Should have 4 tables in transaction");
        assert!(tables_in_tx.contains(&"temp_table".to_string()));

        tx.rollback().await.expect("Failed to rollback");

        // After rollback, should be back to 3 tables
        let tables_after_rollback = db
            .list_tables()
            .await
            .expect("Failed to list tables after rollback");
        assert_eq!(
            tables_after_rollback.len(),
            3,
            "Should be back to 3 tables after rollback"
        );
        assert!(!tables_after_rollback.contains(&"temp_table".to_string()));
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test(no_simulator)]
    async fn test_sqlx_sqlite_list_tables_empty() {
        let db_url = "sqlite::memory:".to_string();
        let pool = SqlitePool::connect(&db_url)
            .await
            .expect("Failed to create SQLite pool");

        let db = SqliteSqlxDatabase::new(Arc::new(tokio::sync::Mutex::new(pool)));

        let tables = db.list_tables().await.expect("Failed to list tables");

        assert!(tables.is_empty(), "Empty database should have no tables");
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test(no_simulator)]
    async fn test_sqlx_sqlite_list_tables_after_create_drop() {
        let db_url = "sqlite::memory:".to_string();
        let pool = SqlitePool::connect(&db_url)
            .await
            .expect("Failed to create SQLite pool");

        let db = SqliteSqlxDatabase::new(Arc::new(tokio::sync::Mutex::new(pool)));

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
    #[switchy_async::test(no_simulator)]
    async fn test_sqlx_sqlite_column_exists() {
        let db = create_sqlx_introspection_test_db().await;

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
    #[switchy_async::test(no_simulator)]
    async fn test_sqlx_sqlite_get_table_columns() {
        let db = create_sqlx_introspection_test_db().await;

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
        assert_eq!(id_col.data_type, DataType::BigInt);
        assert!(
            id_col.nullable,
            "id should be nullable (SQLite PRIMARY KEY without NOT NULL)"
        );
        assert!(id_col.is_primary_key, "id should be primary key");

        let name_col = columns
            .iter()
            .find(|c| c.name == "name")
            .expect("name column should exist");
        assert_eq!(name_col.data_type, DataType::Text);
        assert!(!name_col.nullable, "name should not be nullable");
        assert!(!name_col.is_primary_key, "name should not be primary key");

        let email_col = columns
            .iter()
            .find(|c| c.name == "email")
            .expect("email column should exist");
        assert_eq!(email_col.data_type, DataType::Text);
        assert!(email_col.nullable, "email should be nullable");

        // Test ordinal positions
        let sorted_columns: Vec<_> = columns.iter().collect();
        for (i, column) in sorted_columns.iter().enumerate() {
            assert_eq!(
                column.ordinal_position,
                u32::try_from(i + 1).unwrap(),
                "Column {} should have ordinal position {}",
                column.name,
                i + 1
            );
        }

        // Test with non-existing table
        let columns = db
            .get_table_columns("nonexistent")
            .await
            .expect("Should succeed for nonexistent table");
        assert!(
            columns.is_empty(),
            "Should return empty for nonexistent table"
        );
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test(no_simulator)]
    async fn test_sqlx_sqlite_get_table_info() {
        let db = create_sqlx_introspection_test_db().await;

        // Test users table
        let table_info = db
            .get_table_info("users")
            .await
            .expect("Failed to get table info")
            .expect("users table should exist");

        assert_eq!(table_info.name, "users");
        assert!(!table_info.columns.is_empty(), "Should have columns");
        assert!(
            table_info.columns.contains_key("id"),
            "Should have id column"
        );
        assert!(
            table_info.columns.contains_key("name"),
            "Should have name column"
        );

        // Check indexes (should include both explicit indexes and automatic primary key)
        assert!(!table_info.indexes.is_empty(), "Should have indexes");

        // Should have the unique index we created
        let has_email_index = table_info
            .indexes
            .values()
            .any(|idx| idx.columns.contains(&"email".to_string()));
        assert!(has_email_index, "Should have email index");

        // Test non-existing table
        let table_info = db
            .get_table_info("nonexistent")
            .await
            .expect("Should succeed for nonexistent table");
        assert!(
            table_info.is_none(),
            "Should return None for nonexistent table"
        );

        // Test with transaction
        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");
        let table_info = tx
            .get_table_info("posts")
            .await
            .expect("Failed to get table info in transaction")
            .expect("posts table should exist");
        assert_eq!(table_info.name, "posts");
        tx.rollback().await.expect("Failed to rollback");
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test(no_simulator)]
    async fn test_sqlx_sqlite_unsupported_types() {
        let db = create_sqlx_introspection_test_db().await;

        // Test table with unsupported types
        let result = db.get_table_columns("unsupported_types").await;

        // Should now succeed with Custom DataType fallback
        assert!(
            result.is_ok(),
            "Should succeed with Custom DataType fallback"
        );

        let columns = result.unwrap();
        assert!(!columns.is_empty(), "Should have columns");

        // Check that unsupported types become Custom DataType
        for column in &columns {
            match &column.data_type {
                DataType::Text
                | DataType::VarChar(_)
                | DataType::Char(_)
                | DataType::TinyInt
                | DataType::SmallInt
                | DataType::Int
                | DataType::BigInt
                | DataType::Serial
                | DataType::BigSerial
                | DataType::Real
                | DataType::Double
                | DataType::Decimal(_, _)
                | DataType::Money
                | DataType::Bool
                | DataType::Date
                | DataType::Time
                | DataType::DateTime
                | DataType::Timestamp
                | DataType::Blob
                | DataType::Binary(_)
                | DataType::Json
                | DataType::Jsonb
                | DataType::Uuid
                | DataType::Xml
                | DataType::Array(_)
                | DataType::Inet
                | DataType::MacAddr => {}
                DataType::Custom(type_name) => {
                    // Custom types should be preserved
                    assert!(
                        type_name == "CUSTOM_TYPE" || type_name == "UNKNOWN_TYPE",
                        "Unexpected custom type: {type_name}"
                    );
                }
            }
        }
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test(no_simulator)]
    async fn test_sqlx_sqlite_transaction_context() {
        let db = create_sqlx_introspection_test_db().await;

        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        // All introspection methods should work in transaction context
        let exists = tx
            .table_exists("users")
            .await
            .expect("table_exists should work in transaction");
        assert!(exists);

        let columns = tx
            .get_table_columns("users")
            .await
            .expect("get_table_columns should work in transaction");
        assert!(!columns.is_empty());

        let table_info = tx
            .get_table_info("users")
            .await
            .expect("get_table_info should work in transaction");
        assert!(table_info.is_some());

        let col_exists = tx
            .column_exists("users", "name")
            .await
            .expect("column_exists should work in transaction");
        assert!(col_exists);

        tx.rollback().await.expect("Failed to rollback");
    }
}

fn sqlite_transform_query_for_params(
    query: &str,
    params: &[DatabaseValue],
) -> Result<(String, Vec<DatabaseValue>), DatabaseError> {
    transform_query_for_params(query, params, &QuestionMarkHandler, |param| match param {
        DatabaseValue::Now => Some("datetime('now')".to_string()),
        DatabaseValue::NowPlus(interval) => {
            let modifiers = format_sqlite_interval_sqlx(interval);
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
mod savepoint_tests {
    use super::*;

    async fn create_test_db() -> SqliteSqlxDatabase {
        let db_url = "sqlite::memory:".to_string();
        let pool = SqlitePool::connect(&db_url)
            .await
            .expect("Failed to create SQLite pool");

        sqlx::query("CREATE TABLE test_table (id INTEGER, value TEXT)")
            .execute(&pool)
            .await
            .expect("Failed to create test table");

        SqliteSqlxDatabase::new(Arc::new(Mutex::new(pool)))
    }

    #[switchy_async::test(no_simulator)]
    async fn test_basic_savepoint() {
        let db = create_test_db().await;
        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        // Create a savepoint
        let savepoint = tx
            .savepoint("test_savepoint")
            .await
            .expect("Failed to create savepoint");

        // Verify savepoint name
        assert_eq!(savepoint.name(), "test_savepoint");

        // Release the savepoint
        savepoint
            .release()
            .await
            .expect("Failed to release savepoint");

        tx.rollback().await.expect("Failed to rollback transaction");
    }

    #[switchy_async::test(no_simulator)]
    async fn test_savepoint_release() {
        let db = create_test_db().await;
        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        // Insert initial data
        tx.exec_raw("INSERT INTO test_table (id, value) VALUES (1, 'initial')")
            .await
            .expect("Failed to insert");

        // Create savepoint
        let savepoint = tx
            .savepoint("sp1")
            .await
            .expect("Failed to create savepoint");

        // Insert more data
        tx.exec_raw("INSERT INTO test_table (id, value) VALUES (2, 'after_savepoint')")
            .await
            .expect("Failed to insert");

        // Release savepoint (commits savepoint changes)
        savepoint
            .release()
            .await
            .expect("Failed to release savepoint");

        // Data should still be there after release
        tx.rollback().await.expect("Failed to rollback transaction");
    }

    #[switchy_async::test(no_simulator)]
    async fn test_savepoint_rollback() {
        let db = create_test_db().await;
        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        // Insert initial data
        tx.exec_raw("INSERT INTO test_table (id, value) VALUES (1, 'initial')")
            .await
            .expect("Failed to insert");

        // Create savepoint
        let savepoint = tx
            .savepoint("sp1")
            .await
            .expect("Failed to create savepoint");

        // Insert more data
        tx.exec_raw("INSERT INTO test_table (id, value) VALUES (2, 'after_savepoint')")
            .await
            .expect("Failed to insert");

        // Rollback to savepoint
        savepoint
            .rollback_to()
            .await
            .expect("Failed to rollback to savepoint");

        tx.rollback().await.expect("Failed to rollback transaction");
    }

    #[switchy_async::test(no_simulator)]
    async fn test_savepoint_validation() {
        let db = create_test_db().await;
        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        // Test invalid savepoint names
        assert!(
            tx.savepoint("").await.is_err(),
            "Empty name should be invalid"
        );
        assert!(
            tx.savepoint("invalid name").await.is_err(),
            "Space in name should be invalid"
        );
        assert!(
            tx.savepoint("invalid;name").await.is_err(),
            "Semicolon in name should be invalid"
        );

        // Test double release
        let savepoint = tx
            .savepoint("valid_name")
            .await
            .expect("Failed to create savepoint");
        savepoint
            .release()
            .await
            .expect("Failed to release savepoint");

        // Test double rollback
        let savepoint2 = tx
            .savepoint("valid_name2")
            .await
            .expect("Failed to create savepoint");
        savepoint2
            .rollback_to()
            .await
            .expect("Failed to rollback to savepoint");

        tx.rollback().await.expect("Failed to rollback transaction");
    }

    #[switchy_async::test(no_simulator)]
    async fn test_savepoint_after_transaction_commit() {
        let db = create_test_db().await;
        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        // Create savepoint
        let savepoint = tx
            .savepoint("sp1")
            .await
            .expect("Failed to create savepoint");

        // Commit the transaction
        tx.commit().await.expect("Failed to commit transaction");

        // Now try to release the savepoint - should error
        let result = savepoint.release().await;
        assert!(
            result.is_err(),
            "Release should fail after transaction commit"
        );

        match result.unwrap_err() {
            DatabaseError::TransactionCommitted => {} // Expected
            other => panic!("Expected TransactionCommitted, got: {other:?}"),
        }
    }

    #[switchy_async::test(no_simulator)]
    async fn test_savepoint_after_transaction_rollback() {
        let db = create_test_db().await;
        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        // Create savepoint
        let savepoint = tx
            .savepoint("sp1")
            .await
            .expect("Failed to create savepoint");

        // Rollback the transaction
        tx.rollback().await.expect("Failed to rollback transaction");

        // Now try to rollback to the savepoint - should error
        let result = savepoint.rollback_to().await;
        assert!(
            result.is_err(),
            "Rollback should fail after transaction rollback"
        );

        match result.unwrap_err() {
            DatabaseError::TransactionCommitted => {} // Expected (same error for both cases)
            other => panic!("Expected TransactionCommitted, got: {other:?}"),
        }
    }
}
