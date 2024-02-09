use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rusqlite::{types::Value, Connection, Row, Rows, Statement};
use thiserror::Error;

use crate::{
    BooleanExpression, Database, DatabaseError, DatabaseValue, DbConnection, Expression, Join, Sort,
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
}

impl From<RusqliteDatabaseError> for DatabaseError {
    fn from(value: RusqliteDatabaseError) -> Self {
        DatabaseError::Rusqlite(value)
    }
}

#[async_trait]
impl Database for RusqliteDatabase {
    async fn select(
        &self,
        table_name: &str,
        columns: &[&str],
        filters: Option<&[Box<dyn BooleanExpression>]>,
        joins: Option<&[Join]>,
        sort: Option<&[Sort]>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        Ok(select(
            &self.connection.lock().as_ref().unwrap().inner,
            table_name,
            columns,
            filters,
            joins,
            sort,
        )?)
    }

    async fn select_distinct(
        &self,
        table_name: &str,
        columns: &[&str],
        filters: Option<&[Box<dyn BooleanExpression>]>,
        joins: Option<&[Join]>,
        sort: Option<&[Sort]>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        Ok(select_distinct(
            &self.connection.lock().as_ref().unwrap().inner,
            table_name,
            columns,
            filters,
            joins,
            sort,
        )?)
    }

    async fn select_first(
        &self,
        table_name: &str,
        columns: &[&str],
        filters: Option<&[Box<dyn BooleanExpression>]>,
        joins: Option<&[Join]>,
        sort: Option<&[Sort]>,
    ) -> Result<Option<crate::Row>, DatabaseError> {
        Ok(find_row(
            &self.connection.lock().as_ref().unwrap().inner,
            table_name,
            columns,
            filters,
            joins,
            sort,
        )?)
    }

    async fn delete(
        &self,
        table_name: &str,
        filters: Option<&[Box<dyn BooleanExpression>]>,
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        Ok(delete(
            &self.connection.lock().as_ref().unwrap().inner,
            table_name,
            filters,
        )?)
    }

    async fn upsert(
        &self,
        table_name: &str,
        values: &[(&str, DatabaseValue)],
        filters: Option<&[Box<dyn BooleanExpression>]>,
    ) -> Result<crate::Row, DatabaseError> {
        Ok(upsert(
            &self.connection.lock().as_ref().unwrap().inner,
            table_name,
            values,
            filters,
        )?)
    }

    async fn upsert_multi(
        &self,
        table_name: &str,
        unique: &[&str],
        values: &[Vec<(&str, DatabaseValue)>],
    ) -> Result<Vec<crate::Row>, DatabaseError> {
        Ok(upsert_multi(
            &self.connection.lock().as_ref().unwrap().inner,
            table_name,
            unique,
            values,
        )?)
    }

    async fn update_and_get_row(
        &self,
        table_name: &str,
        id: DatabaseValue,
        values: &[(&str, DatabaseValue)],
    ) -> Result<Option<crate::Row>, DatabaseError> {
        Ok(update_and_get_row(
            &self.connection.lock().as_ref().unwrap().inner,
            table_name,
            id,
            values,
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
    id: DatabaseValue,
    values: &[(&str, DatabaseValue)],
) -> Result<Option<crate::Row>, RusqliteDatabaseError> {
    let variable_count: i32 = values
        .iter()
        .map(|v| match v.1 {
            DatabaseValue::BoolOpt(None) => 0,
            DatabaseValue::StringOpt(None) => 0,
            DatabaseValue::NumberOpt(None) => 0,
            DatabaseValue::UNumberOpt(None) => 0,
            DatabaseValue::RealOpt(None) => 0,
            _ => 1,
        })
        .sum();

    let statement = format!(
        "UPDATE {table_name} {} WHERE id=?{} RETURNING *",
        build_set_clause(values),
        variable_count + 1
    );

    let mut statement = connection.prepare_cached(&statement)?;
    bind_values(
        &mut statement,
        Some(&[to_values(values), vec![id]].concat()),
        false,
        0,
    )?;
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

fn build_set_clause(values: &[(&str, DatabaseValue)]) -> String {
    if values.is_empty() {
        "".to_string()
    } else {
        format!("SET {}", build_set_props(values).join(", "))
    }
}

fn build_set_props(values: &[(&str, DatabaseValue)]) -> Vec<String> {
    let mut i = 0;
    let mut props = Vec::new();
    for (name, value) in values {
        props.push(match value {
            DatabaseValue::Null => format!("{name}=NULL"),
            DatabaseValue::BoolOpt(None) => format!("{name}=NULL"),
            DatabaseValue::StringOpt(None) => format!("{name}=NULL"),
            DatabaseValue::NumberOpt(None) => format!("{name}=NULL"),
            DatabaseValue::UNumberOpt(None) => format!("{name}=NULL"),
            DatabaseValue::RealOpt(None) => format!("{name}=NULL"),
            DatabaseValue::NowAdd(add) => {
                format!(
                    "{name}=strftime('%Y-%m-%dT%H:%M:%f', DateTime('now', 'LocalTime', '{add}'))"
                )
            }
            _ => {
                i += 1;
                format!("`{name}`=?{i}").to_string()
            }
        });
    }
    props
}

fn build_values_clause(values: &[(&str, DatabaseValue)]) -> String {
    if values.is_empty() {
        "".to_string()
    } else {
        format!("VALUES({})", build_values_props(values).join(", "))
    }
}

fn build_values_props(values: &[(&str, DatabaseValue)]) -> Vec<String> {
    build_values_props_offset(values, 0, false)
}

#[allow(unused)]
fn build_values_props_offset(
    values: &[(&str, DatabaseValue)],
    offset: u16,
    constant_inc: bool,
) -> Vec<String> {
    let mut i = offset;
    let mut props = Vec::new();
    for (_name, value) in values {
        if constant_inc {
            i += 1;
        }
        props.push(match value {
            DatabaseValue::Null => "NULL".to_string(),
            DatabaseValue::BoolOpt(None) => "NULL".to_string(),
            DatabaseValue::StringOpt(None) => "NULL".to_string(),
            DatabaseValue::NumberOpt(None) => "NULL".to_string(),
            DatabaseValue::UNumberOpt(None) => "NULL".to_string(),
            DatabaseValue::RealOpt(None) => "NULL".to_string(),
            DatabaseValue::NowAdd(add) => {
                format!("strftime('%Y-%m-%dT%H:%M:%f', DateTime('now', 'LocalTime', '{add}'))")
            }
            _ => {
                if !constant_inc {
                    i += 1;
                }
                format!("?{i}").to_string()
            }
        });
    }
    props
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

#[allow(unused)]
fn exprs_to_values<'a>(values: &'a [Box<dyn Expression>]) -> Vec<DatabaseValue> {
    values
        .into_iter()
        .flat_map(|value| value.values().into_iter())
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
fn exprs_to_values_opt(values: Option<&[Box<dyn Expression>]>) -> Option<Vec<DatabaseValue>> {
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
    columns: &[&str],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    joins: Option<&[Join]>,
    sort: Option<&[Sort]>,
) -> Result<Vec<crate::Row>, RusqliteDatabaseError> {
    let query = format!(
        "SELECT {} FROM {table_name} {} {} {}",
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
    let mut statement = connection.prepare_cached(&format!(
        "DELETE FROM {table_name} {} RETURNING *",
        build_where_clause(filters),
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

fn find_row(
    connection: &Connection,
    table_name: &str,
    columns: &[&str],
    filters: Option<&[Box<dyn BooleanExpression>]>,
    joins: Option<&[Join]>,
    sort: Option<&[Sort]>,
) -> Result<Option<crate::Row>, RusqliteDatabaseError> {
    let query = format!(
        "SELECT {} FROM {table_name} {} {} {}",
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
    values: &[(&str, DatabaseValue)],
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

    bind_values(&mut statement, Some(&to_values(values)), false, 0)?;

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

pub fn upsert_multi(
    connection: &Connection,
    table_name: &str,
    unique: &[&str],
    values: &[Vec<(&str, DatabaseValue)>],
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
    values: &[Vec<(&str, DatabaseValue)>],
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
        .iter()
        .enumerate()
        .map(|(i, v)| {
            format!(
                "({})",
                build_values_props_offset(v, (i * expected_value_size) as u16, true).join(", ")
            )
        })
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

    let all_values = to_values(
        &values
            .iter()
            .flat_map(|f| f.iter().cloned())
            .collect::<Vec<_>>(),
    );

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
    values: &[(&str, DatabaseValue)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
) -> Result<crate::Row, RusqliteDatabaseError> {
    match find_row(connection, table_name, &["*"], filters, None, None)? {
        Some(row) => Ok(update_and_get_row(
            connection,
            table_name,
            row.id().ok_or(RusqliteDatabaseError::NoId)?,
            values,
        )?
        .unwrap()),
        None => insert_and_get_row(connection, table_name, values),
    }
}

#[allow(unused)]
fn upsert_and_get_row(
    connection: &Connection,
    table_name: &str,
    values: &[(&str, DatabaseValue)],
    filters: Option<&[Box<dyn BooleanExpression>]>,
) -> Result<crate::Row, RusqliteDatabaseError> {
    match find_row(connection, table_name, &["*"], filters, None, None)? {
        Some(row) => {
            let updated = update_and_get_row(
                connection,
                table_name,
                row.id().ok_or(RusqliteDatabaseError::NoId)?,
                &values,
            )?
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
        None => Ok(insert_and_get_row(connection, table_name, values)?),
    }
}
