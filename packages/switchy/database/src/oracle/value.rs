//! [`DatabaseValue`]s as Oracle bind values, and result values as
//! [`DatabaseValue`]s.

use std::fmt::Write as _;

use oracle::SqlValue;
use oracle::sql_type::{OracleType, ToSql};

use crate::DatabaseValue;

/// Text longer than this (in bytes) is bound as a `CLOB`, which has no
/// length limit; shorter text as `VARCHAR2`.
const MAX_VARCHAR_BIND: usize = 4000;

/// Bytes longer than this are bound as a `BLOB`; shorter ones as `RAW`.
const MAX_RAW_BIND: usize = 2000;

/// A [`DatabaseValue`] converted to something the driver binds.
///
/// Nulls keep their variant's type (`Int64Opt(None)` is a `NUMBER` null);
/// an untyped [`DatabaseValue::Null`] is a `VARCHAR2` null, which Oracle
/// converts to any type. Booleans are `BOOLEAN` on Oracle 23ai and later,
/// and `1`/`0` before. Times of day, which Oracle has no type for, are
/// bound as `HH24:MI:SS[.FF]` text; JSON as text. Long text and bytes are
/// bound as `CLOB`/`BLOB`. [`DatabaseValue::Now`] and
/// [`DatabaseValue::NowPlus`] are never bound: they are written into the
/// statement (see [`now_sql`]).
pub(super) enum Bind {
    Null(OracleType),
    Text(String),
    LongText(String),
    Int(i64),
    UInt(u64),
    Float(f32),
    Double(f64),
    Bool(bool),
    Bytes(Vec<u8>),
    LongBytes(Vec<u8>),
    DateTime(chrono::NaiveDateTime),
    Date(chrono::NaiveDate),
    Zoned(chrono::DateTime<chrono::FixedOffset>),
}

impl Bind {
    /// The bind for `value`; `boolean` says whether the server has a SQL
    /// `BOOLEAN` type (23ai).
    #[allow(clippy::too_many_lines)]
    pub(super) fn new(value: &DatabaseValue, boolean: bool) -> Self {
        use DatabaseValue as V;
        let number_null = || Self::Null(OracleType::Number(0, 0));
        let text = |value: &str| {
            if value.len() > MAX_VARCHAR_BIND {
                Self::LongText(value.to_string())
            } else {
                Self::Text(value.to_string())
            }
        };
        match value {
            V::Null | V::StringOpt(None) | V::Now | V::NowPlus(_) => {
                Self::Null(OracleType::Varchar2(1))
            }
            V::String(value) | V::StringOpt(Some(value)) => text(value),
            V::Bool(value) | V::BoolOpt(Some(value)) => {
                if boolean {
                    Self::Bool(*value)
                } else {
                    Self::Int(i64::from(*value))
                }
            }
            V::BoolOpt(None) => {
                if boolean {
                    Self::Null(OracleType::Boolean)
                } else {
                    number_null()
                }
            }
            V::Int8(value) | V::Int8Opt(Some(value)) => Self::Int(i64::from(*value)),
            V::Int16(value) | V::Int16Opt(Some(value)) => Self::Int(i64::from(*value)),
            V::Int32(value) | V::Int32Opt(Some(value)) => Self::Int(i64::from(*value)),
            V::Int64(value) | V::Int64Opt(Some(value)) => Self::Int(*value),
            V::UInt8(value) | V::UInt8Opt(Some(value)) => Self::Int(i64::from(*value)),
            V::UInt16(value) | V::UInt16Opt(Some(value)) => Self::Int(i64::from(*value)),
            V::UInt32(value) | V::UInt32Opt(Some(value)) => Self::Int(i64::from(*value)),
            V::UInt64(value) | V::UInt64Opt(Some(value)) => Self::UInt(*value),
            V::Int8Opt(None)
            | V::Int16Opt(None)
            | V::Int32Opt(None)
            | V::Int64Opt(None)
            | V::UInt8Opt(None)
            | V::UInt16Opt(None)
            | V::UInt32Opt(None)
            | V::UInt64Opt(None) => number_null(),
            V::Real64(value) | V::Real64Opt(Some(value)) => Self::Double(*value),
            V::Real64Opt(None) => Self::Null(OracleType::BinaryDouble),
            V::Real32(value) | V::Real32Opt(Some(value)) => Self::Float(*value),
            V::Real32Opt(None) => Self::Null(OracleType::BinaryFloat),
            // Decimals keep every digit: Oracle converts the text to NUMBER.
            #[cfg(feature = "decimal")]
            V::Decimal(value) | V::DecimalOpt(Some(value)) => Self::Text(value.to_string()),
            #[cfg(feature = "decimal")]
            V::DecimalOpt(None) => number_null(),
            #[cfg(feature = "uuid")]
            V::Uuid(value) | V::UuidOpt(Some(value)) => Self::Text(value.to_string()),
            #[cfg(feature = "uuid")]
            V::UuidOpt(None) => Self::Null(OracleType::Varchar2(36)),
            V::DateTime(value) => Self::DateTime(*value),
            V::Date(value) | V::DateOpt(Some(value)) => Self::Date(*value),
            V::DateOpt(None) => Self::Null(OracleType::Date),
            V::Time(value) | V::TimeOpt(Some(value)) => {
                Self::Text(value.format("%H:%M:%S%.f").to_string())
            }
            V::TimeOpt(None) => Self::Null(OracleType::Varchar2(1)),
            V::DateTimeTz(value) | V::DateTimeTzOpt(Some(value)) => Self::Zoned(*value),
            V::DateTimeTzOpt(None) => Self::Null(OracleType::TimestampTZ(9)),
            V::Bytes(value) | V::BytesOpt(Some(value)) => {
                if value.len() > MAX_RAW_BIND {
                    Self::LongBytes(value.clone())
                } else {
                    Self::Bytes(value.clone())
                }
            }
            V::BytesOpt(None) => Self::Null(OracleType::Raw(1)),
            #[cfg(feature = "json")]
            V::Json(value) | V::JsonOpt(Some(value)) => text(&value.to_string()),
            #[cfg(feature = "json")]
            V::JsonOpt(None) => Self::Null(OracleType::Varchar2(1)),
            V::Other { text: value, .. } => text(value),
        }
    }

    /// The value as the driver binds it.
    pub(super) fn to_sql(&self) -> Box<dyn ToSql + '_> {
        match self {
            Self::Null(oracle_type) => Box::new(oracle_type.clone()),
            Self::Text(value) => Box::new(Text(value)),
            Self::LongText(value) => Box::new(LongText(value)),
            Self::Int(value) => Box::new(*value),
            Self::UInt(value) => Box::new(*value),
            Self::Float(value) => Box::new(*value),
            Self::Double(value) => Box::new(*value),
            Self::Bool(value) => Box::new(*value),
            Self::Bytes(value) => Box::new(value.as_slice()),
            Self::LongBytes(value) => Box::new(LongBytes(value)),
            Self::DateTime(value) => Box::new(*value),
            Self::Date(value) => Box::new(*value),
            Self::Zoned(value) => Box::new(*value),
        }
    }
}

/// Text bound as a `VARCHAR2` (the driver binds Rust strings as
/// `NVARCHAR2`, which Oracle won't mix with `VARCHAR2` in `COALESCE`,
/// `CASE`, `UNION` and the like).
struct Text<'a>(&'a str);

impl ToSql for Text<'_> {
    fn oratype(&self, _conn: &oracle::Connection) -> oracle::Result<OracleType> {
        Ok(OracleType::Varchar2(
            u32::try_from(self.0.len()).unwrap_or(u32::MAX).max(1),
        ))
    }

    fn to_sql(&self, value: &mut SqlValue) -> oracle::Result<()> {
        value.set(&self.0)
    }
}

/// Text bound as a `CLOB`.
struct LongText<'a>(&'a str);

impl ToSql for LongText<'_> {
    fn oratype(&self, _conn: &oracle::Connection) -> oracle::Result<OracleType> {
        Ok(OracleType::CLOB)
    }

    fn to_sql(&self, value: &mut SqlValue) -> oracle::Result<()> {
        value.set(&self.0)
    }
}

/// Bytes bound as a `BLOB`.
struct LongBytes<'a>(&'a [u8]);

impl ToSql for LongBytes<'_> {
    fn oratype(&self, _conn: &oracle::Connection) -> oracle::Result<OracleType> {
        Ok(OracleType::BLOB)
    }

    fn to_sql(&self, value: &mut SqlValue) -> oracle::Result<()> {
        value.set(&self.0)
    }
}

/// The SQL a `Now`/`NowPlus` value is written as (UTC, a `TIMESTAMP`), or
/// `None` for values that are bound.
pub(super) fn now_sql(value: &DatabaseValue) -> Option<String> {
    const NOW: &str = "SYS_EXTRACT_UTC(SYSTIMESTAMP)";
    match value {
        DatabaseValue::Now => Some(NOW.to_string()),
        DatabaseValue::NowPlus(interval) => {
            let mut sql = format!("({NOW}");
            let months = i64::from(interval.years) * 12 + i64::from(interval.months);
            if months != 0 {
                let _ = write!(sql, " + NUMTOYMINTERVAL({months}, 'MONTH')");
            }
            for (unit, amount) in [
                ("DAY", i64::from(interval.days)),
                ("HOUR", interval.hours),
                ("MINUTE", interval.minutes),
                ("SECOND", interval.seconds),
            ] {
                if amount != 0 {
                    let _ = write!(sql, " + NUMTODSINTERVAL({amount}, '{unit}')");
                }
            }
            if interval.nanos != 0 {
                let _ = write!(
                    sql,
                    " + NUMTODSINTERVAL({} / 1000000000, 'SECOND')",
                    interval.nanos
                );
            }
            sql.push(')');
            Some(sql)
        }
        _ => None,
    }
}

/// The native type name of a result column (`NUMBER`, `VARCHAR2`,
/// `TIMESTAMP WITH TIME ZONE`, ...), without lengths.
pub(super) fn type_name(oracle_type: &OracleType) -> String {
    match oracle_type {
        OracleType::Varchar2(_) => "VARCHAR2".to_string(),
        OracleType::NVarchar2(_) => "NVARCHAR2".to_string(),
        OracleType::Char(_) => "CHAR".to_string(),
        OracleType::NChar(_) => "NCHAR".to_string(),
        OracleType::Rowid => "ROWID".to_string(),
        OracleType::Raw(_) => "RAW".to_string(),
        OracleType::BinaryFloat => "BINARY_FLOAT".to_string(),
        OracleType::BinaryDouble => "BINARY_DOUBLE".to_string(),
        OracleType::Number(..) | OracleType::Int64 | OracleType::UInt64 => "NUMBER".to_string(),
        OracleType::Float(_) => "FLOAT".to_string(),
        OracleType::Date => "DATE".to_string(),
        OracleType::Timestamp(_) => "TIMESTAMP".to_string(),
        OracleType::TimestampTZ(_) => "TIMESTAMP WITH TIME ZONE".to_string(),
        OracleType::TimestampLTZ(_) => "TIMESTAMP WITH LOCAL TIME ZONE".to_string(),
        OracleType::IntervalDS(..) => "INTERVAL DAY TO SECOND".to_string(),
        OracleType::IntervalYM(_) => "INTERVAL YEAR TO MONTH".to_string(),
        OracleType::CLOB => "CLOB".to_string(),
        OracleType::NCLOB => "NCLOB".to_string(),
        OracleType::BLOB => "BLOB".to_string(),
        OracleType::BFILE => "BFILE".to_string(),
        OracleType::RefCursor => "REF CURSOR".to_string(),
        OracleType::Boolean => "BOOLEAN".to_string(),
        OracleType::Object(object) => format!("{}.{}", object.schema(), object.name()),
        OracleType::Long => "LONG".to_string(),
        OracleType::LongRaw => "LONG RAW".to_string(),
        OracleType::Json => "JSON".to_string(),
        OracleType::Xml => "XMLTYPE".to_string(),
    }
}

/// A `NUMBER`'s text (Oracle's exact decimal form) as a value: an integer
/// where it fits, else a `Decimal` where it fits, else its text.
fn number_value(text: &str, integer: bool) -> DatabaseValue {
    if let Ok(value) = text.parse::<i64>() {
        return DatabaseValue::Int64(value);
    }
    // A `Decimal` holds 28 significant digits exactly; a NUMBER holds 38,
    // which keep their text rather than being rounded.
    let digits = text
        .chars()
        .filter(char::is_ascii_digit)
        .skip_while(|digit| *digit == '0')
        .count();
    #[cfg(feature = "decimal")]
    if digits <= 28
        && !text.contains(['e', 'E'])
        && let Ok(value) = text.parse::<rust_decimal::Decimal>()
    {
        return DatabaseValue::Decimal(value);
    }
    // Without the `decimal` feature, fractional numbers a float holds
    // exactly enough (15 digits) are floats.
    if !cfg!(feature = "decimal")
        && !integer
        && digits <= 15
        && let Ok(value) = text.parse::<f64>()
    {
        return DatabaseValue::Real64(value);
    }
    DatabaseValue::Other {
        type_name: "NUMBER".to_string(),
        text: text.to_string(),
    }
}

/// A result value.
pub(super) fn decode(value: &SqlValue) -> DatabaseValue {
    match value.is_null() {
        Ok(true) => return DatabaseValue::Null,
        Ok(false) => {}
        Err(error) => {
            return DatabaseValue::Other {
                type_name: "unknown".to_string(),
                text: format!("<{error}>"),
            };
        }
    }
    let Ok(oracle_type) = value.oracle_type() else {
        return text_value(value, "unknown");
    };
    let decoded = match oracle_type {
        OracleType::Varchar2(_)
        | OracleType::NVarchar2(_)
        | OracleType::Char(_)
        | OracleType::NChar(_)
        | OracleType::Long
        | OracleType::CLOB
        | OracleType::NCLOB
        | OracleType::Rowid
        | OracleType::Xml
        | OracleType::Json
        | OracleType::IntervalDS(..)
        | OracleType::IntervalYM(_) => value.get::<String>().map(DatabaseValue::String),
        OracleType::Number(precision, scale) => value
            .get::<String>()
            .map(|text| number_value(&text, *scale == 0 && *precision > 0)),
        OracleType::Int64 => value.get::<i64>().map(DatabaseValue::Int64),
        OracleType::UInt64 => value.get::<u64>().map(DatabaseValue::UInt64),
        OracleType::Float(_) | OracleType::BinaryDouble => {
            value.get::<f64>().map(DatabaseValue::Real64)
        }
        OracleType::BinaryFloat => value.get::<f32>().map(DatabaseValue::Real32),
        // A local time zone timestamp reads in the session's time zone,
        // without one.
        OracleType::Date | OracleType::Timestamp(_) | OracleType::TimestampLTZ(_) => value
            .get::<chrono::NaiveDateTime>()
            .map(DatabaseValue::DateTime),
        OracleType::TimestampTZ(_) => value
            .get::<chrono::DateTime<chrono::FixedOffset>>()
            .map(DatabaseValue::DateTimeTz),
        OracleType::Raw(_) | OracleType::LongRaw | OracleType::BLOB => {
            value.get::<Vec<u8>>().map(DatabaseValue::Bytes)
        }
        OracleType::Boolean => value.get::<bool>().map(DatabaseValue::Bool),
        OracleType::BFILE | OracleType::RefCursor | OracleType::Object(_) => {
            return text_value(value, &type_name(oracle_type));
        }
    };
    decoded.unwrap_or_else(|_| text_value(value, &type_name(oracle_type)))
}

/// `value`'s text (what the driver converts it to), or the reason it has
/// none.
fn text_value(value: &SqlValue, type_name: &str) -> DatabaseValue {
    DatabaseValue::Other {
        type_name: type_name.to_string(),
        text: value
            .get::<String>()
            .unwrap_or_else(|error| format!("<{error}>")),
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn numbers_keep_every_digit() {
        assert_eq!(number_value("42", true), DatabaseValue::Int64(42));
        assert_eq!(number_value("-7", false), DatabaseValue::Int64(-7));
        let huge = "123456789012345678901234567890123456789";
        assert!(matches!(
            number_value(huge, true),
            DatabaseValue::Other { text, .. } if text == huge
        ));
        let pi = "3.1415926535897932384626433832795029";
        assert!(matches!(
            number_value(pi, false),
            DatabaseValue::Other { text, .. } if text == pi
        ));
        #[cfg(feature = "decimal")]
        assert_eq!(
            number_value("12.50", false),
            DatabaseValue::Decimal(rust_decimal::Decimal::new(1250, 2))
        );
    }

    #[test]
    fn now_becomes_sql() {
        assert_eq!(
            now_sql(&DatabaseValue::Now).as_deref(),
            Some("SYS_EXTRACT_UTC(SYSTIMESTAMP)")
        );
        let interval = crate::sql_interval::SqlInterval {
            months: 1,
            days: 1,
            hours: -2,
            ..Default::default()
        };
        assert_eq!(
            now_sql(&DatabaseValue::NowPlus(interval)).as_deref(),
            Some(
                "(SYS_EXTRACT_UTC(SYSTIMESTAMP) + NUMTOYMINTERVAL(1, 'MONTH') + \
                 NUMTODSINTERVAL(1, 'DAY') + NUMTODSINTERVAL(-2, 'HOUR'))"
            )
        );
        assert_eq!(now_sql(&DatabaseValue::Int32(1)), None);
    }

    #[test]
    fn values_bind_as_oracle_types() {
        assert!(matches!(
            Bind::new(&DatabaseValue::Bool(true), false),
            Bind::Int(1)
        ));
        assert!(matches!(
            Bind::new(&DatabaseValue::Bool(true), true),
            Bind::Bool(true)
        ));
        assert!(matches!(
            Bind::new(&DatabaseValue::String("x".repeat(5000)), true),
            Bind::LongText(_)
        ));
        assert!(matches!(
            Bind::new(&DatabaseValue::Bytes(vec![1; 3000]), true),
            Bind::LongBytes(_)
        ));
        assert!(matches!(
            Bind::new(&DatabaseValue::Int64Opt(None), true),
            Bind::Null(OracleType::Number(0, 0))
        ));
    }
}
