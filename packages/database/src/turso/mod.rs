#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use thiserror::Error;
use turso::{Builder, Database as TursoDb, Value as TursoValue};

use crate::{
    DatabaseValue,
    query_transform::{QuestionMarkHandler, transform_query_for_params},
    sql_interval::SqlInterval,
};

#[derive(Debug, Error)]
pub enum TursoDatabaseError {
    #[error(transparent)]
    Turso(#[from] turso::Error),
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Query error: {0}")]
    Query(String),
    #[error("Transaction error: {0}")]
    Transaction(String),
    #[error("Unsupported type conversion: {0}")]
    UnsupportedType(String),
}

#[derive(Debug)]
pub struct TursoDatabase {
    database: TursoDb,
}

impl TursoDatabase {
    /// Create a new Turso database instance
    ///
    /// # Errors
    ///
    /// * Returns `TursoDatabaseError::Connection` if the database connection cannot be established
    pub async fn new(path: &str) -> Result<Self, TursoDatabaseError> {
        let builder = Builder::new_local(path);
        let database = builder
            .build()
            .await
            .map_err(|e| TursoDatabaseError::Connection(e.to_string()))?;

        Ok(Self { database })
    }
}

fn format_sqlite_interval(interval: &SqlInterval) -> Vec<String> {
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

fn turso_transform_query_for_params(
    query: &str,
    params: &[DatabaseValue],
) -> Result<(String, Vec<DatabaseValue>), crate::DatabaseError> {
    transform_query_for_params(query, params, &QuestionMarkHandler, |param| match param {
        DatabaseValue::Now => Some("datetime('now')".to_string()),
        DatabaseValue::NowPlus(interval) => {
            let modifiers = format_sqlite_interval(interval);
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
    .map_err(crate::DatabaseError::QueryFailed)
}

impl From<TursoValue> for DatabaseValue {
    fn from(value: TursoValue) -> Self {
        match value {
            TursoValue::Null => Self::Null,
            TursoValue::Integer(i) => Self::Int64(i),
            TursoValue::Real(f) => Self::Real64(f),
            TursoValue::Text(s) => Self::String(s),
            TursoValue::Blob(_) => unimplemented!("Blob types are not supported yet"),
        }
    }
}

fn database_value_to_turso_value(value: &DatabaseValue) -> Result<TursoValue, TursoDatabaseError> {
    match value {
        DatabaseValue::Null => Ok(TursoValue::Null),
        DatabaseValue::String(s) | DatabaseValue::StringOpt(Some(s)) => {
            Ok(TursoValue::Text(s.clone()))
        }
        DatabaseValue::StringOpt(None) => Ok(TursoValue::Null),
        DatabaseValue::Bool(b) | DatabaseValue::BoolOpt(Some(b)) => {
            Ok(TursoValue::Integer(i64::from(*b)))
        }
        DatabaseValue::BoolOpt(None) => Ok(TursoValue::Null),
        DatabaseValue::Int8(i) | DatabaseValue::Int8Opt(Some(i)) => {
            Ok(TursoValue::Integer(i64::from(*i)))
        }
        DatabaseValue::Int8Opt(None) => Ok(TursoValue::Null),
        DatabaseValue::Int16(i) | DatabaseValue::Int16Opt(Some(i)) => {
            Ok(TursoValue::Integer(i64::from(*i)))
        }
        DatabaseValue::Int16Opt(None) => Ok(TursoValue::Null),
        DatabaseValue::Int32(i) | DatabaseValue::Int32Opt(Some(i)) => {
            Ok(TursoValue::Integer(i64::from(*i)))
        }
        DatabaseValue::Int32Opt(None) => Ok(TursoValue::Null),
        DatabaseValue::Int64(i) | DatabaseValue::Int64Opt(Some(i)) => Ok(TursoValue::Integer(*i)),
        DatabaseValue::Int64Opt(None) => Ok(TursoValue::Null),
        DatabaseValue::UInt8(u) | DatabaseValue::UInt8Opt(Some(u)) => {
            Ok(TursoValue::Integer(i64::from(*u)))
        }
        DatabaseValue::UInt8Opt(None) => Ok(TursoValue::Null),
        DatabaseValue::UInt16(u) | DatabaseValue::UInt16Opt(Some(u)) => {
            Ok(TursoValue::Integer(i64::from(*u)))
        }
        DatabaseValue::UInt16Opt(None) => Ok(TursoValue::Null),
        DatabaseValue::UInt32(u) | DatabaseValue::UInt32Opt(Some(u)) => {
            Ok(TursoValue::Integer(i64::from(*u)))
        }
        DatabaseValue::UInt32Opt(None) => Ok(TursoValue::Null),
        DatabaseValue::UInt64(u) | DatabaseValue::UInt64Opt(Some(u)) => i64::try_from(*u)
            .map(TursoValue::Integer)
            .map_err(|e| TursoDatabaseError::UnsupportedType(format!("u64 too large: {e}"))),
        DatabaseValue::UInt64Opt(None) => Ok(TursoValue::Null),
        DatabaseValue::Real32(f) | DatabaseValue::Real32Opt(Some(f)) => {
            Ok(TursoValue::Real(f64::from(*f)))
        }
        DatabaseValue::Real32Opt(None) => Ok(TursoValue::Null),
        DatabaseValue::Real64(f) | DatabaseValue::Real64Opt(Some(f)) => Ok(TursoValue::Real(*f)),
        DatabaseValue::Real64Opt(None) => Ok(TursoValue::Null),
        #[cfg(feature = "decimal")]
        DatabaseValue::Decimal(d) | DatabaseValue::DecimalOpt(Some(d)) => {
            Ok(TursoValue::Text(d.to_string()))
        }
        #[cfg(feature = "decimal")]
        DatabaseValue::DecimalOpt(None) => Ok(TursoValue::Null),
        #[cfg(feature = "uuid")]
        DatabaseValue::Uuid(u) | DatabaseValue::UuidOpt(Some(u)) => {
            Ok(TursoValue::Text(u.to_string()))
        }
        #[cfg(feature = "uuid")]
        DatabaseValue::UuidOpt(None) => Ok(TursoValue::Null),
        DatabaseValue::NowPlus(_) | DatabaseValue::Now => Err(TursoDatabaseError::UnsupportedType(
            "Now/NowPlus should be transformed before parameter binding".to_string(),
        )),
        DatabaseValue::DateTime(dt) => Ok(TursoValue::Text(dt.format("%+").to_string())),
    }
}

fn to_turso_params(params: &[DatabaseValue]) -> Result<Vec<TursoValue>, TursoDatabaseError> {
    params.iter().map(database_value_to_turso_value).collect()
}

fn from_turso_row(
    column_names: &[String],
    row: &turso::Row,
) -> Result<crate::Row, TursoDatabaseError> {
    let mut columns = Vec::with_capacity(column_names.len());

    for (index, column_name) in column_names.iter().enumerate() {
        let value = row
            .get_value(index)
            .map_err(|e| TursoDatabaseError::Query(format!("Failed to get column {index}: {e}")))?;

        columns.push((column_name.clone(), DatabaseValue::from(value)));
    }

    Ok(crate::Row { columns })
}

#[async_trait::async_trait]
impl crate::Database for TursoDatabase {
    async fn query_raw(&self, query: &str) -> Result<Vec<crate::Row>, crate::DatabaseError> {
        let conn = self.database.connect().map_err(|e| {
            crate::DatabaseError::Turso(TursoDatabaseError::Connection(e.to_string()))
        })?;

        let mut stmt = conn
            .prepare(query)
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

        let column_info = stmt.columns();
        let column_names: Vec<String> = column_info.iter().map(|c| c.name().to_string()).collect();

        let mut rows = stmt
            .query(())
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

        let mut results = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?
        {
            results.push(from_turso_row(&column_names, &row).map_err(crate::DatabaseError::Turso)?);
        }

        Ok(results)
    }

    async fn query_raw_params(
        &self,
        query: &str,
        params: &[DatabaseValue],
    ) -> Result<Vec<crate::Row>, crate::DatabaseError> {
        let (transformed_query, filtered_params) = turso_transform_query_for_params(query, params)?;

        let conn = self.database.connect().map_err(|e| {
            crate::DatabaseError::Turso(TursoDatabaseError::Connection(e.to_string()))
        })?;

        let mut stmt = conn
            .prepare(&transformed_query)
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

        let column_info = stmt.columns();
        let column_names: Vec<String> = column_info.iter().map(|c| c.name().to_string()).collect();

        let turso_params =
            to_turso_params(&filtered_params).map_err(crate::DatabaseError::Turso)?;

        let mut rows = stmt
            .query(turso_params)
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

        let mut results = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?
        {
            results.push(from_turso_row(&column_names, &row).map_err(crate::DatabaseError::Turso)?);
        }

        Ok(results)
    }

    async fn exec_raw(&self, statement: &str) -> Result<(), crate::DatabaseError> {
        let conn = self.database.connect().map_err(|e| {
            crate::DatabaseError::Turso(TursoDatabaseError::Connection(e.to_string()))
        })?;

        conn.execute(statement, ())
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

        Ok(())
    }

    async fn exec_raw_params(
        &self,
        query: &str,
        params: &[DatabaseValue],
    ) -> Result<u64, crate::DatabaseError> {
        let (transformed_query, filtered_params) = turso_transform_query_for_params(query, params)?;

        let conn = self.database.connect().map_err(|e| {
            crate::DatabaseError::Turso(TursoDatabaseError::Connection(e.to_string()))
        })?;

        let turso_params =
            to_turso_params(&filtered_params).map_err(crate::DatabaseError::Turso)?;

        let mut stmt = conn
            .prepare(&transformed_query)
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

        let affected_rows = stmt
            .execute(turso_params)
            .await
            .map_err(|e| crate::DatabaseError::Turso(e.into()))?;

        Ok(affected_rows)
    }

    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn crate::DatabaseTransaction>, crate::DatabaseError> {
        unimplemented!("Transactions not yet implemented for Turso backend")
    }

    async fn query(
        &self,
        _query: &crate::query::SelectQuery<'_>,
    ) -> Result<Vec<crate::Row>, crate::DatabaseError> {
        unimplemented!(
            "Query builder not yet implemented for Turso backend - use query_raw_params instead"
        )
    }

    async fn query_first(
        &self,
        _query: &crate::query::SelectQuery<'_>,
    ) -> Result<Option<crate::Row>, crate::DatabaseError> {
        unimplemented!(
            "Query builder not yet implemented for Turso backend - use query_raw_params instead"
        )
    }

    async fn exec_update(
        &self,
        _statement: &crate::query::UpdateStatement<'_>,
    ) -> Result<Vec<crate::Row>, crate::DatabaseError> {
        unimplemented!(
            "Query builder not yet implemented for Turso backend - use exec_raw_params instead"
        )
    }

    async fn exec_update_first(
        &self,
        _statement: &crate::query::UpdateStatement<'_>,
    ) -> Result<Option<crate::Row>, crate::DatabaseError> {
        unimplemented!(
            "Query builder not yet implemented for Turso backend - use exec_raw_params instead"
        )
    }

    async fn exec_insert(
        &self,
        _statement: &crate::query::InsertStatement<'_>,
    ) -> Result<crate::Row, crate::DatabaseError> {
        unimplemented!(
            "Query builder not yet implemented for Turso backend - use exec_raw_params instead"
        )
    }

    async fn exec_upsert(
        &self,
        _statement: &crate::query::UpsertStatement<'_>,
    ) -> Result<Vec<crate::Row>, crate::DatabaseError> {
        unimplemented!(
            "Query builder not yet implemented for Turso backend - use exec_raw_params instead"
        )
    }

    async fn exec_upsert_first(
        &self,
        _statement: &crate::query::UpsertStatement<'_>,
    ) -> Result<crate::Row, crate::DatabaseError> {
        unimplemented!(
            "Query builder not yet implemented for Turso backend - use exec_raw_params instead"
        )
    }

    async fn exec_upsert_multi(
        &self,
        _statement: &crate::query::UpsertMultiStatement<'_>,
    ) -> Result<Vec<crate::Row>, crate::DatabaseError> {
        unimplemented!(
            "Query builder not yet implemented for Turso backend - use exec_raw_params instead"
        )
    }

    async fn exec_delete(
        &self,
        _statement: &crate::query::DeleteStatement<'_>,
    ) -> Result<Vec<crate::Row>, crate::DatabaseError> {
        unimplemented!(
            "Query builder not yet implemented for Turso backend - use exec_raw_params instead"
        )
    }

    async fn exec_delete_first(
        &self,
        _statement: &crate::query::DeleteStatement<'_>,
    ) -> Result<Option<crate::Row>, crate::DatabaseError> {
        unimplemented!(
            "Query builder not yet implemented for Turso backend - use exec_raw_params instead"
        )
    }

    #[cfg(feature = "schema")]
    async fn exec_create_table(
        &self,
        _statement: &crate::schema::CreateTableStatement<'_>,
    ) -> Result<(), crate::DatabaseError> {
        unimplemented!(
            "Schema operations not yet implemented for Turso backend - use exec_raw instead"
        )
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_table(
        &self,
        _statement: &crate::schema::DropTableStatement<'_>,
    ) -> Result<(), crate::DatabaseError> {
        unimplemented!(
            "Schema operations not yet implemented for Turso backend - use exec_raw instead"
        )
    }

    #[cfg(feature = "schema")]
    async fn exec_create_index(
        &self,
        _statement: &crate::schema::CreateIndexStatement<'_>,
    ) -> Result<(), crate::DatabaseError> {
        unimplemented!(
            "Schema operations not yet implemented for Turso backend - use exec_raw instead"
        )
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_index(
        &self,
        _statement: &crate::schema::DropIndexStatement<'_>,
    ) -> Result<(), crate::DatabaseError> {
        unimplemented!(
            "Schema operations not yet implemented for Turso backend - use exec_raw instead"
        )
    }

    #[cfg(feature = "schema")]
    async fn exec_alter_table(
        &self,
        _statement: &crate::schema::AlterTableStatement<'_>,
    ) -> Result<(), crate::DatabaseError> {
        unimplemented!(
            "Schema operations not yet implemented for Turso backend - use exec_raw instead"
        )
    }

    #[cfg(feature = "schema")]
    async fn table_exists(&self, _table: &str) -> Result<bool, crate::DatabaseError> {
        unimplemented!("Schema introspection not yet implemented for Turso backend")
    }

    #[cfg(feature = "schema")]
    async fn list_tables(&self) -> Result<Vec<String>, crate::DatabaseError> {
        unimplemented!("Schema introspection not yet implemented for Turso backend")
    }

    #[cfg(feature = "schema")]
    async fn get_table_info(
        &self,
        _table: &str,
    ) -> Result<Option<crate::schema::TableInfo>, crate::DatabaseError> {
        unimplemented!("Schema introspection not yet implemented for Turso backend")
    }

    #[cfg(feature = "schema")]
    async fn get_table_columns(
        &self,
        _table: &str,
    ) -> Result<Vec<crate::schema::ColumnInfo>, crate::DatabaseError> {
        unimplemented!("Schema introspection not yet implemented for Turso backend")
    }

    #[cfg(feature = "schema")]
    async fn column_exists(
        &self,
        _table: &str,
        _column: &str,
    ) -> Result<bool, crate::DatabaseError> {
        unimplemented!("Schema introspection not yet implemented for Turso backend")
    }
}
