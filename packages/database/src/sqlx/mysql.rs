use std::{ops::Deref, pin::Pin, sync::Arc};

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use sqlx::{
    Column, Executor, MySql, MySqlPool, Row, Statement, TypeInfo, Value, ValueRef,
    mysql::{MySqlArguments, MySqlRow, MySqlValueRef},
    query::Query,
};
use thiserror::Error;
use tokio::sync::Mutex;

use crate::{
    Database, DatabaseError, DatabaseValue, DeleteStatement, InsertStatement, SelectQuery,
    UpdateStatement, UpsertMultiStatement, UpsertStatement,
    query::{BooleanExpression, Expression, ExpressionType, Join, Sort, SortDirection},
};

trait ToSql {
    fn to_sql(&self) -> String;
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
            ExpressionType::Literal(value) => value.value.to_string(),
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
                | DatabaseValue::NumberOpt(None)
                | DatabaseValue::UNumberOpt(None)
                | DatabaseValue::RealOpt(None) => "NULL".to_string(),
                DatabaseValue::Now => "NOW()".to_string(),
                DatabaseValue::NowAdd(add) => format!("DATE_ADD(NOW(), {add}))"),
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

#[derive(Debug)]
pub struct MySqlSqlxDatabase {
    connection: Arc<Mutex<MySqlPool>>,
}

impl MySqlSqlxDatabase {
    pub const fn new(connection: Arc<Mutex<MySqlPool>>) -> Self {
        Self { connection }
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
        Self::MysqlSqlx(value)
    }
}

#[async_trait]
impl Database for MySqlSqlxDatabase {
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
        )
        .await?)
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
        )
        .await?)
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
        )
        .await?)
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
            &*self.connection.lock().await,
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
            &*self.connection.lock().await,
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
            &*self.connection.lock().await,
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
            &*self.connection.lock().await,
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
            &*self.connection.lock().await,
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
                &*self.connection.lock().await,
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

        let connection = self.connection.lock().await;
        let statement = connection
            .prepare(statement)
            .await
            .map_err(SqlxDatabaseError::Sqlx)?;
        let query = statement.query();

        connection
            .execute(query)
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
                crate::schema::DataType::Bool => query.push_str("BOOLEAN"),
                crate::schema::DataType::SmallInt => {
                    query.push_str("SMALLINT");
                }
                crate::schema::DataType::Int => {
                    query.push_str("INT");
                }
                crate::schema::DataType::BigInt => {
                    query.push_str("BIGINT");
                }
                crate::schema::DataType::Real => query.push_str("FLOAT"),
                crate::schema::DataType::Double => query.push_str("DOUBLE"),
                crate::schema::DataType::Decimal(precision, scale) => {
                    query.push_str("DECIMAL(");
                    query.push_str(&precision.to_string());
                    query.push(',');
                    query.push_str(&scale.to_string());
                    query.push(')');
                }
                crate::schema::DataType::DateTime => query.push_str("DATETIME"),
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
                        query.push_str("NOW() + ");
                        query.push_str(x);
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
            query.push_str(") REFERENCES (");
            query.push_str(target);
            query.push(')');
        }

        query.push(')');

        self.exec_raw(&query).await?;

        Ok(())
    }
}

fn column_value(value: &MySqlValueRef<'_>) -> Result<DatabaseValue, sqlx::Error> {
    if value.is_null() {
        return Ok(DatabaseValue::Null);
    }
    let owned = sqlx::ValueRef::to_owned(value);
    match value.type_info().name() {
        "BOOL" => Ok(DatabaseValue::Bool(owned.try_decode()?)),
        "CHAR" | "SMALLINT" | "SMALLSERIAL" | "INT2" | "INT" | "SERIAL" | "INT4" | "BIGINT"
        | "BIGSERIAL" | "INT8" => Ok(DatabaseValue::Number(owned.try_decode()?)),
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

fn from_row(column_names: &[String], row: &MySqlRow) -> Result<crate::Row, SqlxDatabaseError> {
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
    connection: &MySqlPool,
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
        "UPDATE {table_name} {} {} RETURNING *",
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

    let statement = connection.prepare(&query).await?;

    let column_names = statement
        .columns()
        .iter()
        .map(|x| x.name().to_string())
        .collect::<Vec<_>>();

    let query = bind_values(statement.query(), Some(&all_values))?;

    let mut stream = query.fetch(connection);
    let pg_row: Option<MySqlRow> = stream.next().await.transpose()?;

    pg_row.map(|row| from_row(&column_names, &row)).transpose()
}

async fn update_and_get_rows(
    connection: &MySqlPool,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, SqlxDatabaseError> {
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
        String::new()
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
    mut rows: Pin<Box<(dyn Stream<Item = Result<MySqlRow, sqlx::Error>> + Send + 'a)>>,
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
    connection: &MySqlPool,
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
    connection: &MySqlPool,
    table_name: &str,
    filters: Option<&[Box<dyn BooleanExpression>]>,
    limit: Option<usize>,
) -> Result<Vec<crate::Row>, SqlxDatabaseError> {
    let query = format!(
        "DELETE FROM {table_name} {} RETURNING * {}",
        build_where_clause(filters),
        limit.map_or_else(String::new, |limit| format!("LIMIT {limit}"))
    );

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

    let filters = bexprs_to_values_opt(filters);
    let query = bind_values(statement.query(), filters.as_deref())?;

    to_rows(&column_names, query.fetch(connection)).await
}

async fn find_row(
    connection: &MySqlPool,
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
    connection: &MySqlPool,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
) -> Result<crate::Row, SqlxDatabaseError> {
    let column_names = values
        .iter()
        .map(|(key, _v)| format!("`{key}`"))
        .collect::<Vec<_>>()
        .join(", ");

    let query = format!(
        "INSERT INTO {table_name} ({column_names}) {} RETURNING *",
        build_values_clause(values),
    );

    let statement = connection.prepare(&query).await?;
    let column_names = statement
        .columns()
        .iter()
        .map(|x| x.name().to_string())
        .collect::<Vec<_>>();

    log::trace!(
        "Running insert_and_get_row query: {query} with params: {:?}",
        values
            .iter()
            .filter_map(|(_, x)| x.params())
            .collect::<Vec<_>>()
    );

    let values = exprs_to_values(values);
    let query = bind_values(statement.query(), Some(&values))?;

    let mut query = query.fetch(connection);

    query
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
    connection: &MySqlPool,
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
    connection: &MySqlPool,
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
        RETURNING *",
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
    connection: &MySqlPool,
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
    connection: &MySqlPool,
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
        RETURNING *"
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
    connection: &MySqlPool,
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
    connection: &MySqlPool,
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

    fn expression_type(&self) -> ExpressionType {
        ExpressionType::DatabaseValue(self)
    }
}
