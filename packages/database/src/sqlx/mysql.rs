use std::{
    ops::Deref,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use sqlx::{
    Column, Executor, MySql, MySqlConnection, MySqlPool, Row, Statement, Transaction, TypeInfo,
    Value, ValueRef,
    mysql::{MySqlArguments, MySqlRow, MySqlValueRef},
    query::Query,
};
use thiserror::Error;
use tokio::sync::Mutex;

use crate::{
    Database, DatabaseError, DatabaseValue, DeleteStatement, InsertStatement, SelectQuery,
    UpdateStatement, UpsertMultiStatement, UpsertStatement,
    query::{BooleanExpression, Expression, ExpressionType, Join, Sort, SortDirection},
    query_transform::{QuestionMarkHandler, transform_query_for_params},
    sql_interval::SqlInterval,
};

trait ToSql {
    fn to_sql(&self) -> String;
}

/// Format `SqlInterval` as `MySQL` INTERVAL expressions
/// Returns multiple INTERVAL expressions for chaining with `DATE_ADD`
fn format_mysql_intervals(interval: &SqlInterval) -> Vec<String> {
    let mut intervals = Vec::new();

    if interval.years != 0 {
        intervals.push(format!("INTERVAL {} YEAR", interval.years));
    }
    if interval.months != 0 {
        intervals.push(format!("INTERVAL {} MONTH", interval.months));
    }
    if interval.days != 0 {
        intervals.push(format!("INTERVAL {} DAY", interval.days));
    }
    if interval.hours != 0 {
        intervals.push(format!("INTERVAL {} HOUR", interval.hours));
    }
    if interval.minutes != 0 {
        intervals.push(format!("INTERVAL {} MINUTE", interval.minutes));
    }
    if interval.seconds != 0 {
        intervals.push(format!("INTERVAL {} SECOND", interval.seconds));
    }
    if interval.nanos != 0 {
        let microseconds = interval.nanos / 1000; // MySQL supports microsecond precision
        if microseconds > 0 {
            intervals.push(format!("INTERVAL {microseconds} MICROSECOND"));
        }
    }

    if intervals.is_empty() {
        vec!["INTERVAL 0 SECOND".to_string()]
    } else {
        intervals
    }
}

/// Generate `MySQL` expression for `NOW()` + intervals
fn format_mysql_now_plus(interval: &SqlInterval) -> String {
    if interval.is_zero() {
        return "NOW()".to_string();
    }

    let intervals = format_mysql_intervals(interval);
    let mut expr = "NOW()".to_string();

    for interval_expr in intervals {
        expr = format!("DATE_ADD({expr}, {interval_expr})");
    }

    expr
}

trait ToParam {
    fn to_param(&self) -> String;
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
                | DatabaseValue::Int64Opt(None)
                | DatabaseValue::UInt64Opt(None)
                | DatabaseValue::Real64Opt(None)
                | DatabaseValue::Real32Opt(None) => "NULL".to_string(),
                #[cfg(feature = "decimal")]
                DatabaseValue::DecimalOpt(None) => "NULL".to_string(),
                #[cfg(feature = "uuid")]
                DatabaseValue::UuidOpt(None) => "NULL".to_string(),
                DatabaseValue::Now => "NOW()".to_string(),
                DatabaseValue::NowPlus(interval) => format_mysql_now_plus(interval),
                _ => "?".to_string(),
            },
        }
    }
}

impl<T: Expression + ?Sized> ToParam for T {
    fn to_param(&self) -> String {
        self.to_sql()
    }
}

/// `MySQL` database transaction using `SQLx`
///
/// Represents an active transaction on a `MySQL` connection. Provides ACID guarantees
/// for a series of database operations. Must be explicitly committed or rolled back.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct MysqlSqlxTransaction {
    transaction: Arc<Mutex<Option<Transaction<'static, MySql>>>>,
}

impl MysqlSqlxTransaction {
    /// Creates a new `MySQL` transaction from an `SQLx` transaction
    #[must_use]
    pub fn new(transaction: Transaction<'static, MySql>) -> Self {
        Self {
            transaction: Arc::new(Mutex::new(Some(transaction))),
        }
    }
}

/// `MySQL` database connection pool using `SQLx`
///
/// Manages a pool of `MySQL` connections for efficient connection reuse
/// and concurrent query execution.
#[derive(Debug)]
pub struct MySqlSqlxDatabase {
    connection: Arc<Mutex<MySqlPool>>,
}

impl MySqlSqlxDatabase {
    /// Creates a new `MySQL` database instance from an `SQLx` connection pool
    pub const fn new(connection: Arc<Mutex<MySqlPool>>) -> Self {
        Self { connection }
    }
}

/// Errors specific to `MySQL` database operations using `SQLx`
///
/// Wraps errors from the underlying `SQLx` `MySQL` driver plus additional error types
/// for query validation and result handling.
#[derive(Debug, Error)]
pub enum SqlxDatabaseError {
    /// Error from the underlying `SQLx` `MySQL` driver
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
        Self::MysqlSqlx(value)
    }
}

/// Get column dependencies (indexes and foreign keys) for a specific column in `MySQL`
#[cfg(feature = "cascade")]
async fn mysql_get_column_dependencies(
    connection: &mut MySqlConnection,
    table_name: &str,
    column_name: &str,
) -> Result<(Vec<String>, Vec<String>), SqlxDatabaseError> {
    let mut indexes = Vec::new();
    let mut foreign_keys = Vec::new();

    // Find indexes that use this column
    let index_query = "
        SELECT DISTINCT INDEX_NAME
        FROM information_schema.STATISTICS
        WHERE TABLE_SCHEMA = DATABASE()
        AND TABLE_NAME = ?
        AND COLUMN_NAME = ?
        AND INDEX_NAME != 'PRIMARY'";

    let index_rows = sqlx::query(index_query)
        .bind(table_name)
        .bind(column_name)
        .fetch_all(&mut *connection)
        .await
        .map_err(SqlxDatabaseError::Sqlx)?;

    for row in index_rows {
        let index_name: String = row.try_get("INDEX_NAME").map_err(SqlxDatabaseError::Sqlx)?;
        indexes.push(index_name);
    }

    // Find foreign key constraints that reference this column
    let fk_query = "
        SELECT CONSTRAINT_NAME
        FROM information_schema.KEY_COLUMN_USAGE
        WHERE TABLE_SCHEMA = DATABASE()
        AND TABLE_NAME = ?
        AND COLUMN_NAME = ?
        AND REFERENCED_TABLE_NAME IS NOT NULL";

    let fk_rows = sqlx::query(fk_query)
        .bind(table_name)
        .bind(column_name)
        .fetch_all(&mut *connection)
        .await
        .map_err(SqlxDatabaseError::Sqlx)?;

    for row in fk_rows {
        let constraint_name: String = row
            .try_get("CONSTRAINT_NAME")
            .map_err(SqlxDatabaseError::Sqlx)?;
        foreign_keys.push(constraint_name);
    }

    Ok((indexes, foreign_keys))
}

#[async_trait]
#[allow(clippy::significant_drop_tightening)]
impl Database for MySqlSqlxDatabase {
    async fn query(&self, query: &SelectQuery<'_>) -> Result<Vec<crate::Row>, DatabaseError> {
        let pool = self.connection.lock().await;
        let mut connection = pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?;

        Ok(select(
            &mut connection,
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
        let pool = self.connection.lock().await;
        let mut connection = pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?;

        Ok(find_row(
            &mut connection,
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
        let pool = self.connection.lock().await;
        let mut connection = pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?;

        Ok(delete(
            &mut connection,
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
        let pool = self.connection.lock().await;
        let mut connection = pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?;

        Ok(delete(
            &mut connection,
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
        let pool = self.connection.lock().await;
        let mut connection = pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?;

        Ok(insert_and_get_row(&mut connection, statement.table_name, &statement.values).await?)
    }

    async fn exec_update(
        &self,
        statement: &UpdateStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        let pool = self.connection.lock().await;
        let mut connection = pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?;

        Ok(update_and_get_rows(
            &mut connection,
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
        let pool = self.connection.lock().await;
        let mut connection = pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?;

        Ok(update_and_get_row(
            &mut connection,
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
        let pool = self.connection.lock().await;
        let mut connection = pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?;

        Ok(upsert(
            &mut connection,
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
        let pool = self.connection.lock().await;
        let mut connection = pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?;

        Ok(upsert_and_get_row(
            &mut connection,
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
        let pool = self.connection.lock().await;
        let mut connection = pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?;

        let rows = {
            upsert_multi(
                &mut connection,
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

        let pool = self.connection.lock().await;
        let mut connection = pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?;

        connection
            .execute(statement)
            .await
            .map_err(SqlxDatabaseError::Sqlx)?;

        Ok(())
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::too_many_lines)]
    async fn exec_create_table(
        &self,
        statement: &crate::schema::CreateTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let pool = self.connection.lock().await;
        let mut connection = pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?;

        mysql_sqlx_exec_create_table(&mut connection, statement).await?;

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

        let pool = self.connection.lock().await;
        let mut connection = pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?;

        mysql_sqlx_exec_drop_table(&mut connection, statement).await?;

        Ok(())
    }

    #[cfg(feature = "schema")]
    async fn exec_create_index(
        &self,
        statement: &crate::schema::CreateIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let pool = self.connection.lock().await;
        let mut connection = pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?;

        mysql_sqlx_exec_create_index(&mut connection, statement).await?;

        Ok(())
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_index(
        &self,
        statement: &crate::schema::DropIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let pool = self.connection.lock().await;
        let mut connection = pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?;

        mysql_sqlx_exec_drop_index(&mut connection, statement).await?;

        Ok(())
    }

    #[cfg(feature = "schema")]
    async fn exec_alter_table(
        &self,
        statement: &crate::schema::AlterTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let pool = self.connection.lock().await;
        let mut connection = pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?;

        mysql_sqlx_exec_alter_table(&mut connection, statement).await?;

        Ok(())
    }

    #[cfg(feature = "schema")]
    async fn table_exists(&self, table_name: &str) -> Result<bool, DatabaseError> {
        let pool = self.connection.lock().await;
        let mut connection = pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?;
        super::mysql_introspection::mysql_sqlx_table_exists(&mut connection, table_name).await
    }

    #[cfg(feature = "schema")]
    async fn list_tables(&self) -> Result<Vec<String>, DatabaseError> {
        let pool = self.connection.lock().await;
        let mut connection = pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?;
        super::mysql_introspection::mysql_sqlx_list_tables(&mut connection).await
    }

    #[cfg(feature = "schema")]
    async fn get_table_info(
        &self,
        table_name: &str,
    ) -> Result<Option<crate::schema::TableInfo>, DatabaseError> {
        let pool = self.connection.lock().await;
        let mut connection = pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?;
        super::mysql_introspection::mysql_sqlx_get_table_info(&mut connection, table_name).await
    }

    #[cfg(feature = "schema")]
    async fn get_table_columns(
        &self,
        table_name: &str,
    ) -> Result<Vec<crate::schema::ColumnInfo>, DatabaseError> {
        let pool = self.connection.lock().await;
        let mut connection = pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?;
        super::mysql_introspection::mysql_sqlx_get_table_columns(&mut connection, table_name).await
    }

    #[cfg(feature = "schema")]
    async fn column_exists(
        &self,
        table_name: &str,
        column_name: &str,
    ) -> Result<bool, DatabaseError> {
        let pool = self.connection.lock().await;
        let mut connection = pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?;
        super::mysql_introspection::mysql_sqlx_column_exists(
            &mut connection,
            table_name,
            column_name,
        )
        .await
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn query_raw(&self, query: &str) -> Result<Vec<crate::Row>, DatabaseError> {
        let pool = self.connection.lock().await;
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
            let pool = self.connection.lock().await;
            pool.begin().await.map_err(SqlxDatabaseError::Sqlx)?
        };

        Ok(Box::new(MysqlSqlxTransaction::new(tx)))
    }

    async fn exec_raw_params(
        &self,
        query: &str,
        params: &[crate::DatabaseValue],
    ) -> Result<u64, DatabaseError> {
        // Transform query to handle Now/NowPlus parameters
        let (transformed_query, filtered_params) = mysql_transform_query_for_params(query, params)?;

        let pool = self.connection.lock().await;
        let mut connection = pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?;

        let mut query_builder: sqlx::query::Query<'_, sqlx::MySql, sqlx::mysql::MySqlArguments> =
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
                crate::DatabaseValue::UInt8(n) => query_builder.bind(*n),
                crate::DatabaseValue::UInt8Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::UInt16(n) => query_builder.bind(*n),
                crate::DatabaseValue::UInt16Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::UInt32(n) => query_builder.bind(*n),
                crate::DatabaseValue::UInt32Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::UInt64(n) => query_builder.bind(*n),
                crate::DatabaseValue::UInt64Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::Real64(r) => query_builder.bind(*r),
                crate::DatabaseValue::Real64Opt(r) => query_builder.bind(r),
                crate::DatabaseValue::Real32(r) => query_builder.bind(*r),
                crate::DatabaseValue::Real32Opt(r) => query_builder.bind(r),
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::Decimal(d) => query_builder.bind(*d),
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::DecimalOpt(d) => query_builder.bind(d),
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::Uuid(u) => query_builder.bind(u.to_string()),
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::UuidOpt(u) => query_builder.bind(u.map(|x| x.to_string())),
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
        // Transform query to handle Now/NowPlus parameters
        let (transformed_query, filtered_params) = mysql_transform_query_for_params(query, params)?;

        let pool = self.connection.lock().await;
        let mut connection = pool.acquire().await.map_err(SqlxDatabaseError::Sqlx)?;

        let mut query_builder: sqlx::query::Query<'_, sqlx::MySql, sqlx::mysql::MySqlArguments> =
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
                crate::DatabaseValue::UInt8(n) => query_builder.bind(*n),
                crate::DatabaseValue::UInt8Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::UInt16(n) => query_builder.bind(*n),
                crate::DatabaseValue::UInt16Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::UInt32(n) => query_builder.bind(*n),
                crate::DatabaseValue::UInt32Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::UInt64(n) => query_builder.bind(*n),
                crate::DatabaseValue::UInt64Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::Real64(r) => query_builder.bind(*r),
                crate::DatabaseValue::Real64Opt(r) => query_builder.bind(r),
                crate::DatabaseValue::Real32(r) => query_builder.bind(*r),
                crate::DatabaseValue::Real32Opt(r) => query_builder.bind(r),
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::Decimal(d) => query_builder.bind(*d),
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::DecimalOpt(d) => query_builder.bind(d),
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::Uuid(u) => query_builder.bind(u.to_string()),
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::UuidOpt(u) => query_builder.bind(u.map(|x| x.to_string())),
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
impl Database for MysqlSqlxTransaction {
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

        mysql_sqlx_exec_create_table(&mut *tx, statement).await?;

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
                    return mysql_sqlx_exec_drop_table_cascade(&mut *tx, statement).await;
                }
                DropBehavior::Restrict => {
                    return mysql_sqlx_exec_drop_table_restrict(&mut *tx, statement).await;
                }
                DropBehavior::Default => {}
            }
        }

        mysql_sqlx_exec_drop_table(&mut *tx, statement).await?;

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

        mysql_sqlx_exec_create_index(&mut *tx, statement).await?;

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

        mysql_sqlx_exec_drop_index(&mut *tx, statement).await?;

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

        mysql_sqlx_exec_alter_table(&mut *tx, statement).await?;

        Ok(())
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::significant_drop_tightening)]
    async fn table_exists(&self, table_name: &str) -> Result<bool, DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;
        super::mysql_introspection::mysql_sqlx_table_exists(&mut *tx, table_name).await
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::significant_drop_tightening)]
    async fn list_tables(&self) -> Result<Vec<String>, DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;
        super::mysql_introspection::mysql_sqlx_list_tables(&mut *tx).await
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
        super::mysql_introspection::mysql_sqlx_get_table_info(&mut *tx, table_name).await
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
        super::mysql_introspection::mysql_sqlx_get_table_columns(&mut *tx, table_name).await
    }

    #[cfg(feature = "schema")]
    #[allow(clippy::significant_drop_tightening)]
    async fn column_exists(
        &self,
        table_name: &str,
        column_name: &str,
    ) -> Result<bool, DatabaseError> {
        let mut transaction_guard = self.transaction.lock().await;
        let tx = transaction_guard
            .as_mut()
            .ok_or(DatabaseError::TransactionCommitted)?;
        super::mysql_introspection::mysql_sqlx_column_exists(&mut *tx, table_name, column_name)
            .await
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
        let mut query_builder: sqlx::query::Query<'_, sqlx::MySql, sqlx::mysql::MySqlArguments> =
            sqlx::query(query);

        // Add parameters in order - MySQL uses ? placeholders
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
                crate::DatabaseValue::UInt8(n) => query_builder.bind(*n),
                crate::DatabaseValue::UInt8Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::UInt16(n) => query_builder.bind(*n),
                crate::DatabaseValue::UInt16Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::UInt32(n) => query_builder.bind(*n),
                crate::DatabaseValue::UInt32Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::UInt64(n) => query_builder.bind(*n),
                crate::DatabaseValue::UInt64Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::Real64(r) => query_builder.bind(*r),
                crate::DatabaseValue::Real64Opt(r) => query_builder.bind(r),
                crate::DatabaseValue::Real32(r) => query_builder.bind(*r),
                crate::DatabaseValue::Real32Opt(r) => query_builder.bind(r),
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::Decimal(d) => query_builder.bind(*d),
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::DecimalOpt(d) => query_builder.bind(d),
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::Uuid(u) => query_builder.bind(u.to_string()),
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::UuidOpt(u) => query_builder.bind(u.map(|x| x.to_string())),
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
        let mut query_builder: sqlx::query::Query<'_, sqlx::MySql, sqlx::mysql::MySqlArguments> =
            sqlx::query(query);

        // Add parameters in order - MySQL uses ? placeholders
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
                crate::DatabaseValue::UInt8(n) => query_builder.bind(*n),
                crate::DatabaseValue::UInt8Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::UInt16(n) => query_builder.bind(*n),
                crate::DatabaseValue::UInt16Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::UInt32(n) => query_builder.bind(*n),
                crate::DatabaseValue::UInt32Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::UInt64(n) => query_builder.bind(*n),
                crate::DatabaseValue::UInt64Opt(n) => query_builder.bind(n),
                crate::DatabaseValue::Real64(r) => query_builder.bind(*r),
                crate::DatabaseValue::Real64Opt(r) => query_builder.bind(r),
                crate::DatabaseValue::Real32(r) => query_builder.bind(*r),
                crate::DatabaseValue::Real32Opt(r) => query_builder.bind(r),
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::Decimal(d) => query_builder.bind(*d),
                #[cfg(feature = "decimal")]
                crate::DatabaseValue::DecimalOpt(d) => query_builder.bind(d),
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::Uuid(u) => query_builder.bind(u.to_string()),
                #[cfg(feature = "uuid")]
                crate::DatabaseValue::UuidOpt(u) => query_builder.bind(u.map(|x| x.to_string())),
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

struct MysqlSqlxSavepoint {
    name: String,
    transaction: Arc<Mutex<Option<Transaction<'static, MySql>>>>,
    released: AtomicBool,
    rolled_back: AtomicBool,
}

#[async_trait]
impl crate::Savepoint for MysqlSqlxSavepoint {
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
            tx.execute(sqlx::raw_sql(&format!("RELEASE SAVEPOINT {}", self.name)))
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
            tx.execute(sqlx::raw_sql(&format!(
                "ROLLBACK TO SAVEPOINT {}",
                self.name
            )))
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
impl crate::DatabaseTransaction for MysqlSqlxTransaction {
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
            tx.execute(sqlx::raw_sql(&format!("SAVEPOINT {name}")))
                .await
                .map_err(SqlxDatabaseError::Sqlx)?;
        } else {
            return Err(DatabaseError::TransactionCommitted);
        }

        Ok(Box::new(MysqlSqlxSavepoint {
            name: name.to_string(),
            transaction: Arc::clone(&self.transaction),
            released: AtomicBool::new(false),
            rolled_back: AtomicBool::new(false),
        }))
    }

    /// MySQL-optimized CASCADE discovery with version detection
    #[cfg(feature = "cascade")]
    async fn find_cascade_targets(
        &self,
        table_name: &str,
    ) -> Result<crate::schema::DropPlan, DatabaseError> {
        // Try recursive CTE first (MySQL 8.0+)
        let recursive_query = format!(
            r"
            WITH RECURSIVE dependent_tables AS (
                SELECT
                    CAST(kcu.TABLE_NAME AS CHAR) as dependent_table,
                    1 as level
                FROM INFORMATION_SCHEMA.KEY_COLUMN_USAGE kcu
                WHERE kcu.REFERENCED_TABLE_NAME = '{}'
                    AND kcu.TABLE_SCHEMA = DATABASE()

                UNION ALL

                SELECT
                    CAST(kcu.TABLE_NAME AS CHAR) as dependent_table,
                    dt.level + 1 as level
                FROM INFORMATION_SCHEMA.KEY_COLUMN_USAGE kcu
                JOIN dependent_tables dt ON kcu.REFERENCED_TABLE_NAME = dt.dependent_table
                WHERE kcu.TABLE_SCHEMA = DATABASE()
            )
            SELECT dependent_table, MAX(level) as max_level
            FROM dependent_tables
            GROUP BY dependent_table
            ORDER BY max_level DESC, dependent_table
            ",
            sanitize_value(table_name)
        );

        let rows = self.query_raw(&recursive_query).await?;
        let mut result = Vec::new();
        for row in rows {
            if let Some((_, crate::DatabaseValue::String(table))) = row.columns.first() {
                result.push(table.clone());
            }
        }
        result.push(table_name.to_string());
        Ok(crate::schema::DropPlan::Simple(result))
    }

    /// MySQL-optimized dependency check
    #[cfg(feature = "cascade")]
    async fn has_any_dependents(&self, table_name: &str) -> Result<bool, DatabaseError> {
        let query = format!(
            r"
            SELECT EXISTS (
                SELECT 1
                FROM INFORMATION_SCHEMA.KEY_COLUMN_USAGE
                WHERE REFERENCED_TABLE_NAME = '{}'
                    AND TABLE_SCHEMA = DATABASE()
                LIMIT 1
            ) as has_dependents
            ",
            sanitize_value(table_name)
        );

        let rows = self.query_raw(&query).await?;

        if let Some(row) = rows.first() {
            // MySQL might return as integer (1/0) or boolean
            match row.columns.first() {
                Some((_, crate::DatabaseValue::Bool(has_deps))) => return Ok(*has_deps),
                Some((_, crate::DatabaseValue::Int64(n))) => return Ok(*n != 0),
                _ => {}
            }
        }

        Ok(false)
    }

    /// Get direct dependents of a table (MySQL-optimized)
    #[cfg(feature = "cascade")]
    async fn get_direct_dependents(
        &self,
        table_name: &str,
    ) -> Result<std::collections::BTreeSet<String>, DatabaseError> {
        let query = format!(
            r"
            SELECT DISTINCT CAST(TABLE_NAME AS CHAR) AS TABLE_NAME
            FROM INFORMATION_SCHEMA.KEY_COLUMN_USAGE
            WHERE REFERENCED_TABLE_NAME = '{}'
                AND TABLE_SCHEMA = DATABASE()
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

#[cfg(feature = "cascade")]
fn sanitize_value(identifier: &str) -> String {
    identifier.replace('\'', "''")
}

#[cfg(feature = "schema")]
#[allow(clippy::too_many_lines)]
async fn mysql_sqlx_exec_create_table(
    connection: &mut MySqlConnection,
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
            crate::schema::DataType::Text | crate::schema::DataType::Xml => query.push_str("TEXT"), // MySQL doesn't have native XML
            crate::schema::DataType::Char(size) => {
                query.push_str("CHAR(");
                query.push_str(&size.to_string());
                query.push(')');
            }
            crate::schema::DataType::Bool => query.push_str("BOOLEAN"),
            crate::schema::DataType::TinyInt => query.push_str("TINYINT"),
            crate::schema::DataType::SmallInt => query.push_str("SMALLINT"),
            crate::schema::DataType::Int | crate::schema::DataType::Serial => query.push_str("INT"), // MySQL doesn't have SERIAL, use INT with AUTO_INCREMENT
            crate::schema::DataType::BigInt | crate::schema::DataType::BigSerial => {
                query.push_str("BIGINT");
            } // MySQL doesn't have BIGSERIAL
            crate::schema::DataType::Real => query.push_str("FLOAT"),
            crate::schema::DataType::Double => query.push_str("DOUBLE"),
            crate::schema::DataType::Decimal(precision, scale) => {
                query.push_str("DECIMAL(");
                query.push_str(&precision.to_string());
                query.push(',');
                query.push_str(&scale.to_string());
                query.push(')');
            }
            crate::schema::DataType::Money => query.push_str("DECIMAL(19,4)"), // MySQL doesn't have MONEY type
            crate::schema::DataType::Date => query.push_str("DATE"),
            crate::schema::DataType::Time => query.push_str("TIME"),
            crate::schema::DataType::DateTime => query.push_str("DATETIME"),
            crate::schema::DataType::Timestamp => query.push_str("TIMESTAMP"),
            crate::schema::DataType::Blob => query.push_str("BLOB"),
            crate::schema::DataType::Binary(size) => {
                if let Some(size) = size {
                    query.push_str("BINARY(");
                    query.push_str(&size.to_string());
                    query.push(')');
                } else {
                    query.push_str("VARBINARY(255)");
                }
            }
            crate::schema::DataType::Json
            | crate::schema::DataType::Jsonb
            | crate::schema::DataType::Array(_) => query.push_str("JSON"), // MySQL doesn't distinguish JSON/JSONB, doesn't have arrays
            crate::schema::DataType::Uuid => query.push_str("CHAR(36)"), // MySQL doesn't have native UUID
            crate::schema::DataType::Inet => query.push_str("VARCHAR(45)"), // MySQL doesn't have INET
            crate::schema::DataType::MacAddr => query.push_str("VARCHAR(17)"), // MySQL doesn't have MACADDR
            crate::schema::DataType::Custom(ref type_name) => query.push_str(type_name),
        }

        if column.auto_increment {
            query.push_str(" AUTO_INCREMENT");
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
                    query.push_str(&x.to_string());
                }
                #[cfg(feature = "uuid")]
                DatabaseValue::Uuid(u) | DatabaseValue::UuidOpt(Some(u)) => {
                    query.push('\'');
                    query.push_str(&u.to_string());
                    query.push('\'');
                }
                DatabaseValue::NowPlus(interval) => {
                    query.push_str(&format_mysql_now_plus(interval));
                }
                DatabaseValue::Now => {
                    query.push_str("NOW()");
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

    log::trace!("exec_create_table: query:\n{query}");

    connection
        .execute(query.as_str())
        .await
        .map_err(SqlxDatabaseError::Sqlx)?;

    Ok(())
}

// Helper functions for CASCADE support using iterative approach and FK disable/enable
#[cfg(feature = "cascade")]
async fn mysql_sqlx_get_direct_dependents(
    connection: &mut MySqlConnection,
    table_name: &str,
) -> Result<Vec<String>, SqlxDatabaseError> {
    let query = r"
        SELECT DISTINCT CAST(TABLE_NAME AS CHAR) AS TABLE_NAME
        FROM information_schema.KEY_COLUMN_USAGE
        WHERE REFERENCED_TABLE_SCHEMA = DATABASE()
            AND REFERENCED_TABLE_NAME = ?
    ";

    let rows: Vec<(String,)> = sqlx::query_as(query)
        .bind(table_name)
        .fetch_all(&mut *connection)
        .await
        .map_err(SqlxDatabaseError::Sqlx)?;

    Ok(rows.into_iter().map(|(name,)| name).collect())
}

#[cfg(feature = "cascade")]
async fn mysql_sqlx_exec_drop_table_cascade(
    connection: &mut MySqlConnection,
    statement: &crate::schema::DropTableStatement<'_>,
) -> Result<(), DatabaseError> {
    // Iterative collection
    let mut to_drop = Vec::new();
    let mut to_check = vec![statement.table_name.to_string()];
    let mut visited = std::collections::BTreeSet::new();

    while let Some(table) = to_check.pop() {
        if !visited.insert(table.clone()) {
            continue;
        }

        let dependents = mysql_sqlx_get_direct_dependents(connection, &table)
            .await
            .map_err(DatabaseError::MysqlSqlx)?;

        for dependent in dependents {
            if !visited.contains(&dependent) {
                to_check.push(dependent);
            }
        }

        to_drop.push(table);
    }

    to_drop.reverse();

    // Always disable FK checks for CASCADE
    connection
        .execute("SET FOREIGN_KEY_CHECKS=0")
        .await
        .map_err(|e| DatabaseError::MysqlSqlx(SqlxDatabaseError::Sqlx(e)))?;

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
            .map_err(|e| DatabaseError::MysqlSqlx(SqlxDatabaseError::Sqlx(e)))?;
    }

    connection
        .execute("SET FOREIGN_KEY_CHECKS=1")
        .await
        .map_err(|e| DatabaseError::MysqlSqlx(SqlxDatabaseError::Sqlx(e)))?;

    Ok(())
}

#[cfg(feature = "cascade")]
async fn mysql_sqlx_exec_drop_table_restrict(
    connection: &mut MySqlConnection,
    statement: &crate::schema::DropTableStatement<'_>,
) -> Result<(), DatabaseError> {
    let dependents = mysql_sqlx_get_direct_dependents(connection, statement.table_name)
        .await
        .map_err(DatabaseError::MysqlSqlx)?;

    if !dependents.is_empty() {
        return Err(DatabaseError::InvalidQuery(format!(
            "Cannot drop table '{}': has dependent tables",
            statement.table_name
        )));
    }

    // Call basic version to avoid recursion
    mysql_sqlx_exec_drop_table_basic(connection, statement)
        .await
        .map_err(Into::into)
}

#[cfg(feature = "cascade")]
async fn mysql_sqlx_exec_drop_table_basic(
    connection: &mut MySqlConnection,
    statement: &crate::schema::DropTableStatement<'_>,
) -> Result<(), SqlxDatabaseError> {
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
async fn mysql_sqlx_exec_drop_table(
    connection: &mut MySqlConnection,
    statement: &crate::schema::DropTableStatement<'_>,
) -> Result<(), SqlxDatabaseError> {
    #[cfg(feature = "cascade")]
    {
        use crate::schema::DropBehavior;
        match statement.behavior {
            DropBehavior::Cascade => {
                return mysql_sqlx_exec_drop_table_cascade(connection, statement)
                    .await
                    .map_err(|e| match e {
                        DatabaseError::MysqlSqlx(mysql_err) => mysql_err,
                        _ => SqlxDatabaseError::Sqlx(sqlx::Error::Protocol(format!(
                            "CASCADE operation failed: {e}"
                        ))),
                    });
            }
            DropBehavior::Restrict => {
                return mysql_sqlx_exec_drop_table_restrict(connection, statement)
                    .await
                    .map_err(|e| match e {
                        DatabaseError::MysqlSqlx(mysql_err) => mysql_err,
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

/// Execute CREATE INDEX statement for `MySQL`
///
/// Note: IF NOT EXISTS support requires `MySQL` 8.0.29 or later.
/// Using `if_not_exists` on older `MySQL` versions will result in a syntax error.
#[cfg(feature = "schema")]
pub(crate) async fn mysql_sqlx_exec_create_index(
    connection: &mut MySqlConnection,
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

    log::trace!("exec_create_index: query:\n{sql}");

    connection
        .execute(sql.as_str())
        .await
        .map_err(SqlxDatabaseError::Sqlx)?;

    Ok(())
}

/// Execute DROP INDEX statement for `MySQL`
///
/// Note: IF EXISTS support requires `MySQL` 8.0.29 or later.
/// Using `if_exists` on older `MySQL` versions will result in a syntax error.
#[cfg(feature = "schema")]
pub(crate) async fn mysql_sqlx_exec_drop_index(
    connection: &mut MySqlConnection,
    statement: &crate::schema::DropIndexStatement<'_>,
) -> Result<(), SqlxDatabaseError> {
    let if_exists_str = if statement.if_exists {
        "IF EXISTS "
    } else {
        ""
    };

    let sql = format!(
        "DROP INDEX {}{}ON {}",
        if_exists_str, statement.index_name, statement.table_name
    );

    log::trace!("exec_drop_index: query:\n{sql}");

    connection
        .execute(sql.as_str())
        .await
        .map_err(SqlxDatabaseError::Sqlx)?;

    Ok(())
}

#[cfg(feature = "schema")]
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
pub(crate) async fn mysql_sqlx_exec_alter_table(
    connection: &mut MySqlConnection,
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
                    crate::schema::DataType::Text | crate::schema::DataType::Xml => {
                        "TEXT".to_string()
                    }
                    crate::schema::DataType::Char(len) => format!("CHAR({len})"),
                    crate::schema::DataType::Bool => "BOOLEAN".to_string(),
                    crate::schema::DataType::TinyInt => "TINYINT".to_string(),
                    crate::schema::DataType::SmallInt => "SMALLINT".to_string(),
                    crate::schema::DataType::Int => "INTEGER".to_string(),
                    crate::schema::DataType::BigInt | crate::schema::DataType::BigSerial => {
                        "BIGINT".to_string()
                    }
                    crate::schema::DataType::Serial => "INT".to_string(),
                    crate::schema::DataType::Real => "FLOAT".to_string(),
                    crate::schema::DataType::Double => "DOUBLE".to_string(),
                    crate::schema::DataType::Decimal(precision, scale) => {
                        format!("DECIMAL({precision}, {scale})")
                    }
                    crate::schema::DataType::Money => "DECIMAL(19,4)".to_string(),
                    crate::schema::DataType::Date => "DATE".to_string(),
                    crate::schema::DataType::Time => "TIME".to_string(),
                    crate::schema::DataType::DateTime => "DATETIME".to_string(),
                    crate::schema::DataType::Timestamp => "TIMESTAMP".to_string(),
                    crate::schema::DataType::Blob => "BLOB".to_string(),
                    crate::schema::DataType::Binary(size) => size.as_ref().map_or_else(
                        || "VARBINARY(255)".to_string(),
                        |size| format!("BINARY({size})"),
                    ),
                    crate::schema::DataType::Json
                    | crate::schema::DataType::Jsonb
                    | crate::schema::DataType::Array(_) => "JSON".to_string(),
                    crate::schema::DataType::Uuid => "CHAR(36)".to_string(),

                    crate::schema::DataType::Inet => "VARCHAR(45)".to_string(),
                    crate::schema::DataType::MacAddr => "VARCHAR(17)".to_string(),
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
                    "ALTER TABLE {} ADD COLUMN `{}` {}{}{}",
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
                #[cfg(feature = "cascade")]
                {
                    use crate::schema::DropBehavior;

                    match behavior {
                        DropBehavior::Cascade => {
                            // Get column dependencies before dropping
                            let (indexes, foreign_keys) = mysql_get_column_dependencies(
                                connection,
                                statement.table_name,
                                name,
                            )
                            .await?;

                            // Drop indexes first (MySQL allows this)
                            for index_name in indexes {
                                let drop_index_sql = format!(
                                    "DROP INDEX `{}` ON `{}`",
                                    index_name, statement.table_name
                                );
                                log::trace!("MySQL CASCADE dropping index: {drop_index_sql}");
                                connection
                                    .execute(drop_index_sql.as_str())
                                    .await
                                    .map_err(SqlxDatabaseError::Sqlx)?;
                            }

                            // Drop foreign key constraints
                            for fk_name in foreign_keys {
                                let drop_fk_sql = format!(
                                    "ALTER TABLE `{}` DROP FOREIGN KEY `{}`",
                                    statement.table_name, fk_name
                                );
                                log::trace!("MySQL CASCADE dropping foreign key: {drop_fk_sql}");
                                connection
                                    .execute(drop_fk_sql.as_str())
                                    .await
                                    .map_err(SqlxDatabaseError::Sqlx)?;
                            }
                        }
                        DropBehavior::Restrict => {
                            // Check for dependencies and fail if any exist
                            let (indexes, foreign_keys) = mysql_get_column_dependencies(
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
                            // MySQL default: auto-drop indexes, fail on FKs
                        }
                    }
                }

                let sql = format!(
                    "ALTER TABLE {} DROP COLUMN `{}`",
                    statement.table_name, name
                );

                log::trace!("exec_alter_table DROP COLUMN: query:\n{sql}");

                connection
                    .execute(sql.as_str())
                    .await
                    .map_err(SqlxDatabaseError::Sqlx)?;
            }
            AlterOperation::RenameColumn { old_name, new_name } => {
                let sql = format!(
                    "ALTER TABLE {} RENAME COLUMN `{}` TO `{}`",
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
                // MySQL supports ALTER TABLE ... MODIFY COLUMN for changing type, nullable, and default
                let type_str = match new_data_type {
                    crate::schema::DataType::VarChar(len) => format!("VARCHAR({len})"),
                    crate::schema::DataType::Text | crate::schema::DataType::Xml => {
                        "TEXT".to_string()
                    }
                    crate::schema::DataType::Char(len) => format!("CHAR({len})"),
                    crate::schema::DataType::Bool => "BOOLEAN".to_string(),
                    crate::schema::DataType::TinyInt => "TINYINT".to_string(),
                    crate::schema::DataType::SmallInt => "SMALLINT".to_string(),
                    crate::schema::DataType::Int => "INTEGER".to_string(),
                    crate::schema::DataType::BigInt | crate::schema::DataType::BigSerial => {
                        "BIGINT".to_string()
                    }
                    crate::schema::DataType::Serial => "INT".to_string(),
                    crate::schema::DataType::Real => "FLOAT".to_string(),
                    crate::schema::DataType::Double => "DOUBLE".to_string(),
                    crate::schema::DataType::Decimal(precision, scale) => {
                        format!("DECIMAL({precision}, {scale})")
                    }
                    crate::schema::DataType::Money => "DECIMAL(19,4)".to_string(),
                    crate::schema::DataType::Date => "DATE".to_string(),
                    crate::schema::DataType::Time => "TIME".to_string(),
                    crate::schema::DataType::DateTime => "DATETIME".to_string(),
                    crate::schema::DataType::Timestamp => "TIMESTAMP".to_string(),
                    crate::schema::DataType::Blob => "BLOB".to_string(),
                    crate::schema::DataType::Binary(size) => size.as_ref().map_or_else(
                        || "VARBINARY(255)".to_string(),
                        |size| format!("BINARY({size})"),
                    ),
                    crate::schema::DataType::Json
                    | crate::schema::DataType::Jsonb
                    | crate::schema::DataType::Array(_) => "JSON".to_string(),
                    crate::schema::DataType::Uuid => "CHAR(36)".to_string(),

                    crate::schema::DataType::Inet => "VARCHAR(45)".to_string(),
                    crate::schema::DataType::MacAddr => "VARCHAR(17)".to_string(),
                    crate::schema::DataType::Custom(type_name) => type_name.clone(),
                };

                let nullable_str = match new_nullable {
                    Some(false) => " NOT NULL",
                    Some(true) | None => "", // Keep existing nullable setting
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
                                    type_name: "Unsupported default value type for MODIFY COLUMN"
                                        .to_string(),
                                }));
                            }
                        };
                        format!(" DEFAULT {val_str}")
                    }
                    None => String::new(),
                };

                let sql = format!(
                    "ALTER TABLE {} MODIFY COLUMN `{}` {}{}{}",
                    statement.table_name, name, type_str, nullable_str, default_str
                );

                log::trace!("exec_alter_table MODIFY COLUMN: query:\n{sql}");

                connection
                    .execute(sql.as_str())
                    .await
                    .map_err(SqlxDatabaseError::Sqlx)?;
            }
        }
    }

    Ok(())
}

fn column_value(value: &MySqlValueRef<'_>) -> Result<DatabaseValue, sqlx::Error> {
    if value.is_null() {
        return Ok(DatabaseValue::Null);
    }
    let owned = sqlx::ValueRef::to_owned(value);
    match value.type_info().name() {
        // MySQL boolean types (TINYINT(1) is used for booleans)
        "BOOLEAN" | "BOOL" => Ok(DatabaseValue::Bool(owned.try_decode()?)),
        "TINYINT" => Ok(DatabaseValue::Int8(owned.try_decode()?)),
        // MySQL integer types - decode based on SQL type
        "SMALLINT" => Ok(DatabaseValue::Int16(owned.try_decode()?)),
        "MEDIUMINT" | "INT" | "INTEGER" => Ok(DatabaseValue::Int32(owned.try_decode()?)),
        #[cfg(feature = "decimal")]
        "DECIMAL" => Ok(DatabaseValue::Decimal(owned.try_decode()?)),
        "BIGINT" => Ok(DatabaseValue::Int64(owned.try_decode()?)),
        // MySQL floating point types
        "FLOAT" | "DOUBLE" | "REAL" | "NUMERIC" => Ok(DatabaseValue::Real64(owned.try_decode()?)),
        // MySQL string types
        "VARCHAR" | "CHAR" | "TEXT" | "TINYTEXT" | "MEDIUMTEXT" | "LONGTEXT" | "BINARY"
        | "VARBINARY" | "BLOB" | "TINYBLOB" | "MEDIUMBLOB" | "LONGBLOB" => {
            Ok(DatabaseValue::String(owned.try_decode()?))
        }
        // MySQL date/time types
        "DATE" | "TIME" | "DATETIME" | "YEAR" => Ok(DatabaseValue::DateTime(owned.try_decode()?)),
        "TIMESTAMP" => {
            // MySQL TIMESTAMP is UTC-based, try different datetime types
            // First try NaiveDateTime directly
            owned.try_decode::<chrono::NaiveDateTime>().map_or_else(
                |_| {
                    owned
                        .try_decode::<chrono::DateTime<chrono::Utc>>()
                        .map_or_else(
                            |_| match owned.try_decode::<chrono::DateTime<chrono::Local>>() {
                                Ok(dt) => Ok(DatabaseValue::DateTime(dt.naive_local())),
                                Err(e) => Err(e), // Give up and return the decode error
                            },
                            |dt| Ok(DatabaseValue::DateTime(dt.naive_utc())),
                        )
                },
                |dt| Ok(DatabaseValue::DateTime(dt)),
            )
        }
        // MySQL JSON type
        "JSON" => Ok(DatabaseValue::String(owned.try_decode()?)),
        _ => Err(sqlx::Error::TypeNotFound {
            type_name: value.type_info().name().to_string(),
        }),
    }
}

fn from_row(column_names: &[String], row: &MySqlRow) -> Result<crate::Row, SqlxDatabaseError> {
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
    connection: &mut MySqlConnection,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Option<crate::Row>, SqlxDatabaseError> {
    let select_query = limit.map(|_| {
        format!(
            "SELECT rowid FROM {table_name} {}",
            build_where_clause(filters),
        )
    });

    let query = format!(
        "UPDATE {table_name} {} {} ",
        build_set_clause(values),
        build_update_where_clause(filters, limit, select_query.as_deref()),
    );

    let all_values = values
        .iter()
        .flat_map(|(_, value)| value.params().unwrap_or(vec![]).into_iter().cloned())
        .map(std::convert::Into::into)
        .collect::<Vec<MySqlDatabaseValue>>();
    let mut all_filter_values = filters
        .map(|filters| {
            filters
                .iter()
                .flat_map(|value| value.params().unwrap_or_default().into_iter().cloned())
                .map(std::convert::Into::into)
                .collect::<Vec<MySqlDatabaseValue>>()
        })
        .unwrap_or_default();

    if limit.is_some() {
        all_filter_values.extend(all_filter_values.clone());
    }

    let all_values = [all_values, all_filter_values].concat();

    log::trace!("Running update query: {query} with params: {all_values:?}");

    let query = bind_values(sqlx::query(&query), Some(&all_values))?;
    query.execute(connection).await?;

    // MySQL doesn't support RETURNING, so we return None for now
    // TODO: Implement SELECT after UPDATE for MySQL 8+ support
    Ok(None)
}

async fn update_and_get_rows(
    connection: &mut MySqlConnection,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, SqlxDatabaseError> {
    // MySQL doesn't support RETURNING, so we emulate it with SELECT + UPDATE + SELECT
    use sqlx::Connection;

    // Start a transaction to ensure atomicity
    let mut tx = connection.begin().await?;

    // Step 1: SELECT the IDs of rows that will be updated (with FOR UPDATE lock)
    let id_select_query = format!(
        "SELECT id FROM {table_name} {} {} FOR UPDATE",
        build_where_clause(filters),
        limit.map_or_else(String::new, |limit| format!("LIMIT {limit}"))
    );

    log::trace!(
        "Running ID select before update query: {id_select_query} with params: {:?}",
        filters.map(|f| f.iter().filter_map(|x| x.params()).collect::<Vec<_>>())
    );

    let filter_params = bexprs_to_values_opt(filters);
    let id_select_bound = bind_values(sqlx::query(&id_select_query), filter_params.as_deref())?;

    // Get the IDs of rows to be updated
    let id_rows: Vec<MySqlRow> = id_select_bound.fetch_all(&mut *tx).await?;

    if id_rows.is_empty() {
        // No rows to update, commit and return empty
        tx.commit().await?;
        return Ok(vec![]);
    }

    // Extract IDs
    let ids: Vec<i64> = id_rows
        .into_iter()
        .map(|row| row.get::<i64, _>("id"))
        .collect();

    // Step 2: Perform the UPDATE using the collected IDs
    let update_query = format!(
        "UPDATE {table_name} {} WHERE id IN ({})",
        build_set_clause(values),
        ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ")
    );

    // Prepare parameters: first update values, then IDs
    let update_values = values
        .iter()
        .flat_map(|(_, value)| value.params().unwrap_or(vec![]).into_iter().cloned())
        .map(std::convert::Into::into)
        .collect::<Vec<MySqlDatabaseValue>>();

    let id_params: Vec<MySqlDatabaseValue> = ids
        .iter()
        .map(|&id| MySqlDatabaseValue::from(crate::DatabaseValue::Int64(id)))
        .collect();

    let all_update_params = [update_values, id_params.clone()].concat();

    log::trace!("Running update query: {update_query} with params: {all_update_params:?}");

    let update_bound = bind_values(sqlx::query(&update_query), Some(&all_update_params))?;
    update_bound.execute(&mut *tx).await?;

    // Step 3: SELECT the updated rows
    let final_select_query = format!(
        "SELECT * FROM {table_name} WHERE id IN ({})",
        ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ")
    );

    let final_select_bound = bind_values(sqlx::query(&final_select_query), Some(&id_params))?;
    let updated_rows: Vec<MySqlRow> = final_select_bound.fetch_all(&mut *tx).await?;

    // Step 4: Commit the transaction
    tx.commit().await?;

    // Step 5: Convert MySQL rows to our Row format
    let mut results = Vec::new();
    if !updated_rows.is_empty() {
        let column_names: Vec<String> = updated_rows[0]
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect();

        for row in updated_rows {
            results.push(from_row(&column_names, &row)?);
        }
    }

    Ok(results)
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
        .map(|(name, value)| format!("{name}={}", value.deref().to_param()))
        .collect()
}

fn build_values_clause(values: &[(&str, Box<dyn Expression>)]) -> String {
    if values.is_empty() {
        "VALUES ()".to_string()
    } else {
        format!("VALUES({})", build_values_props(values).join(", "))
    }
}

fn build_values_props(values: &[(&str, Box<dyn Expression>)]) -> Vec<String> {
    values
        .iter()
        .map(|(_, value)| value.deref().to_param())
        .collect()
}

fn bind_values<'a, 'b>(
    mut query: Query<'a, MySql, MySqlArguments>,
    values: Option<&'b [MySqlDatabaseValue]>,
) -> Result<Query<'a, MySql, MySqlArguments>, SqlxDatabaseError>
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
                    query = query.bind(*value);
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
                    query = query.bind(*value);
                }
                DatabaseValue::UInt16(value) | DatabaseValue::UInt16Opt(Some(value)) => {
                    query = query.bind(*value);
                }
                DatabaseValue::UInt32(value) | DatabaseValue::UInt32Opt(Some(value)) => {
                    query = query.bind(*value);
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
    mut rows: Pin<Box<dyn Stream<Item = Result<MySqlRow, sqlx::Error>> + Send + 'a>>,
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

fn to_values(values: &[(&str, DatabaseValue)]) -> Vec<MySqlDatabaseValue> {
    values
        .iter()
        .map(|(_key, value)| value.clone())
        .map(std::convert::Into::into)
        .collect::<Vec<_>>()
}

fn exprs_to_values(values: &[(&str, Box<dyn Expression>)]) -> Vec<MySqlDatabaseValue> {
    values
        .iter()
        .flat_map(|value| value.1.values().into_iter())
        .flatten()
        .cloned()
        .map(std::convert::Into::into)
        .collect::<Vec<_>>()
}

fn bexprs_to_values(values: &[Box<dyn BooleanExpression>]) -> Vec<MySqlDatabaseValue> {
    values
        .iter()
        .flat_map(|value| value.values().into_iter())
        .flatten()
        .cloned()
        .map(std::convert::Into::into)
        .collect::<Vec<_>>()
}

#[allow(unused)]
fn to_values_opt(values: Option<&[(&str, DatabaseValue)]>) -> Option<Vec<MySqlDatabaseValue>> {
    values.map(to_values)
}

#[allow(unused)]
fn exprs_to_values_opt(
    values: Option<&[(&str, Box<dyn Expression>)]>,
) -> Option<Vec<MySqlDatabaseValue>> {
    values.map(exprs_to_values)
}

fn bexprs_to_values_opt(
    values: Option<&[Box<dyn BooleanExpression>]>,
) -> Option<Vec<MySqlDatabaseValue>> {
    values.map(bexprs_to_values)
}

#[allow(clippy::too_many_arguments)]
async fn select(
    connection: &mut MySqlConnection,
    table_name: &str,
    distinct: bool,
    columns: &[&str],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    joins: Option<&[Join<'_>]>,
    sort: Option<&[Sort]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, SqlxDatabaseError> {
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
    connection: &mut MySqlConnection,
    table_name: &str,
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, SqlxDatabaseError> {
    // MySQL doesn't support RETURNING, so we emulate it with SELECT + DELETE + transaction
    use sqlx::Connection;

    // Start a transaction to ensure atomicity
    let mut tx = connection.begin().await?;

    // Step 1: SELECT the rows that will be deleted
    let select_query = format!(
        "SELECT * FROM {table_name} {} {} FOR UPDATE",
        build_where_clause(filters),
        limit.map_or_else(String::new, |limit| format!("LIMIT {limit}"))
    );

    log::trace!(
        "Running select before delete query: {select_query} with params: {:?}",
        filters.map(|f| f.iter().filter_map(|x| x.params()).collect::<Vec<_>>())
    );

    let filter_params = bexprs_to_values_opt(filters);
    let select_bound = bind_values(sqlx::query(&select_query), filter_params.as_deref())?;

    // Execute SELECT and collect results
    let selected_rows: Vec<MySqlRow> = select_bound.fetch_all(&mut *tx).await?;

    if selected_rows.is_empty() {
        // No rows to delete, commit and return empty
        tx.commit().await?;
        return Ok(vec![]);
    }

    // Get column names from first row
    let column_names: Vec<String> = selected_rows[0]
        .columns()
        .iter()
        .map(|c| c.name().to_string())
        .collect();

    // Step 2: Now delete the rows
    let delete_query = format!(
        "DELETE FROM {table_name} {} {}",
        build_where_clause(filters),
        limit.map_or_else(String::new, |limit| format!("LIMIT {limit}"))
    );

    log::trace!(
        "Running delete query: {delete_query} with params: {:?}",
        filters.map(|f| f.iter().filter_map(|x| x.params()).collect::<Vec<_>>())
    );

    let delete_bound = bind_values(sqlx::query(&delete_query), filter_params.as_deref())?;
    delete_bound.execute(&mut *tx).await?;

    // Step 3: Commit the transaction
    tx.commit().await?;

    // Step 4: Convert MySQL rows to our Row format
    let mut results = Vec::new();
    for row in selected_rows {
        results.push(from_row(&column_names, &row)?);
    }

    Ok(results)
}

async fn find_row(
    connection: &mut MySqlConnection,
    table_name: &str,
    distinct: bool,
    columns: &[&str],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    joins: Option<&[Join<'_>]>,
    sort: Option<&[Sort]>,
) -> Result<Option<crate::Row>, SqlxDatabaseError> {
    let query = format!(
        "SELECT {} {} FROM {table_name} {} {} {} LIMIT 1",
        if distinct { "DISTINCT" } else { "" },
        columns.join(", "),
        build_join_clauses(joins),
        build_where_clause(filters),
        build_sort_clause(sort),
    );

    let statement = connection.prepare(&query).await?;
    let column_names = statement
        .columns()
        .iter()
        .map(|x| x.name().to_string())
        .collect::<Vec<_>>();

    log::trace!(
        "Running find_row query: {query} with params: {:?}",
        filters.map(|f| f.iter().filter_map(|x| x.params()).collect::<Vec<_>>())
    );

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
    connection: &mut MySqlConnection,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
) -> Result<crate::Row, SqlxDatabaseError> {
    // MySQL doesn't support RETURNING, so we emulate it with INSERT + SELECT
    use sqlx::Connection;

    // Start a transaction to ensure atomicity
    let mut tx = connection.begin().await?;

    let column_names = values
        .iter()
        .map(|(key, _v)| format!("`{key}`"))
        .collect::<Vec<_>>()
        .join(", ");

    let query = format!(
        "INSERT INTO {table_name} ({column_names}) {} ",
        build_values_clause(values),
    );

    log::trace!(
        "Running insert query: {query} with params: {:?}",
        values
            .iter()
            .filter_map(|(_, x)| x.params())
            .collect::<Vec<_>>()
    );

    let insert_values = exprs_to_values(values);
    let insert_bound = bind_values(sqlx::query(&query), Some(&insert_values))?;
    let result = insert_bound.execute(&mut *tx).await?;

    // Get the ID of the inserted row
    let inserted_id = result.last_insert_id();

    // Step 2: SELECT the inserted row to get all columns
    let select_query = format!("SELECT * FROM {table_name} WHERE id = ?");

    log::trace!("Running select after insert query: {select_query} with id: {inserted_id}");

    #[allow(clippy::cast_possible_wrap)]
    let select_bound = sqlx::query(&select_query).bind(inserted_id as i64);
    let inserted_row: MySqlRow = select_bound.fetch_one(&mut *tx).await?;

    // Step 3: Commit the transaction
    tx.commit().await?;

    // Step 4: Convert MySQL row to our Row format
    let column_names: Vec<String> = inserted_row
        .columns()
        .iter()
        .map(|c| c.name().to_string())
        .collect();

    from_row(&column_names, &inserted_row)
}

/// # Errors
///
/// Will return `Err` if the update multi execution failed.
pub async fn update_multi(
    connection: &mut MySqlConnection,
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
    connection: &mut MySqlConnection,
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
        ",
        build_update_where_clause(filters, limit, select_query.as_deref()),
    );

    let all_values = values
        .iter()
        .flat_map(std::iter::IntoIterator::into_iter)
        .flat_map(|(_, value)| value.params().unwrap_or(vec![]).into_iter().cloned())
        .map(std::convert::Into::into)
        .collect::<Vec<MySqlDatabaseValue>>();
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
                        .collect::<Vec<_>>()
                })
                .map(std::convert::Into::into)
                .collect::<Vec<MySqlDatabaseValue>>()
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
    connection: &mut MySqlConnection,
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
    connection: &mut MySqlConnection,
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

    let unique_conflict = unique
        .iter()
        .map(|x| x.to_sql())
        .collect::<Vec<_>>()
        .join(", ");

    let query = format!(
        "
        INSERT INTO {table_name} ({column_names})
        VALUES {values_str}
        ON CONFLICT({unique_conflict}) DO UPDATE
            SET {set_clause}
        "
    );

    let all_values = &values
        .iter()
        .flat_map(std::iter::IntoIterator::into_iter)
        .flat_map(|(_, value)| value.params().unwrap_or(vec![]).into_iter().cloned())
        .map(std::convert::Into::into)
        .collect::<Vec<MySqlDatabaseValue>>();

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
    connection: &mut MySqlConnection,
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

#[allow(unused)]
async fn upsert_and_get_row(
    connection: &mut MySqlConnection,
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

/// Wrapper type for converting `DatabaseValue` to `MySQL` `SQLx`-specific parameter types
#[derive(Debug, Clone)]
pub struct MySqlDatabaseValue(DatabaseValue);

impl From<DatabaseValue> for MySqlDatabaseValue {
    fn from(value: DatabaseValue) -> Self {
        Self(value)
    }
}

impl Deref for MySqlDatabaseValue {
    type Target = DatabaseValue;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Expression for MySqlDatabaseValue {
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

fn mysql_transform_query_for_params(
    query: &str,
    params: &[crate::DatabaseValue],
) -> Result<(String, Vec<crate::DatabaseValue>), DatabaseError> {
    transform_query_for_params(
        query,
        params,
        &QuestionMarkHandler, // MySQL uses ? placeholders
        |param| match param {
            DatabaseValue::Now => Some("NOW()".to_string()),
            DatabaseValue::NowPlus(interval) => Some(format_mysql_now_plus(interval)),
            _ => None,
        },
    )
    .map_err(DatabaseError::QueryFailed)
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "schema")]
    mod schema {
        use super::super::*;
        use crate::schema::DataType;
        use sqlx::MySqlPool;
        use std::sync::Arc;
        use tokio::sync::Mutex;

        fn get_mysql_test_url() -> Option<String> {
            std::env::var("MYSQL_TEST_URL").ok()
        }

        async fn create_pool(url: &str) -> Result<Arc<Mutex<MySqlPool>>, sqlx::Error> {
            let pool = MySqlPool::connect(url).await?;
            Ok(Arc::new(Mutex::new(pool)))
        }

        #[switchy_async::test]
        async fn test_mysql_table_exists() {
            let Some(url) = get_mysql_test_url() else {
                return;
            };

            let pool = create_pool(&url).await.expect("Failed to create pool");
            let db = MySqlSqlxDatabase::new(pool);

            // Test non-existent table
            assert!(!db.table_exists("non_existent_table").await.unwrap());

            // Create test table
            db.exec_raw(
            "CREATE TABLE IF NOT EXISTS test_table_exists (id INTEGER PRIMARY KEY AUTO_INCREMENT)",
        )
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
        async fn test_mysql_list_tables() {
            let Some(url) = get_mysql_test_url() else {
                return;
            };

            let pool = create_pool(&url).await.expect("Failed to create pool");
            let db = MySqlSqlxDatabase::new(pool);

            // Clean up any existing test tables first
            db.exec_raw("DROP TABLE IF EXISTS test_list_table1")
                .await
                .ok();
            db.exec_raw("DROP TABLE IF EXISTS test_list_table2")
                .await
                .ok();

            // Create test tables
            db.exec_raw("CREATE TABLE test_list_table1 (id INTEGER PRIMARY KEY AUTO_INCREMENT)")
                .await
                .expect("Failed to create table1");

            db.exec_raw("CREATE TABLE test_list_table2 (id INTEGER PRIMARY KEY AUTO_INCREMENT, name VARCHAR(255))")
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
        async fn test_mysql_get_table_columns() {
            let Some(url) = get_mysql_test_url() else {
                return;
            };

            let pool = create_pool(&url).await.expect("Failed to create pool");
            let db = MySqlSqlxDatabase::new(pool);

            // Create test table with various column types
            db.exec_raw(
                "CREATE TABLE IF NOT EXISTS test_table_columns (
                id INTEGER PRIMARY KEY AUTO_INCREMENT,
                name VARCHAR(100) NOT NULL,
                age INT,
                active BOOLEAN DEFAULT TRUE,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )",
            )
            .await
            .unwrap();

            let columns = db.get_table_columns("test_table_columns").await.unwrap();

            assert_eq!(columns.len(), 5);

            // Check id column
            let id_col = columns.iter().find(|c| c.name == "id").unwrap();
            assert_eq!(id_col.data_type, DataType::Int);
            assert!(!id_col.nullable);
            assert!(id_col.is_primary_key);
            assert!(id_col.auto_increment);

            // Check name column
            let name_col = columns.iter().find(|c| c.name == "name").unwrap();
            assert_eq!(name_col.data_type, DataType::VarChar(100));
            assert!(!name_col.nullable);
            assert!(!name_col.is_primary_key);

            // Check age column
            let age_col = columns.iter().find(|c| c.name == "age").unwrap();
            assert_eq!(age_col.data_type, DataType::Int);
            assert!(age_col.nullable);
            assert!(!age_col.is_primary_key);

            // Check active column
            let active_col = columns.iter().find(|c| c.name == "active").unwrap();
            assert_eq!(active_col.data_type, DataType::Bool);
            assert!(active_col.nullable);
            assert!(active_col.default_value.is_some());

            // Clean up
            db.exec_raw("DROP TABLE IF EXISTS test_table_columns")
                .await
                .unwrap();
        }

        #[switchy_async::test]
        async fn test_mysql_column_exists() {
            let Some(url) = get_mysql_test_url() else {
                return;
            };

            let pool = create_pool(&url).await.expect("Failed to create pool");
            let db = MySqlSqlxDatabase::new(pool);

            // Create test table
            db.exec_raw(
                "CREATE TABLE IF NOT EXISTS test_column_exists (id INTEGER PRIMARY KEY AUTO_INCREMENT, name VARCHAR(50))",
            )
            .await
            .unwrap();

            // Test existing columns
            assert!(db.column_exists("test_column_exists", "id").await.unwrap());
            assert!(
                db.column_exists("test_column_exists", "name")
                    .await
                    .unwrap()
            );

            // Test non-existent column
            assert!(
                !db.column_exists("test_column_exists", "nonexistent")
                    .await
                    .unwrap()
            );

            // Test non-existent table
            assert!(!db.column_exists("non_existent_table", "id").await.unwrap());

            // Clean up
            db.exec_raw("DROP TABLE IF EXISTS test_column_exists")
                .await
                .unwrap();
        }

        #[switchy_async::test]
        async fn test_mysql_get_table_info() {
            let Some(url) = get_mysql_test_url() else {
                return;
            };

            let pool = create_pool(&url).await.expect("Failed to create pool");
            let db = MySqlSqlxDatabase::new(pool);

            // Create test table
            db.exec_raw(
                "CREATE TABLE IF NOT EXISTS test_table_info (
                id INTEGER PRIMARY KEY AUTO_INCREMENT,
                name VARCHAR(100) NOT NULL,
                email VARCHAR(255)
            )",
            )
            .await
            .unwrap();

            let table_info = db.get_table_info("test_table_info").await.unwrap();
            assert!(table_info.is_some());

            let table_info = table_info.unwrap();
            assert_eq!(table_info.name, "test_table_info");
            assert_eq!(table_info.columns.len(), 3);

            // Check that we have the expected columns
            assert!(table_info.columns.contains_key("id"));
            assert!(table_info.columns.contains_key("name"));
            assert!(table_info.columns.contains_key("email"));

            // Clean up
            db.exec_raw("DROP TABLE IF EXISTS test_table_info")
                .await
                .unwrap();
        }

        #[switchy_async::test]
        async fn test_mysql_get_table_info_empty() {
            let Some(url) = get_mysql_test_url() else {
                return;
            };

            let pool = create_pool(&url).await.expect("Failed to create pool");
            let db = MySqlSqlxDatabase::new(pool);

            // Test non-existent table
            let table_info = db.get_table_info("non_existent_table").await.unwrap();
            assert!(table_info.is_none());
        }

        #[switchy_async::test]
        async fn test_mysql_get_table_info_with_indexes_and_foreign_keys() {
            let Some(url) = get_mysql_test_url() else {
                return;
            };

            let pool = create_pool(&url).await.expect("Failed to create pool");
            let db = MySqlSqlxDatabase::new(pool);

            // Create parent table
            db.exec_raw(
                "CREATE TABLE IF NOT EXISTS test_parent (
                id INTEGER PRIMARY KEY AUTO_INCREMENT,
                name VARCHAR(100) UNIQUE
            )",
            )
            .await
            .unwrap();

            // Create child table with foreign key and index
            db.exec_raw(
                "CREATE TABLE IF NOT EXISTS test_child (
                id INTEGER PRIMARY KEY AUTO_INCREMENT,
                parent_id INTEGER,
                description TEXT,
                INDEX idx_description (description(100)),
                FOREIGN KEY (parent_id) REFERENCES test_parent(id) ON DELETE CASCADE
            )",
            )
            .await
            .unwrap();

            let table_info = db.get_table_info("test_child").await.unwrap();
            assert!(table_info.is_some());

            let table_info = table_info.unwrap();
            assert_eq!(table_info.name, "test_child");

            // Check indexes (should have PRIMARY and idx_description)
            assert!(table_info.indexes.len() >= 2);
            assert!(table_info.indexes.contains_key("PRIMARY"));

            // Check foreign keys
            assert!(!table_info.foreign_keys.is_empty());

            // Clean up (order matters due to foreign key)
            db.exec_raw("DROP TABLE IF EXISTS test_child")
                .await
                .unwrap();
            db.exec_raw("DROP TABLE IF EXISTS test_parent")
                .await
                .unwrap();
        }

        #[switchy_async::test]
        async fn test_mysql_varchar_length_preservation() {
            let Some(url) = get_mysql_test_url() else {
                return;
            };

            let pool = create_pool(&url).await.expect("Failed to create pool");
            let db = MySqlSqlxDatabase::new(pool);

            // Create test table with various VARCHAR lengths
            db.exec_raw(
                "CREATE TABLE IF NOT EXISTS test_varchar_lengths (
                id INTEGER PRIMARY KEY AUTO_INCREMENT,
                varchar_50 VARCHAR(50) NOT NULL,
                varchar_255 VARCHAR(255),
                char_10 CHAR(10),
                text_col TEXT,
                bool_col BOOLEAN
            )",
            )
            .await
            .unwrap();

            let columns = db.get_table_columns("test_varchar_lengths").await.unwrap();

            // Verify VARCHAR length preservation
            let varchar_50_col = columns.iter().find(|c| c.name == "varchar_50").unwrap();
            assert!(matches!(
                varchar_50_col.data_type,
                crate::schema::DataType::VarChar(50)
            ));

            let varchar_255_col = columns.iter().find(|c| c.name == "varchar_255").unwrap();
            assert!(matches!(
                varchar_255_col.data_type,
                crate::schema::DataType::VarChar(255)
            ));

            let char_10_col = columns.iter().find(|c| c.name == "char_10").unwrap();
            assert!(matches!(
                char_10_col.data_type,
                crate::schema::DataType::Char(10)
            ));

            // Verify TEXT still maps to Text
            let text_col = columns.iter().find(|c| c.name == "text_col").unwrap();
            assert!(matches!(text_col.data_type, crate::schema::DataType::Text));

            // Verify other types still work
            let bool_col = columns.iter().find(|c| c.name == "bool_col").unwrap();
            assert!(matches!(bool_col.data_type, crate::schema::DataType::Bool));

            // Clean up
            db.exec_raw("DROP TABLE IF EXISTS test_varchar_lengths")
                .await
                .unwrap();
        }

        #[switchy_async::test]
        async fn test_mysql_sqlx_savepoint_basic() {
            let Some(url) = get_mysql_test_url() else {
                return;
            };

            let pool = create_pool(&url).await.expect("Failed to create pool");
            let db = MySqlSqlxDatabase::new(pool);

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
        async fn test_mysql_sqlx_savepoint_release() {
            let Some(url) = get_mysql_test_url() else {
                return;
            };

            let pool = create_pool(&url).await.expect("Failed to create pool");
            let db = MySqlSqlxDatabase::new(pool);

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
        async fn test_mysql_sqlx_savepoint_rollback() {
            let Some(url) = get_mysql_test_url() else {
                return;
            };

            let pool = create_pool(&url).await.expect("Failed to create pool");
            let db = MySqlSqlxDatabase::new(pool);

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
        async fn test_mysql_sqlx_savepoint_after_transaction_commit() {
            let Some(url) = get_mysql_test_url() else {
                return;
            };

            let pool = create_pool(&url).await.expect("Failed to create pool");
            let db = MySqlSqlxDatabase::new(pool);

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
        async fn test_mysql_sqlx_savepoint_after_transaction_rollback() {
            let Some(url) = get_mysql_test_url() else {
                return;
            };

            let pool = create_pool(&url).await.expect("Failed to create pool");
            let db = MySqlSqlxDatabase::new(pool);

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
}
