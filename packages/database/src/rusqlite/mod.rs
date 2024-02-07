use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rusqlite::{types::Value, Connection, Row, Statement};
use thiserror::Error;

use crate::{Database, DatabaseError, DatabaseValue, DbConnection};

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
}

impl From<RusqliteDatabaseError> for DatabaseError {
    fn from(value: RusqliteDatabaseError) -> Self {
        DatabaseError::Rusqlite(value)
    }
}

#[async_trait]
impl Database for RusqliteDatabase {
    async fn update_and_get_row<'a>(
        &self,
        table_name: &str,
        id: DatabaseValue,
        values: &[(&'a str, DatabaseValue)],
    ) -> Result<Option<crate::Row>, DatabaseError> {
        let result = update_and_get_row(
            &self.connection.lock().as_ref().unwrap().inner,
            table_name,
            id,
            values,
        )?;

        Ok(result)
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

fn from_row(column_names: Vec<String>, row: &Row<'_>) -> Result<crate::Row, RusqliteDatabaseError> {
    let mut columns = vec![];

    for column in column_names {
        columns.push((
            column.to_string(),
            row.get::<_, Value>(column.as_str())?.into(),
        ));
    }

    Ok(crate::Row { columns })
}

fn update_and_get_row<'a>(
    connection: &'a Connection,
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
    bind_values(&mut statement, &[values, &[("id", id)]].concat(), false)?;
    let column_names = statement
        .column_names()
        .iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>();
    let mut query = statement.raw_query();

    Ok(query
        .next()?
        .map(|row| from_row(column_names, row))
        .transpose()?)
}

#[allow(unused)]
fn build_where_clause<'a>(values: &'a Vec<(&'a str, DatabaseValue)>) -> String {
    if values.is_empty() {
        "".to_string()
    } else {
        format!("WHERE {}", build_where_props(values).join(" AND "))
    }
}

#[allow(unused)]
fn build_where_props<'a>(values: &'a [(&'a str, DatabaseValue)]) -> Vec<String> {
    values
        .iter()
        .map(|(name, value)| match value {
            DatabaseValue::Null => format!("{name} is NULL"),
            DatabaseValue::BoolOpt(None) => format!("{name} is NULL"),
            DatabaseValue::StringOpt(None) => format!("{name} is NULL"),
            DatabaseValue::NumberOpt(None) => format!("{name} is NULL"),
            DatabaseValue::UNumberOpt(None) => format!("{name} is NULL"),
            DatabaseValue::RealOpt(None) => format!("{name} is NULL"),
            DatabaseValue::NowAdd(add) => {
                format!(
                    "{name} = strftime('%Y-%m-%dT%H:%M:%f', DateTime('now', 'LocalTime', '{add}'))"
                )
            }
            _ => format!("{name}=?"),
        })
        .collect()
}

fn build_set_clause<'a>(values: &'a [(&'a str, DatabaseValue)]) -> String {
    if values.is_empty() {
        "".to_string()
    } else {
        format!("SET {}", build_set_props(values).join(", "))
    }
}

fn build_set_props<'a>(values: &'a [(&'a str, DatabaseValue)]) -> Vec<String> {
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

#[allow(unused)]
fn build_values_clause<'a>(values: &'a Vec<(&'a str, DatabaseValue)>) -> String {
    if values.is_empty() {
        "".to_string()
    } else {
        format!("VALUES({})", build_values_props(values).join(", "))
    }
}

#[allow(unused)]
fn build_values_props<'a>(values: &'a [(&'a str, DatabaseValue)]) -> Vec<String> {
    build_values_props_offset(values, 0, false)
}

#[allow(unused)]
fn build_values_props_offset<'a>(
    values: &'a [(&'a str, DatabaseValue)],
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

fn bind_values<'a>(
    statement: &mut Statement<'_>,
    values: &'a Vec<(&'a str, DatabaseValue)>,
    constant_inc: bool,
) -> Result<(), RusqliteDatabaseError> {
    let mut i = 1;
    for (_key, value) in values {
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

    Ok(())
}
