//! `MySQL` schema introspection implementation using sqlx
//!
//! This module implements schema introspection for `MySQL` using the standard
//! `information_schema` database. It provides MySQL-specific handling for
//! data types, constraints, and version-specific features.
//!
//! # `MySQL` Version Compatibility
//!
//! This implementation targets **`MySQL` 5.7+** and **`MariaDB` 10.2+**:
//!
//! ## Supported Versions
//! - **`MySQL` 5.7, 8.0**: Full support for all introspection features
//! - **`MariaDB` 10.2+**: Compatible with `MySQL` 5.7+ features used
//! - **Percona Server**: Compatible as `MySQL` drop-in replacement
//!
//! ## Version-Specific Features Used
//!
//! ### `information_schema` Tables (`MySQL` 5.0+)
//! - `information_schema.tables` - Basic table metadata
//! - `information_schema.columns` - Column definitions and constraints
//! - `information_schema.key_column_usage` - Primary/foreign key information
//! - `information_schema.referential_constraints` - Foreign key actions
//! - `information_schema.statistics` - Index information
//!
//! ### `MySQL` 8.0 Features NOT Used
//! We intentionally avoid `MySQL` 8.0-specific features for broader compatibility:
//! - **Invisible columns**: Not detected in introspection
//! - **Generated columns**: Not handled specifically (appear as regular columns)
//! - **Check constraints**: Not introspected (would require `MySQL` 8.0.16+)
//! - **Role-based privileges**: Not considered in introspection
//!
//! # MySQL-Specific Data Type Mappings
//!
//! `MySQL` has extensive data type support. Our mapping to [`DataType`](crate::schema::DataType):
//!
//! ## Integer Types
//! - `TINYINT`, `SMALLINT` → `SmallInt` (8-bit, 16-bit)
//! - `MEDIUMINT`, `INT`, `INTEGER` → `Int` (24-bit, 32-bit)
//! - `BIGINT` → `BigInt` (64-bit)
//!
//! ## Floating Point Types
//! - `FLOAT` → `Real` (32-bit float)
//! - `DOUBLE`, `REAL` → `Double` (64-bit float)
//!
//! ## Fixed Point Types
//! - `DECIMAL`, `NUMERIC` → `Decimal(38, 10)` (default precision)
//!
//! ## String Types
//! - `CHAR`, `VARCHAR` → `VarChar(length)` or `VarChar(255)` if no length
//! - `TEXT`, `TINYTEXT`, `MEDIUMTEXT`, `LONGTEXT` → `Text`
//!
//! ## Other Types
//! - `BOOLEAN`, `BOOL` → `Bool` (stored as `TINYINT(1)`)
//! - `DATE`, `TIME`, `DATETIME`, `TIMESTAMP` → `DateTime`
//!
//! ## Unsupported Types
//! Types that generate `UnsupportedDataType` errors:
//! - `BINARY`, `VARBINARY` (binary data)
//! - `BLOB`, `TINYBLOB`, `MEDIUMBLOB`, `LONGBLOB` (binary large objects)
//! - `BIT` (bit field)
//! - `ENUM` (enumeration)
//! - `SET` (set of values)
//! - `JSON` (`MySQL` 5.7+ JSON type)
//! - `GEOMETRY` and spatial types
//! - `YEAR` (year type)
//!
//! # MySQL-Specific Behavior
//!
//! ## Database Scope
//! `MySQL` is database-aware but not schema-aware (unlike PostgreSQL):
//! - Uses `DATABASE()` function to limit queries to current database
//! - No schema concept - tables exist directly in databases
//! - Introspection limited to currently connected database
//!
//! ## Case Sensitivity
//! Table and column name case sensitivity depends on the filesystem:
//! - **Linux/Unix**: Case-sensitive by default (`lower_case_table_names=0`)
//! - **Windows**: Case-insensitive (`lower_case_table_names=1`)
//! - **macOS**: Case-insensitive (`lower_case_table_names=2`)
//!
//! Our introspection preserves the exact case as stored in `information_schema`.
//!
//! ## Storage Engine Considerations
//! Different storage engines affect foreign key support:
//! - **`InnoDB`**: Full foreign key support (default in `MySQL` 5.7+)
//! - **`MyISAM`**: No foreign key support (constraints ignored)
//! - **Memory**: No foreign key support
//!
//! Foreign key introspection only returns meaningful results for `InnoDB` tables.
//!
//! ## Auto-increment Detection
//! `MySQL` provides auto-increment information in the `EXTRA` column:
//! - `auto_increment` in `EXTRA` field → `auto_increment: true`
//! - Empty `EXTRA` or other values → `auto_increment: false`
//!
//! This is more reliable than `SQLite`'s limited detection.
//!
//! ## Character Set Handling
//! - Character sets affect column length calculations
//! - `CHARACTER_MAXIMUM_LENGTH` reflects character count, not byte count
//! - UTF-8 characters may use 1-4 bytes but count as 1 character
//!
//! # Default Value Parsing
//!
//! `MySQL` default values have specific formatting. Our parser handles:
//!
//! ## `MySQL` Functions
//! - `CURRENT_TIMESTAMP`, `NOW()` → `DatabaseValue::Now`
//! - Other functions → `None` (not representable)
//!
//! ## Literal Values
//! - Quoted strings: `'value'` → `DatabaseValue::String("value")`
//! - Numbers: `42`, `3.14` → `DatabaseValue::Int64()` or `Real()`
//! - `NULL` or empty → `None`
//!
//! ## Boolean Values
//! `MySQL` stores booleans as `TINYINT(1)`:
//! - `1` → `DatabaseValue::Bool(true)`
//! - `0` → `DatabaseValue::Bool(false)`
//!
//! # Limitations
//!
//! ## Generated/Computed Columns (`MySQL` 5.7+)
//! Generated columns appear in `information_schema.columns` but:
//! - `GENERATION_EXPRESSION` column not parsed
//! - Treated as regular columns in introspection
//! - May have complex default expressions not representable
//!
//! ## Partitioned Tables
//! - Partition information not included in introspection
//! - Tables appear as single units regardless of partitioning
//!
//! ## Triggers and Procedures
//! - Trigger information not included in table metadata
//! - Stored procedures not introspected

use std::collections::BTreeMap;

use crate::{
    DatabaseError, DatabaseValue,
    schema::{ColumnInfo, DataType, ForeignKeyInfo, IndexInfo, TableInfo},
};

use sqlx::{MySqlConnection, Row};

/// Check if a table exists in the current `MySQL` database
pub async fn mysql_sqlx_table_exists(
    conn: &mut sqlx::MySqlConnection,
    table_name: &str,
) -> Result<bool, DatabaseError> {
    // Note: We don't use LOWER() here because:
    // 1. MySQL's table name case-sensitivity depends on the filesystem and lower_case_table_names setting
    // 2. Using LOWER() with bind parameters can cause issues with collation and query optimization
    // 3. Most MySQL installations are case-insensitive by default (lower_case_table_names=1 or 2)
    // 4. The query should match the exact case stored in information_schema
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

/// List all table names in the current `MySQL` database
pub async fn mysql_sqlx_list_tables(
    conn: &mut MySqlConnection,
) -> Result<Vec<String>, DatabaseError> {
    let query = "SELECT CAST(TABLE_NAME AS CHAR) AS TABLE_NAME FROM information_schema.tables WHERE table_schema = DATABASE() ORDER BY TABLE_NAME";

    let rows = sqlx::query(query)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| DatabaseError::MysqlSqlx(super::mysql::SqlxDatabaseError::from(e)))?;

    let mut tables = Vec::new();
    for row in rows {
        let table_name: String = row
            .try_get("TABLE_NAME")
            .map_err(|e| DatabaseError::MysqlSqlx(super::mysql::SqlxDatabaseError::from(e)))?;
        tables.push(table_name);
    }

    Ok(tables)
}

/// Get column information for a `MySQL` table
pub async fn mysql_sqlx_get_table_columns(
    conn: &mut sqlx::MySqlConnection,
    table_name: &str,
) -> Result<Vec<ColumnInfo>, DatabaseError> {
    let query = "SELECT
        COLUMN_NAME,
        CAST(DATA_TYPE AS CHAR) AS DATA_TYPE,
        CAST(COLUMN_TYPE AS CHAR) AS COLUMN_TYPE,
        CHARACTER_MAXIMUM_LENGTH,
        CAST(IS_NULLABLE AS CHAR) AS IS_NULLABLE,
        CAST(COLUMN_DEFAULT AS CHAR) AS COLUMN_DEFAULT,
        CAST(COLUMN_KEY AS CHAR) AS COLUMN_KEY,
        CAST(EXTRA AS CHAR) AS EXTRA,
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

        let column_type_str: String = row
            .try_get("COLUMN_TYPE")
            .map_err(|e| DatabaseError::MysqlSqlx(super::mysql::SqlxDatabaseError::from(e)))?;

        let char_max_length: Option<i64> = row.try_get("CHARACTER_MAXIMUM_LENGTH").ok();

        // Use column_type for more accurate type detection
        let data_type =
            mysql_column_type_to_data_type(&column_type_str, &data_type_str, char_max_length);

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
    // Note: Column names in MySQL are always case-insensitive regardless of platform
    // However, table names follow the same case-sensitivity rules as table_exists
    let query = "SELECT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = DATABASE()
        AND table_name = ?
        AND column_name = ?
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
        CAST(kcu.CONSTRAINT_NAME AS CHAR) AS CONSTRAINT_NAME,
        kcu.COLUMN_NAME,
        CAST(kcu.REFERENCED_TABLE_NAME AS CHAR) AS REFERENCED_TABLE_NAME,
        CAST(kcu.REFERENCED_COLUMN_NAME AS CHAR) AS REFERENCED_COLUMN_NAME,
        CAST(rc.UPDATE_RULE AS CHAR) AS UPDATE_RULE,
        CAST(rc.DELETE_RULE AS CHAR) AS DELETE_RULE
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
fn mysql_type_to_data_type(mysql_type: &str, char_max_length: Option<i64>) -> DataType {
    match mysql_type.to_uppercase().as_str() {
        "TINYINT" | "SMALLINT" => DataType::SmallInt,
        "MEDIUMINT" | "INT" | "INTEGER" => DataType::Int,
        "BIGINT" => DataType::BigInt,
        "FLOAT" => DataType::Real,
        "DOUBLE" | "REAL" => DataType::Double,
        "DECIMAL" | "NUMERIC" => DataType::Decimal(38, 10),
        "CHAR" => match char_max_length {
            Some(length) if length > 0 && length <= i64::from(u16::MAX) => {
                DataType::Char(u16::try_from(length).unwrap_or(1))
            }
            _ => DataType::Char(1),
        },
        "VARCHAR" => match char_max_length {
            Some(length) if length > 0 && length <= i64::from(u16::MAX) => {
                DataType::VarChar(u16::try_from(length).unwrap_or(255))
            }
            _ => DataType::VarChar(255),
        },
        "TEXT" | "TINYTEXT" | "MEDIUMTEXT" | "LONGTEXT" => DataType::Text,
        "BOOLEAN" | "BOOL" => DataType::Bool,
        "DATE" => DataType::Date,
        "TIME" => DataType::Time,
        "DATETIME" => DataType::DateTime,
        "TIMESTAMP" => DataType::Timestamp,
        "BLOB" | "TINYBLOB" | "MEDIUMBLOB" | "LONGBLOB" => DataType::Blob,
        "BINARY" | "VARBINARY" => DataType::Binary(None),
        "JSON" => DataType::Json,
        _ => DataType::Custom(mysql_type.to_string()),
    }
}

/// Convert `MySQL` `COLUMN_TYPE` to `DataType` (more accurate than `DATA_TYPE` for boolean and sized types)
fn mysql_column_type_to_data_type(
    column_type: &str,
    data_type: &str,
    char_max_length: Option<i64>,
) -> DataType {
    let column_type_upper = column_type.to_uppercase();

    // Handle BOOLEAN as TINYINT(1)
    if column_type_upper == "TINYINT(1)" {
        return DataType::Bool;
    }

    // Extract length from VARCHAR(n) in COLUMN_TYPE
    if column_type_upper.starts_with("VARCHAR(")
        && let Some(end) = column_type.find(')')
        && let Ok(len) = column_type[8..end].parse::<u16>()
    {
        return DataType::VarChar(len);
    }

    // Extract length from CHAR(n) in COLUMN_TYPE
    if column_type_upper.starts_with("CHAR(")
        && let Some(end) = column_type.find(')')
        && let Ok(len) = column_type[5..end].parse::<u16>()
    {
        return DataType::Char(len);
    }

    // Fall back to original type mapping
    mysql_type_to_data_type(data_type, char_max_length)
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
                return Some(DatabaseValue::Int64(int_val));
            }

            if let Ok(float_val) = default_str.parse::<f64>() {
                return Some(DatabaseValue::Real64(float_val));
            }

            // For other complex expressions, return None
            None
        }
    }
}
