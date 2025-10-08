#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

pub mod transaction;

use thiserror::Error;
use turso::{Builder, Database as TursoDb, Value as TursoValue};

use crate::{
    DatabaseValue,
    query_transform::{QuestionMarkHandler, transform_query_for_params},
    sql_interval::SqlInterval,
};

pub use transaction::TursoTransaction;

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

pub(crate) fn format_sqlite_interval(interval: &SqlInterval) -> Vec<String> {
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

pub(crate) fn turso_transform_query_for_params(
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

pub(crate) fn database_value_to_turso_value(
    value: &DatabaseValue,
) -> Result<TursoValue, TursoDatabaseError> {
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
        DatabaseValue::DateTime(dt) => {
            Ok(TursoValue::Text(dt.format("%Y-%m-%d %H:%M:%S").to_string()))
        }
    }
}

pub(crate) fn to_turso_params(
    params: &[DatabaseValue],
) -> Result<Vec<TursoValue>, TursoDatabaseError> {
    params.iter().map(database_value_to_turso_value).collect()
}

pub(crate) fn from_turso_row(
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
        let conn = self.database.connect().map_err(|e| {
            crate::DatabaseError::Turso(TursoDatabaseError::Connection(e.to_string()))
        })?;

        let tx = TursoTransaction::new(conn)
            .await
            .map_err(crate::DatabaseError::Turso)?;

        Ok(Box::new(tx))
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
    async fn table_exists(&self, table: &str) -> Result<bool, crate::DatabaseError> {
        let query = "SELECT name FROM sqlite_master WHERE type='table' AND name=?";
        let rows = self
            .query_raw_params(query, &[DatabaseValue::String(table.to_string())])
            .await?;
        Ok(!rows.is_empty())
    }

    #[cfg(feature = "schema")]
    async fn list_tables(&self) -> Result<Vec<String>, crate::DatabaseError> {
        let query =
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'";
        let rows = self.query_raw(query).await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| row.get("name"))
            .filter_map(|v| match v {
                DatabaseValue::String(s) => Some(s),
                _ => None,
            })
            .collect())
    }

    #[cfg(feature = "schema")]
    async fn get_table_info(
        &self,
        table: &str,
    ) -> Result<Option<crate::schema::TableInfo>, crate::DatabaseError> {
        if !self.table_exists(table).await? {
            return Ok(None);
        }

        let columns = self.get_table_columns(table).await?;

        let columns_map = columns
            .into_iter()
            .map(|col| (col.name.clone(), col))
            .collect();

        let indexes = get_table_indexes(self, table).await?;
        let foreign_keys = get_table_foreign_keys(self, table).await?;

        Ok(Some(crate::schema::TableInfo {
            name: table.to_string(),
            columns: columns_map,
            indexes,
            foreign_keys,
        }))
    }

    #[cfg(feature = "schema")]
    async fn get_table_columns(
        &self,
        table: &str,
    ) -> Result<Vec<crate::schema::ColumnInfo>, crate::DatabaseError> {
        let query = format!("PRAGMA table_info({table})");
        let rows = self.query_raw(&query).await?;

        let create_sql_query = "SELECT sql FROM sqlite_master WHERE type='table' AND name=?";
        let create_sql_rows = self
            .query_raw_params(
                create_sql_query,
                &[DatabaseValue::String(table.to_string())],
            )
            .await?;

        let create_sql = create_sql_rows
            .into_iter()
            .find_map(|row| match row.get("sql") {
                Some(DatabaseValue::String(s)) => Some(s),
                _ => None,
            });

        let mut columns = Vec::new();

        for row in rows {
            let ordinal = match row.get("cid") {
                Some(DatabaseValue::Int64(i)) => {
                    u32::try_from(i).unwrap_or_else(|_| u32::try_from(columns.len()).unwrap_or(0))
                }
                _ => u32::try_from(columns.len()).unwrap_or(0),
            };

            let name = match row.get("name") {
                Some(DatabaseValue::String(s)) => s.clone(),
                _ => continue,
            };

            let type_str = match row.get("type") {
                Some(DatabaseValue::String(s)) => s.clone(),
                _ => String::from("TEXT"),
            };

            let not_null = match row.get("notnull") {
                Some(DatabaseValue::Int64(i)) => i != 0,
                _ => false,
            };

            let is_pk = match row.get("pk") {
                Some(DatabaseValue::Int64(i)) => i != 0,
                _ => false,
            };

            let default_value = match row.get("dflt_value") {
                Some(DatabaseValue::String(s)) => Some(s.clone()),
                _ => None,
            };

            let data_type = sqlite_type_to_data_type(&type_str);
            let default_val = parse_default_value(default_value.as_deref());

            let auto_increment = if is_pk {
                check_autoincrement_in_sql(create_sql.as_deref(), &name)
            } else {
                false
            };

            columns.push(crate::schema::ColumnInfo {
                name,
                data_type,
                nullable: !not_null,
                is_primary_key: is_pk,
                auto_increment,
                default_value: default_val,
                ordinal_position: ordinal + 1,
            });
        }

        Ok(columns)
    }

    #[cfg(feature = "schema")]
    async fn column_exists(&self, table: &str, column: &str) -> Result<bool, crate::DatabaseError> {
        let columns = self.get_table_columns(table).await?;
        Ok(columns.iter().any(|col| col.name == column))
    }
}

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
fn parse_default_value(default_str: Option<&str>) -> Option<crate::DatabaseValue> {
    default_str.and_then(|s| {
        if s == "NULL" {
            Some(crate::DatabaseValue::Null)
        } else if s.starts_with('\'') && s.ends_with('\'') {
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
fn check_autoincrement_in_sql(create_sql: Option<&str>, column_name: &str) -> bool {
    let Some(sql) = create_sql else {
        return false;
    };

    let normalized_sql = sql.to_uppercase();
    let normalized_column = column_name.to_uppercase();

    if let Some(column_start) = normalized_sql.find(&normalized_column) {
        let column_portion = &normalized_sql[column_start..];

        if column_portion.contains("PRIMARY KEY")
            && let Some(pk_pos) = column_portion.find("PRIMARY KEY")
        {
            let after_pk = &column_portion[pk_pos + "PRIMARY KEY".len()..];

            let end_pos = after_pk
                .find(',')
                .unwrap_or_else(|| after_pk.find(')').unwrap_or(after_pk.len()));
            let column_rest = &after_pk[..end_pos];

            return column_rest.contains("AUTOINCREMENT");
        }
    }

    false
}

#[cfg(feature = "schema")]
async fn get_table_indexes(
    db: &TursoDatabase,
    table: &str,
) -> Result<std::collections::BTreeMap<String, crate::schema::IndexInfo>, crate::DatabaseError> {
    use crate::Database;
    use crate::schema::IndexInfo;
    use std::collections::BTreeMap;

    let index_query = "SELECT name, sql FROM sqlite_master WHERE type='index' AND tbl_name=?";
    let index_rows = db
        .query_raw_params(index_query, &[DatabaseValue::String(table.to_string())])
        .await?;

    let mut indexes = BTreeMap::new();

    for index_row in index_rows {
        let index_name = match index_row.get("name") {
            Some(DatabaseValue::String(s)) => s.clone(),
            _ => continue,
        };

        let Some(DatabaseValue::String(sql)) = index_row.get("sql") else {
            let is_primary = index_name.starts_with("sqlite_autoindex_");
            indexes.insert(
                index_name.clone(),
                IndexInfo {
                    name: index_name,
                    unique: false,
                    columns: Vec::new(),
                    is_primary,
                },
            );
            continue;
        };

        let is_unique = sql.to_uppercase().contains("UNIQUE");
        let is_primary = index_name.starts_with("sqlite_autoindex_");

        let index_columns = sql
            .find('(')
            .and_then(|start| {
                sql.rfind(')').map(|end| {
                    let cols_str = &sql[start + 1..end];
                    cols_str
                        .split(',')
                        .map(|s| s.trim().trim_matches('`').trim_matches('"').to_string())
                        .collect()
                })
            })
            .unwrap_or_default();

        indexes.insert(
            index_name.clone(),
            IndexInfo {
                name: index_name,
                unique: is_unique,
                columns: index_columns,
                is_primary,
            },
        );
    }

    Ok(indexes)
}

#[cfg(feature = "schema")]
#[allow(
    clippy::too_many_lines,
    clippy::single_match_else,
    clippy::option_if_let_else,
    clippy::collapsible_if
)]
async fn get_table_foreign_keys(
    db: &TursoDatabase,
    table: &str,
) -> Result<std::collections::BTreeMap<String, crate::schema::ForeignKeyInfo>, crate::DatabaseError>
{
    use crate::Database;
    use crate::schema::ForeignKeyInfo;
    use std::collections::BTreeMap;

    let create_sql_query = "SELECT sql FROM sqlite_master WHERE type='table' AND name=?";
    let create_sql_rows = db
        .query_raw_params(
            create_sql_query,
            &[DatabaseValue::String(table.to_string())],
        )
        .await?;

    let create_sql = create_sql_rows
        .into_iter()
        .find_map(|row| match row.get("sql") {
            Some(DatabaseValue::String(s)) => Some(s),
            _ => None,
        });

    let mut foreign_keys = BTreeMap::new();

    if let Some(sql) = create_sql {
        let sql_upper = sql.to_uppercase();
        if sql_upper.contains("FOREIGN KEY") {
            let parts: Vec<&str> = sql.split("FOREIGN KEY").collect();
            for part in parts.iter().skip(1) {
                if let Some(col_start) = part.find('(') {
                    if let Some(col_end) = part[col_start..].find(')') {
                        let column = part[col_start + 1..col_start + col_end]
                            .trim()
                            .trim_matches('`')
                            .trim_matches('"')
                            .to_string();

                        if let Some(ref_start) = part.find("REFERENCES") {
                            let ref_part = &part[ref_start + 10..];
                            if let Some(ref_table_end) = ref_part.find('(') {
                                let referenced_table = ref_part[..ref_table_end].trim().to_string();

                                if let Some(ref_col_end) = ref_part[ref_table_end..].find(')') {
                                    let referenced_column = ref_part
                                        [ref_table_end + 1..ref_table_end + ref_col_end]
                                        .trim()
                                        .trim_matches('`')
                                        .trim_matches('"')
                                        .to_string();

                                    let on_update = if ref_part
                                        .to_uppercase()
                                        .contains("ON UPDATE CASCADE")
                                    {
                                        Some("CASCADE".to_string())
                                    } else if ref_part.to_uppercase().contains("ON UPDATE SET NULL")
                                    {
                                        Some("SET NULL".to_string())
                                    } else if ref_part.to_uppercase().contains("ON UPDATE RESTRICT")
                                    {
                                        Some("RESTRICT".to_string())
                                    } else {
                                        None
                                    };

                                    let on_delete = if ref_part
                                        .to_uppercase()
                                        .contains("ON DELETE CASCADE")
                                    {
                                        Some("CASCADE".to_string())
                                    } else if ref_part.to_uppercase().contains("ON DELETE SET NULL")
                                    {
                                        Some("SET NULL".to_string())
                                    } else if ref_part.to_uppercase().contains("ON DELETE RESTRICT")
                                    {
                                        Some("RESTRICT".to_string())
                                    } else {
                                        None
                                    };

                                    let fk_name = format!(
                                        "{table}_{column}_{referenced_table}_{referenced_column}"
                                    );

                                    foreign_keys.insert(
                                        fk_name.clone(),
                                        ForeignKeyInfo {
                                            name: fk_name,
                                            column,
                                            referenced_table,
                                            referenced_column,
                                            on_update,
                                            on_delete,
                                        },
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(foreign_keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Database;

    async fn create_test_db() -> TursoDatabase {
        TursoDatabase::new(":memory:")
            .await
            .expect("Failed to create in-memory Turso database")
    }

    #[switchy_async::test]
    async fn test_database_creation_memory() {
        let db = TursoDatabase::new(":memory:").await;
        assert!(db.is_ok(), "Should create in-memory database");
    }

    #[switchy_async::test]
    async fn test_database_creation_file() {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join("test_turso.db");
        let db_path_str = db_path.to_string_lossy();

        let db = TursoDatabase::new(&db_path_str).await;
        assert!(db.is_ok(), "Should create file-based database");

        let _ = std::fs::remove_file(&db_path);
    }

    #[switchy_async::test]
    async fn test_exec_raw_create_table() {
        let db = create_test_db().await;
        let result = db
            .exec_raw("CREATE TABLE test_users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)")
            .await;
        assert!(result.is_ok(), "Should create table");
    }

    #[switchy_async::test]
    async fn test_exec_raw_params_insert() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_users (id INTEGER, name TEXT)")
            .await
            .expect("Failed to create table");

        let params = vec![
            DatabaseValue::Int64(1),
            DatabaseValue::String("Alice".to_string()),
        ];

        let result = db
            .exec_raw_params("INSERT INTO test_users (id, name) VALUES (?, ?)", &params)
            .await;

        assert!(result.is_ok(), "Should insert data");
        assert_eq!(result.unwrap(), 1, "Should affect 1 row");
    }

    #[switchy_async::test]
    async fn test_query_raw_basic() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_users (id INTEGER, name TEXT)")
            .await
            .expect("Failed to create table");

        db.exec_raw("INSERT INTO test_users (id, name) VALUES (1, 'Bob')")
            .await
            .expect("Failed to insert data");

        let rows = db
            .query_raw("SELECT id, name FROM test_users")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1, "Should return 1 row");

        let row = &rows[0];
        assert!(row.get("id").is_some(), "Should have 'id' column");
        assert!(row.get("name").is_some(), "Should have 'name' column");
        assert_eq!(row.get("id"), Some(DatabaseValue::Int64(1)));
        assert_eq!(
            row.get("name"),
            Some(DatabaseValue::String("Bob".to_string()))
        );
    }

    #[switchy_async::test]
    async fn test_query_raw_params() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_users (id INTEGER, name TEXT, active INTEGER)")
            .await
            .expect("Failed to create table");

        let insert_params = vec![
            DatabaseValue::Int64(42),
            DatabaseValue::String("Charlie".to_string()),
            DatabaseValue::Bool(true),
        ];

        db.exec_raw_params(
            "INSERT INTO test_users (id, name, active) VALUES (?, ?, ?)",
            &insert_params,
        )
        .await
        .expect("Failed to insert");

        let query_params = vec![DatabaseValue::Int64(42)];

        let rows = db
            .query_raw_params("SELECT * FROM test_users WHERE id = ?", &query_params)
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.get("id"), Some(DatabaseValue::Int64(42)));
        assert_eq!(
            row.get("name"),
            Some(DatabaseValue::String("Charlie".to_string()))
        );
        assert_eq!(row.get("active"), Some(DatabaseValue::Int64(1)));
    }

    #[switchy_async::test]
    async fn test_parameter_binding_all_types() {
        let db = create_test_db().await;

        db.exec_raw(
            "CREATE TABLE test_types (
                int8_val INTEGER,
                int16_val INTEGER,
                int32_val INTEGER,
                int64_val INTEGER,
                uint8_val INTEGER,
                uint16_val INTEGER,
                uint32_val INTEGER,
                real32_val REAL,
                real64_val REAL,
                text_val TEXT,
                bool_val INTEGER,
                null_val TEXT
            )",
        )
        .await
        .expect("Failed to create table");

        let params = vec![
            DatabaseValue::Int8(i8::MAX),
            DatabaseValue::Int16(i16::MAX),
            DatabaseValue::Int32(i32::MAX),
            DatabaseValue::Int64(i64::MAX),
            DatabaseValue::UInt8(u8::MAX),
            DatabaseValue::UInt16(u16::MAX),
            DatabaseValue::UInt32(u32::MAX),
            DatabaseValue::Real32(1.23_f32),
            DatabaseValue::Real64(4.567_890),
            DatabaseValue::String("test string".to_string()),
            DatabaseValue::Bool(true),
            DatabaseValue::Null,
        ];

        let result = db
            .exec_raw_params(
                "INSERT INTO test_types VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                &params,
            )
            .await;

        assert!(result.is_ok(), "Should insert all types");

        let rows = db
            .query_raw("SELECT * FROM test_types")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1);
        let row = &rows[0];

        assert_eq!(
            row.get("int8_val"),
            Some(DatabaseValue::Int64(i64::from(i8::MAX)))
        );
        assert_eq!(
            row.get("int16_val"),
            Some(DatabaseValue::Int64(i64::from(i16::MAX)))
        );
        assert_eq!(
            row.get("int32_val"),
            Some(DatabaseValue::Int64(i64::from(i32::MAX)))
        );
        assert_eq!(row.get("int64_val"), Some(DatabaseValue::Int64(i64::MAX)));
        assert_eq!(
            row.get("uint8_val"),
            Some(DatabaseValue::Int64(i64::from(u8::MAX)))
        );
        assert_eq!(
            row.get("uint16_val"),
            Some(DatabaseValue::Int64(i64::from(u16::MAX)))
        );
        assert_eq!(
            row.get("uint32_val"),
            Some(DatabaseValue::Int64(i64::from(u32::MAX)))
        );
        assert!(matches!(row.get("bool_val"), Some(DatabaseValue::Int64(1))));
        assert_eq!(row.get("null_val"), Some(DatabaseValue::Null));
    }

    #[switchy_async::test]
    async fn test_parameter_binding_optional_types() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_opts (a INTEGER, b TEXT, c REAL)")
            .await
            .expect("Failed to create table");

        let params = vec![
            DatabaseValue::Int64Opt(Some(100)),
            DatabaseValue::StringOpt(None),
            DatabaseValue::Real64Opt(Some(99.9)),
        ];

        db.exec_raw_params("INSERT INTO test_opts VALUES (?, ?, ?)", &params)
            .await
            .expect("Failed to insert");

        let rows = db
            .query_raw("SELECT * FROM test_opts")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.get("a"), Some(DatabaseValue::Int64(100)));
        assert_eq!(row.get("b"), Some(DatabaseValue::Null));
        assert_eq!(row.get("c"), Some(DatabaseValue::Real64(99.9)));
    }

    #[cfg(feature = "decimal")]
    #[switchy_async::test]
    async fn test_decimal_storage_and_retrieval() {
        use rust_decimal::Decimal;
        use std::str::FromStr;

        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_decimals (id INTEGER, price TEXT)")
            .await
            .expect("Failed to create table");

        let decimal_val = Decimal::from_str("123.456789").expect("Failed to parse decimal");
        let params = vec![DatabaseValue::Int64(1), DatabaseValue::Decimal(decimal_val)];

        db.exec_raw_params("INSERT INTO test_decimals VALUES (?, ?)", &params)
            .await
            .expect("Failed to insert");

        let rows = db
            .query_raw("SELECT * FROM test_decimals")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(
            row.get("price"),
            Some(DatabaseValue::String("123.456789".to_string()))
        );
    }

    #[cfg(feature = "uuid")]
    #[switchy_async::test]
    async fn test_uuid_storage_and_retrieval() {
        use uuid::Uuid;

        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_uuids (id INTEGER, user_id TEXT)")
            .await
            .expect("Failed to create table");

        let uuid_val = Uuid::new_v4();
        let params = vec![DatabaseValue::Int64(1), DatabaseValue::Uuid(uuid_val)];

        db.exec_raw_params("INSERT INTO test_uuids VALUES (?, ?)", &params)
            .await
            .expect("Failed to insert");

        let rows = db
            .query_raw("SELECT * FROM test_uuids")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(
            row.get("user_id"),
            Some(DatabaseValue::String(uuid_val.to_string()))
        );
    }

    #[switchy_async::test]
    async fn test_datetime_storage_and_retrieval() {
        use chrono::NaiveDateTime;

        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_dates (id INTEGER, created_at TEXT)")
            .await
            .expect("Failed to create table");

        let dt = NaiveDateTime::parse_from_str("2024-01-15 12:30:45", "%Y-%m-%d %H:%M:%S")
            .expect("Failed to parse datetime");
        let params = vec![DatabaseValue::Int64(1), DatabaseValue::DateTime(dt)];

        db.exec_raw_params("INSERT INTO test_dates VALUES (?, ?)", &params)
            .await
            .expect("Failed to insert");

        let rows = db
            .query_raw("SELECT * FROM test_dates")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(
            row.get("created_at"),
            Some(DatabaseValue::String(
                dt.format("%Y-%m-%d %H:%M:%S").to_string()
            ))
        );
    }

    #[switchy_async::test]
    async fn test_now_transformation() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_now (id INTEGER, created_at TEXT)")
            .await
            .expect("Failed to create table");

        let params = vec![DatabaseValue::Int64(1), DatabaseValue::Now];

        let result = db
            .exec_raw_params("INSERT INTO test_now VALUES (?, ?)", &params)
            .await;

        assert!(
            result.is_ok(),
            "Now should be transformed to datetime('now')"
        );

        let rows = db
            .query_raw("SELECT * FROM test_now")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert!(
            matches!(row.get("created_at"), Some(DatabaseValue::String(_))),
            "Should have timestamp"
        );
    }

    #[switchy_async::test]
    async fn test_now_plus_transformation() {
        use crate::sql_interval::SqlInterval;

        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_now_plus (id INTEGER, expires_at TEXT)")
            .await
            .expect("Failed to create table");

        let interval = SqlInterval {
            years: 0,
            months: 0,
            days: 7,
            hours: 2,
            minutes: 30,
            seconds: 0,
            nanos: 0,
        };

        let params = vec![DatabaseValue::Int64(1), DatabaseValue::NowPlus(interval)];

        let result = db
            .exec_raw_params("INSERT INTO test_now_plus VALUES (?, ?)", &params)
            .await;

        assert!(
            result.is_ok(),
            "NowPlus should be transformed to datetime with modifiers"
        );

        let rows = db
            .query_raw("SELECT * FROM test_now_plus")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert!(
            matches!(row.get("expires_at"), Some(DatabaseValue::String(_))),
            "Should have future timestamp"
        );
    }

    #[switchy_async::test]
    async fn test_error_handling_invalid_query() {
        let db = create_test_db().await;

        let result = db.query_raw("SELECT * FROM nonexistent_table").await;
        assert!(result.is_err(), "Should return error for invalid query");
    }

    #[switchy_async::test]
    async fn test_error_handling_type_mismatch() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_errors (id INTEGER)")
            .await
            .expect("Failed to create table");

        let params = vec![DatabaseValue::String("not a number".to_string())];

        let result = db
            .exec_raw_params("INSERT INTO test_errors VALUES (?)", &params)
            .await;

        assert!(
            result.is_ok(),
            "SQLite should handle TEXT -> INTEGER conversion"
        );
    }

    #[switchy_async::test]
    async fn test_multiple_rows() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_multi (id INTEGER, value TEXT)")
            .await
            .expect("Failed to create table");

        for i in 1..=10 {
            let params = vec![
                DatabaseValue::Int64(i),
                DatabaseValue::String(format!("value_{i}")),
            ];
            db.exec_raw_params("INSERT INTO test_multi VALUES (?, ?)", &params)
                .await
                .expect("Failed to insert");
        }

        let rows = db
            .query_raw("SELECT * FROM test_multi ORDER BY id")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 10, "Should return 10 rows");

        for (idx, row) in rows.iter().enumerate() {
            let expected_id = i64::try_from(idx + 1).expect("Failed to convert");
            assert_eq!(row.get("id"), Some(DatabaseValue::Int64(expected_id)));
            assert_eq!(
                row.get("value"),
                Some(DatabaseValue::String(format!("value_{expected_id}")))
            );
        }
    }

    #[switchy_async::test]
    async fn test_empty_result_set() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_empty (id INTEGER)")
            .await
            .expect("Failed to create table");

        let rows = db
            .query_raw("SELECT * FROM test_empty")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 0, "Should return empty result set");
    }

    #[switchy_async::test]
    async fn test_column_name_preservation() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_cols (first_name TEXT, last_name TEXT, age INTEGER)")
            .await
            .expect("Failed to create table");

        db.exec_raw("INSERT INTO test_cols VALUES ('John', 'Doe', 30)")
            .await
            .expect("Failed to insert");

        let rows = db
            .query_raw("SELECT first_name, last_name, age FROM test_cols")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1);
        let row = &rows[0];

        assert!(row.get("first_name").is_some(), "Should have first_name");
        assert!(row.get("last_name").is_some(), "Should have last_name");
        assert!(row.get("age").is_some(), "Should have age");

        assert!(
            row.get("FirstName").is_none(),
            "Column names are case-sensitive"
        );
    }

    #[switchy_async::test]
    async fn test_null_handling() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_nulls (id INTEGER, nullable_field TEXT)")
            .await
            .expect("Failed to create table");

        let params = vec![DatabaseValue::Int64(1), DatabaseValue::Null];

        db.exec_raw_params("INSERT INTO test_nulls VALUES (?, ?)", &params)
            .await
            .expect("Failed to insert");

        let rows = db
            .query_raw("SELECT * FROM test_nulls")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.get("nullable_field"), Some(DatabaseValue::Null));
    }

    #[switchy_async::test]
    async fn test_uint64_overflow_error() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_uint64 (id INTEGER, big_val INTEGER)")
            .await
            .expect("Failed to create table");

        let params = vec![DatabaseValue::Int64(1), DatabaseValue::UInt64(u64::MAX)];

        let result = db
            .exec_raw_params("INSERT INTO test_uint64 VALUES (?, ?)", &params)
            .await;

        assert!(
            result.is_err(),
            "u64::MAX should overflow i64 and cause error"
        );
    }

    #[switchy_async::test]
    async fn test_uint64_valid_range() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_uint64_valid (id INTEGER, val INTEGER)")
            .await
            .expect("Failed to create table");

        let params = vec![
            DatabaseValue::Int64(1),
            DatabaseValue::UInt64(i64::MAX as u64),
        ];

        let result = db
            .exec_raw_params("INSERT INTO test_uint64_valid VALUES (?, ?)", &params)
            .await;

        assert!(result.is_ok(), "u64 within i64::MAX range should work");
    }

    #[switchy_async::test]
    async fn test_transaction_commit() {
        use crate::Database;

        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_tx (id INTEGER, name TEXT)")
            .await
            .expect("Failed to create table");

        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        tx.exec_raw("INSERT INTO test_tx VALUES (1, 'Alice')")
            .await
            .expect("Failed to insert");

        Box::new(tx).commit().await.expect("Failed to commit");

        let rows = db
            .query_raw("SELECT * FROM test_tx")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1, "Should have 1 row after commit");
        assert_eq!(rows[0].get("id"), Some(DatabaseValue::Int64(1)));
        assert_eq!(
            rows[0].get("name"),
            Some(DatabaseValue::String("Alice".to_string()))
        );
    }

    #[switchy_async::test]
    async fn test_transaction_rollback() {
        use crate::Database;

        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_tx_rollback (id INTEGER, name TEXT)")
            .await
            .expect("Failed to create table");

        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        tx.exec_raw("INSERT INTO test_tx_rollback VALUES (1, 'Bob')")
            .await
            .expect("Failed to insert");

        Box::new(tx).rollback().await.expect("Failed to rollback");

        let rows = db
            .query_raw("SELECT * FROM test_tx_rollback")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 0, "Should have 0 rows after rollback");
    }

    #[switchy_async::test]
    async fn test_transaction_query() {
        use crate::Database;

        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_tx_query (id INTEGER, value TEXT)")
            .await
            .expect("Failed to create table");

        db.exec_raw("INSERT INTO test_tx_query VALUES (1, 'original')")
            .await
            .expect("Failed to insert initial data");

        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        tx.exec_raw("INSERT INTO test_tx_query VALUES (2, 'in_tx')")
            .await
            .expect("Failed to insert in transaction");

        let rows = tx
            .query_raw("SELECT * FROM test_tx_query ORDER BY id")
            .await
            .expect("Failed to query in transaction");

        assert_eq!(rows.len(), 2, "Should see both rows within transaction");

        Box::new(tx).commit().await.expect("Failed to commit");

        let rows_after = db
            .query_raw("SELECT * FROM test_tx_query ORDER BY id")
            .await
            .expect("Failed to query after commit");

        assert_eq!(rows_after.len(), 2, "Should have 2 rows after commit");
    }

    #[switchy_async::test]
    async fn test_transaction_params() {
        use crate::Database;

        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_tx_params (id INTEGER, name TEXT, active INTEGER)")
            .await
            .expect("Failed to create table");

        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        let params = vec![
            DatabaseValue::Int64(100),
            DatabaseValue::String("Carol".to_string()),
            DatabaseValue::Bool(true),
        ];

        let affected = tx
            .exec_raw_params("INSERT INTO test_tx_params VALUES (?, ?, ?)", &params)
            .await
            .expect("Failed to insert with params");

        assert_eq!(affected, 1, "Should affect 1 row");

        let query_params = vec![DatabaseValue::Int64(100)];
        let rows = tx
            .query_raw_params("SELECT * FROM test_tx_params WHERE id = ?", &query_params)
            .await
            .expect("Failed to query with params");

        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].get("name"),
            Some(DatabaseValue::String("Carol".to_string()))
        );

        Box::new(tx).commit().await.expect("Failed to commit");
    }

    #[switchy_async::test]
    async fn test_transaction_nested_error() {
        use crate::Database;

        let db = create_test_db().await;

        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        let nested_result = tx.begin_transaction().await;

        assert!(
            nested_result.is_err(),
            "Should not allow nested transactions"
        );
        assert!(
            matches!(
                nested_result,
                Err(crate::DatabaseError::AlreadyInTransaction)
            ),
            "Should return AlreadyInTransaction error"
        );

        Box::new(tx).rollback().await.expect("Failed to rollback");
    }

    #[switchy_async::test]
    async fn test_transaction_state_guards() {
        use crate::Database;

        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_state (id INTEGER)")
            .await
            .expect("Failed to create table");

        let tx = db
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        tx.exec_raw("INSERT INTO test_state VALUES (1)")
            .await
            .expect("Failed to insert");

        Box::new(tx).commit().await.expect("Commit should succeed");

        let rows = db
            .query_raw("SELECT * FROM test_state")
            .await
            .expect("Failed to query");

        assert_eq!(rows.len(), 1, "Transaction was committed successfully");
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_table_exists() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_table (id INTEGER)")
            .await
            .expect("Failed to create table");

        assert!(
            db.table_exists("test_table")
                .await
                .expect("Failed to check table existence"),
            "test_table should exist"
        );

        assert!(
            !db.table_exists("nonexistent_table")
                .await
                .expect("Failed to check table existence"),
            "nonexistent_table should not exist"
        );
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_list_tables() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE users (id INTEGER, name TEXT)")
            .await
            .expect("Failed to create users table");

        db.exec_raw("CREATE TABLE posts (id INTEGER, title TEXT)")
            .await
            .expect("Failed to create posts table");

        let tables = db.list_tables().await.expect("Failed to list tables");

        assert!(tables.len() >= 2, "Should have at least 2 tables");
        assert!(tables.contains(&"users".to_string()));
        assert!(tables.contains(&"posts".to_string()));
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_get_table_columns() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_columns (id INTEGER PRIMARY KEY, name TEXT NOT NULL, age INTEGER, email TEXT DEFAULT 'none')")
            .await
            .expect("Failed to create table");

        let columns = db
            .get_table_columns("test_columns")
            .await
            .expect("Failed to get columns");

        assert_eq!(columns.len(), 4, "Should have 4 columns");

        let id_col = columns
            .iter()
            .find(|c| c.name == "id")
            .expect("Should have id column");
        assert!(id_col.is_primary_key, "id should be primary key");
        assert_eq!(id_col.ordinal_position, 1);

        let name_col = columns
            .iter()
            .find(|c| c.name == "name")
            .expect("Should have name column");
        assert!(!name_col.nullable, "name should not be nullable");
        assert_eq!(name_col.ordinal_position, 2);

        let age_col = columns
            .iter()
            .find(|c| c.name == "age")
            .expect("Should have age column");
        assert!(age_col.nullable, "age should be nullable");
        assert_eq!(age_col.ordinal_position, 3);

        let email_col = columns
            .iter()
            .find(|c| c.name == "email")
            .expect("Should have email column");
        assert!(
            email_col.default_value.is_some(),
            "email should have default value"
        );
        assert_eq!(email_col.ordinal_position, 4);
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_column_exists() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_columns (id INTEGER, name TEXT)")
            .await
            .expect("Failed to create table");

        assert!(
            db.column_exists("test_columns", "id")
                .await
                .expect("Failed to check column existence"),
            "id column should exist"
        );

        assert!(
            db.column_exists("test_columns", "name")
                .await
                .expect("Failed to check column existence"),
            "name column should exist"
        );

        assert!(
            !db.column_exists("test_columns", "nonexistent")
                .await
                .expect("Failed to check column existence"),
            "nonexistent column should not exist"
        );
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_get_table_info() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_info (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
            .await
            .expect("Failed to create table");

        let table_info = db
            .get_table_info("test_info")
            .await
            .expect("Failed to get table info")
            .expect("Table should exist");

        assert_eq!(table_info.name, "test_info");
        assert_eq!(table_info.columns.len(), 2);
        assert!(table_info.columns.contains_key("id"));
        assert!(table_info.columns.contains_key("name"));

        let nonexistent = db
            .get_table_info("nonexistent")
            .await
            .expect("Failed to get table info");

        assert!(
            nonexistent.is_none(),
            "Nonexistent table should return None"
        );
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_autoincrement_detection() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_autoincr (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT)")
            .await
            .expect("Failed to create table");

        let columns = db
            .get_table_columns("test_autoincr")
            .await
            .expect("Failed to get columns");

        let id_col = columns
            .iter()
            .find(|c| c.name == "id")
            .expect("Should have id column");

        assert!(id_col.is_primary_key, "id should be primary key");
        assert!(id_col.auto_increment, "id should be auto_increment");

        let name_col = columns
            .iter()
            .find(|c| c.name == "name")
            .expect("Should have name column");

        assert!(
            !name_col.auto_increment,
            "name should not be auto_increment"
        );
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_primary_key_without_autoincrement() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE test_pk_only (id INTEGER PRIMARY KEY, name TEXT)")
            .await
            .expect("Failed to create table");

        let columns = db
            .get_table_columns("test_pk_only")
            .await
            .expect("Failed to get columns");

        let id_col = columns
            .iter()
            .find(|c| c.name == "id")
            .expect("Should have id column");

        assert!(id_col.is_primary_key, "id should be primary key");
        assert!(!id_col.auto_increment, "id should NOT be auto_increment");
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_table_info_with_indexes() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT UNIQUE, name TEXT)")
            .await
            .expect("Failed to create table");

        db.exec_raw("CREATE INDEX idx_users_name ON users(name)")
            .await
            .expect("Failed to create index");

        let table_info = db
            .get_table_info("users")
            .await
            .expect("Failed to get table info")
            .expect("Table should exist");

        assert!(
            !table_info.indexes.is_empty(),
            "Should have at least 1 index"
        );

        let has_name_index = table_info
            .indexes
            .values()
            .any(|idx| idx.columns.contains(&"name".to_string()));
        assert!(has_name_index, "Should have index on name column");
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_table_info_with_foreign_keys() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE departments (id INTEGER PRIMARY KEY, name TEXT)")
            .await
            .expect("Failed to create departments table");

        db.exec_raw(
            "CREATE TABLE employees (
                id INTEGER PRIMARY KEY,
                name TEXT,
                dept_id INTEGER,
                FOREIGN KEY (dept_id) REFERENCES departments(id) ON DELETE CASCADE
            )",
        )
        .await
        .expect("Failed to create employees table");

        let table_info = db
            .get_table_info("employees")
            .await
            .expect("Failed to get table info")
            .expect("Table should exist");

        assert_eq!(
            table_info.foreign_keys.len(),
            1,
            "Should have 1 foreign key"
        );

        let fk = table_info
            .foreign_keys
            .values()
            .next()
            .expect("Should have FK");
        assert_eq!(fk.column, "dept_id");
        assert_eq!(fk.referenced_table, "departments");
        assert_eq!(fk.referenced_column, "id");
        assert_eq!(fk.on_delete, Some("CASCADE".to_string()));
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_table_info_complete() {
        let db = create_test_db().await;

        db.exec_raw("CREATE TABLE categories (id INTEGER PRIMARY KEY, name TEXT UNIQUE)")
            .await
            .expect("Failed to create categories");

        db.exec_raw(
            "CREATE TABLE products (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                price REAL,
                category_id INTEGER,
                FOREIGN KEY (category_id) REFERENCES categories(id) ON UPDATE CASCADE ON DELETE SET NULL
            )",
        )
        .await
        .expect("Failed to create products");

        db.exec_raw("CREATE INDEX idx_products_name ON products(name)")
            .await
            .expect("Failed to create index");

        let table_info = db
            .get_table_info("products")
            .await
            .expect("Failed to get table info")
            .expect("Table should exist");

        assert_eq!(table_info.columns.len(), 4);
        assert!(table_info.columns.contains_key("id"));
        assert!(table_info.columns.contains_key("name"));
        assert!(table_info.columns.contains_key("price"));
        assert!(table_info.columns.contains_key("category_id"));

        let id_col = &table_info.columns["id"];
        assert!(id_col.auto_increment, "id should have AUTOINCREMENT");

        assert!(
            !table_info.indexes.is_empty(),
            "Should have at least 1 index"
        );

        assert_eq!(table_info.foreign_keys.len(), 1);
        let fk = table_info.foreign_keys.values().next().unwrap();
        assert_eq!(fk.column, "category_id");
        assert_eq!(fk.referenced_table, "categories");
        assert_eq!(fk.on_update, Some("CASCADE".to_string()));
        assert_eq!(fk.on_delete, Some("SET NULL".to_string()));
    }
}
