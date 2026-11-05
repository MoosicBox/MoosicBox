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
//! - **Check constraints**: Not introspected (would require `MySQL` 8.0.16+)
//! - **Role-based privileges**: Not considered in introspection
//!
//! # MySQL-Specific Data Type Mappings
//!
//! `MySQL` has extensive data type support. Our mapping to [`DataType`](crate::schema::DataType)
//! is derived from `COLUMN_TYPE` (falling back to `DATA_TYPE`). The exact
//! `COLUMN_TYPE` (e.g. `int(10) unsigned`, `enum('a','b')`) is always reported in
//! [`ColumnInfo::native_type`].
//!
//! ## Integer Types
//! - `TINYINT(1)` → `Bool`
//! - `TINYINT`, `SMALLINT` → `SmallInt` (8-bit, 16-bit)
//! - `MEDIUMINT`, `INT`, `INTEGER` → `Int` (24-bit, 32-bit)
//! - `BIGINT` → `BigInt` (64-bit)
//!
//! ## Floating Point Types
//! - `FLOAT` → `Real` (32-bit float)
//! - `DOUBLE`, `REAL` → `Double` (64-bit float)
//!
//! ## Fixed Point Types
//! - `DECIMAL(p,s)`, `NUMERIC(p,s)` → `Decimal(p, s)` (`Decimal(38, 10)` when unparseable)
//!
//! ## String Types
//! - `CHAR(n)` → `Char(n)`, `VARCHAR(n)` → `VarChar(n)`
//! - `TEXT`, `TINYTEXT`, `MEDIUMTEXT`, `LONGTEXT` → `Text`
//!
//! ## Binary Types
//! - `BLOB`, `TINYBLOB`, `MEDIUMBLOB`, `LONGBLOB` → `Blob`
//! - `BINARY(n)`, `VARBINARY(n)` → `Binary(Some(n))`
//!
//! ## Date/Time Types
//! - `DATE` → `Date`, `TIME` → `Time`, `DATETIME` → `DateTime`, `TIMESTAMP` → `Timestamp`
//!
//! ## Other Types
//! - `JSON` → `Json` (`MariaDB` reports `JSON` columns as `longtext`)
//! - `BIT`, `ENUM`, `SET`, `YEAR`, `GEOMETRY` and any other type →
//!   `Custom(column_type)`; introspection never fails on unknown types
//!
//! # MySQL-Specific Behavior
//!
//! ## Database Scope
//! `MySQL` databases are reported as schemas:
//! - The table-oriented methods use `DATABASE()` (the connected database)
//! - The relation methods accept any database name as the schema
//! - `information_schema`, `mysql`, `performance_schema` and `sys` are system schemas
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
//! Generated columns are detected from `EXTRA` (`VIRTUAL GENERATED` /
//! `STORED GENERATED`) and reported in `generated`, with
//! `GENERATION_EXPRESSION` as written by the server in
//! `generation_expression` (`MySQL` backslash-escapes quotes in it).
//!
//! ## Other Column Details
//! `COLUMN_DEFAULT` is reported verbatim in `default_text`, the `ON UPDATE`
//! expression from `EXTRA` in `on_update`, `unsigned` from `COLUMN_TYPE`, and
//! `CHARACTER_SET_NAME` / `COLLATION_NAME` in `character_set` / `collation`.
//! `STATISTICS.NON_UNIQUE` is read whether the server reports it as a number
//! or as text (`TiDB`).
//!
//! ## Enum Columns
//! `ENUM` labels are parsed from `COLUMN_TYPE` into `enum_values` (in
//! declaration order); `SET` columns are not enums and report `None`.
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
    schema::{
        ColumnInfo, DataType, DatabaseInfo, ForeignKeyInfo, GeneratedKind, IndexInfo, RelationKind,
        RelationRef, RelationSummary, SchemaInfo, TableInfo,
    },
};

use sqlx::{MySqlConnection, Row, mysql::MySqlRow};

/// Schemas (databases) that `MySQL`/`MariaDB` use for their own catalogs.
const MYSQL_SYSTEM_SCHEMAS: [&str; 4] =
    ["information_schema", "mysql", "performance_schema", "sys"];

fn map_err(error: sqlx::Error) -> DatabaseError {
    DatabaseError::MysqlSqlx(super::mysql::SqlxDatabaseError::from(error))
}

/// Whether `name` is a `MySQL`/`MariaDB` system schema.
fn is_mysql_system_schema(name: &str) -> bool {
    MYSQL_SYSTEM_SCHEMAS
        .iter()
        .any(|system| system.eq_ignore_ascii_case(name))
}

/// Read an unsigned integer column regardless of the exact integer width/signedness
/// reported by the server (`MySQL` and `MariaDB` differ in `information_schema` types),
/// or as text (`TiDB` reports `STATISTICS.NON_UNIQUE` as a `VARCHAR`).
fn get_lenient_u64(row: &MySqlRow, column: &str) -> Option<u64> {
    row.try_get::<Option<u64>, _>(column)
        .ok()
        .flatten()
        .or_else(|| {
            row.try_get::<Option<u32>, _>(column)
                .ok()
                .flatten()
                .map(u64::from)
        })
        .or_else(|| {
            row.try_get::<Option<i64>, _>(column)
                .ok()
                .flatten()
                .and_then(|value| u64::try_from(value).ok())
        })
        .or_else(|| {
            row.try_get::<Option<i32>, _>(column)
                .ok()
                .flatten()
                .and_then(|value| u64::try_from(value).ok())
        })
        .or_else(|| {
            row.try_get::<Option<String>, _>(column)
                .ok()
                .flatten()
                .and_then(|value| parse_lenient_u64(&value))
        })
}

/// An unsigned integer written as text (`"0"`, `" 1 "`)
fn parse_lenient_u64(value: &str) -> Option<u64> {
    value.trim().parse().ok()
}

fn get_string(row: &MySqlRow, column: &str) -> Result<String, DatabaseError> {
    row.try_get(column).map_err(map_err)
}

fn get_opt_string(row: &MySqlRow, column: &str) -> Option<String> {
    row.try_get::<Option<String>, _>(column).ok().flatten()
}

/// Map an `information_schema.tables.TABLE_TYPE` value to a [`RelationKind`].
fn mysql_relation_kind(schema: &str, table_type: &str) -> RelationKind {
    match table_type.to_ascii_uppercase().as_str() {
        "BASE TABLE" | "SYSTEM VERSIONED" if is_mysql_system_schema(schema) => {
            RelationKind::SystemTable
        }
        "BASE TABLE" | "SYSTEM VERSIONED" | "TEMPORARY" => RelationKind::Table,
        "VIEW" => RelationKind::View,
        "SYSTEM VIEW" => RelationKind::SystemTable,
        _ => RelationKind::Other(table_type.to_ascii_lowercase()),
    }
}

/// Normalize a `TABLE_COMMENT` value (empty comments and `MySQL`'s `VIEW`
/// placeholder comment on views are reported as `None`).
fn mysql_relation_comment(kind: &RelationKind, comment: Option<String>) -> Option<String> {
    comment.filter(|comment| !comment.is_empty() && (!kind.is_view() || comment != "VIEW"))
}

/// List all databases visible to the connection as schemas
///
/// # Errors
///
/// * If the `information_schema.schemata` query fails
pub async fn mysql_sqlx_list_schemas(
    conn: &mut MySqlConnection,
) -> Result<Vec<SchemaInfo>, DatabaseError> {
    let query = "SELECT
        CAST(SCHEMA_NAME AS CHAR) AS SCHEMA_NAME,
        CAST(COALESCE(SCHEMA_NAME = DATABASE(), 0) AS SIGNED) AS IS_DEFAULT
    FROM information_schema.schemata
    ORDER BY SCHEMA_NAME";

    let rows = sqlx::query(sqlx::AssertSqlSafe(query.to_string()))
        .fetch_all(&mut *conn)
        .await
        .map_err(map_err)?;

    rows.iter()
        .map(|row| {
            let name = get_string(row, "SCHEMA_NAME")?;
            let is_default = row.try_get::<i64, _>("IS_DEFAULT").map_err(map_err)? != 0;
            Ok(SchemaInfo {
                is_system: is_mysql_system_schema(&name),
                name,
                is_default,
            })
        })
        .collect()
}

/// List the databases of the server
///
/// `information_schema`, `mysql`, `performance_schema` and `sys` are reported
/// as system databases; the current database is `DATABASE()`.
///
/// # Errors
///
/// * If the `information_schema.schemata` query fails
pub async fn mysql_sqlx_list_databases(
    conn: &mut MySqlConnection,
) -> Result<Vec<DatabaseInfo>, DatabaseError> {
    let query = "SELECT
        CAST(SCHEMA_NAME AS CHAR) AS SCHEMA_NAME,
        CAST(COALESCE(SCHEMA_NAME = DATABASE(), 0) AS SIGNED) AS IS_CURRENT
    FROM information_schema.schemata
    ORDER BY SCHEMA_NAME";

    let rows = sqlx::query(sqlx::AssertSqlSafe(query.to_string()))
        .fetch_all(&mut *conn)
        .await
        .map_err(map_err)?;

    rows.iter()
        .map(|row| {
            let name = get_string(row, "SCHEMA_NAME")?;
            let is_current = row.try_get::<i64, _>("IS_CURRENT").map_err(map_err)? != 0;
            Ok(DatabaseInfo {
                is_system: is_mysql_system_schema(&name),
                name,
                is_current,
            })
        })
        .collect()
}

/// Fetch relation summaries from `information_schema.tables`, optionally
/// restricted to a single relation name. `schema` of `None` means `DATABASE()`.
async fn fetch_relations(
    conn: &mut MySqlConnection,
    schema: Option<&str>,
    name: Option<&str>,
) -> Result<Vec<RelationSummary>, DatabaseError> {
    let mut query = "SELECT
        CAST(TABLE_SCHEMA AS CHAR) AS TABLE_SCHEMA,
        CAST(TABLE_NAME AS CHAR) AS TABLE_NAME,
        CAST(TABLE_TYPE AS CHAR) AS TABLE_TYPE,
        CAST(TABLE_COMMENT AS CHAR) AS TABLE_COMMENT,
        TABLE_ROWS
    FROM information_schema.tables
    WHERE TABLE_SCHEMA = COALESCE(?, DATABASE())"
        .to_string();
    if name.is_some() {
        query.push_str(" AND TABLE_NAME = ?");
    }
    query.push_str(" ORDER BY TABLE_NAME");

    let mut statement = sqlx::query(sqlx::AssertSqlSafe(query)).bind(schema);
    if let Some(name) = name {
        statement = statement.bind(name);
    }
    let rows = statement.fetch_all(&mut *conn).await.map_err(map_err)?;

    rows.iter()
        .map(|row| {
            let schema = get_string(row, "TABLE_SCHEMA")?;
            let table_type = get_opt_string(row, "TABLE_TYPE").unwrap_or_default();
            let kind = mysql_relation_kind(&schema, &table_type);
            let comment = mysql_relation_comment(&kind, get_opt_string(row, "TABLE_COMMENT"));
            let estimated_rows = if kind.is_view() {
                None
            } else {
                get_lenient_u64(row, "TABLE_ROWS")
            };
            Ok(RelationSummary {
                name: get_string(row, "TABLE_NAME")?,
                schema: Some(schema),
                kind,
                comment,
                estimated_rows,
            })
        })
        .collect()
}

/// List tables and views in a database (`None` = `DATABASE()`)
///
/// # Errors
///
/// * If the `information_schema.tables` query fails
pub async fn mysql_sqlx_list_relations(
    conn: &mut MySqlConnection,
    schema: Option<&str>,
) -> Result<Vec<RelationSummary>, DatabaseError> {
    fetch_relations(conn, schema, None).await
}

/// Check if a table exists in the current `MySQL` database
///
/// # Errors
///
/// * If the `information_schema.tables` query fails
pub async fn mysql_sqlx_table_exists(
    conn: &mut MySqlConnection,
    table_name: &str,
) -> Result<bool, DatabaseError> {
    // Note: We don't use LOWER() here because:
    // 1. MySQL's table name case-sensitivity depends on the filesystem and lower_case_table_names setting
    // 2. Using LOWER() with bind parameters can cause issues with collation and query optimization
    // 3. The query should match the exact case stored in information_schema
    Ok(!fetch_relations(conn, None, Some(table_name))
        .await?
        .is_empty())
}

/// List all table (and view) names in the current `MySQL` database
///
/// # Errors
///
/// * If the `information_schema.tables` query fails
pub async fn mysql_sqlx_list_tables(
    conn: &mut MySqlConnection,
) -> Result<Vec<String>, DatabaseError> {
    Ok(fetch_relations(conn, None, None)
        .await?
        .into_iter()
        .map(|relation| relation.name)
        .collect())
}

/// Get column information for a table or view (`schema` of `None` = `DATABASE()`)
async fn fetch_columns(
    conn: &mut MySqlConnection,
    schema: Option<&str>,
    table_name: &str,
) -> Result<Vec<ColumnInfo>, DatabaseError> {
    let query = "SELECT
        CAST(COLUMN_NAME AS CHAR) AS COLUMN_NAME,
        CAST(DATA_TYPE AS CHAR) AS DATA_TYPE,
        CAST(COLUMN_TYPE AS CHAR) AS COLUMN_TYPE,
        CHARACTER_MAXIMUM_LENGTH,
        CAST(IS_NULLABLE AS CHAR) AS IS_NULLABLE,
        CAST(COLUMN_DEFAULT AS CHAR) AS COLUMN_DEFAULT,
        CAST(COLUMN_KEY AS CHAR) AS COLUMN_KEY,
        CAST(EXTRA AS CHAR) AS EXTRA,
        ORDINAL_POSITION,
        CAST(GENERATION_EXPRESSION AS CHAR) AS GENERATION_EXPRESSION,
        CAST(CHARACTER_SET_NAME AS CHAR) AS CHARACTER_SET_NAME,
        CAST(COLLATION_NAME AS CHAR) AS COLLATION_NAME
    FROM information_schema.columns
    WHERE TABLE_SCHEMA = COALESCE(?, DATABASE()) AND TABLE_NAME = ?
    ORDER BY ORDINAL_POSITION";

    let rows = sqlx::query(sqlx::AssertSqlSafe(query.to_string()))
        .bind(schema)
        .bind(table_name)
        .fetch_all(&mut *conn)
        .await
        .map_err(map_err)?;

    let pk_query = "SELECT CAST(COLUMN_NAME AS CHAR) AS COLUMN_NAME
    FROM information_schema.key_column_usage
    WHERE TABLE_SCHEMA = COALESCE(?, DATABASE())
      AND TABLE_NAME = ?
      AND CONSTRAINT_NAME = 'PRIMARY'";

    let pk_rows = sqlx::query(sqlx::AssertSqlSafe(pk_query.to_string()))
        .bind(schema)
        .bind(table_name)
        .fetch_all(&mut *conn)
        .await
        .map_err(map_err)?;

    let primary_key_columns: Vec<String> = pk_rows
        .iter()
        .filter_map(|row| get_opt_string(row, "COLUMN_NAME"))
        .collect();

    let mut columns = Vec::with_capacity(rows.len());

    for row in rows {
        let column_name = get_string(&row, "COLUMN_NAME")?;
        let data_type_str = get_opt_string(&row, "DATA_TYPE").unwrap_or_default();
        let column_type_str =
            get_opt_string(&row, "COLUMN_TYPE").unwrap_or_else(|| data_type_str.clone());
        let char_max_length = get_lenient_u64(&row, "CHARACTER_MAXIMUM_LENGTH")
            .and_then(|length| i64::try_from(length).ok());

        let data_type =
            mysql_column_type_to_data_type(&column_type_str, &data_type_str, char_max_length);

        let nullable = get_opt_string(&row, "IS_NULLABLE")
            .is_some_and(|value| value.eq_ignore_ascii_case("YES"));

        let ordinal_position = get_lenient_u64(&row, "ORDINAL_POSITION")
            .and_then(|position| u32::try_from(position).ok())
            .unwrap_or_default();

        let extra = get_opt_string(&row, "EXTRA").unwrap_or_default();
        let auto_increment = extra.to_uppercase().contains("AUTO_INCREMENT");
        let generated = mysql_extra_generated_kind(&extra);

        let default_value = get_opt_string(&row, "COLUMN_DEFAULT").filter(|_| generated.is_none());
        let parsed_default = default_value.as_deref().and_then(parse_mysql_default_value);
        let enum_values = parse_mysql_enum_values(&column_type_str);
        let generation_expression = get_opt_string(&row, "GENERATION_EXPRESSION")
            .filter(|expression| generated.is_some() && !expression.is_empty());

        let is_primary_key = primary_key_columns.contains(&column_name);

        columns.push(ColumnInfo {
            name: column_name,
            data_type,
            nullable,
            is_primary_key,
            auto_increment,
            default_value: parsed_default,
            ordinal_position,
            unsigned: mysql_column_type_is_unsigned(&column_type_str),
            native_type: column_type_str,
            enum_values,
            generated,
            default_text: default_value,
            generation_expression,
            on_update: mysql_extra_on_update(&extra),
            character_set: get_opt_string(&row, "CHARACTER_SET_NAME"),
            collation: get_opt_string(&row, "COLLATION_NAME"),
        });
    }

    Ok(columns)
}

/// Get column information for a `MySQL` table in the current database
///
/// # Errors
///
/// * If the `information_schema` queries fail
pub async fn mysql_sqlx_get_table_columns(
    conn: &mut MySqlConnection,
    table_name: &str,
) -> Result<Vec<ColumnInfo>, DatabaseError> {
    fetch_columns(conn, None, table_name).await
}

/// Check if a column exists in a `MySQL` table
///
/// # Errors
///
/// * If the `information_schema.columns` query fails
pub async fn mysql_sqlx_column_exists(
    conn: &mut MySqlConnection,
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

    let row = sqlx::query(sqlx::AssertSqlSafe(query.to_string()))
        .bind(table_name)
        .bind(column_name)
        .fetch_one(&mut *conn)
        .await
        .map_err(map_err)?;

    let exists: i64 = row.try_get(0).map_err(map_err)?;

    Ok(exists != 0)
}

/// Get index information for a relation
async fn fetch_indexes(
    conn: &mut MySqlConnection,
    schema: Option<&str>,
    table_name: &str,
) -> Result<BTreeMap<String, IndexInfo>, DatabaseError> {
    let index_query = "SELECT
        CAST(INDEX_NAME AS CHAR) AS INDEX_NAME,
        NON_UNIQUE,
        CAST(COLUMN_NAME AS CHAR) AS COLUMN_NAME
    FROM information_schema.STATISTICS
    WHERE TABLE_SCHEMA = COALESCE(?, DATABASE()) AND TABLE_NAME = ?
    ORDER BY INDEX_NAME, SEQ_IN_INDEX";

    let index_rows = sqlx::query(sqlx::AssertSqlSafe(index_query.to_string()))
        .bind(schema)
        .bind(table_name)
        .fetch_all(&mut *conn)
        .await
        .map_err(map_err)?;

    let mut indexes_map: BTreeMap<String, IndexInfo> = BTreeMap::new();
    for row in index_rows {
        let index_name = get_string(&row, "INDEX_NAME")?;
        let non_unique = get_lenient_u64(&row, "NON_UNIQUE").unwrap_or(1);
        // Functional index parts have a NULL column name
        let Some(column_name) = get_opt_string(&row, "COLUMN_NAME") else {
            continue;
        };

        indexes_map
            .entry(index_name.clone())
            .or_insert_with(|| IndexInfo {
                is_primary: index_name == "PRIMARY",
                name: index_name,
                unique: non_unique == 0,
                columns: vec![],
            })
            .columns
            .push(column_name);
    }

    Ok(indexes_map)
}

/// Get foreign key information for a relation
async fn fetch_foreign_keys(
    conn: &mut MySqlConnection,
    schema: Option<&str>,
    table_name: &str,
) -> Result<BTreeMap<String, ForeignKeyInfo>, DatabaseError> {
    let fk_query = "SELECT
        CAST(kcu.CONSTRAINT_NAME AS CHAR) AS CONSTRAINT_NAME,
        CAST(kcu.COLUMN_NAME AS CHAR) AS COLUMN_NAME,
        CAST(kcu.REFERENCED_TABLE_SCHEMA AS CHAR) AS REFERENCED_TABLE_SCHEMA,
        CAST(kcu.REFERENCED_TABLE_NAME AS CHAR) AS REFERENCED_TABLE_NAME,
        CAST(kcu.REFERENCED_COLUMN_NAME AS CHAR) AS REFERENCED_COLUMN_NAME,
        CAST(rc.UPDATE_RULE AS CHAR) AS UPDATE_RULE,
        CAST(rc.DELETE_RULE AS CHAR) AS DELETE_RULE
    FROM information_schema.KEY_COLUMN_USAGE kcu
    JOIN information_schema.REFERENTIAL_CONSTRAINTS rc
        ON kcu.CONSTRAINT_NAME = rc.CONSTRAINT_NAME
        AND kcu.CONSTRAINT_SCHEMA = rc.CONSTRAINT_SCHEMA
    WHERE kcu.TABLE_SCHEMA = COALESCE(?, DATABASE())
      AND kcu.TABLE_NAME = ?
      AND kcu.REFERENCED_TABLE_NAME IS NOT NULL
    ORDER BY kcu.CONSTRAINT_NAME, kcu.ORDINAL_POSITION";

    let fk_rows = sqlx::query(sqlx::AssertSqlSafe(fk_query.to_string()))
        .bind(schema)
        .bind(table_name)
        .fetch_all(&mut *conn)
        .await
        .map_err(map_err)?;

    let mut foreign_keys_map = BTreeMap::new();
    for row in fk_rows {
        let constraint_name = get_string(&row, "CONSTRAINT_NAME")?;
        let referenced_table = get_string(&row, "REFERENCED_TABLE_NAME")?;
        foreign_keys_map
            .entry(constraint_name.clone())
            .or_insert_with(|| ForeignKeyInfo {
                referenced_schema: get_opt_string(&row, "REFERENCED_TABLE_SCHEMA"),
                on_update: get_opt_string(&row, "UPDATE_RULE"),
                on_delete: get_opt_string(&row, "DELETE_RULE"),
                ..ForeignKeyInfo::new(constraint_name, referenced_table)
            })
            .push_pair(
                get_string(&row, "COLUMN_NAME")?,
                get_string(&row, "REFERENCED_COLUMN_NAME")?,
            );
    }

    Ok(foreign_keys_map)
}

/// Get columns, indexes and foreign keys of a table or view in any database
///
/// # Errors
///
/// * If the `information_schema` queries fail
pub async fn mysql_sqlx_get_relation_info(
    conn: &mut MySqlConnection,
    relation: &RelationRef,
) -> Result<Option<TableInfo>, DatabaseError> {
    let schema = relation.schema.as_deref();
    let Some(summary) = fetch_relations(conn, schema, Some(&relation.name))
        .await?
        .into_iter()
        .next()
    else {
        return Ok(None);
    };

    // Use the resolved names so later queries don't depend on DATABASE() again
    let resolved_schema = summary.schema.as_deref().or(schema);
    let columns = fetch_columns(conn, resolved_schema, &summary.name)
        .await?
        .into_iter()
        .map(|column| (column.name.clone(), column))
        .collect();
    let indexes = fetch_indexes(conn, resolved_schema, &summary.name).await?;
    let foreign_keys = fetch_foreign_keys(conn, resolved_schema, &summary.name).await?;

    Ok(Some(TableInfo {
        name: summary.name,
        schema: summary.schema,
        kind: summary.kind,
        comment: summary.comment,
        columns,
        indexes,
        foreign_keys,
    }))
}

/// Get comprehensive table information for a `MySQL` table in the current database
///
/// # Errors
///
/// * If the `information_schema` queries fail
pub async fn mysql_sqlx_get_table_info(
    conn: &mut MySqlConnection,
    table_name: &str,
) -> Result<Option<TableInfo>, DatabaseError> {
    mysql_sqlx_get_relation_info(conn, &RelationRef::new(table_name)).await
}

/// Get the `SELECT` text of a view from `information_schema.views`
///
/// Returns `None` if the relation isn't a view. `MySQL` reports an empty
/// definition when the user lacks the `SHOW VIEW` privilege.
///
/// # Errors
///
/// * If the `information_schema.views` query fails
pub async fn mysql_sqlx_get_view_definition(
    conn: &mut MySqlConnection,
    relation: &RelationRef,
) -> Result<Option<String>, DatabaseError> {
    let query = "SELECT CAST(VIEW_DEFINITION AS CHAR) AS VIEW_DEFINITION
    FROM information_schema.views
    WHERE TABLE_SCHEMA = COALESCE(?, DATABASE()) AND TABLE_NAME = ?";

    let row = sqlx::query(sqlx::AssertSqlSafe(query.to_string()))
        .bind(relation.schema.as_deref())
        .bind(&relation.name)
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_err)?;

    Ok(row.map(|row| get_opt_string(&row, "VIEW_DEFINITION").unwrap_or_default()))
}

/// Parse the first `(a)` or `(a,b)` numeric arguments from a column type
fn type_args(column_type: &str) -> Vec<u32> {
    column_type
        .find('(')
        .and_then(|start| {
            column_type[start + 1..]
                .find(')')
                .map(|end| &column_type[start + 1..start + 1 + end])
        })
        .map(|args| {
            args.split(',')
                .filter_map(|arg| arg.trim().parse::<u32>().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Map `MySQL` `DATA_TYPE` values to our `DataType` enum
fn mysql_type_to_data_type(mysql_type: &str, char_max_length: Option<i64>) -> Option<DataType> {
    let length = || {
        char_max_length
            .filter(|length| *length > 0)
            .and_then(|length| u16::try_from(length).ok())
    };
    Some(match mysql_type.to_ascii_uppercase().as_str() {
        "TINYINT" | "SMALLINT" => DataType::SmallInt,
        "MEDIUMINT" | "INT" | "INTEGER" => DataType::Int,
        "BIGINT" => DataType::BigInt,
        "FLOAT" => DataType::Real,
        "DOUBLE" | "REAL" | "DOUBLE PRECISION" => DataType::Double,
        "DECIMAL" | "NUMERIC" => DataType::Decimal(38, 10),
        "CHAR" => DataType::Char(length().unwrap_or(1)),
        "VARCHAR" => DataType::VarChar(length().unwrap_or(255)),
        "TEXT" | "TINYTEXT" | "MEDIUMTEXT" | "LONGTEXT" => DataType::Text,
        "BOOLEAN" | "BOOL" => DataType::Bool,
        "DATE" => DataType::Date,
        "TIME" => DataType::Time,
        "DATETIME" => DataType::DateTime,
        "TIMESTAMP" => DataType::Timestamp,
        "BLOB" | "TINYBLOB" | "MEDIUMBLOB" | "LONGBLOB" => DataType::Blob,
        "BINARY" | "VARBINARY" => DataType::Binary(None),
        "JSON" => DataType::Json,
        "UUID" => DataType::Uuid,
        "INET4" | "INET6" => DataType::Inet,
        _ => return None,
    })
}

/// Convert `MySQL` `COLUMN_TYPE` to `DataType` (more accurate than `DATA_TYPE` for
/// boolean and sized types). Unknown types map to `DataType::Custom(column_type)`.
fn mysql_column_type_to_data_type(
    column_type: &str,
    data_type: &str,
    char_max_length: Option<i64>,
) -> DataType {
    let column_type_upper = column_type.trim().to_ascii_uppercase();
    let base = column_type_upper
        .split(['(', ' '])
        .next()
        .unwrap_or_default();
    let args = type_args(&column_type_upper);

    match (base, args.as_slice()) {
        // BOOLEAN is stored as TINYINT(1)
        ("TINYINT", [1]) if column_type_upper == "TINYINT(1)" => return DataType::Bool,
        ("VARCHAR", [length]) => {
            if let Ok(length) = u16::try_from(*length) {
                return DataType::VarChar(length);
            }
        }
        ("CHAR", [length]) => {
            if let Ok(length) = u16::try_from(*length) {
                return DataType::Char(length);
            }
        }
        ("DECIMAL" | "NUMERIC", [precision, scale]) => {
            if let (Ok(precision), Ok(scale)) = (u8::try_from(*precision), u8::try_from(*scale)) {
                return DataType::Decimal(precision, scale);
            }
        }
        ("DECIMAL" | "NUMERIC", [precision]) => {
            if let Ok(precision) = u8::try_from(*precision) {
                return DataType::Decimal(precision, 0);
            }
        }
        ("BINARY" | "VARBINARY", [length]) => return DataType::Binary(Some(*length)),
        _ => {}
    }

    mysql_type_to_data_type(data_type, char_max_length)
        .or_else(|| mysql_type_to_data_type(base, char_max_length))
        .unwrap_or_else(|| DataType::Custom(column_type.to_string()))
}

/// The generated-column kind marked by an `information_schema.columns.EXTRA` value.
///
/// `MySQL` reports `VIRTUAL GENERATED` / `STORED GENERATED` (`MariaDB` may use
/// `PERSISTENT GENERATED` for stored); `DEFAULT_GENERATED` marks an expression
/// default, not a generated column, so only the standalone `GENERATED` word counts.
fn mysql_extra_generated_kind(extra: &str) -> Option<GeneratedKind> {
    let words: Vec<&str> = extra.split_whitespace().collect();
    if !words
        .iter()
        .any(|word| word.eq_ignore_ascii_case("GENERATED"))
    {
        return None;
    }
    Some(
        if words.iter().any(|word| {
            word.eq_ignore_ascii_case("STORED") || word.eq_ignore_ascii_case("PERSISTENT")
        }) {
            GeneratedKind::Stored
        } else {
            GeneratedKind::Virtual
        },
    )
}

/// The `ON UPDATE` expression in an `information_schema.columns.EXTRA`
/// value (`on update CURRENT_TIMESTAMP`, `on update current_timestamp(3)`).
fn mysql_extra_on_update(extra: &str) -> Option<String> {
    let mut words = extra.split_whitespace();
    while let Some(word) = words.next() {
        if word.eq_ignore_ascii_case("on") {
            let mut rest = words.clone();
            if rest
                .next()
                .is_some_and(|next| next.eq_ignore_ascii_case("update"))
            {
                return rest.next().map(ToString::to_string);
            }
        }
    }
    None
}

/// Whether a `COLUMN_TYPE` is `unsigned` (`int(10) unsigned zerofill`).
fn mysql_column_type_is_unsigned(column_type: &str) -> bool {
    // The attributes follow the type's arguments, which may be quoted labels.
    let attributes = column_type.rfind(')').map_or_else(
        || column_type.split_once(' ').map_or("", |(_, rest)| rest),
        |close| &column_type[close + 1..],
    );
    attributes
        .split_whitespace()
        .any(|word| word.eq_ignore_ascii_case("unsigned"))
}

/// Labels of an `enum('a','b')` `COLUMN_TYPE` (`None` for any other type).
///
/// `COLUMN_TYPE` doubles embedded single quotes and backslash-escapes
/// backslashes and control characters, both of which are decoded.
fn parse_mysql_enum_values(column_type: &str) -> Option<Vec<String>> {
    crate::schema::parse_enum_type_labels(column_type, true)
}

/// Parse `MySQL` default values into `DatabaseValue`
fn parse_mysql_default_value(default_str: &str) -> Option<DatabaseValue> {
    if default_str.is_empty() || default_str.to_uppercase() == "NULL" {
        return None;
    }

    // Handle MySQL specific defaults
    match default_str.to_uppercase().as_str() {
        "CURRENT_TIMESTAMP" | "CURRENT_TIMESTAMP()" | "NOW()" => Some(DatabaseValue::Now),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn map(column_type: &str, data_type: &str) -> DataType {
        mysql_column_type_to_data_type(column_type, data_type, None)
    }

    #[test]
    fn maps_column_types() {
        assert_eq!(map("tinyint(1)", "tinyint"), DataType::Bool);
        assert_eq!(map("tinyint(4)", "tinyint"), DataType::SmallInt);
        assert_eq!(map("int(10) unsigned", "int"), DataType::Int);
        assert_eq!(map("bigint unsigned", "bigint"), DataType::BigInt);
        assert_eq!(map("varchar(255)", "varchar"), DataType::VarChar(255));
        assert_eq!(map("char(3)", "char"), DataType::Char(3));
        assert_eq!(map("decimal(10,2)", "decimal"), DataType::Decimal(10, 2));
        assert_eq!(map("decimal(8)", "decimal"), DataType::Decimal(8, 0));
        assert_eq!(
            map("varbinary(16)", "varbinary"),
            DataType::Binary(Some(16))
        );
        assert_eq!(map("longblob", "longblob"), DataType::Blob);
        assert_eq!(map("date", "date"), DataType::Date);
        assert_eq!(map("time(3)", "time"), DataType::Time);
        assert_eq!(map("datetime(6)", "datetime"), DataType::DateTime);
        assert_eq!(map("timestamp", "timestamp"), DataType::Timestamp);
        assert_eq!(map("json", "json"), DataType::Json);
        assert_eq!(map("double", "double"), DataType::Double);
        assert_eq!(map("float", "float"), DataType::Real);
    }

    #[test]
    fn unknown_types_map_to_custom_native_type() {
        assert_eq!(
            map("enum('a','b')", "enum"),
            DataType::Custom("enum('a','b')".to_string())
        );
        assert_eq!(
            map("set('x')", "set"),
            DataType::Custom("set('x')".to_string())
        );
        assert_eq!(map("bit(8)", "bit"), DataType::Custom("bit(8)".to_string()));
        assert_eq!(map("year", "year"), DataType::Custom("year".to_string()));
        assert_eq!(
            map("geometry", "geometry"),
            DataType::Custom("geometry".to_string())
        );
        assert_eq!(map("", ""), DataType::Custom(String::new()));
    }

    #[test]
    fn relation_kinds_and_comments() {
        assert_eq!(
            mysql_relation_kind("app", "BASE TABLE"),
            RelationKind::Table
        );
        assert_eq!(mysql_relation_kind("app", "VIEW"), RelationKind::View);
        assert_eq!(
            mysql_relation_kind("information_schema", "SYSTEM VIEW"),
            RelationKind::SystemTable
        );
        assert_eq!(
            mysql_relation_kind("mysql", "BASE TABLE"),
            RelationKind::SystemTable
        );
        assert_eq!(
            mysql_relation_kind("app", "SEQUENCE"),
            RelationKind::Other("sequence".to_string())
        );
        assert_eq!(
            mysql_relation_comment(&RelationKind::View, Some("VIEW".to_string())),
            None
        );
        assert_eq!(
            mysql_relation_comment(&RelationKind::Table, Some(String::new())),
            None
        );
        assert_eq!(
            mysql_relation_comment(&RelationKind::Table, Some("users".to_string())),
            Some("users".to_string())
        );
        assert!(is_mysql_system_schema("PERFORMANCE_SCHEMA"));
        assert!(!is_mysql_system_schema("app"));
    }

    #[test]
    fn parses_enum_values() {
        let labels = |values: &[&str]| Some(values.iter().map(ToString::to_string).collect());
        assert_eq!(
            parse_mysql_enum_values("enum('a','b')"),
            labels(&["a", "b"])
        );
        assert_eq!(
            parse_mysql_enum_values("ENUM('it''s','x,y', ' spaced ')"),
            labels(&["it's", "x,y", " spaced "])
        );
        assert_eq!(
            parse_mysql_enum_values(r"enum('back\\slash','new\nline','')"),
            labels(&["back\\slash", "new\nline", ""])
        );
        assert_eq!(
            parse_mysql_enum_values("enum('(paren)')"),
            labels(&["(paren)"])
        );
        assert_eq!(parse_mysql_enum_values("set('a','b')"), None);
        assert_eq!(parse_mysql_enum_values("varchar(10)"), None);
        assert_eq!(parse_mysql_enum_values("enum"), None);
        assert_eq!(parse_mysql_enum_values("enum('unterminated)"), None);
    }

    #[test]
    fn detects_generated_columns_from_extra() {
        let kind = mysql_extra_generated_kind;
        assert_eq!(kind("VIRTUAL GENERATED"), Some(GeneratedKind::Virtual));
        assert_eq!(kind("STORED GENERATED"), Some(GeneratedKind::Stored));
        assert_eq!(kind("PERSISTENT GENERATED"), Some(GeneratedKind::Stored));
        assert_eq!(kind("DEFAULT_GENERATED"), None);
        assert_eq!(kind("DEFAULT_GENERATED on update CURRENT_TIMESTAMP"), None);
        assert_eq!(kind("auto_increment"), None);
        assert_eq!(kind(""), None);
    }

    #[test]
    fn reads_on_update_and_unsigned() {
        assert_eq!(
            mysql_extra_on_update("DEFAULT_GENERATED on update CURRENT_TIMESTAMP"),
            Some("CURRENT_TIMESTAMP".to_string())
        );
        assert_eq!(
            mysql_extra_on_update("on update current_timestamp(3)"),
            Some("current_timestamp(3)".to_string())
        );
        assert_eq!(mysql_extra_on_update("auto_increment"), None);
        assert_eq!(mysql_extra_on_update("on"), None);
        assert!(mysql_column_type_is_unsigned("int(10) unsigned"));
        assert!(mysql_column_type_is_unsigned("bigint UNSIGNED zerofill"));
        assert!(!mysql_column_type_is_unsigned("int"));
        assert!(!mysql_column_type_is_unsigned("enum('unsigned')"));
        assert!(!mysql_column_type_is_unsigned("enum('a', 'b unsigned c')"));
    }

    #[test]
    fn reads_integers_written_as_text() {
        assert_eq!(parse_lenient_u64("0"), Some(0));
        assert_eq!(parse_lenient_u64(" 1 "), Some(1));
        assert_eq!(parse_lenient_u64("yes"), None);
    }

    #[test]
    fn parses_defaults() {
        assert_eq!(
            parse_mysql_default_value("current_timestamp()"),
            Some(DatabaseValue::Now)
        );
        assert_eq!(
            parse_mysql_default_value("'abc'"),
            Some(DatabaseValue::String("abc".to_string()))
        );
        assert_eq!(parse_mysql_default_value("NULL"), None);
    }
}
