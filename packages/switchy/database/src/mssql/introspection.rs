//! Schema introspection for the SQL Server backend, from the `sys` catalog
//! views of the current database.
//!
//! A relation without a schema is looked up in the login's default schema
//! (`SCHEMA_NAME()`, usually `dbo`). Tables and views are reported;
//! comments are the `MS_Description` extended property; row estimates come
//! from `sys.partitions`. Every query here is a plain `SELECT`, so it also
//! passes the read-only check.

use std::collections::BTreeMap;

use crate::schema::{
    ColumnInfo, DataType, DatabaseInfo, ForeignKeyInfo, GeneratedKind, IndexInfo, RelationKind,
    RelationRef, RelationSummary, SchemaInfo, TableInfo,
};
use crate::{DatabaseError, DatabaseValue};

use super::{Executor, Mode};

/// Run a catalog query.
async fn rows(
    executor: &dyn Executor,
    sql: &str,
    params: &[DatabaseValue],
) -> Result<Vec<Vec<DatabaseValue>>, DatabaseError> {
    Ok(executor.run(sql, params, Mode::Rows).await?.rows)
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

/// A 0/1 (or bit) column.
fn flag(row: &[DatabaseValue], index: usize) -> bool {
    match row.get(index) {
        Some(DatabaseValue::Bool(value)) => *value,
        Some(value) => value.as_i64().is_some_and(|value| value != 0),
        None => false,
    }
}

/// `schema` as a parameter: `NULL` means the default schema.
fn schema_param(schema: Option<&str>) -> DatabaseValue {
    schema.map_or(DatabaseValue::Null, |schema| {
        DatabaseValue::String(schema.to_string())
    })
}

/// Whether `table` is a table in the default schema (or the schema it names).
pub(super) async fn table_exists(
    executor: &dyn Executor,
    table: &str,
) -> Result<bool, DatabaseError> {
    let found = rows(
        executor,
        "SELECT CASE WHEN OBJECT_ID(@P1, N'U') IS NULL THEN 0 ELSE 1 END",
        &[DatabaseValue::String(table.to_string())],
    )
    .await?;
    Ok(found.first().is_some_and(|row| flag(row, 0)))
}

/// Tables in the default schema.
pub(super) async fn list_tables(executor: &dyn Executor) -> Result<Vec<String>, DatabaseError> {
    Ok(rows(
        executor,
        "SELECT name FROM sys.tables WHERE schema_id = SCHEMA_ID() AND is_ms_shipped = 0 \
         ORDER BY name",
        &[],
    )
    .await?
    .iter()
    .filter_map(|row| text(row, 0))
    .collect())
}

/// Schemas of the current database. `sys`, `INFORMATION_SCHEMA`, `guest`
/// and the fixed database roles' schemas (`db_owner`, ...) are system
/// schemas; the default is `SCHEMA_NAME()`.
pub(super) async fn list_schemas(
    executor: &dyn Executor,
) -> Result<Vec<SchemaInfo>, DatabaseError> {
    Ok(rows(
        executor,
        "SELECT s.name, CASE WHEN s.name = SCHEMA_NAME() THEN 1 ELSE 0 END, \
         CASE WHEN s.name IN (N'sys', N'INFORMATION_SCHEMA', N'guest') \
         OR s.schema_id BETWEEN 16384 AND 16399 THEN 1 ELSE 0 END \
         FROM sys.schemas s ORDER BY s.name",
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

/// Databases the login can use; `master`, `tempdb`, `model` and `msdb` are
/// system databases.
pub(super) async fn list_databases(
    executor: &dyn Executor,
) -> Result<Vec<DatabaseInfo>, DatabaseError> {
    Ok(rows(
        executor,
        "SELECT name, CASE WHEN name = DB_NAME() THEN 1 ELSE 0 END, \
         CASE WHEN database_id <= 4 THEN 1 ELSE 0 END \
         FROM sys.databases WHERE state = 0 AND HAS_DBACCESS(name) = 1 ORDER BY name",
        &[],
    )
    .await?
    .iter()
    .filter_map(|row| {
        Some(DatabaseInfo {
            name: text(row, 0)?,
            is_current: flag(row, 1),
            is_system: flag(row, 2),
        })
    })
    .collect())
}

/// The kind of a `sys.objects` type code.
fn relation_kind(code: &str) -> RelationKind {
    match code.trim() {
        "U" => RelationKind::Table,
        "V" => RelationKind::View,
        "S" | "IT" => RelationKind::SystemTable,
        other => RelationKind::Other(other.to_string()),
    }
}

/// Tables and views in `schema` (`None`: the default schema).
pub(super) async fn list_relations(
    executor: &dyn Executor,
    schema: Option<&str>,
) -> Result<Vec<RelationSummary>, DatabaseError> {
    Ok(rows(
        executor,
        "SELECT s.name, o.name, o.type, CAST(ep.value AS nvarchar(max)), \
         (SELECT SUM(p.rows) FROM sys.partitions p \
         WHERE p.object_id = o.object_id AND p.index_id IN (0, 1)) \
         FROM sys.objects o JOIN sys.schemas s ON s.schema_id = o.schema_id \
         LEFT JOIN sys.extended_properties ep ON ep.class = 1 AND ep.major_id = o.object_id \
         AND ep.minor_id = 0 AND ep.name = N'MS_Description' \
         WHERE o.type IN ('U', 'V') AND o.is_ms_shipped = 0 \
         AND s.name = COALESCE(@P1, SCHEMA_NAME()) ORDER BY o.name",
        &[schema_param(schema)],
    )
    .await?
    .iter()
    .filter_map(|row| {
        Some(RelationSummary {
            schema: text(row, 0),
            name: text(row, 1)?,
            kind: relation_kind(&text(row, 2).unwrap_or_default()),
            comment: text(row, 3),
            estimated_rows: int(row, 4).and_then(|rows| u64::try_from(rows).ok()),
        })
    })
    .collect())
}

/// The object id, schema, kind and comment of a table or view.
async fn relation_meta(
    executor: &dyn Executor,
    schema: Option<&str>,
    name: &str,
) -> Result<Option<(i64, Option<String>, RelationKind, Option<String>)>, DatabaseError> {
    let found = rows(
        executor,
        "SELECT CAST(o.object_id AS bigint), s.name, o.type, CAST(ep.value AS nvarchar(max)) \
         FROM sys.objects o JOIN sys.schemas s ON s.schema_id = o.schema_id \
         LEFT JOIN sys.extended_properties ep ON ep.class = 1 AND ep.major_id = o.object_id \
         AND ep.minor_id = 0 AND ep.name = N'MS_Description' \
         WHERE o.type IN ('U', 'V') AND o.name = @P1 AND s.name = COALESCE(@P2, SCHEMA_NAME())",
        &[
            DatabaseValue::String(name.to_string()),
            schema_param(schema),
        ],
    )
    .await?;
    Ok(found.first().and_then(|row| {
        Some((
            int(row, 0)?,
            text(row, 1),
            relation_kind(&text(row, 2).unwrap_or_default()),
            text(row, 3),
        ))
    }))
}

/// The full native type name of a column (`nvarchar(50)`, `decimal(10, 2)`,
/// `varbinary(max)`, `datetime2(3)`); alias types keep their own name.
fn native_type(type_name: &str, base: &str, max_length: i64, precision: i64, scale: i64) -> String {
    if !type_name.eq_ignore_ascii_case(base) {
        return type_name.to_string();
    }
    let length = |bytes_per_char: i64| {
        if max_length < 0 {
            "max".to_string()
        } else {
            (max_length / bytes_per_char).to_string()
        }
    };
    match base {
        "varchar" | "char" | "varbinary" | "binary" => format!("{base}({})", length(1)),
        "nvarchar" | "nchar" => format!("{base}({})", length(2)),
        "decimal" | "numeric" => format!("{base}({precision}, {scale})"),
        "datetime2" | "time" | "datetimeoffset" => format!("{base}({scale})"),
        _ => base.to_string(),
    }
}

/// The [`DataType`] of a column of SQL Server type `base`.
fn data_type(base: &str, max_length: i64, precision: i64, scale: i64) -> DataType {
    let chars =
        |bytes_per_char: i64| u16::try_from(max_length / bytes_per_char).unwrap_or(u16::MAX);
    match base {
        "bit" => DataType::Bool,
        "tinyint" | "smallint" => DataType::SmallInt,
        "int" => DataType::Int,
        "bigint" => DataType::BigInt,
        "real" => DataType::Real,
        "float" => DataType::Double,
        "decimal" | "numeric" => DataType::Decimal(
            u8::try_from(precision).unwrap_or(38),
            u8::try_from(scale).unwrap_or(0),
        ),
        "money" | "smallmoney" => DataType::Money,
        "varchar" | "nvarchar" if max_length < 0 => DataType::Text,
        "varchar" => DataType::VarChar(chars(1)),
        "nvarchar" => DataType::VarChar(chars(2)),
        "char" => DataType::Char(chars(1)),
        "nchar" => DataType::Char(chars(2)),
        "text" | "ntext" => DataType::Text,
        "date" => DataType::Date,
        "time" => DataType::Time,
        "datetime" | "datetime2" | "smalldatetime" => DataType::DateTime,
        "binary" | "varbinary" if max_length >= 0 => {
            DataType::Binary(u32::try_from(max_length).ok())
        }
        "binary" | "varbinary" | "image" => DataType::Blob,
        "uniqueidentifier" => DataType::Uuid,
        "xml" => DataType::Xml,
        other => DataType::Custom(other.to_string()),
    }
}

/// A default constraint's definition (`((0))`, `(N'a')`, `(getdate())`) as
/// a value: numbers and string literals as values, anything else as the
/// expression's text.
fn parse_default(definition: &str) -> Option<DatabaseValue> {
    let mut expression = definition.trim();
    while let Some(inner) = expression
        .strip_prefix('(')
        .and_then(|rest| rest.strip_suffix(')'))
        .filter(|inner| balanced(inner))
    {
        expression = inner.trim();
    }
    if expression.eq_ignore_ascii_case("NULL") {
        return None;
    }
    let string = expression
        .strip_prefix("N'")
        .or_else(|| expression.strip_prefix('\''))
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

/// Whether `text`'s parentheses balance (so outer ones can be stripped).
fn balanced(text: &str) -> bool {
    let mut depth = 0_i64;
    let mut quoted = false;
    for ch in text.chars() {
        match ch {
            '\'' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// The columns of object `object_id`.
async fn object_columns(
    executor: &dyn Executor,
    object_id: i64,
) -> Result<Vec<ColumnInfo>, DatabaseError> {
    Ok(rows(
        executor,
        "SELECT c.name, t.name, COALESCE(bt.name, t.name), CAST(c.max_length AS int), \
         CAST(c.precision AS int), CAST(c.scale AS int), CAST(c.is_nullable AS int), \
         CAST(c.is_identity AS int), CAST(c.is_computed AS int), \
         CAST(COALESCE(cc.is_persisted, 0) AS int), dc.definition, CAST(c.column_id AS int), \
         CASE WHEN EXISTS (SELECT 1 FROM sys.index_columns ic JOIN sys.indexes i \
         ON i.object_id = ic.object_id AND i.index_id = ic.index_id \
         WHERE i.is_primary_key = 1 AND ic.object_id = c.object_id \
         AND ic.column_id = c.column_id) THEN 1 ELSE 0 END, cc.definition \
         FROM sys.columns c JOIN sys.types t ON t.user_type_id = c.user_type_id \
         LEFT JOIN sys.types bt ON bt.user_type_id = t.system_type_id AND t.is_user_defined = 1 \
         LEFT JOIN sys.computed_columns cc \
         ON cc.object_id = c.object_id AND cc.column_id = c.column_id \
         LEFT JOIN sys.default_constraints dc ON dc.object_id = c.default_object_id \
         WHERE c.object_id = @P1 ORDER BY c.column_id",
        &[DatabaseValue::Int64(object_id)],
    )
    .await?
    .iter()
    .filter_map(|row| {
        let type_name = text(row, 1).unwrap_or_default();
        let base = text(row, 2).unwrap_or_default();
        let (max_length, precision, scale) = (
            int(row, 3).unwrap_or_default(),
            int(row, 4).unwrap_or_default(),
            int(row, 5).unwrap_or_default(),
        );
        let generated = flag(row, 8).then(|| {
            if flag(row, 9) {
                GeneratedKind::Stored
            } else {
                GeneratedKind::Virtual
            }
        });
        Some(ColumnInfo {
            name: text(row, 0)?,
            data_type: data_type(&base, max_length, precision, scale),
            nullable: flag(row, 6),
            is_primary_key: flag(row, 12),
            auto_increment: flag(row, 7),
            default_value: text(row, 10)
                .filter(|_| generated.is_none())
                .and_then(|definition| parse_default(&definition)),
            ordinal_position: int(row, 11)
                .and_then(|position| u32::try_from(position).ok())
                .unwrap_or(0),
            native_type: native_type(&type_name, &base, max_length, precision, scale),
            enum_values: None,
            default_text: text(row, 10).filter(|_| generated.is_none()),
            generation_expression: text(row, 13).filter(|_| generated.is_some()),
            on_update: None,
            unsigned: false,
            character_set: None,
            collation: None,
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
        Some((object_id, ..)) => object_columns(executor, object_id).await,
        None => Ok(Vec::new()),
    }
}

/// The SQL Server name of a referential action (`NO_ACTION` -> `NO ACTION`).
fn referential_action(action: &str) -> String {
    action.replace('_', " ")
}

/// Columns, indexes and foreign keys of a table or view.
pub(super) async fn relation_info(
    executor: &dyn Executor,
    relation: &RelationRef,
) -> Result<Option<TableInfo>, DatabaseError> {
    let Some((object_id, schema, kind, comment)) =
        relation_meta(executor, relation.schema.as_deref(), &relation.name).await?
    else {
        return Ok(None);
    };
    let columns = object_columns(executor, object_id)
        .await?
        .into_iter()
        .map(|column| (column.name.clone(), column))
        .collect();

    let mut indexes: BTreeMap<String, IndexInfo> = BTreeMap::new();
    for row in rows(
        executor,
        "SELECT i.name, CAST(i.is_unique AS int), CAST(i.is_primary_key AS int), c.name \
         FROM sys.indexes i JOIN sys.index_columns ic \
         ON ic.object_id = i.object_id AND ic.index_id = i.index_id \
         JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id \
         WHERE i.object_id = @P1 AND i.name IS NOT NULL AND i.is_hypothetical = 0 \
         AND ic.is_included_column = 0 ORDER BY i.name, ic.key_ordinal",
        &[DatabaseValue::Int64(object_id)],
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
        "SELECT fk.name, pc.name, OBJECT_NAME(fk.referenced_object_id), rc.name, \
         fk.update_referential_action_desc, fk.delete_referential_action_desc, \
         OBJECT_SCHEMA_NAME(fk.referenced_object_id) \
         FROM sys.foreign_keys fk JOIN sys.foreign_key_columns fkc \
         ON fkc.constraint_object_id = fk.object_id \
         JOIN sys.columns pc ON pc.object_id = fkc.parent_object_id \
         AND pc.column_id = fkc.parent_column_id \
         JOIN sys.columns rc ON rc.object_id = fkc.referenced_object_id \
         AND rc.column_id = fkc.referenced_column_id \
         WHERE fk.parent_object_id = @P1 ORDER BY fk.name, fkc.constraint_column_id",
        &[DatabaseValue::Int64(object_id)],
    )
    .await?
    {
        let Some(name) = text(&row, 0) else {
            continue;
        };
        foreign_keys
            .entry(name.clone())
            .or_insert_with(|| ForeignKeyInfo {
                referenced_schema: text(&row, 6),
                on_update: text(&row, 4).map(|action| referential_action(&action)),
                on_delete: text(&row, 5).map(|action| referential_action(&action)),
                ..ForeignKeyInfo::new(name, text(&row, 2).unwrap_or_default())
            })
            .push_pair(
                text(&row, 1).unwrap_or_default(),
                text(&row, 3).unwrap_or_default(),
            );
    }

    Ok(Some(TableInfo {
        name: relation.name.clone(),
        schema,
        kind,
        comment,
        columns,
        indexes,
        foreign_keys,
    }))
}

/// The definition of a view.
pub(super) async fn view_definition(
    executor: &dyn Executor,
    relation: &RelationRef,
) -> Result<Option<String>, DatabaseError> {
    let found = rows(
        executor,
        "SELECT m.definition FROM sys.sql_modules m JOIN sys.objects o \
         ON o.object_id = m.object_id JOIN sys.schemas s ON s.schema_id = o.schema_id \
         WHERE o.type = 'V' AND o.name = @P1 AND s.name = COALESCE(@P2, SCHEMA_NAME())",
        &[
            DatabaseValue::String(relation.name.clone()),
            schema_param(relation.schema.as_deref()),
        ],
    )
    .await?;
    Ok(found.first().and_then(|row| text(row, 0)))
}

/// Procedures and functions in `schema`, or in every schema with `None`
/// (user routines only; SQL Server's built-in functions aren't in the
/// catalog).
#[cfg(feature = "raw-sql")]
pub(super) async fn list_routines(
    executor: &dyn Executor,
    schema: Option<&str>,
) -> Result<Vec<crate::schema::RoutineInfo>, DatabaseError> {
    let found = rows(
        executor,
        "SELECT s.name, o.name, \
         CASE WHEN o.type IN ('P', 'PC') THEN 'procedure' \
         WHEN o.type IN ('FN', 'FS') THEN 'function' \
         WHEN o.type = 'AF' THEN 'aggregate' ELSE 'table function' END, \
         COALESCE(STUFF((SELECT N', ' + p.name + N' ' + TYPE_NAME(p.user_type_id) \
         FROM sys.parameters p WHERE p.object_id = o.object_id AND p.parameter_id > 0 \
         ORDER BY p.parameter_id FOR XML PATH(''), TYPE).value('.', 'nvarchar(max)'), 1, 2, N''), \
         N''), \
         CASE WHEN o.type IN ('IF', 'TF', 'FT') THEN N'TABLE' ELSE \
         (SELECT TYPE_NAME(p.user_type_id) FROM sys.parameters p \
         WHERE p.object_id = o.object_id AND p.parameter_id = 0) END, 'false' \
         FROM sys.objects o JOIN sys.schemas s ON s.schema_id = o.schema_id \
         WHERE o.type IN ('P', 'PC', 'FN', 'FS', 'IF', 'TF', 'FT', 'AF') \
         AND o.is_ms_shipped = 0 AND (@P1 IS NULL OR s.name = @P1) ORDER BY o.name",
        &[schema_param(schema)],
    )
    .await?;
    Ok(crate::schema::routines_from_rows(&found))
}

/// Foreign keys from or to a table, or from or to any table of a schema
/// (see [`crate::schema::foreign_keys_sql::mssql`]).
#[cfg(feature = "raw-sql")]
pub(super) async fn list_foreign_keys(
    executor: &dyn Executor,
    scope: &crate::schema::ForeignKeyScope,
) -> Result<Vec<crate::schema::TableForeignKey>, DatabaseError> {
    let (sql, params) = crate::schema::foreign_keys_sql::mssql(scope);
    let found = rows(executor, &sql, &params).await?;
    Ok(crate::schema::foreign_keys_sql::foreign_keys_from_rows(
        &found,
    ))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn defaults_lose_their_parentheses() {
        assert_eq!(parse_default("((0))"), Some(DatabaseValue::Int64(0)));
        assert_eq!(parse_default("((1.5))"), Some(DatabaseValue::Real64(1.5)));
        assert_eq!(
            parse_default("(N'it''s')"),
            Some(DatabaseValue::String("it's".to_string()))
        );
        assert_eq!(
            parse_default("(getdate())"),
            Some(DatabaseValue::String("getdate()".to_string()))
        );
        assert_eq!(
            parse_default("((1)+(2))"),
            Some(DatabaseValue::String("(1)+(2)".to_string()))
        );
        assert_eq!(parse_default("(NULL)"), None);
    }

    #[test]
    fn native_types_carry_lengths() {
        assert_eq!(
            native_type("nvarchar", "nvarchar", 100, 0, 0),
            "nvarchar(50)"
        );
        assert_eq!(
            native_type("varbinary", "varbinary", -1, 0, 0),
            "varbinary(max)"
        );
        assert_eq!(
            native_type("decimal", "decimal", 9, 10, 2),
            "decimal(10, 2)"
        );
        assert_eq!(
            native_type("datetime2", "datetime2", 8, 27, 3),
            "datetime2(3)"
        );
        assert_eq!(native_type("Phone", "nvarchar", 40, 0, 0), "Phone");
        assert_eq!(data_type("nvarchar", -1, 0, 0), DataType::Text);
        assert_eq!(data_type("nvarchar", 100, 0, 0), DataType::VarChar(50));
        assert_eq!(
            data_type("geography", -1, 0, 0),
            DataType::Custom("geography".to_string())
        );
    }
}
