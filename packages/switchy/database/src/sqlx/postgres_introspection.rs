use crate::schema::{ColumnInfo, DataType, ForeignKeyInfo, IndexInfo, TableInfo};
use crate::{DatabaseError, DatabaseValue};
use sqlx::{PgConnection, Row};
use std::collections::BTreeMap;

/// Check if a table exists in the `PostgreSQL` database
pub async fn postgres_sqlx_table_exists(
    conn: &mut PgConnection,
    table_name: &str,
) -> Result<bool, DatabaseError> {
    let query = "SELECT EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'public' AND table_name = $1
    )";

    let row = sqlx::query(query)
        .bind(table_name)
        .fetch_one(conn)
        .await
        .map_err(|e| {
            DatabaseError::PostgresSqlx(crate::sqlx::postgres::SqlxDatabaseError::from(e))
        })?;

    let exists: bool = row.get(0);
    Ok(exists)
}

/// List all table names in the 'public' schema of `PostgreSQL`
pub async fn postgres_sqlx_list_tables(
    conn: &mut PgConnection,
) -> Result<Vec<String>, DatabaseError> {
    let query = "SELECT tablename FROM pg_tables WHERE schemaname = 'public'";

    let rows = sqlx::query(query).fetch_all(conn).await.map_err(|e| {
        DatabaseError::PostgresSqlx(crate::sqlx::postgres::SqlxDatabaseError::from(e))
    })?;

    let mut tables = Vec::new();
    for row in rows {
        let table_name: String = row.get("tablename");
        tables.push(table_name);
    }

    Ok(tables)
}

/// Get column metadata for a table in `PostgreSQL`
pub async fn postgres_sqlx_get_table_columns(
    conn: &mut PgConnection,
    table_name: &str,
) -> Result<Vec<ColumnInfo>, DatabaseError> {
    let query = "SELECT
        column_name,
        data_type,
        character_maximum_length,
        is_nullable,
        column_default,
        ordinal_position
    FROM information_schema.columns
    WHERE table_schema = 'public' AND table_name = $1
    ORDER BY ordinal_position";

    let rows = sqlx::query(query)
        .bind(table_name)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| {
            DatabaseError::PostgresSqlx(crate::sqlx::postgres::SqlxDatabaseError::from(e))
        })?;

    // Get primary key columns
    let pk_query = "SELECT kcu.column_name
    FROM information_schema.table_constraints tc
    JOIN information_schema.key_column_usage kcu
      ON tc.constraint_name = kcu.constraint_name
    WHERE tc.table_schema = 'public'
      AND tc.table_name = $1
      AND tc.constraint_type = 'PRIMARY KEY'";

    let pk_rows = sqlx::query(pk_query)
        .bind(table_name)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| {
            DatabaseError::PostgresSqlx(crate::sqlx::postgres::SqlxDatabaseError::from(e))
        })?;

    let primary_key_columns: Vec<String> =
        pk_rows.iter().map(|row| row.get::<String, _>(0)).collect();

    let mut columns = Vec::new();

    for row in rows {
        let column_name: String = row.get(0);
        let data_type_str: String = row.get(1);
        let char_max_length: Option<i32> = row.get(2);
        let is_nullable_str: String = row.get(3);
        let column_default: Option<String> = row.get(4);
        let ordinal_position: i32 = row.get(5);

        let data_type = postgres_sqlx_type_to_data_type(&data_type_str, char_max_length)?;
        let nullable = is_nullable_str == "YES";
        let is_primary_key = primary_key_columns.contains(&column_name);
        let default_value = column_default.as_deref().and_then(parse_sqlx_default_value);

        columns.push(ColumnInfo {
            name: column_name,
            data_type,
            nullable,
            is_primary_key,
            auto_increment: false, // PostgreSQL uses SERIAL/IDENTITY, handled separately
            default_value,
            ordinal_position: u32::try_from(ordinal_position).unwrap_or(0),
        });
    }

    Ok(columns)
}

/// Map `PostgreSQL` data types to our `DataType` enum (sqlx version)
fn postgres_sqlx_type_to_data_type(
    pg_type: &str,
    char_max_length: Option<i32>,
) -> Result<DataType, DatabaseError> {
    match pg_type.to_lowercase().as_str() {
        "smallint" | "int2" => Ok(DataType::SmallInt),
        "integer" | "int" | "int4" => Ok(DataType::Int),
        "bigint" | "int8" => Ok(DataType::BigInt),
        "serial" => Ok(DataType::Serial),
        "bigserial" => Ok(DataType::BigSerial),
        "character varying" | "varchar" => match char_max_length {
            Some(length) if length > 0 => {
                Ok(DataType::VarChar(u16::try_from(length).unwrap_or(255)))
            }
            _ => Ok(DataType::VarChar(255)),
        },
        "character" | "char" => Ok(DataType::Char(1)),
        "text" => Ok(DataType::Text),
        "boolean" | "bool" => Ok(DataType::Bool),
        "real" | "float4" => Ok(DataType::Real),
        "double precision" | "float8" => Ok(DataType::Double),
        "numeric" | "decimal" => Ok(DataType::Decimal(38, 10)),
        "money" => Ok(DataType::Money),
        "date" => Ok(DataType::Date),
        "time" => Ok(DataType::Time),
        "timestamp" | "timestamp without time zone" => Ok(DataType::Timestamp),
        "timestamptz" | "timestamp with time zone" => Ok(DataType::DateTime),
        "bytea" => Ok(DataType::Blob),
        "json" => Ok(DataType::Json),
        "jsonb" => Ok(DataType::Jsonb),
        "uuid" => Ok(DataType::Uuid),
        "xml" => Ok(DataType::Xml),
        "inet" => Ok(DataType::Inet),
        "macaddr" => Ok(DataType::MacAddr),
        t if t.starts_with('_') => {
            // Array types in PostgreSQL start with underscore
            let inner = &t[1..];
            postgres_sqlx_type_to_data_type(inner, None).map(|dt| DataType::Array(Box::new(dt)))
        }
        _ => Ok(DataType::Custom(pg_type.to_string())),
    }
}

/// Parse `PostgreSQL` default value formats (sqlx version)
fn parse_sqlx_default_value(default_str: &str) -> Option<DatabaseValue> {
    // Handle common PostgreSQL default formats
    if default_str.starts_with('\'') && default_str.contains("'::") {
        // Format: 'value'::type
        if let Some(end_quote) = default_str[1..].find('\'') {
            let value = &default_str[1..=end_quote];
            return Some(DatabaseValue::String(value.to_string()));
        }
    }

    if default_str.starts_with("nextval(") {
        // Sequence default - not representable as simple value
        return None;
    }

    match default_str.to_uppercase().as_str() {
        "TRUE" => Some(DatabaseValue::Bool(true)),
        "FALSE" => Some(DatabaseValue::Bool(false)),
        "NULL" => None,
        _ => {
            // Try parsing as number
            default_str.parse::<i64>().map_or_else(
                |_| {
                    default_str.parse::<f64>().map_or_else(
                        |_| {
                            // Treat as string literal
                            Some(DatabaseValue::String(default_str.to_string()))
                        },
                        |float_val| Some(DatabaseValue::Real64(float_val)),
                    )
                },
                |int_val| Some(DatabaseValue::Int64(int_val)),
            )
        }
    }
}

/// Check if a column exists in a table (sqlx version)
pub async fn postgres_sqlx_column_exists(
    conn: &mut PgConnection,
    table_name: &str,
    column_name: &str,
) -> Result<bool, DatabaseError> {
    let query = "SELECT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public'
        AND table_name = $1
        AND column_name = $2
    )";

    let row = sqlx::query(query)
        .bind(table_name)
        .bind(column_name)
        .fetch_one(conn)
        .await
        .map_err(|e| {
            DatabaseError::PostgresSqlx(crate::sqlx::postgres::SqlxDatabaseError::from(e))
        })?;

    let exists: bool = row.get(0);
    Ok(exists)
}

/// Get full table information including indexes and foreign keys (sqlx version)
pub async fn postgres_sqlx_get_table_info(
    conn: &mut PgConnection,
    table_name: &str,
) -> Result<Option<TableInfo>, DatabaseError> {
    // Check if table exists first
    if !postgres_sqlx_table_exists(conn, table_name).await? {
        return Ok(None);
    }

    // Get columns
    let columns_list = postgres_sqlx_get_table_columns(conn, table_name).await?;
    let mut columns = BTreeMap::new();
    for column in columns_list {
        columns.insert(column.name.clone(), column);
    }

    // Get indexes
    let index_query = "SELECT
        i.indexname as index_name,
        i.indexdef
    FROM pg_indexes i
    WHERE i.schemaname = 'public' AND i.tablename = $1";

    let index_rows = sqlx::query(index_query)
        .bind(table_name)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| {
            DatabaseError::PostgresSqlx(crate::sqlx::postgres::SqlxDatabaseError::from(e))
        })?;

    let mut indexes = BTreeMap::new();
    for row in index_rows {
        let index_name: String = row.get(0);
        let index_def: String = row.get(1);

        // Parse index definition to determine if unique and get columns
        let unique = index_def.to_uppercase().contains("UNIQUE");
        let is_primary = index_name.ends_with("_pkey");

        // Simple column extraction (this could be enhanced for complex indexes)
        let columns_part = if let Some(start) = index_def.find('(') {
            if let Some(end) = index_def.find(')') {
                &index_def[start + 1..end]
            } else {
                continue;
            }
        } else {
            continue;
        };

        let index_columns: Vec<String> = columns_part
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();

        indexes.insert(
            index_name.clone(),
            IndexInfo {
                name: index_name,
                unique,
                columns: index_columns,
                is_primary,
            },
        );
    }

    // Get foreign keys
    let fk_query = "SELECT
        tc.constraint_name,
        kcu.column_name,
        ccu.table_name AS foreign_table_name,
        ccu.column_name AS foreign_column_name
    FROM information_schema.table_constraints AS tc
    JOIN information_schema.key_column_usage AS kcu
        ON tc.constraint_name = kcu.constraint_name
        AND tc.table_schema = kcu.table_schema
    JOIN information_schema.constraint_column_usage AS ccu
        ON ccu.constraint_name = tc.constraint_name
        AND ccu.table_schema = tc.table_schema
    WHERE tc.constraint_type = 'FOREIGN KEY'
        AND tc.table_schema = 'public'
        AND tc.table_name = $1";

    let fk_rows = sqlx::query(fk_query)
        .bind(table_name)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| {
            DatabaseError::PostgresSqlx(crate::sqlx::postgres::SqlxDatabaseError::from(e))
        })?;

    let mut foreign_keys = BTreeMap::new();
    for row in fk_rows {
        let constraint_name: String = row.get(0);
        let column_name: String = row.get(1);
        let referenced_table: String = row.get(2);
        let referenced_column: String = row.get(3);

        foreign_keys.insert(
            constraint_name.clone(),
            ForeignKeyInfo {
                name: constraint_name,
                column: column_name,
                referenced_table,
                referenced_column,
                on_update: None, // Could be enhanced to query referential actions
                on_delete: None,
            },
        );
    }

    Ok(Some(TableInfo {
        name: table_name.to_string(),
        columns,
        indexes,
        foreign_keys,
    }))
}
