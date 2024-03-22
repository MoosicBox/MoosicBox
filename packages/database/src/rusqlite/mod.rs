use std::{
    ops::Deref,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use rusqlite::{types::Value, Connection, Row, Rows, Statement};
use thiserror::Error;

use crate::{
    BooleanExpression, Database, DatabaseError, DatabaseValue, DeleteStatement, Expression,
    ExpressionType, InsertStatement, Join, SelectQuery, Sort, SortDirection, UpdateStatement,
    UpsertMultiStatement, UpsertStatement,
};

#[derive(Debug)]
pub struct RusqliteDatabase {
    connection: Arc<Mutex<Connection>>,
}

impl RusqliteDatabase {
    pub fn new(connection: Arc<Mutex<Connection>>) -> Self {
        Self { connection }
    }
}

trait ToSql {
    fn to_sql(&self) -> String;
}

impl<T: Expression + ?Sized> ToSql for T {
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
            ExpressionType::InList(value) => format!(
                "{}",
                value
                    .values
                    .iter()
                    .map(|value| value.to_sql())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            ExpressionType::Coalesce(value) => format!(
                "IFNULL({})",
                value
                    .values
                    .iter()
                    .map(|value| value.to_sql())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            ExpressionType::Identifier(value) => value.value.clone(),
            ExpressionType::SelectQuery(value) => {
                let joins = if let Some(joins) = &value.joins {
                    joins
                        .iter()
                        .map(|x| x.to_sql())
                        .collect::<Vec<_>>()
                        .join(" ")
                } else {
                    "".to_string()
                };

                let where_clause = if let Some(filters) = &value.filters {
                    if filters.is_empty() {
                        "".to_string()
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
                } else {
                    "".to_string()
                };

                let sort_clause = if let Some(sorts) = &value.sorts {
                    if sorts.is_empty() {
                        "".to_string()
                    } else {
                        format!(
                            "ORDER BY {}",
                            sorts
                                .iter()
                                .map(|x| x.to_sql())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    }
                } else {
                    "".to_string()
                };

                let limit = if let Some(limit) = value.limit {
                    format!("LIMIT {}", limit)
                } else {
                    "".to_string()
                };

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
                DatabaseValue::Null => format!("NULL"),
                DatabaseValue::BoolOpt(None) => format!("NULL"),
                DatabaseValue::StringOpt(None) => format!("NULL"),
                DatabaseValue::NumberOpt(None) => format!("NULL"),
                DatabaseValue::UNumberOpt(None) => format!("NULL"),
                DatabaseValue::RealOpt(None) => format!("NULL"),
                DatabaseValue::Now => format!("strftime('%Y-%m-%dT%H:%M:%f', 'now')"),
                DatabaseValue::NowAdd(ref add) => {
                    format!("strftime('%Y-%m-%dT%H:%M:%f', DateTime('now', 'LocalTime', '{add}'))")
                }
                _ => format!("?"),
            },
        }
    }
}

#[derive(Debug, Error)]
pub enum RusqliteDatabaseError {
    #[error(transparent)]
    Rusqlite(#[from] rusqlite::Error),
    #[error("No ID")]
    NoId,
    #[error("No row")]
    NoRow,
    #[error("Invalid request")]
    InvalidRequest,
    #[error("Missing unique")]
    MissingUnique,
}

impl From<RusqliteDatabaseError> for DatabaseError {
    fn from(value: RusqliteDatabaseError) -> Self {
        DatabaseError::Rusqlite(value)
    }
}

#[async_trait]
impl Database for RusqliteDatabase {
    async fn query(&self, query: &SelectQuery<'_>) -> Result<Vec<crate::Row>, DatabaseError> {
        Ok(select(
            &self.connection.lock().as_ref().unwrap(),
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
            &self.connection.lock().as_ref().unwrap(),
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
            &self.connection.lock().as_ref().unwrap(),
            statement.table_name,
            statement.filters.as_deref(),
            statement.limit,
        )?)
    }

    async fn exec_insert(
        &self,
        statement: &InsertStatement<'_>,
    ) -> Result<crate::Row, DatabaseError> {
        Ok(insert_and_get_row(
            &self.connection.lock().as_ref().unwrap(),
            statement.table_name,
            &statement.values,
        )?)
    }

    async fn exec_update(
        &self,
        statement: &UpdateStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        Ok(update_and_get_rows(
            &self.connection.lock().as_ref().unwrap(),
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
            &self.connection.lock().as_ref().unwrap(),
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
            &self.connection.lock().as_ref().unwrap(),
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
        Ok(upsert_and_get_row(
            &self.connection.lock().as_ref().unwrap(),
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
        Ok(upsert_multi(
            &self.connection.lock().as_ref().unwrap(),
            statement.table_name,
            statement
                .unique
                .as_ref()
                .ok_or(RusqliteDatabaseError::MissingUnique)?,
            &statement.values,
        )?)
    }
}

impl From<Value> for DatabaseValue {
    fn from(value: Value) -> Self {
        match value {
            Value::Null => DatabaseValue::Null,
            Value::Integer(value) => DatabaseValue::Number(value),
            Value::Real(value) => DatabaseValue::Real(value),
            Value::Text(value) => DatabaseValue::String(value),
            Value::Blob(_value) => unimplemented!("Blob types are not supported yet"),
        }
    }
}

fn from_row(column_names: &[String], row: &Row<'_>) -> Result<crate::Row, RusqliteDatabaseError> {
    let mut columns = vec![];

    for column in column_names {
        columns.push((
            column.to_string(),
            row.get::<_, Value>(column.as_str())?.into(),
        ));
    }

    Ok(crate::Row { columns })
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
        .into_iter()
        .flat_map(|(_, value)| value.params().unwrap_or(vec![]).into_iter().cloned())
        .map(|x| x.into())
        .collect::<Vec<_>>();
    let mut all_filter_values = filters
        .map(|filters| {
            filters
                .into_iter()
                .flat_map(|value| value.params().unwrap_or(vec![]).into_iter().cloned())
                .map(|x| x.into())
                .collect::<Vec<_>>()
        })
        .unwrap_or(vec![]);

    if limit.is_some() {
        all_filter_values.extend(all_filter_values.clone());
    }

    let all_values = [all_values, all_filter_values].concat();

    log::trace!(
        "Running update query: {query} with params: {:?}",
        all_values
    );

    let mut statement = connection.prepare_cached(&query)?;

    bind_values(&mut statement, Some(&all_values), false, 0)?;

    let column_names = statement
        .column_names()
        .iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>();

    let mut query = statement.raw_query();

    Ok(query
        .next()?
        .map(|row| from_row(&column_names, row))
        .transpose()?)
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
        .into_iter()
        .flat_map(|(_, value)| value.params().unwrap_or(vec![]).into_iter().cloned())
        .map(|x| x.into())
        .collect::<Vec<_>>();
    let mut all_filter_values = filters
        .map(|filters| {
            filters
                .into_iter()
                .flat_map(|value| value.params().unwrap_or(vec![]).into_iter().cloned())
                .map(|x| x.into())
                .collect::<Vec<_>>()
        })
        .unwrap_or(vec![]);

    if limit.is_some() {
        all_filter_values.extend(all_filter_values.clone());
    }

    let all_values = [all_values, all_filter_values].concat();

    log::trace!(
        "Running update query: {query} with params: {:?}",
        all_values
    );

    let mut statement = connection.prepare_cached(&query)?;
    bind_values(&mut statement, Some(&all_values), false, 0)?;
    let column_names = statement
        .column_names()
        .iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>();

    to_rows(&column_names, statement.raw_query())
}

fn build_join_clauses(joins: Option<&[Join]>) -> String {
    if let Some(joins) = joins {
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
    } else {
        "".into()
    }
}

fn build_where_clause(filters: Option<&[Box<dyn BooleanExpression>]>) -> String {
    if let Some(filters) = filters {
        if filters.is_empty() {
            "".to_string()
        } else {
            format!("WHERE {}", build_where_props(filters).join(" AND "))
        }
    } else {
        "".to_string()
    }
}

fn build_where_props(filters: &[Box<dyn BooleanExpression>]) -> Vec<String> {
    filters
        .iter()
        .map(|filter| filter.deref().to_sql())
        .collect()
}

fn build_sort_clause(sorts: Option<&[Sort]>) -> String {
    if let Some(sorts) = sorts {
        if sorts.is_empty() {
            "".to_string()
        } else {
            format!("ORDER BY {}", build_sort_props(sorts).join(", "))
        }
    } else {
        "".to_string()
    }
}

fn build_sort_props(sorts: &[Sort]) -> Vec<String> {
    sorts.iter().map(|sort| sort.to_sql()).collect()
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
    if let Some(limit) = limit {
        if let Some(query) = query {
            format!("rowid IN ({query} LIMIT {limit})")
        } else {
            "".into()
        }
    } else {
        "".into()
    }
}

fn build_set_clause(values: &[(&str, Box<dyn Expression>)]) -> String {
    if values.is_empty() {
        "".to_string()
    } else {
        format!("SET {}", build_set_props(values).join(", "))
    }
}

fn build_set_props(values: &[(&str, Box<dyn Expression>)]) -> Vec<String> {
    values
        .into_iter()
        .map(|(name, value)| format!("{name}={}", value.deref().to_sql()))
        .collect()
}

fn build_values_clause(values: &[(&str, Box<dyn Expression>)]) -> String {
    if values.is_empty() {
        "".to_string()
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

fn bind_values(
    statement: &mut Statement<'_>,
    values: Option<&[RusqliteDatabaseValue]>,
    constant_inc: bool,
    offset: usize,
) -> Result<usize, RusqliteDatabaseError> {
    if let Some(values) = values {
        let mut i = 1 + offset;
        for value in values {
            match value.deref() {
                DatabaseValue::String(value) => {
                    statement.raw_bind_parameter(i, value)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::StringOpt(Some(value)) => {
                    statement.raw_bind_parameter(i, value)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::StringOpt(None) => (),
                DatabaseValue::Bool(value) => {
                    statement.raw_bind_parameter(i, if *value { 1 } else { 0 })?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::BoolOpt(Some(value)) => {
                    statement.raw_bind_parameter(i, value)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::BoolOpt(None) => (),
                DatabaseValue::Number(value) => {
                    statement.raw_bind_parameter(i, *value)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::NumberOpt(Some(value)) => {
                    statement.raw_bind_parameter(i, *value)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::NumberOpt(None) => (),
                DatabaseValue::UNumber(value) => {
                    statement.raw_bind_parameter(i, *value)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::UNumberOpt(Some(value)) => {
                    statement.raw_bind_parameter(i, *value)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::UNumberOpt(None) => (),
                DatabaseValue::Real(value) => {
                    statement.raw_bind_parameter(i, *value)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::RealOpt(Some(value)) => {
                    statement.raw_bind_parameter(i, *value)?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::RealOpt(None) => (),
                DatabaseValue::NowAdd(_add) => (),
                DatabaseValue::Now => (),
                DatabaseValue::DateTime(value) => {
                    // FIXME: Actually format the date
                    statement.raw_bind_parameter(i, value.to_string())?;
                    if !constant_inc {
                        i += 1;
                    }
                }
                DatabaseValue::Null => (),
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

fn to_values<'a>(values: &'a [(&str, DatabaseValue)]) -> Vec<RusqliteDatabaseValue> {
    values
        .into_iter()
        .map(|(_key, value)| value.clone())
        .map(|x| x.into())
        .collect::<Vec<_>>()
}

fn exprs_to_values<'a>(values: &'a [(&str, Box<dyn Expression>)]) -> Vec<RusqliteDatabaseValue> {
    values
        .into_iter()
        .flat_map(|value| value.1.values().into_iter())
        .flatten()
        .cloned()
        .map(|x| x.into())
        .collect::<Vec<_>>()
}

fn bexprs_to_values<'a>(values: &'a [Box<dyn BooleanExpression>]) -> Vec<RusqliteDatabaseValue> {
    values
        .into_iter()
        .flat_map(|value| value.values().into_iter())
        .flatten()
        .cloned()
        .map(|x| x.into())
        .collect::<Vec<_>>()
}

#[allow(unused)]
fn to_values_opt(values: Option<&[(&str, DatabaseValue)]>) -> Option<Vec<RusqliteDatabaseValue>> {
    values.map(|x| to_values(x))
}

#[allow(unused)]
fn exprs_to_values_opt(
    values: Option<&[(&str, Box<dyn Expression>)]>,
) -> Option<Vec<RusqliteDatabaseValue>> {
    values.map(|x| exprs_to_values(x))
}

fn bexprs_to_values_opt(
    values: Option<&[Box<dyn BooleanExpression>]>,
) -> Option<Vec<RusqliteDatabaseValue>> {
    values.map(|x| bexprs_to_values(x))
}

fn select(
    connection: &Connection,
    table_name: &str,
    distinct: bool,
    columns: &[&str],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    joins: Option<&[Join]>,
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
        if let Some(limit) = limit {
            format!("LIMIT {limit}")
        } else {
            "".to_string()
        }
    );

    log::trace!(
        "Running select query: {query} with params: {:?}",
        filters.map(|f| f.iter().flat_map(|x| x.params()).collect::<Vec<_>>())
    );

    let mut statement = connection.prepare_cached(&query)?;
    let column_names = statement
        .column_names()
        .iter()
        .map(|x| x.to_string())
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
    let query = format!(
        "DELETE FROM {table_name} {} RETURNING * {}",
        build_where_clause(filters),
        if let Some(limit) = limit {
            format!("LIMIT {limit}")
        } else {
            "".to_string()
        }
    );

    log::trace!(
        "Running delete query: {query} with params: {:?}",
        filters.map(|f| f.iter().flat_map(|x| x.params()).collect::<Vec<_>>())
    );

    let mut statement = connection.prepare_cached(&query)?;
    let column_names = statement
        .column_names()
        .iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>();

    bind_values(
        &mut statement,
        bexprs_to_values_opt(filters).as_deref(),
        false,
        0,
    )?;

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
        .map(|x| x.to_string())
        .collect::<Vec<_>>();

    bind_values(
        &mut statement,
        bexprs_to_values_opt(filters).as_deref(),
        false,
        0,
    )?;

    log::trace!(
        "Running find_row query: {query} with params: {:?}",
        filters.map(|f| f.iter().flat_map(|x| x.params()).collect::<Vec<_>>())
    );

    let mut query = statement.raw_query();

    Ok(query
        .next()?
        .map(|row| from_row(&column_names, row))
        .transpose()?)
}

fn insert_and_get_row(
    connection: &Connection,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
) -> Result<crate::Row, RusqliteDatabaseError> {
    let column_names = values
        .into_iter()
        .map(|(key, _v)| format!("`{key}`"))
        .collect::<Vec<_>>()
        .join(", ");

    let query = format!(
        "INSERT INTO {table_name} ({column_names}) {} RETURNING *",
        build_values_clause(values),
    );

    let mut statement = connection.prepare_cached(&query)?;
    let column_names = statement
        .column_names()
        .iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>();

    bind_values(&mut statement, Some(&exprs_to_values(values)), false, 0)?;

    log::trace!(
        "Running insert_and_get_row query: {query} with params: {:?}",
        values
            .iter()
            .flat_map(|(_, x)| x.params())
            .collect::<Vec<_>>()
    );

    let mut query = statement.raw_query();

    Ok(query
        .next()?
        .map(|row| from_row(&column_names, row))
        .ok_or(RusqliteDatabaseError::NoRow)??)
}

pub fn update_multi(
    connection: &Connection,
    table_name: &str,
    values: &[Vec<(&str, Box<dyn Expression>)>],
    filters: Option<Vec<Box<dyn BooleanExpression>>>,
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
                &filters,
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
            &filters,
            limit,
        )?);
    }

    Ok(results)
}

fn update_chunk(
    connection: &Connection,
    table_name: &str,
    values: &[Vec<(&str, Box<dyn Expression>)>],
    filters: &Option<Vec<Box<dyn BooleanExpression>>>,
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
            build_where_clause(filters.as_deref()),
        )
    });

    let query = format!(
        "
        UPDATE {table_name} ({column_names})
        {}
        SET {set_clause}
        RETURNING *",
        build_update_where_clause(filters.as_deref(), limit, select_query.as_deref()),
    );

    let all_values = values
        .into_iter()
        .flat_map(|x| x.into_iter())
        .flat_map(|(_, value)| value.params().unwrap_or(vec![]).into_iter().cloned())
        .map(|x| x.into())
        .collect::<Vec<_>>();
    let mut all_filter_values = filters
        .as_ref()
        .map(|filters| {
            filters
                .into_iter()
                .flat_map(|value| {
                    value
                        .params()
                        .unwrap_or(vec![])
                        .into_iter()
                        .cloned()
                        .map(|x| x.into())
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or(vec![]);

    if limit.is_some() {
        all_filter_values.extend(all_filter_values.clone());
    }

    let all_values = [all_values, all_filter_values].concat();

    log::trace!(
        "Running update chunk query: {query} with params: {:?}",
        all_values
    );

    let mut statement = connection.prepare_cached(&query)?;
    let column_names = statement
        .column_names()
        .iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>();

    bind_values(&mut statement, Some(&all_values), true, 0)?;

    to_rows(&column_names, statement.raw_query())
}

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
        .into_iter()
        .map(|v| format!("({})", build_values_props(&v).join(", ")))
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
        .into_iter()
        .flat_map(|x| x.into_iter())
        .flat_map(|(_, value)| value.params().unwrap_or(vec![]).into_iter().cloned())
        .map(|x| x.into())
        .collect::<Vec<_>>();

    log::trace!(
        "Running upsert chunk query: {query} with params: {:?}",
        all_values
    );

    let mut statement = connection.prepare_cached(&query)?;
    let column_names = statement
        .column_names()
        .iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>();

    bind_values(&mut statement, Some(&all_values), true, 0)?;

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
                update_and_get_row(connection, table_name, &values, filters, limit)?.unwrap();

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

#[derive(Debug, Clone)]
pub struct RusqliteDatabaseValue(DatabaseValue);

impl From<DatabaseValue> for RusqliteDatabaseValue {
    fn from(value: DatabaseValue) -> Self {
        RusqliteDatabaseValue(value)
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
        match self.0 {
            DatabaseValue::Null => true,
            DatabaseValue::BoolOpt(None) => true,
            DatabaseValue::RealOpt(None) => true,
            DatabaseValue::StringOpt(None) => true,
            DatabaseValue::NumberOpt(None) => true,
            DatabaseValue::UNumberOpt(None) => true,
            _ => false,
        }
    }

    fn expression_type(&self) -> crate::ExpressionType {
        ExpressionType::DatabaseValue(self.deref())
    }
}
