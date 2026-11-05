//! Schema introspection through `ClickHouse`'s `system` tables.
//!
//! Databases are reported as schemas (as on `MySQL`), relations come from
//! `system.tables`, columns from `system.columns`, and indexes from the
//! table's primary key and `system.data_skipping_indices`. `ClickHouse` has
//! no foreign keys.
//!
//! A `ClickHouse` primary key orders and indexes the data but does not make
//! rows unique, so it is reported as a non-unique index named `PRIMARY KEY`,
//! and no column is marked as a primary-key column: consumers that identify
//! rows by their primary key must not take it for a unique key.

use std::collections::BTreeMap;

use crate::schema::{
    ColumnInfo, DataType, GeneratedKind, IndexInfo, RelationKind, RelationRef, RelationSummary,
    SchemaInfo, TableInfo,
};
#[cfg(feature = "raw-sql")]
use crate::schema::{RoutineInfo, RoutineKind};
use crate::{DatabaseError, DatabaseValue};

use super::ClickHouseDatabase;

/// Databases `ClickHouse` keeps for itself.
const SYSTEM_DATABASES: &[&str] = &["system", "information_schema", "INFORMATION_SCHEMA"];

/// Engines of tables that read another system.
const FOREIGN_ENGINES: &[&str] = &[
    "MySQL",
    "PostgreSQL",
    "MaterializedPostgreSQL",
    "MongoDB",
    "ODBC",
    "JDBC",
    "SQLite",
    "Redis",
    "URL",
    "S3",
    "S3Queue",
    "HDFS",
    "AzureBlobStorage",
    "AzureQueue",
    "Kafka",
    "RabbitMQ",
    "NATS",
    "DeltaLake",
    "Hudi",
    "Iceberg",
    "IcebergS3",
];

/// The SQL naming the relation's database: the given one as `$n`, or the
/// current one.
fn database_sql(schema: Option<&str>, params: &mut Vec<DatabaseValue>) -> String {
    schema.map_or_else(
        || "currentDatabase()".to_string(),
        |schema| {
            params.push(DatabaseValue::String(schema.to_string()));
            format!("${}", params.len())
        },
    )
}

fn text(value: Option<&DatabaseValue>) -> Option<String> {
    match value? {
        DatabaseValue::String(text) => Some(text.clone()),
        DatabaseValue::Null => None,
        other => Some(format!("{other:?}")),
    }
}

fn number(value: Option<&DatabaseValue>) -> Option<u64> {
    match value? {
        DatabaseValue::UInt8(value) => Some(u64::from(*value)),
        DatabaseValue::UInt16(value) => Some(u64::from(*value)),
        DatabaseValue::UInt32(value) => Some(u64::from(*value)),
        DatabaseValue::UInt64(value) => Some(*value),
        DatabaseValue::Int64(value) => u64::try_from(*value).ok(),
        DatabaseValue::String(text) => text.parse().ok(),
        _ => None,
    }
}

fn truthy(value: Option<&DatabaseValue>) -> bool {
    match value {
        Some(DatabaseValue::Bool(value)) => *value,
        other => number(other).is_some_and(|value| value != 0),
    }
}

pub(super) async fn list_tables(db: &ClickHouseDatabase) -> Result<Vec<String>, DatabaseError> {
    let result = db
        .send(
            "SELECT name FROM system.tables WHERE database = currentDatabase() \
             AND NOT is_temporary ORDER BY name",
            &[],
        )
        .await?;
    Ok(result
        .rows
        .iter()
        .filter_map(|row| text(row.first()))
        .collect())
}

pub(super) async fn table_exists(
    db: &ClickHouseDatabase,
    table: &str,
) -> Result<bool, DatabaseError> {
    let result = db
        .send(
            "SELECT count() FROM system.tables WHERE database = currentDatabase() AND name = $1",
            &[DatabaseValue::String(table.to_string())],
        )
        .await?;
    Ok(number(result.rows.first().and_then(|row| row.first())).is_some_and(|count| count > 0))
}

pub(super) async fn list_schemas(
    db: &ClickHouseDatabase,
) -> Result<Vec<SchemaInfo>, DatabaseError> {
    let result = db
        .send(
            "SELECT name, name = currentDatabase() FROM system.databases ORDER BY name",
            &[],
        )
        .await?;
    Ok(result
        .rows
        .iter()
        .filter_map(|row| {
            let name = text(row.first())?;
            Some(SchemaInfo {
                is_system: SYSTEM_DATABASES.contains(&name.as_str()),
                is_default: truthy(row.get(1)),
                name,
            })
        })
        .collect())
}

/// The kind of relation a table `engine` makes in `database`.
fn relation_kind(database: &str, engine: &str) -> RelationKind {
    match engine {
        "View" | "LiveView" | "WindowView" => RelationKind::View,
        "MaterializedView" => RelationKind::MaterializedView,
        "Dictionary" => RelationKind::Other("dictionary".to_string()),
        _ if SYSTEM_DATABASES.contains(&database) => RelationKind::SystemTable,
        engine if FOREIGN_ENGINES.contains(&engine) => RelationKind::ForeignTable,
        _ => RelationKind::Table,
    }
}

pub(super) async fn list_relations(
    db: &ClickHouseDatabase,
    schema: Option<&str>,
) -> Result<Vec<RelationSummary>, DatabaseError> {
    let mut params = Vec::new();
    let database = database_sql(schema, &mut params);
    let result = db
        .send(
            &format!(
                "SELECT database, name, engine, comment, total_rows FROM system.tables \
                 WHERE database = {database} AND NOT is_temporary ORDER BY name"
            ),
            &params,
        )
        .await?;
    Ok(result
        .rows
        .iter()
        .filter_map(|row| {
            let database = text(row.first())?;
            let engine = text(row.get(2)).unwrap_or_default();
            Some(RelationSummary {
                kind: relation_kind(&database, &engine),
                name: text(row.get(1))?,
                comment: text(row.get(3)).filter(|comment| !comment.is_empty()),
                estimated_rows: number(row.get(4)),
                schema: Some(database),
            })
        })
        .collect())
}

/// A `ClickHouse` column type as a [`DataType`].
fn data_type(type_name: &str) -> DataType {
    let base = super::tsv::base_type(type_name);
    let name = base.split('(').next().unwrap_or(base).trim();
    let decimal = || {
        let (precision, scale) = base
            .strip_prefix("Decimal(")
            .and_then(|rest| rest.strip_suffix(')'))
            .and_then(|args| args.split_once(','))?;
        Some(DataType::Decimal(
            precision.trim().parse().ok()?,
            scale.trim().parse().ok()?,
        ))
    };
    match name {
        "String" | "FixedString" => DataType::Text,
        "Int8" => DataType::TinyInt,
        "Int16" => DataType::SmallInt,
        "Int32" => DataType::Int,
        "Int64" => DataType::BigInt,
        "Float32" => DataType::Real,
        "Float64" => DataType::Double,
        "Decimal" => decimal().unwrap_or_else(|| DataType::Custom(type_name.to_string())),
        "Bool" => DataType::Bool,
        "Date" | "Date32" => DataType::Date,
        "DateTime" | "DateTime64" => DataType::DateTime,
        "UUID" => DataType::Uuid,
        "JSON" => DataType::Json,
        "IPv4" | "IPv6" => DataType::Inet,
        _ => DataType::Custom(type_name.to_string()),
    }
}

/// The names of an `Enum8`/`Enum16` type's values.
fn enum_values(type_name: &str) -> Option<Vec<String>> {
    let base = super::tsv::base_type(type_name);
    let inner = ["Enum8(", "Enum16(", "Enum("]
        .iter()
        .find_map(|prefix| base.strip_prefix(prefix))?
        .strip_suffix(')')?;
    let mut values = Vec::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\'' {
            continue;
        }
        let mut value = String::new();
        while let Some(c) = chars.next() {
            match c {
                '\\' => value.extend(chars.next()),
                '\'' if chars.peek() == Some(&'\'') => {
                    chars.next();
                    value.push('\'');
                }
                '\'' => break,
                other => value.push(other),
            }
        }
        values.push(value);
        // Skip `= n` up to the next value.
        for c in chars.by_ref() {
            if c == ',' {
                break;
            }
        }
    }
    Some(values)
}

/// A `system.columns` row (`name`, `type`, `default_kind`,
/// `default_expression`, `position`) as a [`ColumnInfo`].
fn column_info(row: &[DatabaseValue]) -> Option<ColumnInfo> {
    let name = text(row.first())?;
    let native_type = text(row.get(1)).unwrap_or_default();
    let default_kind = text(row.get(2)).unwrap_or_default();
    let default_value = text(row.get(3))
        .filter(|expression| default_kind == "DEFAULT" && !expression.is_empty())
        .map(DatabaseValue::String);
    Some(ColumnInfo {
        name,
        data_type: data_type(&native_type),
        nullable: native_type.starts_with("Nullable(")
            || native_type.starts_with("LowCardinality(Nullable("),
        is_primary_key: false,
        auto_increment: false,
        default_value,
        ordinal_position: u32::try_from(number(row.get(4)).unwrap_or(0)).unwrap_or(u32::MAX),
        enum_values: enum_values(&native_type),
        default_text: text(row.get(3))
            .filter(|expression| default_kind == "DEFAULT" && !expression.is_empty()),
        generation_expression: text(row.get(3)).filter(|expression| {
            matches!(default_kind.as_str(), "MATERIALIZED" | "ALIAS") && !expression.is_empty()
        }),
        on_update: None,
        unsigned: false,
        character_set: None,
        collation: None,
        generated: match default_kind.as_str() {
            "MATERIALIZED" => Some(GeneratedKind::Stored),
            "ALIAS" => Some(GeneratedKind::Virtual),
            _ => None,
        },
        native_type,
    })
}

pub(super) async fn relation_info(
    db: &ClickHouseDatabase,
    relation: &RelationRef,
) -> Result<Option<TableInfo>, DatabaseError> {
    let mut params = Vec::new();
    let database = database_sql(relation.schema.as_deref(), &mut params);
    params.push(DatabaseValue::String(relation.name.clone()));
    let name = format!("${}", params.len());
    let table = db
        .send(
            &format!(
                "SELECT database, engine, comment, primary_key FROM system.tables \
                 WHERE database = {database} AND name = {name}"
            ),
            &params,
        )
        .await?;
    let Some(row) = table.rows.first() else {
        return Ok(None);
    };
    let database_name = text(row.first()).unwrap_or_default();
    let engine = text(row.get(1)).unwrap_or_default();
    let comment = text(row.get(2)).filter(|comment| !comment.is_empty());
    let primary_key = text(row.get(3)).unwrap_or_default();

    let columns = db
        .send(
            &format!(
                "SELECT name, type, default_kind, default_expression, position \
                 FROM system.columns WHERE database = {database} AND table = {name} \
                 ORDER BY position"
            ),
            &params,
        )
        .await?;
    let columns: BTreeMap<String, ColumnInfo> = columns
        .rows
        .iter()
        .filter_map(|row| column_info(row))
        .map(|info| (info.name.clone(), info))
        .collect();

    let mut indexes = BTreeMap::new();
    if !primary_key.trim().is_empty() {
        indexes.insert(
            "PRIMARY KEY".to_string(),
            IndexInfo {
                name: "PRIMARY KEY".to_string(),
                unique: false,
                columns: primary_key
                    .split(',')
                    .map(|column| column.trim().to_string())
                    .collect(),
                is_primary: false,
            },
        );
    }
    let skipping = db
        .send(
            &format!(
                "SELECT name, expr FROM system.data_skipping_indices \
                 WHERE database = {database} AND table = {name} ORDER BY name"
            ),
            &params,
        )
        .await?;
    for row in &skipping.rows {
        if let Some(index) = text(row.first()) {
            indexes.insert(
                index.clone(),
                IndexInfo {
                    name: index,
                    unique: false,
                    columns: text(row.get(1)).into_iter().collect(),
                    is_primary: false,
                },
            );
        }
    }

    Ok(Some(TableInfo {
        name: relation.name.clone(),
        kind: relation_kind(&database_name, &engine),
        schema: Some(database_name),
        comment,
        columns,
        indexes,
        foreign_keys: BTreeMap::new(),
    }))
}

pub(super) async fn view_definition(
    db: &ClickHouseDatabase,
    relation: &RelationRef,
) -> Result<Option<String>, DatabaseError> {
    let mut params = Vec::new();
    let database = database_sql(relation.schema.as_deref(), &mut params);
    params.push(DatabaseValue::String(relation.name.clone()));
    let name = format!("${}", params.len());
    let result = db
        .send(
            &format!(
                "SELECT as_select FROM system.tables WHERE database = {database} \
                 AND name = {name} AND engine IN ('View', 'MaterializedView', 'LiveView', \
                 'WindowView')"
            ),
            &params,
        )
        .await?;
    Ok(result
        .rows
        .first()
        .and_then(|row| text(row.first()))
        .filter(|definition| !definition.is_empty()))
}

#[cfg(feature = "raw-sql")]
pub(super) async fn list_routines(
    db: &ClickHouseDatabase,
) -> Result<Vec<RoutineInfo>, DatabaseError> {
    let result = db
        .send(
            "SELECT name, is_aggregate, origin FROM system.functions \
             WHERE alias_to = '' ORDER BY name",
            &[],
        )
        .await?;
    Ok(result
        .rows
        .iter()
        .filter_map(|row| {
            let origin = text(row.get(2)).unwrap_or_default();
            Some(RoutineInfo {
                schema: None,
                name: text(row.first())?,
                kind: if truthy(row.get(1)) {
                    RoutineKind::Aggregate
                } else {
                    RoutineKind::Function
                },
                arguments: None,
                result: None,
                builtin: origin == "System",
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn engines_map_to_relation_kinds() {
        assert_eq!(relation_kind("shop", "MergeTree"), RelationKind::Table);
        assert_eq!(relation_kind("shop", "View"), RelationKind::View);
        assert_eq!(
            relation_kind("shop", "MaterializedView"),
            RelationKind::MaterializedView
        );
        assert_eq!(
            relation_kind("shop", "PostgreSQL"),
            RelationKind::ForeignTable
        );
        assert_eq!(
            relation_kind("system", "SystemNumbers"),
            RelationKind::SystemTable
        );
        assert_eq!(
            relation_kind("shop", "Dictionary"),
            RelationKind::Other("dictionary".to_string())
        );
    }

    #[test]
    fn column_types_map_to_data_types() {
        assert_eq!(
            data_type("Nullable(Decimal(10, 2))"),
            DataType::Decimal(10, 2)
        );
        assert_eq!(data_type("LowCardinality(String)"), DataType::Text);
        assert_eq!(data_type("DateTime64(3, 'UTC')"), DataType::DateTime);
        assert_eq!(
            data_type("Array(String)"),
            DataType::Custom("Array(String)".to_string())
        );
    }

    #[test]
    fn enum_types_list_their_values() {
        assert_eq!(
            enum_values("Enum8('vinyl' = 1, 'it''s' = 2, 'a\\'b' = 3)"),
            Some(vec![
                "vinyl".to_string(),
                "it's".to_string(),
                "a'b".to_string()
            ])
        );
        assert_eq!(enum_values("String"), None);
    }
}
