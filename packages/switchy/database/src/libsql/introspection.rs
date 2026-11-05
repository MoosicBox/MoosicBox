//! Schema introspection for the `libSQL` backend.
//!
//! `libSQL` servers run the same catalog as `SQLite` (`sqlite_master`,
//! `PRAGMA table_xinfo`, `index_list`, `index_info`, `foreign_key_list`), so
//! this reads it with the shared `SQLite` catalog SQL. Schemas are attached
//! databases as `PRAGMA database_list` reports them (`main`; `temp` is a
//! system schema).

use std::collections::BTreeMap;

use crate::schema::{
    ColumnInfo, ForeignKeyInfo, IndexInfo, RelationKind, RelationRef, RelationSummary, SchemaInfo,
    TableInfo,
};
use crate::sqlite_common::{
    SQLITE_DEFAULT_SCHEMA, SQLITE_LIST_SCHEMAS_SQL, sqlite_catalog_table_for,
    sqlite_hidden_generated_kind, sqlite_hidden_is_virtual_table_column, sqlite_list_relations_sql,
    sqlite_parse_default_value, sqlite_pragma_sql, sqlite_relation_kind, sqlite_schema_info,
    sqlite_schema_or_default, sqlite_sql_has_autoincrement, sqlite_type_to_data_type,
};
use crate::{DatabaseError, DatabaseValue, Row};

use super::Executor;

async fn rows(
    executor: &dyn Executor,
    sql: &str,
    params: &[DatabaseValue],
) -> Result<Vec<Row>, DatabaseError> {
    Ok(executor.run(sql, params, true).await?.into_rows())
}

fn text(row: &Row, column: &str) -> Option<String> {
    match row.get(column) {
        Some(DatabaseValue::String(value) | DatabaseValue::StringOpt(Some(value))) => Some(value),
        _ => None,
    }
}

fn int(row: &Row, column: &str) -> Option<i64> {
    row.get(column).and_then(|value| value.as_i64())
}

/// Attached databases as schemas.
pub(super) async fn list_schemas(
    executor: &dyn Executor,
) -> Result<Vec<SchemaInfo>, DatabaseError> {
    let mut schemas: Vec<SchemaInfo> = rows(executor, SQLITE_LIST_SCHEMAS_SQL, &[])
        .await?
        .iter()
        .filter_map(|row| text(row, "name"))
        .map(|name| sqlite_schema_info(&name))
        .collect();
    if !schemas.iter().any(|schema| schema.is_default) {
        schemas.insert(0, sqlite_schema_info(SQLITE_DEFAULT_SCHEMA));
    }
    Ok(schemas)
}

/// Tables and views in `schema` (`None`: `main`).
pub(super) async fn list_relations(
    executor: &dyn Executor,
    schema: Option<&str>,
) -> Result<Vec<RelationSummary>, DatabaseError> {
    let schema = sqlite_schema_or_default(schema);
    if !list_schemas(executor)
        .await?
        .iter()
        .any(|info| info.name.eq_ignore_ascii_case(schema))
    {
        return Ok(Vec::new());
    }
    Ok(rows(executor, &sqlite_list_relations_sql(schema), &[])
        .await?
        .iter()
        .filter_map(|row| {
            let name = text(row, "name")?;
            let kind = text(row, "type").unwrap_or_default();
            Some(RelationSummary {
                schema: Some(schema.to_string()),
                kind: sqlite_relation_kind(&kind, &name),
                name,
                comment: None,
                estimated_rows: None,
            })
        })
        .collect())
}

/// Planner statistics of the tables in `schema` (`None`: `main`).
pub(super) async fn list_relation_statistics(
    executor: &dyn Executor,
    schema: Option<&str>,
) -> Result<Vec<crate::schema::RelationStatistics>, DatabaseError> {
    use crate::sqlite_common::{
        sqlite_has_stat1_sql, sqlite_relation_statistics, sqlite_relation_statistics_sql,
    };

    let schema = sqlite_schema_or_default(schema);
    if !list_schemas(executor)
        .await?
        .iter()
        .any(|info| info.name.eq_ignore_ascii_case(schema))
    {
        return Ok(Vec::new());
    }
    let has_stat1 = rows(executor, &sqlite_has_stat1_sql(schema), &[])
        .await?
        .first()
        .and_then(|row| int(row, "stat1_count"))
        .is_some_and(|count| count > 0);
    let sql = sqlite_relation_statistics_sql(schema, has_stat1);
    Ok(rows(executor, &sql, &[])
        .await?
        .iter()
        .filter_map(|row| {
            let name = text(row, "name")?;
            Some(sqlite_relation_statistics(
                schema,
                name,
                int(row, "row_count"),
                has_stat1,
            ))
        })
        .collect())
}

/// The catalog `type` and `sql` of a table or view (`None` schema:
/// unqualified lookup).
async fn catalog_entry(
    executor: &dyn Executor,
    schema: Option<&str>,
    name: &str,
) -> Result<Option<(String, Option<String>)>, DatabaseError> {
    let sql = format!(
        "SELECT type, sql FROM {} WHERE type IN ('table', 'view') AND name = ?",
        sqlite_catalog_table_for(schema)
    );
    Ok(
        rows(executor, &sql, &[DatabaseValue::String(name.to_string())])
            .await?
            .first()
            .and_then(|row| text(row, "type").map(|kind| (kind, text(row, "sql")))),
    )
}

/// The SQL of a view.
pub(super) async fn view_definition(
    executor: &dyn Executor,
    relation: &RelationRef,
) -> Result<Option<String>, DatabaseError> {
    let schema = sqlite_schema_or_default(relation.schema.as_deref());
    Ok(catalog_entry(executor, Some(schema), &relation.name)
        .await?
        .filter(|(kind, _)| kind == "view")
        .and_then(|(_, sql)| sql))
}

/// Columns of a table or view.
pub(super) async fn columns(
    executor: &dyn Executor,
    schema: Option<&str>,
    name: &str,
) -> Result<Vec<ColumnInfo>, DatabaseError> {
    // `table_xinfo` (unlike `table_info`) also lists generated columns.
    let rows = rows(
        executor,
        &sqlite_pragma_sql(schema, "table_xinfo", name),
        &[],
    )
    .await?;
    let mut create_sql: Option<Option<String>> = None;
    let mut columns = Vec::with_capacity(rows.len());
    for row in &rows {
        let Some(column_name) = text(row, "name") else {
            continue;
        };
        let hidden = int(row, "hidden").unwrap_or(0);
        if sqlite_hidden_is_virtual_table_column(hidden) {
            continue;
        }
        let generated = sqlite_hidden_generated_kind(hidden);
        let is_pk = int(row, "pk").is_some_and(|pk| pk != 0);
        let auto_increment = if is_pk {
            if create_sql.is_none() {
                create_sql = Some(
                    catalog_entry(executor, schema, name)
                        .await?
                        .and_then(|(_, sql)| sql),
                );
            }
            create_sql
                .as_ref()
                .and_then(Option::as_deref)
                .is_some_and(|sql| sqlite_sql_has_autoincrement(sql, &column_name))
        } else {
            false
        };
        let native_type = text(row, "type").unwrap_or_default();
        let ordinal = int(row, "cid")
            .and_then(|cid| u32::try_from(cid).ok())
            .unwrap_or_else(|| u32::try_from(columns.len()).unwrap_or(u32::MAX));
        columns.push(ColumnInfo {
            data_type: sqlite_type_to_data_type(&native_type),
            native_type,
            name: column_name,
            nullable: int(row, "notnull").is_none_or(|not_null| not_null == 0),
            is_primary_key: is_pk,
            auto_increment,
            default_value: if generated.is_some() {
                None
            } else {
                sqlite_parse_default_value(text(row, "dflt_value").as_deref())
            },
            ordinal_position: ordinal + 1,
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
    Ok(columns)
}

/// The primary key column a foreign key without a column list refers to.
async fn referenced_primary_key(
    executor: &dyn Executor,
    schema: Option<&str>,
    table: &str,
    sequence: i64,
) -> Result<String, DatabaseError> {
    rows(
        executor,
        &sqlite_pragma_sql(schema, "table_info", table),
        &[],
    )
    .await?
    .iter()
    .find(|row| int(row, "pk") == Some(sequence + 1))
    .and_then(|row| text(row, "name"))
    .ok_or_else(|| {
        DatabaseError::InvalidSchema("implicit foreign key has no matching primary key".into())
    })
}

/// Columns, indexes and foreign keys of a relation.
async fn build_table_info(
    executor: &dyn Executor,
    schema: Option<&str>,
    name: &str,
    kind: RelationKind,
) -> Result<TableInfo, DatabaseError> {
    let columns = columns(executor, schema, name)
        .await?
        .into_iter()
        .map(|column| (column.name.clone(), column))
        .collect();
    let mut indexes = BTreeMap::new();
    let mut foreign_keys = BTreeMap::new();
    let mut names: BTreeMap<i64, String> = BTreeMap::new();
    if !kind.is_view() {
        for row in rows(
            executor,
            &sqlite_pragma_sql(schema, "index_list", name),
            &[],
        )
        .await?
        {
            let Some(index_name) = text(&row, "name") else {
                continue;
            };
            let index_columns = rows(
                executor,
                &sqlite_pragma_sql(schema, "index_info", &index_name),
                &[],
            )
            .await?
            .iter()
            .filter_map(|column| text(column, "name"))
            .collect();
            indexes.insert(
                index_name.clone(),
                IndexInfo {
                    name: index_name,
                    unique: int(&row, "unique").is_some_and(|unique| unique != 0),
                    columns: index_columns,
                    is_primary: text(&row, "origin").as_deref() == Some("pk"),
                },
            );
        }
        for row in rows(
            executor,
            &sqlite_pragma_sql(schema, "foreign_key_list", name),
            &[],
        )
        .await?
        {
            let (Some(referenced_table), Some(column)) = (text(&row, "table"), text(&row, "from"))
            else {
                continue;
            };
            let referenced_column = match text(&row, "to") {
                Some(column) => column,
                None => {
                    referenced_primary_key(
                        executor,
                        schema,
                        &referenced_table,
                        int(&row, "seq").unwrap_or(0),
                    )
                    .await?
                }
            };
            let action = |column: &str| text(&row, column).filter(|action| action != "NO ACTION");
            // Keys are unnamed: each is named after its first column pair.
            let fk_name = names
                .entry(int(&row, "id").unwrap_or(0))
                .or_insert_with(|| {
                    format!("{name}_{column}_{referenced_table}_{referenced_column}")
                })
                .clone();
            foreign_keys
                .entry(fk_name.clone())
                .or_insert_with(|| ForeignKeyInfo {
                    referenced_schema: Some(schema.unwrap_or(SQLITE_DEFAULT_SCHEMA).to_string()),
                    on_update: action("on_update"),
                    on_delete: action("on_delete"),
                    ..ForeignKeyInfo::new(fk_name, referenced_table)
                })
                .push_pair(column, referenced_column);
        }
    }
    Ok(TableInfo {
        name: name.to_string(),
        schema: Some(sqlite_schema_or_default(schema).to_string()),
        kind,
        comment: None,
        columns,
        indexes,
        foreign_keys,
    })
}

/// Full metadata of a table or view.
pub(super) async fn relation_info(
    executor: &dyn Executor,
    relation: &RelationRef,
) -> Result<Option<TableInfo>, DatabaseError> {
    let schema = sqlite_schema_or_default(relation.schema.as_deref());
    let Some((kind, _)) = catalog_entry(executor, Some(schema), &relation.name).await? else {
        return Ok(None);
    };
    let kind = sqlite_relation_kind(&kind, &relation.name);
    build_table_info(executor, Some(schema), &relation.name, kind)
        .await
        .map(Some)
}

/// Full metadata of a table in the default schema (`None` for views).
pub(super) async fn table_info(
    executor: &dyn Executor,
    table: &str,
) -> Result<Option<TableInfo>, DatabaseError> {
    match catalog_entry(executor, None, table).await? {
        Some((kind, _)) if kind == "table" => {
            build_table_info(executor, None, table, sqlite_relation_kind(&kind, table))
                .await
                .map(Some)
        }
        _ => Ok(None),
    }
}

/// Table names in the default schema, without internal `sqlite_%` tables.
pub(super) async fn list_tables(executor: &dyn Executor) -> Result<Vec<String>, DatabaseError> {
    Ok(rows(
        executor,
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        &[],
    )
    .await?
    .iter()
    .filter_map(|row| text(row, "name"))
    .collect())
}

/// Whether a table exists in the default schema.
pub(super) async fn table_exists(
    executor: &dyn Executor,
    table: &str,
) -> Result<bool, DatabaseError> {
    Ok(catalog_entry(executor, None, table)
        .await?
        .is_some_and(|(kind, _)| kind == "table"))
}
