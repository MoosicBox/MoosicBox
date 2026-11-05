//! Connection-level helpers shared by [`super::TursoDatabase`] and
//! [`super::TursoTransaction`]: positional raw query results and schema
//! introspection.
//!
//! All functions operate on a borrowed [`turso::Connection`] so the database
//! handle and transactions run the exact same code on their own connection.
//!
//! # Schemas
//!
//! # Turso Limitations
//!
//! Turso reports declared column types without their arguments (e.g.
//! `VARCHAR(50)` is reported as `VARCHAR`, `DECIMAL(10, 2)` as `DECIMAL`), so
//! [`ColumnInfo::native_type`](crate::schema::ColumnInfo::native_type) and
//! [`ResultColumn::type_name`] carry the base type name only and sized types
//! map to their unsized [`DataType`](crate::schema::DataType) fallback.
//! Internal `__turso_internal*` bookkeeping tables are hidden from listings.
//!
//! # Schemas
//!
//! Schemas are `SQLite` attached databases as reported by
//! `PRAGMA database_list` (`main` is the default, `temp` is reported as a
//! system schema). Relations in a schema that is not attached are reported as
//! absent (empty list / `None`) rather than as an error.

#[cfg(feature = "schema")]
use std::collections::BTreeMap;

use crate::{DatabaseError, DatabaseValue, QueryResult, ResultColumn};

use super::{to_turso_params, turso_transform_query_for_params};

#[cfg(feature = "schema")]
use crate::{
    Row,
    schema::{
        ColumnInfo, ForeignKeyInfo, IndexInfo, RelationKind, RelationSummary, SchemaInfo, TableInfo,
    },
    sqlite_common::{
        SQLITE_DEFAULT_SCHEMA, SQLITE_LIST_SCHEMAS_SQL, quote_sqlite_identifier,
        sqlite_catalog_table, sqlite_list_relations_sql, sqlite_relation_kind, sqlite_relation_sql,
        sqlite_schema_info, sqlite_type_to_data_type,
    },
};

fn turso_err(error: turso::Error) -> DatabaseError {
    DatabaseError::Turso(error.into())
}

/// Execute a query on `connection` and return a positional result set.
///
/// Column names and declared types come from the prepared statement, so they
/// are present even when no rows are returned. Duplicate column names are
/// preserved.
///
/// # Errors
///
/// * If the query cannot be prepared or executed
/// * If a parameter cannot be converted to a Turso value
pub(super) async fn query_result(
    connection: &turso::Connection,
    query: &str,
    params: &[DatabaseValue],
) -> Result<QueryResult, DatabaseError> {
    let (transformed_query, filtered_params) = turso_transform_query_for_params(query, params)?;

    let mut stmt = connection
        .prepare(&transformed_query)
        .await
        .map_err(turso_err)?;

    let columns: Vec<ResultColumn> = stmt
        .columns()
        .iter()
        .map(|column| ResultColumn {
            name: column.name().to_string(),
            type_name: column
                .decl_type()
                .filter(|decl| !decl.is_empty())
                .map(str::to_string),
        })
        .collect();

    let turso_params = to_turso_params(&filtered_params).map_err(DatabaseError::Turso)?;

    let mut rows = stmt.query(turso_params).await.map_err(turso_err)?;

    let mut results = Vec::new();
    while let Some(row) = rows.next().await.map_err(turso_err)? {
        let mut values = Vec::with_capacity(columns.len());
        for index in 0..columns.len() {
            values.push(DatabaseValue::from(
                row.get_value(index).map_err(turso_err)?,
            ));
        }
        results.push(values);
    }

    Ok(QueryResult {
        columns,
        rows: results,
    })
}

#[cfg(feature = "schema")]
async fn query_rows(
    connection: &turso::Connection,
    query: &str,
    params: &[DatabaseValue],
) -> Result<Vec<Row>, DatabaseError> {
    Ok(query_result(connection, query, params).await?.into_rows())
}

#[cfg(feature = "schema")]
fn text(row: &Row, column: &str) -> Option<String> {
    match row.get(column) {
        Some(DatabaseValue::String(value) | DatabaseValue::StringOpt(Some(value))) => Some(value),
        _ => None,
    }
}

/// Prefix of Turso-internal bookkeeping tables (e.g. `AUTOINCREMENT` sequences).
#[cfg(feature = "schema")]
const TURSO_INTERNAL_PREFIX: &str = "__turso_internal";

/// Whether a catalog name is an internal object hidden from relation listings.
#[cfg(feature = "schema")]
fn is_internal_name(name: &str) -> bool {
    name.starts_with(TURSO_INTERNAL_PREFIX)
}

#[cfg(feature = "schema")]
fn int(row: &Row, column: &str) -> Option<i64> {
    row.get(column).and_then(|value| value.as_i64())
}

/// List attached databases as schemas.
///
/// # Errors
///
/// * If `PRAGMA database_list` fails
#[cfg(feature = "schema")]
pub(super) async fn list_schemas(
    connection: &turso::Connection,
) -> Result<Vec<SchemaInfo>, DatabaseError> {
    let rows = query_rows(connection, SQLITE_LIST_SCHEMAS_SQL, &[]).await?;
    let mut schemas: Vec<SchemaInfo> = rows
        .iter()
        .filter_map(|row| text(row, "name"))
        .map(|name| sqlite_schema_info(&name))
        .collect();
    if !schemas.iter().any(|schema| schema.is_default) {
        schemas.insert(0, sqlite_schema_info(SQLITE_DEFAULT_SCHEMA));
    }
    Ok(schemas)
}

/// Whether `schema` is `main` or an attached database name.
#[cfg(feature = "schema")]
async fn schema_exists(
    connection: &turso::Connection,
    schema: &str,
) -> Result<bool, DatabaseError> {
    if schema.eq_ignore_ascii_case(SQLITE_DEFAULT_SCHEMA) {
        return Ok(true);
    }
    Ok(list_schemas(connection)
        .await?
        .iter()
        .any(|info| info.name.eq_ignore_ascii_case(schema)))
}

/// List tables and views in `schema` (`None` = `main`).
///
/// # Errors
///
/// * If the catalog query fails
#[cfg(feature = "schema")]
pub(super) async fn list_relations(
    connection: &turso::Connection,
    schema: Option<&str>,
) -> Result<Vec<RelationSummary>, DatabaseError> {
    let resolved = schema.unwrap_or(SQLITE_DEFAULT_SCHEMA);
    if !schema_exists(connection, resolved).await? {
        return Ok(Vec::new());
    }
    let rows = query_rows(connection, &sqlite_list_relations_sql(resolved), &[]).await?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            let name = text(row, "name").filter(|name| !is_internal_name(name))?;
            let kind = text(row, "type").unwrap_or_default();
            Some(RelationSummary {
                schema: Some(resolved.to_string()),
                kind: sqlite_relation_kind(&kind, &name),
                name,
                comment: None,
                estimated_rows: None,
            })
        })
        .collect())
}

/// Fetch the catalog `type` and `sql` of a table or view.
#[cfg(feature = "schema")]
async fn relation_catalog_entry(
    connection: &turso::Connection,
    schema: Option<&str>,
    name: &str,
) -> Result<Option<(String, Option<String>)>, DatabaseError> {
    if let Some(schema) = schema
        && !schema_exists(connection, schema).await?
    {
        return Ok(None);
    }
    let query = schema.map_or_else(
        || {
            "SELECT type, sql FROM sqlite_master WHERE type IN ('table', 'view') AND name = ?"
                .to_string()
        },
        sqlite_relation_sql,
    );
    let rows = query_rows(
        connection,
        &query,
        &[DatabaseValue::String(name.to_string())],
    )
    .await?;
    Ok(rows
        .first()
        .and_then(|row| text(row, "type").map(|kind| (kind, text(row, "sql")))))
}

/// Get the SQL definition of a view.
///
/// # Errors
///
/// * If the catalog query fails
#[cfg(feature = "schema")]
pub(super) async fn view_definition(
    connection: &turso::Connection,
    schema: Option<&str>,
    name: &str,
) -> Result<Option<String>, DatabaseError> {
    Ok(relation_catalog_entry(connection, schema, name)
        .await?
        .and_then(|(kind, sql)| if kind == "view" { sql } else { None }))
}

/// Read column metadata of a table or view.
///
/// Uses `PRAGMA table_xinfo`; when that yields nothing for a view, columns are
/// derived from the prepared `SELECT *` statement instead.
///
/// # Errors
///
/// * If the catalog query fails
#[cfg(feature = "schema")]
pub(super) async fn columns(
    connection: &turso::Connection,
    schema: Option<&str>,
    name: &str,
    create_sql: Option<&str>,
) -> Result<Vec<ColumnInfo>, DatabaseError> {
    let prefix = schema.map_or_else(String::new, |schema| {
        format!("{}.", quote_sqlite_identifier(schema))
    });
    // `table_xinfo` (unlike `table_info`) also lists generated columns.
    let pragma = format!(
        "PRAGMA {prefix}table_xinfo({})",
        quote_sqlite_identifier(name)
    );
    let rows = query_rows(connection, &pragma, &[]).await?;

    let mut columns = Vec::with_capacity(rows.len());
    for row in &rows {
        let Some(column_name) = text(row, "name") else {
            continue;
        };
        let hidden = int(row, "hidden").unwrap_or(0);
        if crate::sqlite_common::sqlite_hidden_is_virtual_table_column(hidden) {
            continue;
        }
        let generated = crate::sqlite_common::sqlite_hidden_generated_kind(hidden);
        let fallback_ordinal = u32::try_from(columns.len()).unwrap_or(0);
        let ordinal = int(row, "cid")
            .and_then(|cid| u32::try_from(cid).ok())
            .unwrap_or(fallback_ordinal);
        let native_type = text(row, "type").unwrap_or_default();
        let not_null = int(row, "notnull").is_some_and(|value| value != 0);
        let is_pk = int(row, "pk").is_some_and(|value| value != 0);
        let default_value = if generated.is_some() {
            None
        } else {
            super::parse_default_value(text(row, "dflt_value").as_deref())
        };
        let auto_increment = is_pk && super::check_autoincrement_in_sql(create_sql, &column_name);

        columns.push(ColumnInfo {
            name: column_name,
            data_type: sqlite_type_to_data_type(&native_type),
            nullable: !not_null,
            is_primary_key: is_pk,
            auto_increment,
            default_value,
            ordinal_position: ordinal + 1,
            native_type,
            enum_values: None,
            generated,
            default_text: text(row, "dflt_value").filter(|_| generated.is_none()),
            generation_expression: None,
            on_update: None,
            unsigned: false,
            character_set: None,
            collation: None,
        });
    }

    if columns.is_empty() {
        // Fallback for relations PRAGMA table_info can't describe (e.g. views
        // on engines without view support in the pragma).
        let query = format!(
            "SELECT * FROM {prefix}{} LIMIT 0",
            quote_sqlite_identifier(name)
        );
        if let Ok(result) = query_result(connection, &query, &[]).await {
            for (index, column) in result.columns.into_iter().enumerate() {
                let native_type = column.type_name.unwrap_or_default();
                columns.push(ColumnInfo {
                    name: column.name,
                    data_type: sqlite_type_to_data_type(&native_type),
                    nullable: true,
                    is_primary_key: false,
                    auto_increment: false,
                    default_value: None,
                    ordinal_position: u32::try_from(index + 1).unwrap_or(u32::MAX),
                    native_type,
                    enum_values: None,
                    generated: None,
                    default_text: None,
                    generation_expression: None,
                    on_update: None,
                    unsigned: false,
                    character_set: None,
                    collation: None,
                });
            }
        }
    }

    Ok(columns)
}

/// Read index metadata of a table.
///
/// # Errors
///
/// * If the catalog query fails
#[cfg(feature = "schema")]
pub(super) async fn indexes(
    connection: &turso::Connection,
    schema: Option<&str>,
    table: &str,
) -> Result<BTreeMap<String, IndexInfo>, DatabaseError> {
    let catalog = schema.map_or_else(|| "sqlite_master".to_string(), sqlite_catalog_table);
    let query = format!("SELECT name, sql FROM {catalog} WHERE type='index' AND tbl_name=?");
    let rows = query_rows(
        connection,
        &query,
        &[DatabaseValue::String(table.to_string())],
    )
    .await?;

    let mut indexes = BTreeMap::new();
    for row in &rows {
        let Some(index_name) = text(row, "name") else {
            continue;
        };
        let is_primary = index_name.starts_with("sqlite_autoindex_");
        let (unique, index_columns) = text(row, "sql").map_or_else(
            || (false, Vec::new()),
            |sql| {
                let index_columns = sql
                    .find('(')
                    .and_then(|start| {
                        sql.rfind(')').map(|end| {
                            sql[start + 1..end]
                                .split(',')
                                .map(|s| s.trim().trim_matches('`').trim_matches('"').to_string())
                                .collect()
                        })
                    })
                    .unwrap_or_default();
                (sql.to_uppercase().contains("UNIQUE"), index_columns)
            },
        );
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

    Ok(indexes)
}

/// Parse foreign key constraints from a `CREATE TABLE` statement.
///
/// Every column pair of a composite key is reported, in declaration order,
/// and the key is named after its first pair. See the known limitations
/// documented on the `FK_PATTERN` parser: column-level `REFERENCES` clauses
/// aren't parsed, and `MATCH` / `DEFERRABLE` clauses are ignored.
#[cfg(feature = "schema")]
#[must_use]
pub(super) fn foreign_keys(
    table: &str,
    create_sql: Option<&str>,
) -> BTreeMap<String, ForeignKeyInfo> {
    let mut foreign_keys = BTreeMap::new();
    let Some(sql) = create_sql else {
        return foreign_keys;
    };

    let action = |pattern: &regex::Regex, actions: &str| {
        pattern.captures(actions).and_then(|c| {
            let action = c[1].to_uppercase();
            if action == "NO ACTION" {
                None
            } else {
                Some(action)
            }
        })
    };

    for cap in super::FK_PATTERN.captures_iter(sql) {
        let columns = identifier_list(&cap[1]);
        let referenced_table = super::strip_identifier_quotes(&cap[2]);
        let referenced_columns = identifier_list(&cap[3]);
        let fk_actions = &cap[4];
        let (Some(column), Some(referenced_column)) = (columns.first(), referenced_columns.first())
        else {
            continue;
        };
        let fk_name = format!("{table}_{column}_{referenced_table}_{referenced_column}");

        let mut foreign_key = ForeignKeyInfo {
            on_update: action(&super::ON_UPDATE_PATTERN, fk_actions),
            on_delete: action(&super::ON_DELETE_PATTERN, fk_actions),
            ..ForeignKeyInfo::new(fk_name.clone(), referenced_table)
        };
        for (column, referenced_column) in columns.into_iter().zip(referenced_columns) {
            foreign_key.push_pair(column, referenced_column);
        }
        foreign_keys.insert(fk_name, foreign_key);
    }

    foreign_keys
}

/// Split a parenthesized identifier list (`a, "b c", [d]`) into unquoted
/// names. Commas inside quoted identifiers don't split.
#[cfg(feature = "schema")]
fn identifier_list(list: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for c in list.chars() {
        match quote {
            Some(open) => {
                let close = if open == '[' { ']' } else { open };
                if c == close {
                    quote = None;
                }
                current.push(c);
            }
            None if c == ',' => {
                names.push(super::strip_identifier_quotes(&current));
                current.clear();
            }
            None => {
                if matches!(c, '"' | '`' | '[' | '\'') {
                    quote = Some(c);
                }
                current.push(c);
            }
        }
    }
    names.push(super::strip_identifier_quotes(&current));
    names.retain(|name| !name.is_empty());
    names
}

/// Get full relation metadata.
///
/// `schema` of `None` uses unqualified catalog lookups (the historical
/// behavior of the table-oriented methods); the reported schema is `main`.
/// With `tables_only`, views are reported as absent.
///
/// # Errors
///
/// * If a catalog query fails
#[cfg(feature = "schema")]
pub(super) async fn relation_info(
    connection: &turso::Connection,
    schema: Option<&str>,
    name: &str,
    tables_only: bool,
) -> Result<Option<TableInfo>, DatabaseError> {
    let Some((kind, create_sql)) = relation_catalog_entry(connection, schema, name).await? else {
        return Ok(None);
    };
    let kind = sqlite_relation_kind(&kind, name);
    if tables_only && kind != RelationKind::Table {
        return Ok(None);
    }

    let columns = columns(connection, schema, name, create_sql.as_deref())
        .await?
        .into_iter()
        .map(|column| (column.name.clone(), column))
        .collect();

    let (indexes, foreign_keys) = if kind.is_view() {
        (BTreeMap::new(), BTreeMap::new())
    } else {
        (
            indexes(connection, schema, name).await?,
            foreign_keys(name, create_sql.as_deref()),
        )
    };

    Ok(Some(TableInfo {
        name: name.to_string(),
        schema: Some(schema.unwrap_or(SQLITE_DEFAULT_SCHEMA).to_string()),
        kind,
        comment: None,
        columns,
        indexes,
        foreign_keys,
    }))
}

/// Column metadata of a table in the default schema (empty if absent).
///
/// # Errors
///
/// * If a catalog query fails
#[cfg(feature = "schema")]
pub(super) async fn table_columns(
    connection: &turso::Connection,
    table: &str,
) -> Result<Vec<ColumnInfo>, DatabaseError> {
    let create_sql = query_rows(
        connection,
        "SELECT sql FROM sqlite_master WHERE type='table' AND name=?",
        &[DatabaseValue::String(table.to_string())],
    )
    .await?
    .first()
    .and_then(|row| text(row, "sql"));
    columns(connection, None, table, create_sql.as_deref()).await
}

/// Table names in the default schema, excluding internal `sqlite_%` tables.
///
/// # Errors
///
/// * If the catalog query fails
#[cfg(feature = "schema")]
pub(super) async fn list_tables(
    connection: &turso::Connection,
) -> Result<Vec<String>, DatabaseError> {
    let rows = query_rows(
        connection,
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        &[],
    )
    .await?;
    Ok(rows
        .iter()
        .filter_map(|row| text(row, "name"))
        .filter(|name| !is_internal_name(name))
        .collect())
}

/// Whether a table exists in the default schema.
///
/// # Errors
///
/// * If the catalog query fails
#[cfg(feature = "schema")]
pub(super) async fn table_exists(
    connection: &turso::Connection,
    table: &str,
) -> Result<bool, DatabaseError> {
    Ok(!query_rows(
        connection,
        "SELECT name FROM sqlite_master WHERE type='table' AND name=?",
        &[DatabaseValue::String(table.to_string())],
    )
    .await?
    .is_empty())
}
