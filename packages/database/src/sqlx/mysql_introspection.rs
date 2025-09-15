use std::collections::BTreeMap;

use crate::{
    DatabaseError, DatabaseValue,
    schema::{ColumnInfo, DataType, ForeignKeyInfo, IndexInfo, TableInfo},
};

use sqlx::Row;

/// Check if a table exists in the current `MySQL` database
pub async fn mysql_sqlx_table_exists(
    conn: &mut sqlx::MySqlConnection,
    table_name: &str,
) -> Result<bool, DatabaseError> {
    let query = "SELECT EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = DATABASE() AND table_name = ?
    )";

    let row = sqlx::query(query)
        .bind(table_name)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| DatabaseError::MysqlSqlx(super::mysql::SqlxDatabaseError::from(e)))?;

    let exists: i64 = row
        .try_get(0)
        .map_err(|e| DatabaseError::MysqlSqlx(super::mysql::SqlxDatabaseError::from(e)))?;

    Ok(exists != 0)
}

/// Get column information for a `MySQL` table
pub async fn mysql_sqlx_get_table_columns(
    conn: &mut sqlx::MySqlConnection,
    table_name: &str,
) -> Result<Vec<ColumnInfo>, DatabaseError> {
    let query = "SELECT
        COLUMN_NAME,
        DATA_TYPE,
        CHARACTER_MAXIMUM_LENGTH,
        IS_NULLABLE,
        COLUMN_DEFAULT,
        COLUMN_KEY,
        EXTRA,
        ORDINAL_POSITION
    FROM information_schema.columns
    WHERE table_schema = DATABASE() AND table_name = ?
    ORDER BY ORDINAL_POSITION";

    let rows = sqlx::query(query)
        .bind(table_name)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| DatabaseError::MysqlSqlx(super::mysql::SqlxDatabaseError::from(e)))?;

    // Get primary key columns for this table
    let pk_query = "SELECT COLUMN_NAME
    FROM information_schema.key_column_usage
    WHERE table_schema = DATABASE()
      AND table_name = ?
      AND constraint_name = 'PRIMARY'";

    let pk_rows = sqlx::query(pk_query)
        .bind(table_name)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| DatabaseError::MysqlSqlx(super::mysql::SqlxDatabaseError::from(e)))?;

    let primary_key_columns: Vec<String> = pk_rows
        .iter()
        .map(|row| row.try_get::<String, _>("COLUMN_NAME").unwrap_or_default())
        .collect();

    let mut columns = Vec::new();

    for row in rows {
        let column_name: String = row
            .try_get("COLUMN_NAME")
            .map_err(|e| DatabaseError::MysqlSqlx(super::mysql::SqlxDatabaseError::from(e)))?;

        let data_type_str: String = row
            .try_get("DATA_TYPE")
            .map_err(|e| DatabaseError::MysqlSqlx(super::mysql::SqlxDatabaseError::from(e)))?;

        let data_type = mysql_type_to_data_type(&data_type_str)?;

        let is_nullable_str: String = row
            .try_get("IS_NULLABLE")
            .map_err(|e| DatabaseError::MysqlSqlx(super::mysql::SqlxDatabaseError::from(e)))?;
        let nullable = is_nullable_str.to_uppercase() == "YES";

        let ordinal_position: u32 = row
            .try_get::<u32, _>("ORDINAL_POSITION")
            .map_err(|e| DatabaseError::MysqlSqlx(super::mysql::SqlxDatabaseError::from(e)))?;

        let default_value: Option<String> = row.try_get("COLUMN_DEFAULT").ok();
        let parsed_default = default_value.as_deref().and_then(parse_mysql_default_value);

        let extra: String = row.try_get("EXTRA").unwrap_or_default();
        let auto_increment = extra.to_uppercase().contains("AUTO_INCREMENT");

        let is_primary_key = primary_key_columns.contains(&column_name);

        columns.push(ColumnInfo {
            name: column_name,
            data_type,
            nullable,
            is_primary_key,
            auto_increment,
            default_value: parsed_default,
            ordinal_position,
        });
    }

    Ok(columns)
}

/// Check if a column exists in a `MySQL` table
pub async fn mysql_sqlx_column_exists(
    conn: &mut sqlx::MySqlConnection,
    table_name: &str,
    column_name: &str,
) -> Result<bool, DatabaseError> {
    let query = "SELECT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = DATABASE() AND table_name = ? AND column_name = ?
    )";

    let row = sqlx::query(query)
        .bind(table_name)
        .bind(column_name)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| DatabaseError::MysqlSqlx(super::mysql::SqlxDatabaseError::from(e)))?;

    let exists: i64 = row
        .try_get(0)
        .map_err(|e| DatabaseError::MysqlSqlx(super::mysql::SqlxDatabaseError::from(e)))?;

    Ok(exists != 0)
}

/// Get comprehensive table information for a `MySQL` table
pub async fn mysql_sqlx_get_table_info(
    conn: &mut sqlx::MySqlConnection,
    table_name: &str,
) -> Result<Option<TableInfo>, DatabaseError> {
    // First check if table exists
    if !mysql_sqlx_table_exists(conn, table_name).await? {
        return Ok(None);
    }

    // Get columns
    let columns = mysql_sqlx_get_table_columns(conn, table_name).await?;
    let mut columns_map = BTreeMap::new();
    for column in columns {
        columns_map.insert(column.name.clone(), column);
    }

    // Get indexes
    let index_query = "SELECT INDEX_NAME, NON_UNIQUE, COLUMN_NAME
    FROM information_schema.STATISTICS
    WHERE table_schema = DATABASE() AND table_name = ?
    ORDER BY INDEX_NAME, SEQ_IN_INDEX";

    let index_rows = sqlx::query(index_query)
        .bind(table_name)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| DatabaseError::MysqlSqlx(super::mysql::SqlxDatabaseError::from(e)))?;

    let mut indexes_map: BTreeMap<String, IndexInfo> = BTreeMap::new();
    for row in index_rows {
        let index_name: String = row
            .try_get("INDEX_NAME")
            .map_err(|e| DatabaseError::MysqlSqlx(super::mysql::SqlxDatabaseError::from(e)))?;

        let non_unique: i64 = row
            .try_get("NON_UNIQUE")
            .map_err(|e| DatabaseError::MysqlSqlx(super::mysql::SqlxDatabaseError::from(e)))?;

        let column_name: String = row
            .try_get("COLUMN_NAME")
            .map_err(|e| DatabaseError::MysqlSqlx(super::mysql::SqlxDatabaseError::from(e)))?;

        let is_primary = index_name == "PRIMARY";
        let unique = non_unique == 0;

        if let Some(existing_index) = indexes_map.get_mut(&index_name) {
            existing_index.columns.push(column_name);
        } else {
            indexes_map.insert(
                index_name.clone(),
                IndexInfo {
                    name: index_name,
                    unique,
                    columns: vec![column_name],
                    is_primary,
                },
            );
        }
    }

    // Get foreign keys
    let fk_query = "SELECT
        CONSTRAINT_NAME,
        COLUMN_NAME,
        REFERENCED_TABLE_NAME,
        REFERENCED_COLUMN_NAME,
        UPDATE_RULE,
        DELETE_RULE
    FROM information_schema.KEY_COLUMN_USAGE kcu
    JOIN information_schema.REFERENTIAL_CONSTRAINTS rc
        ON kcu.CONSTRAINT_NAME = rc.CONSTRAINT_NAME
        AND kcu.CONSTRAINT_SCHEMA = rc.CONSTRAINT_SCHEMA
    WHERE kcu.table_schema = DATABASE()
      AND kcu.table_name = ?
      AND kcu.REFERENCED_TABLE_NAME IS NOT NULL";

    let fk_rows = sqlx::query(fk_query)
        .bind(table_name)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| DatabaseError::MysqlSqlx(super::mysql::SqlxDatabaseError::from(e)))?;

    let mut foreign_keys_map = BTreeMap::new();
    for row in fk_rows {
        let constraint_name: String = row
            .try_get("CONSTRAINT_NAME")
            .map_err(|e| DatabaseError::MysqlSqlx(super::mysql::SqlxDatabaseError::from(e)))?;

        let column_name: String = row
            .try_get("COLUMN_NAME")
            .map_err(|e| DatabaseError::MysqlSqlx(super::mysql::SqlxDatabaseError::from(e)))?;

        let referenced_table: String = row
            .try_get("REFERENCED_TABLE_NAME")
            .map_err(|e| DatabaseError::MysqlSqlx(super::mysql::SqlxDatabaseError::from(e)))?;

        let referenced_column: String = row
            .try_get("REFERENCED_COLUMN_NAME")
            .map_err(|e| DatabaseError::MysqlSqlx(super::mysql::SqlxDatabaseError::from(e)))?;

        let update_rule: Option<String> = row.try_get("UPDATE_RULE").ok();
        let delete_rule: Option<String> = row.try_get("DELETE_RULE").ok();

        foreign_keys_map.insert(
            constraint_name.clone(),
            ForeignKeyInfo {
                name: constraint_name,
                column: column_name,
                referenced_table,
                referenced_column,
                on_update: update_rule,
                on_delete: delete_rule,
            },
        );
    }

    Ok(Some(TableInfo {
        name: table_name.to_string(),
        columns: columns_map,
        indexes: indexes_map,
        foreign_keys: foreign_keys_map,
    }))
}

/// Map `MySQL` data types to our `DataType` enum
fn mysql_type_to_data_type(mysql_type: &str) -> Result<DataType, DatabaseError> {
    match mysql_type.to_uppercase().as_str() {
        "TINYINT" | "SMALLINT" => Ok(DataType::SmallInt),
        "MEDIUMINT" | "INT" | "INTEGER" => Ok(DataType::Int),
        "BIGINT" => Ok(DataType::BigInt),
        "FLOAT" => Ok(DataType::Real),
        "DOUBLE" | "REAL" => Ok(DataType::Double),
        "DECIMAL" | "NUMERIC" => Ok(DataType::Decimal(38, 10)), // Default precision
        "CHAR" | "VARCHAR" | "TEXT" | "TINYTEXT" | "MEDIUMTEXT" | "LONGTEXT" => Ok(DataType::Text),
        "BOOLEAN" | "BOOL" => Ok(DataType::Bool),
        "DATE" | "TIME" | "DATETIME" | "TIMESTAMP" => Ok(DataType::DateTime),
        _ => Err(DatabaseError::UnsupportedDataType(mysql_type.to_string())),
    }
}

/// Parse `MySQL` default values into `DatabaseValue`
fn parse_mysql_default_value(default_str: &str) -> Option<DatabaseValue> {
    if default_str.is_empty() || default_str.to_uppercase() == "NULL" {
        return None;
    }

    // Handle MySQL specific defaults
    match default_str.to_uppercase().as_str() {
        "CURRENT_TIMESTAMP" | "NOW()" => Some(DatabaseValue::Now),
        _ => {
            // Handle quoted string literals
            if default_str.starts_with('\'') && default_str.ends_with('\'') {
                let unquoted = &default_str[1..default_str.len() - 1];
                return Some(DatabaseValue::String(unquoted.to_string()));
            }

            // Try to parse as number
            if let Ok(int_val) = default_str.parse::<i64>() {
                return Some(DatabaseValue::Number(int_val));
            }

            if let Ok(float_val) = default_str.parse::<f64>() {
                return Some(DatabaseValue::Real(float_val));
            }

            // For other complex expressions, return None
            None
        }
    }
}
