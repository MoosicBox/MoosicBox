//! [`DatabaseValue`]s as SQL Server parameters, and result values as
//! [`DatabaseValue`]s.

use std::borrow::Cow;

use tiberius::{ColumnData, ColumnType, FromSql, ToSql};

use crate::DatabaseValue;

/// A [`DatabaseValue`] bound as a statement parameter.
///
/// Nulls are typed after their variant (`Int32Opt(None)` is an `int` null);
/// an untyped [`DatabaseValue::Null`] is an `nvarchar` null, which SQL
/// Server converts to any column type. Unsigned integers take the next wider
/// signed type (`u64` above `i64::MAX` becomes a `decimal(20, 0)`), JSON is
/// sent as text, and [`DatabaseValue::Other`] as its text.
/// [`DatabaseValue::Now`] / [`DatabaseValue::NowPlus`] are never bound:
/// they are written into the statement as `SYSUTCDATETIME()` expressions
/// (see [`now_sql`]).
pub(super) struct Param<'a>(pub &'a DatabaseValue);

impl ToSql for Param<'_> {
    #[allow(clippy::too_many_lines)]
    fn to_sql(&self) -> ColumnData<'_> {
        use DatabaseValue as V;
        match self.0 {
            V::Null | V::StringOpt(None) | V::Now | V::NowPlus(_) => ColumnData::String(None),
            V::String(value) | V::StringOpt(Some(value)) => {
                ColumnData::String(Some(Cow::Borrowed(value)))
            }
            V::Bool(value) | V::BoolOpt(Some(value)) => ColumnData::Bit(Some(*value)),
            V::BoolOpt(None) => ColumnData::Bit(None),
            // tinyint is unsigned in SQL Server.
            V::Int8(value) | V::Int8Opt(Some(value)) => ColumnData::I16(Some(i16::from(*value))),
            V::Int16(value) | V::Int16Opt(Some(value)) => ColumnData::I16(Some(*value)),
            V::Int8Opt(None) | V::Int16Opt(None) => ColumnData::I16(None),
            V::Int32(value) | V::Int32Opt(Some(value)) => ColumnData::I32(Some(*value)),
            V::Int32Opt(None) => ColumnData::I32(None),
            V::Int64(value) | V::Int64Opt(Some(value)) => ColumnData::I64(Some(*value)),
            V::Int64Opt(None) => ColumnData::I64(None),
            V::UInt8(value) | V::UInt8Opt(Some(value)) => ColumnData::U8(Some(*value)),
            V::UInt8Opt(None) => ColumnData::U8(None),
            V::UInt16(value) | V::UInt16Opt(Some(value)) => {
                ColumnData::I32(Some(i32::from(*value)))
            }
            V::UInt16Opt(None) => ColumnData::I32(None),
            V::UInt32(value) | V::UInt32Opt(Some(value)) => {
                ColumnData::I64(Some(i64::from(*value)))
            }
            V::UInt32Opt(None) => ColumnData::I64(None),
            V::UInt64(value) | V::UInt64Opt(Some(value)) => i64::try_from(*value).map_or_else(
                |_| {
                    ColumnData::Numeric(Some(tiberius::numeric::Numeric::new_with_scale(
                        i128::from(*value),
                        0,
                    )))
                },
                |value| ColumnData::I64(Some(value)),
            ),
            V::UInt64Opt(None) => ColumnData::I64(None),
            V::Real64(value) | V::Real64Opt(Some(value)) => ColumnData::F64(Some(*value)),
            V::Real64Opt(None) => ColumnData::F64(None),
            V::Real32(value) | V::Real32Opt(Some(value)) => ColumnData::F32(Some(*value)),
            V::Real32Opt(None) => ColumnData::F32(None),
            #[cfg(feature = "decimal")]
            V::Decimal(value) | V::DecimalOpt(Some(value)) => {
                ColumnData::Numeric(Some(tiberius::numeric::Numeric::new_with_scale(
                    value.mantissa(),
                    u8::try_from(value.scale()).unwrap_or(28),
                )))
            }
            #[cfg(feature = "decimal")]
            V::DecimalOpt(None) => ColumnData::Numeric(None),
            #[cfg(feature = "uuid")]
            V::Uuid(value) | V::UuidOpt(Some(value)) => ColumnData::Guid(Some(*value)),
            #[cfg(feature = "uuid")]
            V::UuidOpt(None) => ColumnData::Guid(None),
            V::DateTime(value) => value.to_sql(),
            V::Date(value) | V::DateOpt(Some(value)) => value.to_sql(),
            V::DateOpt(None) => ColumnData::Date(None),
            V::Time(value) | V::TimeOpt(Some(value)) => value.to_sql(),
            V::TimeOpt(None) => ColumnData::Time(None),
            V::DateTimeTz(value) | V::DateTimeTzOpt(Some(value)) => value.to_sql(),
            V::DateTimeTzOpt(None) => ColumnData::DateTimeOffset(None),
            V::Bytes(value) | V::BytesOpt(Some(value)) => {
                ColumnData::Binary(Some(Cow::Borrowed(value)))
            }
            V::BytesOpt(None) => ColumnData::Binary(None),
            #[cfg(feature = "json")]
            V::Json(value) | V::JsonOpt(Some(value)) => {
                ColumnData::String(Some(Cow::Owned(value.to_string())))
            }
            #[cfg(feature = "json")]
            V::JsonOpt(None) => ColumnData::String(None),
            V::Other { text, .. } => ColumnData::String(Some(Cow::Borrowed(text))),
        }
    }
}

/// The SQL a `Now`/`NowPlus` value is written as (UTC, `datetime2`), or
/// `None` for values that are bound.
pub(super) fn now_sql(value: &DatabaseValue) -> Option<String> {
    match value {
        DatabaseValue::Now => Some("SYSUTCDATETIME()".to_string()),
        DatabaseValue::NowPlus(interval) => {
            let mut sql = "SYSUTCDATETIME()".to_string();
            for (part, amount) in [
                ("nanosecond", i64::from(interval.nanos)),
                ("second", interval.seconds),
                ("minute", interval.minutes),
                ("hour", interval.hours),
                ("day", i64::from(interval.days)),
                ("month", i64::from(interval.months)),
                ("year", i64::from(interval.years)),
            ] {
                if amount != 0 {
                    sql = format!("DATEADD({part}, {amount}, {sql})");
                }
            }
            Some(sql)
        }
        _ => None,
    }
}

/// The native type name a result column reports (`int`, `nvarchar`,
/// `datetime2`, ...). Variable-width types are named without their length.
#[must_use]
pub(super) const fn type_name(column_type: ColumnType) -> Option<&'static str> {
    Some(match column_type {
        ColumnType::Null => return None,
        ColumnType::Bit | ColumnType::Bitn => "bit",
        ColumnType::Int1 => "tinyint",
        ColumnType::Int2 => "smallint",
        ColumnType::Int4 | ColumnType::Intn => "int",
        ColumnType::Int8 => "bigint",
        ColumnType::Datetime4 => "smalldatetime",
        ColumnType::Float4 => "real",
        ColumnType::Float8 | ColumnType::Floatn => "float",
        ColumnType::Money => "money",
        ColumnType::Money4 => "smallmoney",
        ColumnType::Datetime | ColumnType::Datetimen => "datetime",
        ColumnType::Guid => "uniqueidentifier",
        ColumnType::Decimaln => "decimal",
        ColumnType::Numericn => "numeric",
        ColumnType::Daten => "date",
        ColumnType::Timen => "time",
        ColumnType::Datetime2 => "datetime2",
        ColumnType::DatetimeOffsetn => "datetimeoffset",
        ColumnType::BigVarBin => "varbinary",
        ColumnType::BigVarChar => "varchar",
        ColumnType::BigBinary => "binary",
        ColumnType::BigChar => "char",
        ColumnType::NVarchar => "nvarchar",
        ColumnType::NChar => "nchar",
        ColumnType::Xml => "xml",
        ColumnType::Udt => "udt",
        ColumnType::Text => "text",
        ColumnType::Image => "image",
        ColumnType::NText => "ntext",
        ColumnType::SSVariant => "sql_variant",
    })
}

/// `value` (scaled integer and scale) as decimal text: `-12.50`.
fn numeric_text(numeric: tiberius::numeric::Numeric) -> String {
    let scale = u32::from(numeric.scale());
    let magnitude = numeric.value().unsigned_abs();
    let sign = if numeric.value() < 0 { "-" } else { "" };
    if scale == 0 {
        return format!("{sign}{magnitude}");
    }
    let factor = 10_u128.pow(scale);
    format!(
        "{sign}{}.{:0width$}",
        magnitude / factor,
        magnitude % factor,
        width = scale as usize
    )
}

/// A `decimal`/`numeric` value: a `Decimal` where it fits (28 digits),
/// otherwise its exact text.
fn numeric_value(numeric: tiberius::numeric::Numeric) -> DatabaseValue {
    #[cfg(feature = "decimal")]
    if let Ok(decimal) =
        rust_decimal::Decimal::try_from_i128_with_scale(numeric.value(), u32::from(numeric.scale()))
    {
        return DatabaseValue::Decimal(decimal);
    }
    DatabaseValue::Other {
        type_name: "decimal".to_string(),
        text: numeric_text(numeric),
    }
}

/// A `money`/`smallmoney` value (decoded by the driver as a float with four
/// decimal places).
fn money_value(value: f64) -> DatabaseValue {
    #[cfg(feature = "decimal")]
    if let Some(decimal) = rust_decimal::Decimal::from_f64_retain(value) {
        return DatabaseValue::Decimal(decimal.round_dp(4));
    }
    DatabaseValue::Other {
        type_name: "money".to_string(),
        text: format!("{value:.4}"),
    }
}

/// Lowercase hex of `bytes`, `0x`-prefixed.
fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(2 + bytes.len() * 2);
    text.push_str("0x");
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(text, "{byte:02X}");
    }
    text
}

/// A result value of a column of `column_type`.
pub(super) fn decode(data: &ColumnData<'static>, column_type: ColumnType) -> DatabaseValue {
    if column_type == ColumnType::SSVariant {
        // A sql_variant holds values of any type, row by row; it reads as
        // the text of the value it holds.
        return match decode(data, ColumnType::Null) {
            DatabaseValue::Null => DatabaseValue::Null,
            value => DatabaseValue::String(value_text(&value)),
        };
    }
    match data {
        ColumnData::U8(value) => value.map_or(DatabaseValue::Null, DatabaseValue::UInt8),
        ColumnData::I16(value) => value.map_or(DatabaseValue::Null, DatabaseValue::Int16),
        ColumnData::I32(value) => value.map_or(DatabaseValue::Null, DatabaseValue::Int32),
        ColumnData::I64(value) => value.map_or(DatabaseValue::Null, DatabaseValue::Int64),
        ColumnData::F32(value) => value.map_or(DatabaseValue::Null, DatabaseValue::Real32),
        ColumnData::F64(value) => value.map_or(DatabaseValue::Null, |value| {
            if matches!(column_type, ColumnType::Money | ColumnType::Money4) {
                money_value(value)
            } else {
                DatabaseValue::Real64(value)
            }
        }),
        ColumnData::Bit(value) => value.map_or(DatabaseValue::Null, DatabaseValue::Bool),
        ColumnData::String(value) => value.as_ref().map_or(DatabaseValue::Null, |value| {
            DatabaseValue::String(value.to_string())
        }),
        ColumnData::Guid(value) => value.map_or(DatabaseValue::Null, |value| {
            #[cfg(feature = "uuid")]
            {
                DatabaseValue::Uuid(value)
            }
            #[cfg(not(feature = "uuid"))]
            {
                DatabaseValue::String(value.to_string())
            }
        }),
        ColumnData::Binary(value) => value.as_ref().map_or(DatabaseValue::Null, |value| {
            DatabaseValue::Bytes(value.to_vec())
        }),
        ColumnData::Numeric(value) => value.map_or(DatabaseValue::Null, numeric_value),
        ColumnData::Xml(value) => value.as_ref().map_or(DatabaseValue::Null, |value| {
            DatabaseValue::String(value.to_string())
        }),
        ColumnData::DateTime(None)
        | ColumnData::SmallDateTime(None)
        | ColumnData::DateTime2(None)
        | ColumnData::Date(None)
        | ColumnData::Time(None)
        | ColumnData::DateTimeOffset(None) => DatabaseValue::Null,
        // `datetime` counts 1/300 seconds, which SQL Server itself shows
        // (and parses back) rounded to milliseconds.
        ColumnData::DateTime(_) => {
            match from_sql::<chrono::NaiveDateTime>(data, DatabaseValue::DateTime) {
                DatabaseValue::DateTime(value) => DatabaseValue::DateTime(round_to_millis(value)),
                other => other,
            }
        }
        ColumnData::SmallDateTime(_) | ColumnData::DateTime2(_) => {
            from_sql::<chrono::NaiveDateTime>(data, DatabaseValue::DateTime)
        }
        ColumnData::Date(_) => from_sql::<chrono::NaiveDate>(data, DatabaseValue::Date),
        ColumnData::Time(_) => from_sql::<chrono::NaiveTime>(data, DatabaseValue::Time),
        ColumnData::DateTimeOffset(_) => {
            from_sql::<chrono::DateTime<chrono::FixedOffset>>(data, DatabaseValue::DateTimeTz)
        }
    }
}

/// `value` rounded to the nearest millisecond.
fn round_to_millis(value: chrono::NaiveDateTime) -> chrono::NaiveDateTime {
    use chrono::{Duration, Timelike as _};
    let nanos = i64::from(value.nanosecond() % 1_000_000_000);
    let rounded = (nanos + 500_000) / 1_000_000 * 1_000_000;
    value + Duration::nanoseconds(rounded - nanos)
}

/// `data` through the driver's chrono conversion, or its debug text if the
/// driver can't convert it (a value out of chrono's range).
fn from_sql<'a, T: FromSql<'a>>(
    data: &'a ColumnData<'static>,
    wrap: fn(T) -> DatabaseValue,
) -> DatabaseValue {
    match T::from_sql(data) {
        Ok(Some(value)) => wrap(value),
        Ok(None) => DatabaseValue::Null,
        Err(error) => DatabaseValue::Other {
            type_name: "datetime".to_string(),
            text: format!("{data:?} ({error})"),
        },
    }
}

/// The text of a decoded value (for `sql_variant`).
fn value_text(value: &DatabaseValue) -> String {
    match value {
        DatabaseValue::String(text) => text.clone(),
        DatabaseValue::Bytes(bytes) => hex(bytes),
        DatabaseValue::Other { text, .. } => text.clone(),
        DatabaseValue::Bool(value) => if *value { "1" } else { "0" }.to_string(),
        DatabaseValue::UInt8(value) => value.to_string(),
        DatabaseValue::Int16(value) => value.to_string(),
        DatabaseValue::Int32(value) => value.to_string(),
        DatabaseValue::Int64(value) => value.to_string(),
        DatabaseValue::Real32(value) => value.to_string(),
        DatabaseValue::Real64(value) => value.to_string(),
        #[cfg(feature = "decimal")]
        DatabaseValue::Decimal(value) => value.to_string(),
        #[cfg(feature = "uuid")]
        DatabaseValue::Uuid(value) => value.to_string().to_uppercase(),
        DatabaseValue::DateTime(value) => value.format("%Y-%m-%d %H:%M:%S%.f").to_string(),
        DatabaseValue::Date(value) => value.to_string(),
        DatabaseValue::Time(value) => value.to_string(),
        DatabaseValue::DateTimeTz(value) => value.to_rfc3339(),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn numerics_keep_their_scale() {
        use tiberius::numeric::Numeric;
        assert_eq!(numeric_text(Numeric::new_with_scale(-1250, 2)), "-12.50");
        assert_eq!(numeric_text(Numeric::new_with_scale(5, 0)), "5");
        assert_eq!(numeric_text(Numeric::new_with_scale(7, 3)), "0.007");
        // 38 digits don't fit a Decimal; they keep their exact text.
        let huge = Numeric::new_with_scale(10_i128.pow(37), 0);
        assert!(matches!(
            numeric_value(huge),
            DatabaseValue::Other { text, .. } if text == format!("1{}", "0".repeat(37))
        ));
    }

    #[test]
    fn values_bind_as_their_sql_server_types() {
        assert_eq!(
            Param(&DatabaseValue::Int8(-3)).to_sql(),
            ColumnData::I16(Some(-3))
        );
        assert_eq!(
            Param(&DatabaseValue::UInt64(u64::MAX)).to_sql(),
            ColumnData::Numeric(Some(tiberius::numeric::Numeric::new_with_scale(
                i128::from(u64::MAX),
                0
            )))
        );
        assert_eq!(
            Param(&DatabaseValue::Null).to_sql(),
            ColumnData::String(None)
        );
        assert_eq!(
            Param(&DatabaseValue::BytesOpt(None)).to_sql(),
            ColumnData::Binary(None)
        );
    }

    #[test]
    fn now_becomes_sql() {
        assert_eq!(
            now_sql(&DatabaseValue::Now).as_deref(),
            Some("SYSUTCDATETIME()")
        );
        let interval = crate::sql_interval::SqlInterval {
            days: 1,
            hours: -2,
            ..Default::default()
        };
        assert_eq!(
            now_sql(&DatabaseValue::NowPlus(interval)).as_deref(),
            Some("DATEADD(day, 1, DATEADD(hour, -2, SYSUTCDATETIME()))")
        );
        assert_eq!(now_sql(&DatabaseValue::Int32(1)), None);
    }

    #[test]
    fn variants_read_as_text() {
        assert_eq!(
            decode(&ColumnData::I32(Some(5)), ColumnType::SSVariant),
            DatabaseValue::String("5".to_string())
        );
        assert_eq!(
            decode(&ColumnData::String(None), ColumnType::SSVariant),
            DatabaseValue::Null
        );
        assert_eq!(
            decode(
                &ColumnData::Binary(Some(Cow::Owned(vec![0xab]))),
                ColumnType::SSVariant
            ),
            DatabaseValue::String("0xAB".to_string())
        );
    }

    #[test]
    fn legacy_datetimes_round_to_milliseconds() {
        let at = |nanos| {
            chrono::NaiveDate::from_ymd_opt(2024, 3, 1)
                .unwrap()
                .and_hms_nano_opt(23, 59, 59, nanos)
                .unwrap()
        };
        assert_eq!(round_to_millis(at(996_666_666)), at(997_000_000));
        assert_eq!(round_to_millis(at(3_333_333)), at(3_000_000));
        assert_eq!(
            round_to_millis(at(999_666_666)),
            chrono::NaiveDate::from_ymd_opt(2024, 3, 2)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
        );
    }
}
