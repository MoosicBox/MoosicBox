use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rusqlite::{types::Value, Connection, Row, Rows, Statement};
use thiserror::Error;

use crate::{
    BooleanExpression, Database, DatabaseError, DatabaseValue, DbConnection, DeleteStatement,
    Expression, Join, SelectQuery, Sort, UpdateStatement, UpsertMultiStatement,
};

impl From<Connection> for DbConnection {
    fn from(value: Connection) -> Self {
        DbConnection { inner: value }
    }
}

pub struct RusqliteDatabase {
    connection: Arc<Mutex<DbConnection>>,
}

impl RusqliteDatabase {
    pub fn new(connection: Arc<Mutex<DbConnection>>) -> Self {
        Self { connection }
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
            &self.connection.lock().as_ref().unwrap().inner,
            query.table_name,
            query.distinct,
            query.columns,
            query.filters.as_deref(),
            query.joins.as_deref(),
            query.sorts.as_deref(),
        )?)
    }

    async fn query_first(
        &self,
        query: &SelectQuery<'_>,
    ) -> Result<Option<crate::Row>, DatabaseError> {
        Ok(find_row(
            &self.connection.lock().as_ref().unwrap().inner,
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
            &self.connection.lock().as_ref().unwrap().inner,
            statement.table_name,
            statement.filters.as_deref(),
        )?)
    }

    async fn exec_update(
        &self,
        statement: &UpdateStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        Ok(update_and_get_rows(
            &self.connection.lock().as_ref().unwrap().inner,
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
        )?)
    }

    async fn exec_update_first(
        &self,
        statement: &UpdateStatement<'_>,
    ) -> Result<Option<crate::Row>, DatabaseError> {
        Ok(update_and_get_row(
            &self.connection.lock().as_ref().unwrap().inner,
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
        )?)
    }

    async fn exec_upsert(
        &self,
        statement: &UpdateStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        Ok(upsert(
            &self.connection.lock().as_ref().unwrap().inner,
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
        )?)
    }

    async fn exec_upsert_multi(
        &self,
        statement: &UpsertMultiStatement<'_>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        Ok(upsert_multi(
            &self.connection.lock().as_ref().unwrap().inner,
            statement.table_name,
            statement
                .unique
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
) -> Result<Option<crate::Row>, RusqliteDatabaseError> {
    let statement = format!(
        "UPDATE {table_name} {} {} RETURNING *",
        build_set_clause(values),
        build_where_clause(filters)
    );

    let mut statement = connection.prepare_cached(&statement)?;
    bind_values(&mut statement, Some(&exprs_to_values(values)), false, 0)?;
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
) -> Result<Vec<crate::Row>, RusqliteDatabaseError> {
    let query = format!(
        "UPDATE {table_name} {} {} RETURNING *",
        build_set_clause(values),
        build_where_clause(filters)
    );

    let mut statement = connection.prepare_cached(&query)?;
    let offset = bind_values(&mut statement, Some(&exprs_to_values(values)), false, 0)?;
    bind_values(
        &mut statement,
        bexprs_to_values_opt(filters).as_deref(),
        false,
        offset,
    )?;
    let column_names = statement
        .column_names()
        .iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>();

    log::trace!(
        "Running delete query: {query} with params: {:?}",
        filters.map(|f| f.iter().flat_map(|x| x.values()).collect::<Vec<_>>())
    );

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
    filters.iter().map(|filter| filter.to_sql()).collect()
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
    sorts
        .iter()
        .map(|sort| {
            format!(
                "{} {}",
                sort.expression.to_sql(),
                match sort.direction {
                    crate::SortDirection::Asc => "ASC",
                    crate::SortDirection::Desc => "DESC",
                }
            )
        })
        .collect()
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
        .map(|(name, value)| format!("{name}={}", value.to_param()))
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
    values.iter().map(|(_, value)| value.to_param()).collect()
}

fn bind_values(
    statement: &mut Statement<'_>,
    values: Option<&[DatabaseValue]>,
    constant_inc: bool,
    offset: usize,
) -> Result<usize, RusqliteDatabaseError> {
    if let Some(values) = values {
        let mut i = 1 + offset;
        for value in values {
            match value {
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

    Ok(results)
}

fn to_values<'a>(values: &'a [(&str, DatabaseValue)]) -> Vec<DatabaseValue> {
    values
        .into_iter()
        .map(|(_key, value)| value.clone())
        .collect::<Vec<_>>()
}

fn exprs_to_values<'a>(values: &'a [(&str, Box<dyn Expression>)]) -> Vec<DatabaseValue> {
    values
        .into_iter()
        .flat_map(|value| value.1.values().into_iter())
        .flatten()
        .cloned()
        .collect::<Vec<_>>()
}

fn bexprs_to_values<'a>(values: &'a [Box<dyn BooleanExpression>]) -> Vec<DatabaseValue> {
    values
        .into_iter()
        .flat_map(|value| value.values().into_iter())
        .flatten()
        .cloned()
        .collect::<Vec<_>>()
}

#[allow(unused)]
fn to_values_opt(values: Option<&[(&str, DatabaseValue)]>) -> Option<Vec<DatabaseValue>> {
    values.map(|x| to_values(x))
}

#[allow(unused)]
fn exprs_to_values_opt(
    values: Option<&[(&str, Box<dyn Expression>)]>,
) -> Option<Vec<DatabaseValue>> {
    values.map(|x| exprs_to_values(x))
}

fn bexprs_to_values_opt(
    values: Option<&[Box<dyn BooleanExpression>]>,
) -> Option<Vec<DatabaseValue>> {
    values.map(|x| bexprs_to_values(x))
}

#[allow(unused)]
fn select_distinct(
    connection: &Connection,
    table_name: &str,
    columns: &[&str],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    joins: Option<&[Join]>,
    sort: Option<&[Sort]>,
) -> Result<Vec<crate::Row>, RusqliteDatabaseError> {
    let mut statement = connection.prepare_cached(&format!(
        "SELECT DISTINCT {} FROM {table_name} {} {} {}",
        columns.join(", "),
        build_join_clauses(joins),
        build_where_clause(filters),
        build_sort_clause(sort),
    ))?;
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

fn select(
    connection: &Connection,
    table_name: &str,
    distinct: bool,
    columns: &[&str],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    joins: Option<&[Join]>,
    sort: Option<&[Sort]>,
) -> Result<Vec<crate::Row>, RusqliteDatabaseError> {
    let query = format!(
        "SELECT {} {} FROM {table_name} {} {} {}",
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

    log::trace!(
        "Running select query: {query} with params: {:?}",
        filters.map(|f| f.iter().flat_map(|x| x.values()).collect::<Vec<_>>())
    );

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
) -> Result<Vec<crate::Row>, RusqliteDatabaseError> {
    let query = format!(
        "DELETE FROM {table_name} {} RETURNING *",
        build_where_clause(filters),
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
        "Running delete query: {query} with params: {:?}",
        filters.map(|f| f.iter().flat_map(|x| x.values()).collect::<Vec<_>>())
    );

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
        "SELECT {} {} FROM {table_name} {} {} {}",
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
        filters.map(|f| f.iter().flat_map(|x| x.values()).collect::<Vec<_>>())
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
            .flat_map(|(_, x)| x.values())
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
            )?);
            last_i = i;
            pos = 0;
        }
        i += 1;
        pos += count;
    }

    if i > last_i {
        results.append(&mut update_chunk(
            connection,
            table_name,
            &values[last_i..],
        )?);
    }

    Ok(results)
}

fn update_chunk(
    connection: &Connection,
    table_name: &str,
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

    let statement = format!(
        "
        UPDATE {table_name} ({column_names})
        SET {set_clause}
        RETURNING *"
    );

    let all_values = &values
        .into_iter()
        .flat_map(|x| x.into_iter())
        .flat_map(|(_, value)| value.values().unwrap_or(vec![]).into_iter().cloned())
        .collect::<Vec<_>>();

    let mut statement = connection.prepare_cached(&statement)?;
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
    unique: &[&str],
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
    unique: &[&str],
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

    let unique_conflict = unique.join(", ");

    let statement = format!(
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
        .flat_map(|(_, value)| value.values().unwrap_or(vec![]).into_iter().cloned())
        .collect::<Vec<_>>();

    let mut statement = connection.prepare_cached(&statement)?;
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
) -> Result<Vec<crate::Row>, RusqliteDatabaseError> {
    let rows = update_and_get_rows(connection, table_name, values, filters)?;

    Ok(if rows.is_empty() {
        rows
    } else {
        vec![insert_and_get_row(connection, table_name, values)?]
    })
}

#[allow(unused)]
fn upsert_and_get_row(
    connection: &Connection,
    table_name: &str,
    values: &[(&str, Box<dyn Expression>)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
) -> Result<crate::Row, RusqliteDatabaseError> {
    match find_row(connection, table_name, false, &["*"], filters, None, None)? {
        Some(row) => {
            let updated = update_and_get_row(connection, table_name, &values, filters)?.unwrap();

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
