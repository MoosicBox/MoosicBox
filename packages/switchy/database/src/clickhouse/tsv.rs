//! Results in `ClickHouse`'s `TabSeparatedWithNamesAndTypes` format, decoded
//! into [`DatabaseValue`]s by each column's reported type.
//!
//! The format is lossless: every value is text in `ClickHouse`'s escaped
//! form, `\N` is `NULL`, and the second line names each column's type.
//! Scalars become the matching [`DatabaseValue`] (`Nullable(...)` and
//! `LowCardinality(...)` are looked through); arrays, maps, tuples, enums,
//! big integers and other types stay text as `ClickHouse` writes them
//! (`['a','b']`, `{'k':1}`, `(1,'x')`), which also reads back as SQL.

use crate::{DatabaseValue, ResultColumn};

/// A decoded result.
#[derive(Debug, Default)]
pub(super) struct Table {
    pub columns: Vec<ResultColumn>,
    pub rows: Vec<Vec<DatabaseValue>>,
}

/// Decode a `TabSeparatedWithNamesAndTypes` body.
pub(super) fn decode(body: &[u8]) -> Table {
    let mut lines = body
        .strip_suffix(b"\n")
        .unwrap_or(body)
        .split(|byte| *byte == b'\n');
    let (Some(names), Some(types)) = (lines.next(), lines.next()) else {
        return Table::default();
    };
    let names = fields(names);
    let types = fields(types);
    let types: Vec<String> = types
        .iter()
        .map(|field| String::from_utf8_lossy(&unescape(field)).into_owned())
        .collect();
    let columns = names
        .iter()
        .enumerate()
        .map(|(index, name)| ResultColumn {
            name: String::from_utf8_lossy(&unescape(name)).into_owned(),
            type_name: types.get(index).cloned(),
        })
        .collect();
    let rows = lines
        .map(|line| {
            fields(line)
                .iter()
                .enumerate()
                .map(|(index, field)| {
                    value(types.get(index).map_or("String", String::as_str), field)
                })
                .collect()
        })
        .collect();
    Table { columns, rows }
}

/// The fields of one line.
fn fields(line: &[u8]) -> Vec<&[u8]> {
    line.split(|byte| *byte == b'\t').collect()
}

/// Undo `ClickHouse`'s escaping (`\t`, `\n`, `\\`, `\'`, ...).
fn unescape(field: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(field.len());
    let mut bytes = field.iter();
    while let Some(&byte) = bytes.next() {
        if byte != b'\\' {
            out.push(byte);
            continue;
        }
        match bytes.next() {
            Some(b't') => out.push(b'\t'),
            Some(b'n') => out.push(b'\n'),
            Some(b'r') => out.push(b'\r'),
            Some(b'0') => out.push(0),
            Some(b'b') => out.push(0x08),
            Some(b'f') => out.push(0x0c),
            Some(b'a') => out.push(0x07),
            Some(b'v') => out.push(0x0b),
            Some(&other) => out.push(other),
            None => out.push(b'\\'),
        }
    }
    out
}

/// The type inside `Nullable(...)` and `LowCardinality(...)` wrappers.
pub(super) fn base_type(type_name: &str) -> &str {
    let mut base = type_name.trim();
    loop {
        let inner = ["Nullable(", "LowCardinality("].iter().find_map(|wrapper| {
            base.strip_prefix(wrapper)
                .and_then(|rest| rest.strip_suffix(')'))
        });
        match inner {
            Some(inner) => base = inner.trim(),
            None => return base,
        }
    }
}

/// One field as a value of `type_name`. Text that doesn't parse as the
/// type stays text, and non-UTF-8 text becomes bytes.
fn value(type_name: &str, field: &[u8]) -> DatabaseValue {
    if field == b"\\N" {
        return DatabaseValue::Null;
    }
    let raw = unescape(field);
    let text = match String::from_utf8(raw) {
        Ok(text) => text,
        Err(error) => return DatabaseValue::Bytes(error.into_bytes()),
    };
    scalar(base_type(type_name), &text).unwrap_or(DatabaseValue::String(text))
}

/// `text` as the scalar `base` names, if it is one and parses.
fn scalar(base: &str, text: &str) -> Option<DatabaseValue> {
    let name = base.split('(').next().unwrap_or(base).trim();
    Some(match name {
        "Int8" => DatabaseValue::Int8(text.parse().ok()?),
        "Int16" => DatabaseValue::Int16(text.parse().ok()?),
        "Int32" => DatabaseValue::Int32(text.parse().ok()?),
        "Int64" => DatabaseValue::Int64(text.parse().ok()?),
        "UInt8" => DatabaseValue::UInt8(text.parse().ok()?),
        "UInt16" => DatabaseValue::UInt16(text.parse().ok()?),
        "UInt32" => DatabaseValue::UInt32(text.parse().ok()?),
        "UInt64" => DatabaseValue::UInt64(text.parse().ok()?),
        "Float32" => DatabaseValue::Real32(text.parse().ok()?),
        "Float64" => DatabaseValue::Real64(text.parse().ok()?),
        "Bool" => DatabaseValue::Bool(match text {
            "true" => true,
            "false" => false,
            _ => return None,
        }),
        #[cfg(feature = "decimal")]
        "Decimal" | "Decimal32" | "Decimal64" | "Decimal128" | "Decimal256" => {
            DatabaseValue::Decimal(text.parse().ok()?)
        }
        #[cfg(feature = "uuid")]
        "UUID" => DatabaseValue::Uuid(text.parse().ok()?),
        "Date" | "Date32" => {
            DatabaseValue::Date(chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()?)
        }
        "DateTime" | "DateTime64" => DatabaseValue::DateTime(
            chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.f").ok()?,
        ),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn decodes_names_types_and_values() {
        let body = b"id\ttitle\tprice\ttags\tdt\tf\tbig\n\
            UInt64\tLowCardinality(String)\tNullable(Decimal(10, 2))\tArray(String)\tDateTime64(3)\tFloat64\tInt128\n\
            1\tit\\'s\\ta\\\\b\t9.99\t['x','y']\t2024-01-02 03:04:05.678\tnan\t-170141183460469231731687303715884105728\n\
            2\t\\\\N\t\\N\t[]\t2024-01-02 03:04:05.000\tinf\t5\n";
        let table = decode(body);
        assert_eq!(
            table
                .columns
                .iter()
                .map(|column| (column.name.as_str(), column.type_name.as_deref()))
                .collect::<Vec<_>>(),
            vec![
                ("id", Some("UInt64")),
                ("title", Some("LowCardinality(String)")),
                ("price", Some("Nullable(Decimal(10, 2))")),
                ("tags", Some("Array(String)")),
                ("dt", Some("DateTime64(3)")),
                ("f", Some("Float64")),
                ("big", Some("Int128")),
            ]
        );
        let first = &table.rows[0];
        assert_eq!(first[0], DatabaseValue::UInt64(1));
        assert_eq!(first[1], DatabaseValue::String("it's\ta\\b".to_string()));
        assert_eq!(first[3], DatabaseValue::String("['x','y']".to_string()));
        assert!(matches!(first[5], DatabaseValue::Real64(value) if value.is_nan()));
        assert_eq!(
            first[6],
            DatabaseValue::String("-170141183460469231731687303715884105728".to_string())
        );
        let second = &table.rows[1];
        // An escaped `\N` is text; a bare one is NULL.
        assert_eq!(second[1], DatabaseValue::String("\\N".to_string()));
        assert_eq!(second[2], DatabaseValue::Null);
        assert_eq!(second[5], DatabaseValue::Real64(f64::INFINITY));
        assert!(matches!(second[4], DatabaseValue::DateTime(_)));
    }

    #[test]
    fn empty_bodies_have_no_columns() {
        assert_eq!(decode(b"").columns.len(), 0);
        let table = decode(b"n\nUInt8\n");
        assert_eq!(table.columns.len(), 1);
        assert_eq!(table.rows.len(), 0);
    }

    #[test]
    fn non_utf8_text_is_bytes() {
        let table = decode(b"b\nString\n\xff\xfe\n");
        assert_eq!(table.rows[0][0], DatabaseValue::Bytes(vec![0xff, 0xfe]));
    }

    #[test]
    fn wrappers_are_looked_through() {
        assert_eq!(base_type("LowCardinality(Nullable(String))"), "String");
        assert_eq!(base_type("Nullable(Decimal(10, 2))"), "Decimal(10, 2)");
        assert_eq!(
            base_type("Array(Nullable(String))"),
            "Array(Nullable(String))"
        );
    }
}
