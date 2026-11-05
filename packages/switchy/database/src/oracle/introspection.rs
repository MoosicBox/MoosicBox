//! Schema introspection for the Oracle backend, from the `ALL_*` catalog
//! views (what the user can see).
//!
//! Schemas are users (`ALL_USERS`; Oracle-maintained ones are system
//! schemas). A relation without a schema is looked up in the session's
//! current schema (`SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA')`). Tables,
//! views and materialized views are reported (a materialized view's
//! container table is not listed separately; nested, overflow and dropped
//! tables are left out); comments come from `ALL_TAB_COMMENTS` /
//! `ALL_MVIEW_COMMENTS`, row estimates from the optimizer statistics.
//! Oracle keeps names in the case they were created with (upper case unless
//! quoted), and they are compared exactly. Every query here is a plain
//! `SELECT` that passes the read-only check.

use std::collections::BTreeMap;

use crate::schema::{
    ColumnInfo, DataType, DatabaseInfo, ForeignKeyInfo, GeneratedKind, IndexInfo, RelationKind,
    RelationRef, RelationSummary, SchemaInfo, TableInfo,
};
use crate::{DatabaseError, DatabaseValue};

use super::Executor;

/// The session's current schema.
const CURRENT_SCHEMA: &str = "SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA')";

/// Run a catalog query.
async fn rows(
    executor: &dyn Executor,
    sql: &str,
    params: &[DatabaseValue],
) -> Result<Vec<Vec<DatabaseValue>>, DatabaseError> {
    Ok(executor.run_sql(sql, params).await?.rows)
}

/// A text column.
fn text(row: &[DatabaseValue], index: usize) -> Option<String> {
    match row.get(index)? {
        DatabaseValue::String(value) | DatabaseValue::StringOpt(Some(value)) => Some(value.clone()),
        _ => None,
    }
}

/// An integer column.
fn int(row: &[DatabaseValue], index: usize) -> Option<i64> {
    row.get(index).and_then(DatabaseValue::as_i64)
}

/// A 0/1 column.
fn flag(row: &[DatabaseValue], index: usize) -> bool {
    match row.get(index) {
        Some(DatabaseValue::Bool(value)) => *value,
        Some(value) => value.as_i64().is_some_and(|value| value != 0),
        None => false,
    }
}

/// `schema` as a parameter: `NULL` means the current schema.
fn schema_param(schema: Option<&str>) -> DatabaseValue {
    schema.map_or(DatabaseValue::Null, |schema| {
        DatabaseValue::String(schema.to_string())
    })
}

/// Whether `table` is a table in the current schema.
pub(super) async fn table_exists(
    executor: &dyn Executor,
    table: &str,
) -> Result<bool, DatabaseError> {
    let found = rows(
        executor,
        &format!(
            "SELECT COUNT(*) FROM all_tables WHERE owner = {CURRENT_SCHEMA} AND table_name = :1"
        ),
        &[DatabaseValue::String(table.to_string())],
    )
    .await?;
    Ok(found.first().is_some_and(|row| flag(row, 0)))
}

/// Tables in the current schema.
pub(super) async fn list_tables(executor: &dyn Executor) -> Result<Vec<String>, DatabaseError> {
    Ok(rows(
        executor,
        &format!(
            "SELECT table_name FROM all_tables WHERE owner = {CURRENT_SCHEMA} \
             AND nested = 'NO' AND secondary = 'N' AND dropped = 'NO' ORDER BY table_name"
        ),
        &[],
    )
    .await?
    .iter()
    .filter_map(|row| text(row, 0))
    .collect())
}

/// Schemas (users) the session can see; Oracle-maintained users are
/// system schemas, and the current schema is the default.
pub(super) async fn list_schemas(
    executor: &dyn Executor,
) -> Result<Vec<SchemaInfo>, DatabaseError> {
    Ok(rows(
        executor,
        &format!(
            "SELECT username, CASE WHEN username = {CURRENT_SCHEMA} THEN 1 ELSE 0 END, \
             CASE WHEN oracle_maintained = 'Y' THEN 1 ELSE 0 END \
             FROM all_users ORDER BY username"
        ),
        &[],
    )
    .await?
    .iter()
    .filter_map(|row| {
        Some(SchemaInfo {
            name: text(row, 0)?,
            is_default: flag(row, 1),
            is_system: flag(row, 2),
        })
    })
    .collect())
}

/// The database connected to (a pluggable database's container name): one
/// connection reaches one database.
pub(super) async fn list_databases(
    executor: &dyn Executor,
) -> Result<Vec<DatabaseInfo>, DatabaseError> {
    Ok(rows(
        executor,
        "SELECT SYS_CONTEXT('USERENV', 'CON_NAME') FROM dual",
        &[],
    )
    .await?
    .iter()
    .filter_map(|row| {
        Some(DatabaseInfo {
            name: text(row, 0)?,
            is_current: true,
            is_system: false,
        })
    })
    .collect())
}

/// The relations of a schema (`:1`, or the current one when `NULL`), and
/// of one name (`:2`, or every name when `NULL`): owner, name, kind code
/// (`T`, `V`, `M`), comment, estimated rows.
fn relations_sql() -> String {
    format!(
        "SELECT r.owner, r.name, r.kind, \
         CASE WHEN r.kind = 'M' THEN (SELECT mc.comments FROM all_mview_comments mc \
         WHERE mc.owner = r.owner AND mc.mview_name = r.name) \
         ELSE (SELECT tc.comments FROM all_tab_comments tc \
         WHERE tc.owner = r.owner AND tc.table_name = r.name) END, \
         r.num_rows FROM ( \
         SELECT t.owner, t.table_name AS name, 'T' AS kind, t.num_rows FROM all_tables t \
         WHERE t.nested = 'NO' AND t.secondary = 'N' AND t.dropped = 'NO' \
         AND (t.iot_type IS NULL OR t.iot_type = 'IOT') \
         AND NOT EXISTS (SELECT 1 FROM all_mviews m \
         WHERE m.owner = t.owner AND m.mview_name = t.table_name) \
         UNION ALL SELECT v.owner, v.view_name, 'V', NULL FROM all_views v \
         UNION ALL SELECT m.owner, m.mview_name, 'M', \
         (SELECT t.num_rows FROM all_tables t WHERE t.owner = m.owner \
         AND t.table_name = m.mview_name) FROM all_mviews m \
         ) r WHERE r.owner = COALESCE(:1, {CURRENT_SCHEMA}) \
         AND (:2 IS NULL OR r.name = :2) ORDER BY r.name"
    )
}

/// The kind of a relation kind code.
fn relation_kind(code: &str) -> RelationKind {
    match code {
        "T" => RelationKind::Table,
        "V" => RelationKind::View,
        "M" => RelationKind::MaterializedView,
        other => RelationKind::Other(other.to_string()),
    }
}

/// A row of [`relations_sql`].
fn relation_summary(row: &[DatabaseValue]) -> Option<RelationSummary> {
    Some(RelationSummary {
        schema: text(row, 0),
        name: text(row, 1)?,
        kind: relation_kind(&text(row, 2).unwrap_or_default()),
        comment: text(row, 3),
        estimated_rows: int(row, 4).and_then(|rows| u64::try_from(rows).ok()),
    })
}

/// Tables, views and materialized views in `schema` (`None`: the current
/// schema).
pub(super) async fn list_relations(
    executor: &dyn Executor,
    schema: Option<&str>,
) -> Result<Vec<RelationSummary>, DatabaseError> {
    Ok(rows(
        executor,
        &relations_sql(),
        &[schema_param(schema), DatabaseValue::Null],
    )
    .await?
    .iter()
    .filter_map(|row| relation_summary(row))
    .collect())
}

/// The relation `name` in `schema`.
async fn relation_meta(
    executor: &dyn Executor,
    schema: Option<&str>,
    name: &str,
) -> Result<Option<RelationSummary>, DatabaseError> {
    Ok(rows(
        executor,
        &relations_sql(),
        &[
            schema_param(schema),
            DatabaseValue::String(name.to_string()),
        ],
    )
    .await?
    .iter()
    .find_map(|row| relation_summary(row)))
}

/// The full native type of a column (`NUMBER(10,2)`, `VARCHAR2(40 CHAR)`,
/// `TIMESTAMP(6) WITH TIME ZONE`, `RAW(16)`).
fn native_type(
    data_type: &str,
    length: Option<i64>,
    char_length: Option<i64>,
    char_used: Option<&str>,
    precision: Option<i64>,
    scale: Option<i64>,
) -> String {
    match data_type {
        "NUMBER" => match (precision, scale) {
            (None, Some(0)) => "INTEGER".to_string(),
            (None, _) => "NUMBER".to_string(),
            (Some(precision), Some(0) | None) => format!("NUMBER({precision})"),
            (Some(precision), Some(scale)) => format!("NUMBER({precision},{scale})"),
        },
        "FLOAT" => precision.map_or_else(|| "FLOAT".to_string(), |p| format!("FLOAT({p})")),
        "VARCHAR2" | "CHAR" => {
            if char_used == Some("C") {
                format!("{data_type}({} CHAR)", char_length.unwrap_or_default())
            } else {
                format!("{data_type}({})", length.unwrap_or_default())
            }
        }
        "NVARCHAR2" | "NCHAR" => format!("{data_type}({})", char_length.unwrap_or_default()),
        "RAW" => format!("RAW({})", length.unwrap_or_default()),
        other => other.to_string(),
    }
}

/// The [`DataType`] of a column.
fn data_type(
    data_type: &str,
    char_length: Option<i64>,
    length: Option<i64>,
    precision: Option<i64>,
    scale: Option<i64>,
) -> DataType {
    let chars = || u16::try_from(char_length.unwrap_or_default()).unwrap_or(u16::MAX);
    match data_type {
        "NUMBER" => match (precision, scale) {
            (Some(p @ 1..=4), Some(0)) if p > 0 => DataType::SmallInt,
            (Some(5..=9), Some(0)) => DataType::Int,
            (Some(10..=18), Some(0)) => DataType::BigInt,
            (Some(precision), scale) => DataType::Decimal(
                u8::try_from(precision).unwrap_or(38),
                u8::try_from(scale.unwrap_or(0).max(0)).unwrap_or(0),
            ),
            (None, _) => DataType::Custom(native_type("NUMBER", None, None, None, None, scale)),
        },
        "FLOAT" | "BINARY_DOUBLE" => DataType::Double,
        "BINARY_FLOAT" => DataType::Real,
        "VARCHAR2" | "NVARCHAR2" => DataType::VarChar(chars()),
        "CHAR" | "NCHAR" => DataType::Char(chars()),
        "CLOB" | "NCLOB" | "LONG" => DataType::Text,
        "DATE" => DataType::DateTime,
        "RAW" => DataType::Binary(length.and_then(|length| u32::try_from(length).ok())),
        "BLOB" | "LONG RAW" => DataType::Blob,
        "BOOLEAN" => DataType::Bool,
        "JSON" => DataType::Json,
        "XMLTYPE" => DataType::Xml,
        other if other.starts_with("TIMESTAMP") && other.contains("TIME ZONE") => {
            DataType::Timestamp
        }
        other if other.starts_with("TIMESTAMP") => DataType::DateTime,
        other => DataType::Custom(other.to_string()),
    }
}

/// A column default (`DATA_DEFAULT`, the expression's text) as a value:
/// numbers and string literals as values, `NULL` as none, anything else as
/// the expression's text.
fn parse_default(definition: &str) -> Option<DatabaseValue> {
    let expression = definition.trim();
    if expression.is_empty() || expression.eq_ignore_ascii_case("NULL") {
        return None;
    }
    let string = expression
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
        .filter(|body| !body.replace("''", "").contains('\''));
    if let Some(body) = string {
        return Some(DatabaseValue::String(body.replace("''", "'")));
    }
    expression.parse::<i64>().map_or_else(
        |_| {
            expression.parse::<f64>().map_or_else(
                |_| Some(DatabaseValue::String(expression.to_string())),
                |value| Some(DatabaseValue::Real64(value)),
            )
        },
        |value| Some(DatabaseValue::Int64(value)),
    )
}

/// Columns of the relation `name` in `owner`.
async fn relation_columns(
    executor: &dyn Executor,
    owner: &str,
    name: &str,
) -> Result<Vec<ColumnInfo>, DatabaseError> {
    let params = [
        DatabaseValue::String(owner.to_string()),
        DatabaseValue::String(name.to_string()),
    ];
    let primary: Vec<String> = rows(
        executor,
        "SELECT cc.column_name FROM all_constraints c JOIN all_cons_columns cc \
         ON cc.owner = c.owner AND cc.constraint_name = c.constraint_name \
         WHERE c.constraint_type = 'P' AND c.owner = :1 AND c.table_name = :2",
        &params,
    )
    .await?
    .iter()
    .filter_map(|row| text(row, 0))
    .collect();
    Ok(rows(
        executor,
        "SELECT column_name, data_type, data_length, char_length, char_used, data_precision, \
         data_scale, CASE WHEN nullable = 'Y' THEN 1 ELSE 0 END, \
         CASE WHEN identity_column = 'YES' THEN 1 ELSE 0 END, \
         CASE WHEN virtual_column = 'YES' THEN 1 ELSE 0 END, data_default, column_id \
         FROM all_tab_cols WHERE owner = :1 AND table_name = :2 AND hidden_column = 'NO' \
         ORDER BY column_id",
        &params,
    )
    .await?
    .iter()
    .filter_map(|row| {
        let name = text(row, 0)?;
        let type_name = text(row, 1).unwrap_or_default();
        let (length, char_length, precision, scale) =
            (int(row, 2), int(row, 3), int(row, 5), int(row, 6));
        let char_used = text(row, 4);
        let identity = flag(row, 8);
        let generated = flag(row, 9).then_some(GeneratedKind::Virtual);
        Some(ColumnInfo {
            is_primary_key: primary.contains(&name),
            data_type: data_type(&type_name, char_length, length, precision, scale),
            native_type: native_type(
                &type_name,
                length,
                char_length,
                char_used.as_deref(),
                precision,
                scale,
            ),
            name,
            nullable: flag(row, 7),
            auto_increment: identity,
            // An identity column's default is its sequence; a virtual
            // column's is its expression.
            default_value: text(row, 10)
                .filter(|_| generated.is_none() && !identity)
                .and_then(|definition| parse_default(&definition)),
            ordinal_position: int(row, 11)
                .and_then(|position| u32::try_from(position).ok())
                .unwrap_or(0),
            enum_values: None,
            generated,
        })
    })
    .collect())
}

/// Columns of a table or view.
pub(super) async fn columns(
    executor: &dyn Executor,
    schema: Option<&str>,
    name: &str,
) -> Result<Vec<ColumnInfo>, DatabaseError> {
    match relation_meta(executor, schema, name).await? {
        Some(relation) => {
            relation_columns(
                executor,
                relation.schema.as_deref().unwrap_or_default(),
                &relation.name,
            )
            .await
        }
        None => Ok(Vec::new()),
    }
}

/// Columns, indexes and foreign keys of a table or view.
pub(super) async fn relation_info(
    executor: &dyn Executor,
    relation: &RelationRef,
) -> Result<Option<TableInfo>, DatabaseError> {
    let Some(meta) = relation_meta(executor, relation.schema.as_deref(), &relation.name).await?
    else {
        return Ok(None);
    };
    let owner = meta.schema.clone().unwrap_or_default();
    let params = [
        DatabaseValue::String(owner.clone()),
        DatabaseValue::String(meta.name.clone()),
    ];
    let columns = relation_columns(executor, &owner, &meta.name)
        .await?
        .into_iter()
        .map(|column| (column.name.clone(), column))
        .collect();

    let mut indexes: BTreeMap<String, IndexInfo> = BTreeMap::new();
    for row in rows(
        executor,
        "SELECT i.index_name, CASE WHEN i.uniqueness = 'UNIQUE' THEN 1 ELSE 0 END, \
         CASE WHEN EXISTS (SELECT 1 FROM all_constraints c WHERE c.owner = i.table_owner \
         AND c.table_name = i.table_name AND c.constraint_type = 'P' \
         AND c.index_name = i.index_name) THEN 1 ELSE 0 END, ic.column_name \
         FROM all_indexes i JOIN all_ind_columns ic \
         ON ic.index_owner = i.owner AND ic.index_name = i.index_name \
         WHERE i.table_owner = :1 AND i.table_name = :2 AND i.index_type <> 'LOB' \
         ORDER BY i.index_name, ic.column_position",
        &params,
    )
    .await?
    {
        let Some(name) = text(&row, 0) else {
            continue;
        };
        let index = indexes.entry(name.clone()).or_insert_with(|| IndexInfo {
            name,
            unique: flag(&row, 1),
            columns: Vec::new(),
            is_primary: flag(&row, 2),
        });
        if let Some(column) = text(&row, 3) {
            index.columns.push(column);
        }
    }

    let mut foreign_keys = BTreeMap::new();
    for row in rows(
        executor,
        "SELECT c.constraint_name, cc.column_name, r.table_name, rc.column_name, c.delete_rule \
         FROM all_constraints c \
         JOIN all_cons_columns cc ON cc.owner = c.owner AND cc.constraint_name = c.constraint_name \
         JOIN all_constraints r ON r.owner = c.r_owner AND r.constraint_name = c.r_constraint_name \
         JOIN all_cons_columns rc ON rc.owner = r.owner AND rc.constraint_name = r.constraint_name \
         AND rc.position = cc.position \
         WHERE c.constraint_type = 'R' AND c.owner = :1 AND c.table_name = :2 \
         ORDER BY c.constraint_name, cc.position",
        &params,
    )
    .await?
    {
        let Some(name) = text(&row, 0) else {
            continue;
        };
        // Multi-column keys report their first column pair.
        foreign_keys
            .entry(name.clone())
            .or_insert_with(|| ForeignKeyInfo {
                name,
                column: text(&row, 1).unwrap_or_default(),
                referenced_table: text(&row, 2).unwrap_or_default(),
                referenced_column: text(&row, 3).unwrap_or_default(),
                // Oracle has no ON UPDATE actions.
                on_update: Some("NO ACTION".to_string()),
                on_delete: text(&row, 4),
            });
    }

    Ok(Some(TableInfo {
        name: meta.name,
        schema: meta.schema,
        kind: meta.kind,
        comment: meta.comment,
        columns,
        indexes,
        foreign_keys,
    }))
}

/// The query of a view or materialized view.
pub(super) async fn view_definition(
    executor: &dyn Executor,
    relation: &RelationRef,
) -> Result<Option<String>, DatabaseError> {
    let Some(meta) = relation_meta(executor, relation.schema.as_deref(), &relation.name).await?
    else {
        return Ok(None);
    };
    let sql = match meta.kind {
        RelationKind::View => "SELECT text FROM all_views WHERE owner = :1 AND view_name = :2",
        RelationKind::MaterializedView => {
            "SELECT query FROM all_mviews WHERE owner = :1 AND mview_name = :2"
        }
        _ => return Ok(None),
    };
    let found = rows(
        executor,
        sql,
        &[
            DatabaseValue::String(meta.schema.unwrap_or_default()),
            DatabaseValue::String(meta.name),
        ],
    )
    .await?;
    Ok(found
        .first()
        .and_then(|row| text(row, 0))
        .map(|text| text.trim().to_string()))
}

/// Standalone procedures and functions in `schema`, or in every schema
/// with `None` (package members and Oracle's built-in functions are not
/// listed).
#[cfg(feature = "raw-sql")]
pub(super) async fn list_routines(
    executor: &dyn Executor,
    schema: Option<&str>,
) -> Result<Vec<crate::schema::RoutineInfo>, DatabaseError> {
    let found = rows(
        executor,
        "SELECT o.owner, o.object_name, LOWER(o.object_type), \
         (SELECT LISTAGG(a.argument_name || ' ' || a.data_type, ', ') \
         WITHIN GROUP (ORDER BY a.position) FROM all_arguments a \
         WHERE a.object_id = o.object_id AND a.package_name IS NULL AND a.position > 0 \
         AND a.data_level = 0), \
         (SELECT MAX(a.data_type) FROM all_arguments a WHERE a.object_id = o.object_id \
         AND a.package_name IS NULL AND a.position = 0 AND a.data_level = 0), 'false' \
         FROM all_objects o WHERE o.object_type IN ('PROCEDURE', 'FUNCTION') \
         AND (:1 IS NULL OR o.owner = :1) ORDER BY o.object_name",
        &[schema_param(schema)],
    )
    .await?;
    Ok(crate::schema::routines_from_rows(&found))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn defaults_read_as_values() {
        assert_eq!(parse_default("0 "), Some(DatabaseValue::Int64(0)));
        assert_eq!(parse_default("1.5"), Some(DatabaseValue::Real64(1.5)));
        assert_eq!(
            parse_default("'it''s'"),
            Some(DatabaseValue::String("it's".to_string()))
        );
        assert_eq!(
            parse_default("SYSTIMESTAMP\n"),
            Some(DatabaseValue::String("SYSTIMESTAMP".to_string()))
        );
        assert_eq!(parse_default("NULL"), None);
    }

    #[test]
    fn native_types_carry_lengths() {
        assert_eq!(
            native_type("NUMBER", Some(22), None, None, Some(10), Some(2)),
            "NUMBER(10,2)"
        );
        assert_eq!(
            native_type("NUMBER", Some(22), None, None, None, Some(0)),
            "INTEGER"
        );
        assert_eq!(
            native_type("VARCHAR2", Some(160), Some(40), Some("C"), None, None),
            "VARCHAR2(40 CHAR)"
        );
        assert_eq!(
            native_type("VARCHAR2", Some(40), Some(40), Some("B"), None, None),
            "VARCHAR2(40)"
        );
        assert_eq!(
            native_type(
                "TIMESTAMP(6) WITH TIME ZONE",
                Some(13),
                None,
                None,
                None,
                Some(6)
            ),
            "TIMESTAMP(6) WITH TIME ZONE"
        );
        assert_eq!(
            data_type("NUMBER", None, Some(22), Some(10), Some(0)),
            DataType::BigInt
        );
        assert_eq!(
            data_type("NUMBER", None, Some(22), Some(10), Some(2)),
            DataType::Decimal(10, 2)
        );
        assert_eq!(
            data_type("TIMESTAMP(6) WITH TIME ZONE", None, Some(13), None, Some(6)),
            DataType::Timestamp
        );
        assert_eq!(
            data_type("DATE", None, Some(7), None, None),
            DataType::DateTime
        );
    }
}
