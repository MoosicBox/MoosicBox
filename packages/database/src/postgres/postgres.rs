use std::{
    ops::Deref,
    sync::{atomic::AtomicU16, LazyLock},
};

use async_trait::async_trait;
use chrono::{NaiveDateTime, Utc};
use futures::StreamExt;
use postgres_protocol::types::{bool_from_sql, float8_from_sql, int8_from_sql, text_from_sql};
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    pin,
    task::JoinHandle,
};
use tokio_postgres::{types::IsNull, Client, Row, RowStream};

use crate::{
    query::{BooleanExpression, Expression, ExpressionType, Join, Sort, SortDirection},
    Database, DatabaseError, DatabaseValue, DeleteStatement, InsertStatement, SelectQuery,
    UpdateStatement, UpsertMultiStatement, UpsertStatement,
};

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
                DatabaseValue::Now => "NOW()".to_string(),
                DatabaseValue::NowAdd(ref add) => format!("NOW() + {add}"),
                _ => {
                    let pos = index.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                    format!("${pos}")
                }
            },
        }
    }
}

#[allow(clippy::module_name_repetitions)]
pub struct PostgresDatabase {
    client: Client,
    handle: JoinHandle<std::result::Result<(), tokio_postgres::Error>>,
}

impl PostgresDatabase {
    #[must_use]
    pub fn new<T: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
        client: Client,
        connection: tokio_postgres::Connection<tokio_postgres::Socket, T>,
    ) -> Self {
        let handle = moosicbox_task::spawn("Postgres database connection", connection);

        Self { client, handle }
    }
}

impl Drop for PostgresDatabase {
    fn drop(&mut self) {
        if let Err(e) = self.trigger_close() {
            log::error!("Failed to drop postgres database connection: {e:?}");
        }
    }
}

impl std::fmt::Debug for PostgresDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresDatabase")
            .field("client", &self.client)
            .finish_non_exhaustive()
    }
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Error)]
pub enum PostgresDatabaseError {
    #[error(transparent)]
    Postgres(#[from] tokio_postgres::Error),
    #[error("No ID")]
    NoId,
    #[error("No row")]
    NoRow,
    #[error("Invalid request")]
    InvalidRequest,
    #[error("Missing unique")]
    MissingUnique,
    #[error("Type Not Found: '{type_name}'")]
    TypeNotFound { type_name: String },
}

impl From<PostgresDatabaseError> for DatabaseError {
    fn from(value: PostgresDatabaseError) -> Self {
        Self::Postgres(value)
    }
}

#[async_trait]
impl Database for PostgresDatabase {
    async fn query(&self, query: &SelectQuery<'_>) -> Result<Vec<crate::Row>, DatabaseError> {
        Ok(select(
            &self.client,
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
            &self.client,
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
            &self.client,
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
            &self.client,
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
        Ok(insert_and_get_row(&self.client, statement.table_name, &statement.values).await?)
    }

    async fn exec_update(
        &self,
        statement: &UpdateStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        Ok(update_and_get_rows(
            &self.client,
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
            &self.client,
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
            &self.client,
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
            &self.client,
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
                &self.client,
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

    fn trigger_close(&self) -> Result<(), DatabaseError> {
        self.handle.abort();
        Ok(())
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
        columns.push((column.to_string(), column_value(row, column)?));
    }

    Ok(crate::Row { columns })
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

    log::trace!(
        "Running update_and_get_row query: {query} with params: {:?}",
        all_values
    );

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

    log::trace!(
        "Running update_and_get_rows query: {query} with params: {:?}",
        all_values
    );

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

fn format_identifier(identifier: &str) -> String {
    static NON_ALPHA_NUMERIC_REGEX: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"[^A-Za-z0-9_]").expect("Invalid Regex"));

    if NON_ALPHA_NUMERIC_REGEX.is_match(identifier) {
        identifier.to_string()
    } else {
        format!("\"{identifier}\"")
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
    let query = format!(
        "DELETE FROM {table_name} {} RETURNING * {}",
        build_where_clause(filters, &index),
        limit.map_or_else(String::new, |limit| format!("LIMIT {limit}"))
    );

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

    let filters = bexprs_to_params_opt(filters);
    let rows = client.query_raw(&statement, filters).await?;

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
    log::trace!("Running find_row query: {query} with params: {:?}", filters);

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
    log::trace!(
        "Running insert_and_get_row query: '{query}' with params: {:?}",
        values
    );

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

    log::trace!(
        "Running update chunk query: {query} with params: {:?}",
        all_values
    );

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

    log::trace!(
        "Running upsert chunk query: {query} with params: {:?}",
        all_values
    );

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

    fn expression_type(&self) -> ExpressionType {
        ExpressionType::DatabaseValue(self)
    }
}

impl<'a> tokio_postgres::types::FromSql<'a> for DatabaseValue {
    fn from_sql(
        ty: &tokio_postgres::types::Type,
        raw: &'a [u8],
    ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        Ok(match ty.name() {
            "bool" => Self::Bool(bool_from_sql(raw)?),
            "char" | "smallint" | "smallserial" | "int2" | "int" | "serial" | "int4" | "bigint"
            | "bigserial" | "int8" => Self::Number(int8_from_sql(raw)?),
            "real" | "float4" | "double precision" | "float8" => Self::Real(float8_from_sql(raw)?),
            "varchar" | "char(n)" | "text" | "name" | "citext" => {
                Self::String(text_from_sql(raw)?.to_string())
            }
            "timestamp" => Self::DateTime(NaiveDateTime::from_sql(ty, raw)?),
            _ => {
                return Err(Box::new(PostgresDatabaseError::TypeNotFound {
                    type_name: ty.to_string(),
                }))
            }
        })
    }

    fn from_sql_nullable(
        ty: &tokio_postgres::types::Type,
        raw: Option<&'a [u8]>,
    ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        Ok(match ty.name() {
            "bool" => raw
                .map(|raw| {
                    Ok::<_, Box<dyn std::error::Error + Sync + Send>>(Self::Bool(bool_from_sql(
                        raw,
                    )?))
                })
                .transpose()?
                .unwrap_or(Self::BoolOpt(None)),
            "char" | "smallint" | "smallserial" | "int2" | "int" | "serial" | "int4" | "bigint"
            | "bigserial" | "int8" => raw
                .map(|raw| {
                    Ok::<_, Box<dyn std::error::Error + Sync + Send>>(Self::Number(int8_from_sql(
                        raw,
                    )?))
                })
                .transpose()?
                .unwrap_or(Self::NumberOpt(None)),
            "real" | "float4" | "double precision" | "float8" => raw
                .map(|raw| {
                    Ok::<_, Box<dyn std::error::Error + Sync + Send>>(Self::Real(float8_from_sql(
                        raw,
                    )?))
                })
                .transpose()?
                .unwrap_or(Self::RealOpt(None)),
            "varchar" | "char(n)" | "text" | "name" | "citext" => raw
                .map(|raw| {
                    Ok::<_, Box<dyn std::error::Error + Sync + Send>>(Self::String(
                        text_from_sql(raw)?.to_string(),
                    ))
                })
                .transpose()?
                .unwrap_or(Self::StringOpt(None)),
            "timestamp" => raw
                .map(|raw| {
                    Ok::<_, Box<dyn std::error::Error + Sync + Send>>(Self::DateTime(
                        NaiveDateTime::from_sql(ty, raw)?,
                    ))
                })
                .transpose()?
                .unwrap_or(Self::Null),
            _ => {
                return Err(Box::new(PostgresDatabaseError::TypeNotFound {
                    type_name: ty.to_string(),
                }))
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

    fn to_sql_checked(
        &self,
        ty: &tokio_postgres::types::Type,
        out: &mut tokio_util::bytes::BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        log::trace!("to_sql_checked: ty={}, {ty:?}", ty.name());
        Ok(match &self.0 {
            DatabaseValue::Null | DatabaseValue::UNumberOpt(None) => IsNull::Yes,
            DatabaseValue::String(value) => value.to_sql(ty, out)?,
            DatabaseValue::StringOpt(value) => value.to_sql(ty, out)?,
            DatabaseValue::Bool(value) => i64::from(*value).to_sql(ty, out)?,
            DatabaseValue::BoolOpt(value) => value.map(i64::from).to_sql(ty, out)?,
            DatabaseValue::Number(value) => value.to_sql(ty, out)?,
            DatabaseValue::NumberOpt(value) => value.to_sql(ty, out)?,
            DatabaseValue::UNumber(value) => i64::try_from(*value)?.to_sql(ty, out)?,
            DatabaseValue::UNumberOpt(Some(value)) => i64::try_from(*value)?.to_sql(ty, out)?,
            DatabaseValue::Real(value) => value.to_sql(ty, out)?,
            DatabaseValue::RealOpt(value) => value.to_sql(ty, out)?,
            DatabaseValue::NowAdd(value) => value.to_sql(ty, out)?,
            DatabaseValue::Now => Utc::now().naive_utc().to_sql(ty, out)?,
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
