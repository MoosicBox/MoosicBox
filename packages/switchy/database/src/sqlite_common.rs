//! Shared helpers for `SQLite`-family backends (rusqlite, sqlx-sqlite, Turso).
//!
//! Everything here is backend-agnostic: SQL text for catalog queries and pure
//! functions mapping catalog values to the [`crate::schema`] introspection
//! types.

#![allow(dead_code)]

#[cfg(feature = "schema")]
use crate::schema::{DataType, RelationKind, SchemaInfo};

/// Name of the default `SQLite` schema.
pub const SQLITE_DEFAULT_SCHEMA: &str = "main";

/// Name of the `SQLite` temporary schema.
pub const SQLITE_TEMP_SCHEMA: &str = "temp";

/// Lists attached databases: columns `seq`, `name`, `file`.
pub const SQLITE_LIST_SCHEMAS_SQL: &str = "PRAGMA database_list";

/// Quote an identifier for use in `SQLite` SQL (`"name"`, with embedded quotes doubled).
#[must_use]
pub fn quote_sqlite_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

/// The catalog table for a schema (`sqlite_temp_schema` for `temp`).
#[must_use]
pub fn sqlite_catalog_table(schema: &str) -> String {
    if schema.eq_ignore_ascii_case(SQLITE_TEMP_SCHEMA) {
        // The temp catalog is only addressable by its own name
        "sqlite_temp_master".to_string()
    } else {
        format!("{}.sqlite_master", quote_sqlite_identifier(schema))
    }
}

/// SQL listing tables and views in `schema`: columns `name`, `type`.
///
/// Internal `sqlite_%` objects are excluded.
#[must_use]
pub fn sqlite_list_relations_sql(schema: &str) -> String {
    format!(
        "SELECT name, type FROM {} \
         WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%' \
         ORDER BY name",
        sqlite_catalog_table(schema)
    )
}

/// SQL selecting `type` and `sql` for a named relation in `schema` (one `?` parameter: name).
#[must_use]
pub fn sqlite_relation_sql(schema: &str) -> String {
    format!(
        "SELECT type, sql FROM {} WHERE type IN ('table', 'view') AND name = ?",
        sqlite_catalog_table(schema)
    )
}

/// The catalog table for an optional schema (`sqlite_master` when `None`).
#[must_use]
pub fn sqlite_catalog_table_for(schema: Option<&str>) -> String {
    schema.map_or_else(|| "sqlite_master".to_string(), sqlite_catalog_table)
}

/// Build `PRAGMA [schema.]pragma("arg")`, quoting both identifiers.
///
/// With `schema` of `None` the pragma is unqualified, so `SQLite` applies its
/// normal name resolution (temp, main, then attached databases).
#[must_use]
pub fn sqlite_pragma_sql(schema: Option<&str>, pragma: &str, arg: &str) -> String {
    schema.map_or_else(
        || format!("PRAGMA {pragma}({})", quote_sqlite_identifier(arg)),
        |schema| {
            format!(
                "PRAGMA {}.{pragma}({})",
                quote_sqlite_identifier(schema),
                quote_sqlite_identifier(arg)
            )
        },
    )
}

/// Whether a `PRAGMA table_xinfo` `hidden` value (`1`) marks a hidden
/// virtual-table column, which `PRAGMA table_info` omits and introspection skips.
#[must_use]
pub const fn sqlite_hidden_is_virtual_table_column(hidden: i64) -> bool {
    hidden == 1
}

/// The generated-column kind of a `PRAGMA table_xinfo` `hidden` value
/// (`2` = virtual, `3` = stored; anything else is not generated).
#[cfg(feature = "schema")]
#[must_use]
pub const fn sqlite_hidden_generated_kind(hidden: i64) -> Option<crate::schema::GeneratedKind> {
    match hidden {
        2 => Some(crate::schema::GeneratedKind::Virtual),
        3 => Some(crate::schema::GeneratedKind::Stored),
        _ => None,
    }
}

/// Whether `column_name` is declared `PRIMARY KEY AUTOINCREMENT` in a `CREATE TABLE` statement.
#[must_use]
pub fn sqlite_sql_has_autoincrement(create_sql: &str, column_name: &str) -> bool {
    let normalized_sql = create_sql.to_uppercase();
    let normalized_column = column_name.to_uppercase();

    let Some(column_start) = normalized_sql.find(&normalized_column) else {
        return false;
    };
    let column_portion = &normalized_sql[column_start..];
    let Some(pk_pos) = column_portion.find("PRIMARY KEY") else {
        return false;
    };
    let after_pk = &column_portion[pk_pos + "PRIMARY KEY".len()..];
    let end_pos = after_pk
        .find(',')
        .unwrap_or_else(|| after_pk.find(')').unwrap_or(after_pk.len()));
    after_pk[..end_pos].contains("AUTOINCREMENT")
}

/// Parse a `PRAGMA table_info` `dflt_value` into a [`crate::DatabaseValue`] where possible.
#[must_use]
pub fn sqlite_parse_default_value(default_str: Option<&str>) -> Option<crate::DatabaseValue> {
    use crate::DatabaseValue;

    default_str.and_then(|s| {
        if s == "NULL" {
            Some(DatabaseValue::Null)
        } else if s.len() >= 2 && s.starts_with('\'') && s.ends_with('\'') {
            Some(DatabaseValue::String(s[1..s.len() - 1].replace("''", "'")))
        } else if let Ok(num) = s.parse::<i64>() {
            Some(DatabaseValue::Int64(num))
        } else if let Ok(real) = s.parse::<f64>() {
            Some(DatabaseValue::Real64(real))
        } else if s.eq_ignore_ascii_case("FALSE") {
            Some(DatabaseValue::Bool(false))
        } else if s.eq_ignore_ascii_case("TRUE") {
            Some(DatabaseValue::Bool(true))
        } else {
            None
        }
    })
}

/// Render bytes as an `SQLite` blob literal (`X'0aff'`).
#[must_use]
pub fn sqlite_blob_literal(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut literal = String::with_capacity(bytes.len() * 2 + 3);
    literal.push_str("X'");
    for byte in bytes {
        let _ = write!(literal, "{byte:02x}");
    }
    literal.push('\'');
    literal
}

/// Text representation used to store a value in `SQLite`, for the variants
/// that `SQLite` has no native storage class for.
///
/// * `Date` -> `YYYY-MM-DD`
/// * `Time` -> `HH:MM:SS[.fraction]`
/// * `DateTimeTz` -> RFC 3339
/// * `Json` -> serialized JSON text
/// * `Other` -> its text
///
/// Returns `None` for every other variant (including the `*Opt(None)` nulls).
#[must_use]
pub fn sqlite_text_for_value(value: &crate::DatabaseValue) -> Option<String> {
    use crate::DatabaseValue;

    match value {
        DatabaseValue::Date(date) | DatabaseValue::DateOpt(Some(date)) => {
            Some(date.format("%Y-%m-%d").to_string())
        }
        DatabaseValue::Time(time) | DatabaseValue::TimeOpt(Some(time)) => {
            Some(time.format("%H:%M:%S%.f").to_string())
        }
        DatabaseValue::DateTimeTz(dt) | DatabaseValue::DateTimeTzOpt(Some(dt)) => {
            Some(dt.to_rfc3339())
        }
        #[cfg(feature = "json")]
        DatabaseValue::Json(json) | DatabaseValue::JsonOpt(Some(json)) => Some(json.to_string()),
        DatabaseValue::Other { text, .. } => Some(text.clone()),
        _ => None,
    }
}

/// Render a value as an `SQLite` SQL literal for the newer variants
/// (`Bytes`, `Date`, `Time`, `DateTimeTz`, `Json`, `Other`).
///
/// Returns `None` for variants not covered here; nulls render as `NULL`.
#[must_use]
pub fn sqlite_literal_for_value(value: &crate::DatabaseValue) -> Option<String> {
    use crate::DatabaseValue;

    match value {
        DatabaseValue::Bytes(bytes) | DatabaseValue::BytesOpt(Some(bytes)) => {
            Some(sqlite_blob_literal(bytes))
        }
        DatabaseValue::BytesOpt(None)
        | DatabaseValue::DateOpt(None)
        | DatabaseValue::TimeOpt(None)
        | DatabaseValue::DateTimeTzOpt(None) => Some("NULL".to_string()),
        #[cfg(feature = "json")]
        DatabaseValue::JsonOpt(None) => Some("NULL".to_string()),
        other => sqlite_text_for_value(other).map(|text| format!("'{}'", text.replace('\'', "''"))),
    }
}

/// Resolve an optional schema name to the effective `SQLite` schema.
#[must_use]
pub fn sqlite_schema_or_default(schema: Option<&str>) -> &str {
    schema.unwrap_or(SQLITE_DEFAULT_SCHEMA)
}

/// Build a [`SchemaInfo`] from a `PRAGMA database_list` name.
#[cfg(feature = "schema")]
#[must_use]
pub fn sqlite_schema_info(name: &str) -> SchemaInfo {
    SchemaInfo {
        name: name.to_string(),
        is_default: name == SQLITE_DEFAULT_SCHEMA,
        is_system: name == SQLITE_TEMP_SCHEMA,
    }
}

/// Map a `sqlite_master.type` value to a [`RelationKind`].
#[cfg(feature = "schema")]
#[must_use]
pub fn sqlite_relation_kind(kind: &str, name: &str) -> RelationKind {
    if name.starts_with("sqlite_") {
        return RelationKind::SystemTable;
    }
    match kind {
        "table" => RelationKind::Table,
        "view" => RelationKind::View,
        other => RelationKind::Other(other.to_string()),
    }
}

/// Parse a parenthesized numeric argument list like `(10, 2)` from a type name.
fn type_args(native: &str) -> Vec<u32> {
    native
        .find('(')
        .and_then(|start| {
            native[start + 1..]
                .find(')')
                .map(|end| &native[start + 1..start + 1 + end])
        })
        .map(|args| {
            args.split(',')
                .filter_map(|arg| arg.trim().parse::<u32>().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Map a declared `SQLite` column type to a [`DataType`].
///
/// `SQLite` accepts arbitrary declared types; well-known names map to the
/// matching [`DataType`] variant and anything else to [`DataType::Custom`]
/// with the declared type preserved. This never fails.
#[cfg(feature = "schema")]
#[must_use]
pub fn sqlite_type_to_data_type(sqlite_type: &str) -> DataType {
    let trimmed = sqlite_type.trim();
    let upper = trimmed.to_ascii_uppercase();
    let base = upper.split('(').next().unwrap_or("").trim();
    let args = type_args(&upper);
    let first_arg = || args.first().and_then(|n| u16::try_from(*n).ok());

    match base {
        "INTEGER" | "BIGINT" | "INT8" | "UNSIGNED BIG INT" => DataType::BigInt,
        "INT" | "INT4" | "MEDIUMINT" => DataType::Int,
        "SMALLINT" | "INT2" => DataType::SmallInt,
        "TINYINT" => DataType::TinyInt,
        "TEXT" | "CLOB" => DataType::Text,
        "VARCHAR" | "CHARACTER VARYING" | "NVARCHAR" | "VARYING CHARACTER" => {
            first_arg().map_or(DataType::Text, DataType::VarChar)
        }
        "CHAR" | "CHARACTER" | "NCHAR" | "NATIVE CHARACTER" => {
            first_arg().map_or(DataType::Text, DataType::Char)
        }
        "REAL" | "DOUBLE" | "DOUBLE PRECISION" | "FLOAT" => DataType::Double,
        "NUMERIC" | "DECIMAL" => match args.as_slice() {
            [precision, scale] => match (u8::try_from(*precision), u8::try_from(*scale)) {
                (Ok(p), Ok(s)) => DataType::Decimal(p, s),
                _ => DataType::Custom(trimmed.to_string()),
            },
            [precision] => u8::try_from(*precision).map_or_else(
                |_| DataType::Custom(trimmed.to_string()),
                |p| DataType::Decimal(p, 0),
            ),
            _ => DataType::Custom(trimmed.to_string()),
        },
        "BLOB" => DataType::Blob,
        "BINARY" | "VARBINARY" => DataType::Binary(args.first().copied()),
        "BOOLEAN" | "BOOL" => DataType::Bool,
        "DATE" => DataType::Date,
        "TIME" => DataType::Time,
        "DATETIME" => DataType::DateTime,
        "TIMESTAMP" | "TIMESTAMPTZ" => DataType::Timestamp,
        "JSON" => DataType::Json,
        "JSONB" => DataType::Jsonb,
        "UUID" => DataType::Uuid,
        _ => DataType::Custom(trimmed.to_string()),
    }
}

#[cfg(all(test, feature = "schema"))]
mod tests {
    use super::*;

    #[test]
    fn maps_declared_types() {
        assert_eq!(sqlite_type_to_data_type("integer"), DataType::BigInt);
        assert_eq!(
            sqlite_type_to_data_type("VARCHAR(255)"),
            DataType::VarChar(255)
        );
        assert_eq!(sqlite_type_to_data_type("varchar"), DataType::Text);
        assert_eq!(sqlite_type_to_data_type("CHAR(3)"), DataType::Char(3));
        assert_eq!(
            sqlite_type_to_data_type("DECIMAL(10, 2)"),
            DataType::Decimal(10, 2)
        );
        assert_eq!(sqlite_type_to_data_type("blob"), DataType::Blob);
        assert_eq!(sqlite_type_to_data_type("TIME"), DataType::Time);
        assert_eq!(sqlite_type_to_data_type("uuid"), DataType::Uuid);
        assert_eq!(
            sqlite_type_to_data_type("my_custom thing"),
            DataType::Custom("my_custom thing".to_string())
        );
        assert_eq!(
            sqlite_type_to_data_type(""),
            DataType::Custom(String::new())
        );
    }

    #[test]
    fn literal_helpers() {
        use crate::DatabaseValue;

        assert_eq!(sqlite_blob_literal(&[0x0a, 0xff]), "X'0aff'");
        assert_eq!(
            sqlite_literal_for_value(&DatabaseValue::Bytes(vec![1, 2])).as_deref(),
            Some("X'0102'")
        );
        let date = chrono::NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
        assert_eq!(
            sqlite_text_for_value(&DatabaseValue::Date(date)).as_deref(),
            Some("2024-02-29")
        );
        let time = chrono::NaiveTime::from_hms_milli_opt(1, 2, 3, 500).unwrap();
        assert_eq!(
            sqlite_text_for_value(&DatabaseValue::Time(time)).as_deref(),
            Some("01:02:03.500")
        );
        assert_eq!(
            sqlite_literal_for_value(&DatabaseValue::Other {
                type_name: "x".into(),
                text: "it's".into()
            })
            .as_deref(),
            Some("'it''s'")
        );
        assert_eq!(
            sqlite_literal_for_value(&DatabaseValue::DateOpt(None)).as_deref(),
            Some("NULL")
        );
        assert!(sqlite_literal_for_value(&DatabaseValue::Int64(1)).is_none());
        assert_eq!(
            sqlite_pragma_sql(Some("aux"), "table_info", "t"),
            "PRAGMA \"aux\".table_info(\"t\")"
        );
        assert_eq!(
            sqlite_pragma_sql(None, "index_list", "t"),
            "PRAGMA index_list(\"t\")"
        );
        assert!(sqlite_sql_has_autoincrement(
            "CREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT, x TEXT)",
            "id"
        ));
        assert!(!sqlite_sql_has_autoincrement(
            "CREATE TABLE t (id INTEGER PRIMARY KEY, x TEXT)",
            "id"
        ));
        assert_eq!(
            sqlite_parse_default_value(Some("'a''b'")),
            Some(DatabaseValue::String("a'b".into()))
        );
    }

    #[test]
    fn table_xinfo_hidden_values() {
        use crate::schema::GeneratedKind;
        assert!(!sqlite_hidden_is_virtual_table_column(0));
        assert!(sqlite_hidden_is_virtual_table_column(1));
        assert_eq!(sqlite_hidden_generated_kind(0), None);
        assert_eq!(sqlite_hidden_generated_kind(1), None);
        assert_eq!(
            sqlite_hidden_generated_kind(2),
            Some(GeneratedKind::Virtual)
        );
        assert_eq!(sqlite_hidden_generated_kind(3), Some(GeneratedKind::Stored));
    }

    #[test]
    fn schema_helpers() {
        assert!(sqlite_schema_info("main").is_default);
        assert!(sqlite_schema_info("temp").is_system);
        assert!(!sqlite_schema_info("aux").is_default);
        assert_eq!(sqlite_catalog_table("temp"), "sqlite_temp_master");
        assert_eq!(sqlite_catalog_table("a\"b"), "\"a\"\"b\".sqlite_master");
        assert_eq!(sqlite_relation_kind("view", "v"), RelationKind::View);
        assert_eq!(
            sqlite_relation_kind("table", "sqlite_sequence"),
            RelationKind::SystemTable
        );
    }
}
