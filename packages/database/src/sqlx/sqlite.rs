use std::{
    ops::Deref,
    pin::Pin,
    sync::{Arc, atomic::AtomicU16},
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
};

#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct SqliteSqlxTransaction {
    transaction: Arc<Mutex<Option<Transaction<'static, Sqlite>>>>,
}

impl SqliteSqlxTransaction {
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
            ExpressionType::Literal(value) => value.value.to_string(),
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
                | DatabaseValue::NumberOpt(None)
                | DatabaseValue::UNumberOpt(None)
                | DatabaseValue::RealOpt(None) => "NULL".to_string(),
                DatabaseValue::Now => "strftime('%Y-%m-%dT%H:%M:%f', 'now')".to_string(),
                DatabaseValue::NowAdd(add) => {
                    format!("strftime('%Y-%m-%dT%H:%M:%f', DateTime('now', 'LocalTime', {add}))")
                }
                _ => {
                    let pos = index.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                    format!("${pos}")
                }
            },
        }
    }
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct SqliteSqlxDatabase {
    pool: Arc<Mutex<SqlitePool>>,
    #[allow(clippy::type_complexity)]
    connection: Arc<Mutex<Option<Arc<Mutex<PoolConnection<Sqlite>>>>>>,
}

impl SqliteSqlxDatabase {
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

#[derive(Debug, Error)]
pub enum SqlxDatabaseError {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error("No ID")]
    NoId,
    #[error("No row")]
    NoRow,
    #[error("Invalid request")]
    InvalidRequest,
    #[error("Missing unique")]
    MissingUnique,
}

impl From<SqlxDatabaseError> for DatabaseError {
    fn from(value: SqlxDatabaseError) -> Self {
        Self::SqliteSqlx(value)
    }
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
    async fn table_exists(&self, _table_name: &str) -> Result<bool, DatabaseError> {
        // TODO: Implement in Phase 16.4 - SQLite (sqlx)
        unimplemented!("table_exists not yet implemented for SqliteSqlxDatabase")
    }

    #[cfg(feature = "schema")]
    async fn get_table_info(
        &self,
        _table_name: &str,
    ) -> Result<Option<crate::schema::TableInfo>, DatabaseError> {
        // TODO: Implement in Phase 16.4 - SQLite (sqlx)
        unimplemented!("get_table_info not yet implemented for SqliteSqlxDatabase")
    }

    #[cfg(feature = "schema")]
    async fn get_table_columns(
        &self,
        _table_name: &str,
    ) -> Result<Vec<crate::schema::ColumnInfo>, DatabaseError> {
        // TODO: Implement in Phase 16.4 - SQLite (sqlx)
        unimplemented!("get_table_columns not yet implemented for SqliteSqlxDatabase")
    }

    #[cfg(feature = "schema")]
    async fn column_exists(
        &self,
        _table_name: &str,
        _column_name: &str,
    ) -> Result<bool, DatabaseError> {
        // TODO: Implement in Phase 16.4 - SQLite (sqlx)
        unimplemented!("column_exists not yet implemented for SqliteSqlxDatabase")
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
        | "BIGINT" | "BIGSERIAL" | "INT8" => Ok(DatabaseValue::Number(owned.try_decode()?)),
        "REAL" | "FLOAT4" | "DOUBLE PRECISION" | "FLOAT8" => {
            Ok(DatabaseValue::Real(owned.try_decode()?))
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
            column.to_string(),
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
                | DatabaseValue::NumberOpt(None)
                | DatabaseValue::UNumberOpt(None)
                | DatabaseValue::RealOpt(None)
                | DatabaseValue::Now => (),
                DatabaseValue::Bool(value) | DatabaseValue::BoolOpt(Some(value)) => {
                    query = query.bind(value);
                }
                DatabaseValue::Number(value) | DatabaseValue::NumberOpt(Some(value)) => {
                    query = query.bind(*value);
                }
                DatabaseValue::UNumber(value) | DatabaseValue::UNumberOpt(Some(value)) => {
                    query = query.bind(
                        i64::try_from(*value).map_err(|_| SqlxDatabaseError::InvalidRequest)?,
                    );
                }
                DatabaseValue::Real(value) | DatabaseValue::RealOpt(Some(value)) => {
                    query = query.bind(*value);
                }
                DatabaseValue::NowAdd(_add) => (),
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
    mut rows: Pin<Box<(dyn Stream<Item = Result<SqliteRow, sqlx::Error>> + Send + 'a)>>,
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
            crate::schema::DataType::Text => query.push_str("TEXT"),
            crate::schema::DataType::Bool
            | crate::schema::DataType::SmallInt
            | crate::schema::DataType::Int
            | crate::schema::DataType::BigInt => {
                query.push_str("INTEGER");
            }
            crate::schema::DataType::Double
            | crate::schema::DataType::Decimal(..)
            | crate::schema::DataType::Real => query.push_str("REAL"),
            crate::schema::DataType::DateTime => query.push_str("VARCHAR(23)"),
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
                | DatabaseValue::NumberOpt(None)
                | DatabaseValue::UNumberOpt(None)
                | DatabaseValue::RealOpt(None) => {
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
                DatabaseValue::NumberOpt(Some(x)) | DatabaseValue::Number(x) => {
                    query.push_str(&x.to_string());
                }
                DatabaseValue::UNumberOpt(Some(x)) | DatabaseValue::UNumber(x) => {
                    query.push_str(&x.to_string());
                }
                DatabaseValue::RealOpt(Some(x)) | DatabaseValue::Real(x) => {
                    query.push_str(&x.to_string());
                }
                DatabaseValue::NowAdd(x) => {
                    query.push_str("(strftime('%Y-%m-%dT%H:%M:%f', DateTime('now', 'LocalTime', ");
                    query.push_str(x);
                    query.push_str(")))");
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
                    crate::schema::DataType::Text => "TEXT".to_string(),
                    crate::schema::DataType::Bool => "BOOLEAN".to_string(),
                    crate::schema::DataType::SmallInt => "SMALLINT".to_string(),
                    crate::schema::DataType::Int => "INTEGER".to_string(),
                    crate::schema::DataType::BigInt => "BIGINT".to_string(),
                    crate::schema::DataType::Real => "REAL".to_string(),
                    crate::schema::DataType::Double => "DOUBLE PRECISION".to_string(),
                    crate::schema::DataType::Decimal(precision, scale) => {
                        format!("DECIMAL({precision}, {scale})")
                    }
                    crate::schema::DataType::DateTime => "DATETIME".to_string(),
                };

                let nullable_str = if *nullable { "" } else { " NOT NULL" };
                let default_str = match default {
                    Some(val) => {
                        let val_str = match val {
                            crate::DatabaseValue::String(s) => format!("'{s}'"),
                            crate::DatabaseValue::Number(n) => n.to_string(),
                            crate::DatabaseValue::UNumber(n) => n.to_string(),
                            crate::DatabaseValue::Bool(b) => if *b { "1" } else { "0" }.to_string(),
                            crate::DatabaseValue::Real(r) => r.to_string(),
                            crate::DatabaseValue::Null => "NULL".to_string(),
                            crate::DatabaseValue::Now => "CURRENT_TIMESTAMP".to_string(),
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
            AlterOperation::DropColumn { name } => {
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
                        *new_data_type,
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
                        *new_data_type,
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
        crate::schema::DataType::Text => "TEXT".to_string(),
        crate::schema::DataType::Bool => "BOOLEAN".to_string(),
        crate::schema::DataType::SmallInt => "SMALLINT".to_string(),
        crate::schema::DataType::Int => "INTEGER".to_string(),
        crate::schema::DataType::BigInt => "BIGINT".to_string(),
        crate::schema::DataType::Real => "REAL".to_string(),
        crate::schema::DataType::Double => "DOUBLE PRECISION".to_string(),
        crate::schema::DataType::Decimal(precision, scale) => {
            format!("DECIMAL({precision}, {scale})")
        }
        crate::schema::DataType::DateTime => "DATETIME".to_string(),
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
                crate::DatabaseValue::Number(n) => n.to_string(),
                crate::DatabaseValue::UNumber(n) => n.to_string(),
                crate::DatabaseValue::Bool(b) => if *b { "1" } else { "0" }.to_string(),
                crate::DatabaseValue::Real(r) => r.to_string(),
                crate::DatabaseValue::Null => "NULL".to_string(),
                crate::DatabaseValue::Now => "CURRENT_TIMESTAMP".to_string(),
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
fn sqlite_modify_create_table_sql(
    original_sql: &str,
    original_table_name: &str,
    new_table_name: &str,
    column_name: &str,
    new_data_type: crate::schema::DataType,
    new_nullable: Option<bool>,
    new_default: Option<&crate::DatabaseValue>,
) -> Result<String, SqlxDatabaseError> {
    // Simple regex-based approach to modify column definition
    // This handles most common cases but could be enhanced with a proper SQL parser

    let data_type_str = match new_data_type {
        crate::schema::DataType::Text | crate::schema::DataType::VarChar(_) => "TEXT",
        crate::schema::DataType::Bool => "BOOLEAN",
        crate::schema::DataType::SmallInt
        | crate::schema::DataType::Int
        | crate::schema::DataType::BigInt => "INTEGER",
        crate::schema::DataType::Real
        | crate::schema::DataType::Double
        | crate::schema::DataType::Decimal(_, _) => "REAL",
        crate::schema::DataType::DateTime => "TIMESTAMP",
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
            crate::DatabaseValue::Number(i) | crate::DatabaseValue::NumberOpt(Some(i)) => {
                i.to_string()
            }
            crate::DatabaseValue::NumberOpt(None)
            | crate::DatabaseValue::UNumberOpt(None)
            | crate::DatabaseValue::RealOpt(None)
            | crate::DatabaseValue::BoolOpt(None) => "NULL".to_string(),
            crate::DatabaseValue::UNumber(i) | crate::DatabaseValue::UNumberOpt(Some(i)) => {
                i.to_string()
            }
            crate::DatabaseValue::Real(f) | crate::DatabaseValue::RealOpt(Some(f)) => f.to_string(),
            crate::DatabaseValue::Bool(b) | crate::DatabaseValue::BoolOpt(Some(b)) => {
                if *b { "1" } else { "0" }.to_string()
            }
            crate::DatabaseValue::DateTime(dt) => format!("'{}'", dt.format("%Y-%m-%d %H:%M:%S")),
            crate::DatabaseValue::Now => "CURRENT_TIMESTAMP".to_string(),
            crate::DatabaseValue::NowAdd(_) => return Err(SqlxDatabaseError::InvalidRequest),
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
            new_data_type,
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
                        crate::schema::DataType::Text |
                        crate::schema::DataType::VarChar(_) => "TEXT",
                        crate::schema::DataType::Bool => "BOOLEAN",
                        crate::schema::DataType::SmallInt |
                        crate::schema::DataType::Int |
                        crate::schema::DataType::BigInt => "INTEGER",
                        crate::schema::DataType::Real |
                        crate::schema::DataType::Double |
                        crate::schema::DataType::Decimal(_, _) => "REAL",
                        crate::schema::DataType::DateTime => "TIMESTAMP",
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
    async fn table_exists(&self, _table_name: &str) -> Result<bool, DatabaseError> {
        // TODO: Implement in Phase 16.4 - SQLite (sqlx)
        unimplemented!("table_exists not yet implemented for SqliteSqlxTransaction")
    }

    #[cfg(feature = "schema")]
    async fn get_table_info(
        &self,
        _table_name: &str,
    ) -> Result<Option<crate::schema::TableInfo>, DatabaseError> {
        // TODO: Implement in Phase 16.4 - SQLite (sqlx)
        unimplemented!("get_table_info not yet implemented for SqliteSqlxTransaction")
    }

    #[cfg(feature = "schema")]
    async fn get_table_columns(
        &self,
        _table_name: &str,
    ) -> Result<Vec<crate::schema::ColumnInfo>, DatabaseError> {
        // TODO: Implement in Phase 16.4 - SQLite (sqlx)
        unimplemented!("get_table_columns not yet implemented for SqliteSqlxTransaction")
    }

    #[cfg(feature = "schema")]
    async fn column_exists(
        &self,
        _table_name: &str,
        _column_name: &str,
    ) -> Result<bool, DatabaseError> {
        // TODO: Implement in Phase 16.4 - SQLite (sqlx)
        unimplemented!("column_exists not yet implemented for SqliteSqlxTransaction")
    }

    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn crate::DatabaseTransaction>, DatabaseError> {
        Err(DatabaseError::AlreadyInTransaction)
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
}
