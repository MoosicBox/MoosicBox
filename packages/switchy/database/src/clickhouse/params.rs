//! Query parameters for `ClickHouse`: `$1`, `$2`, ... in the statement
//! become `ClickHouse`'s own typed substitutions (`{p1:Int64}`), and each
//! value is sent as a `param_p1` HTTP parameter, which the server parses as
//! that type. Values never become part of the statement text.
//!
//! The only values written into the statement are the ones that are SQL by
//! nature: [`DatabaseValue::Now`] and [`DatabaseValue::NowPlus`] become
//! `now64(6)` expressions.

use std::fmt::Write as _;

use crate::{DatabaseError, DatabaseValue, sql_interval::SqlInterval};

/// A statement with `$n` placeholders replaced, and the values to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BoundStatement {
    /// The statement to send.
    pub sql: String,
    /// `(name, value)` for each `param_<name>` HTTP parameter, the value in
    /// `ClickHouse`'s escaped text form (raw bytes: `String` columns hold
    /// bytes).
    pub params: Vec<(String, Vec<u8>)>,
}

/// Replace the `$n` placeholders of `sql` outside string literals, quoted
/// identifiers and comments with typed substitutions for `params[n - 1]`.
///
/// # Errors
///
/// * [`DatabaseError::InvalidQuery`] if a placeholder has no parameter, or
///   a string, identifier or comment is not closed
pub(super) fn bind(sql: &str, params: &[DatabaseValue]) -> Result<BoundStatement, DatabaseError> {
    if params.is_empty() {
        return Ok(BoundStatement {
            sql: sql.to_string(),
            params: Vec::new(),
        });
    }
    let bytes = sql.as_bytes();
    let mut out = String::with_capacity(sql.len() + params.len() * 12);
    let mut used = vec![false; params.len()];
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            quote @ (b'\'' | b'"' | b'`') => index = skip_quoted(bytes, index, quote)?,
            b'-' if bytes.get(index + 1) == Some(&b'-') => index = skip_line(bytes, index),
            b'#' => index = skip_line(bytes, index),
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index = skip_block_comment(bytes, index)?;
            }
            b'$' if bytes.get(index + 1).is_some_and(u8::is_ascii_digit) => {
                let digits = bytes[index + 1..]
                    .iter()
                    .take_while(|byte| byte.is_ascii_digit())
                    .count();
                let end = index + 1 + digits;
                let number: usize = sql[index + 1..end].parse().map_err(|_| {
                    DatabaseError::InvalidQuery(format!("bad placeholder in {sql}"))
                })?;
                let value = number
                    .checked_sub(1)
                    .and_then(|position| params.get(position))
                    .ok_or_else(|| {
                        DatabaseError::InvalidQuery(format!(
                            "placeholder ${number} has no parameter ({} given)",
                            params.len()
                        ))
                    })?;
                out.push_str(&sql[start..index]);
                out.push_str(&substitution(number, value));
                used[number - 1] = true;
                start = end;
                index = end;
            }
            b'$' => index = skip_heredoc(bytes, index)?,
            _ => index += 1,
        }
    }
    out.push_str(&sql[start..]);
    let params = params
        .iter()
        .enumerate()
        .filter(|(position, value)| used[*position] && sql_expression(value).is_none())
        .map(|(position, value)| (format!("p{}", position + 1), escaped(value)))
        .collect();
    Ok(BoundStatement { sql: out, params })
}

/// The text replacing placeholder `$number` for `value`.
fn substitution(number: usize, value: &DatabaseValue) -> String {
    sql_expression(value).unwrap_or_else(|| format!("{{p{number}:{}}}", type_name(value)))
}

/// SQL written in place of values that are expressions by nature.
fn sql_expression(value: &DatabaseValue) -> Option<String> {
    match value {
        DatabaseValue::Now => Some("now64(6)".to_string()),
        DatabaseValue::NowPlus(interval) => Some(now_plus(interval)),
        _ => None,
    }
}

/// `now64(6)` moved by `interval`, as `ClickHouse` interval arithmetic
/// (only integers are written).
fn now_plus(interval: &SqlInterval) -> String {
    let mut sql = "now64(6)".to_string();
    let parts: [(i64, &str); 6] = [
        (i64::from(interval.years), "YEAR"),
        (i64::from(interval.months), "MONTH"),
        (i64::from(interval.days), "DAY"),
        (interval.hours, "HOUR"),
        (interval.minutes, "MINUTE"),
        (interval.seconds, "SECOND"),
    ];
    for (amount, unit) in parts {
        if amount != 0 {
            let _ = write!(sql, " + INTERVAL {amount} {unit}");
        }
    }
    if interval.nanos != 0 {
        let _ = write!(sql, " + INTERVAL {} MICROSECOND", interval.nanos / 1_000);
    }
    sql
}

/// The `ClickHouse` type a value is sent as.
fn type_name(value: &DatabaseValue) -> String {
    use DatabaseValue as V;

    let nullable = |base: &str| format!("Nullable({base})");
    match value {
        V::Null => nullable("String"),
        V::String(_) | V::Bytes(_) => "String".to_string(),
        V::StringOpt(_) | V::BytesOpt(_) => nullable("String"),
        V::Bool(_) => "Bool".to_string(),
        V::BoolOpt(_) => nullable("Bool"),
        V::Int8(_) | V::Int16(_) | V::Int32(_) | V::Int64(_) => "Int64".to_string(),
        V::Int8Opt(_) | V::Int16Opt(_) | V::Int32Opt(_) | V::Int64Opt(_) => nullable("Int64"),
        V::UInt8(_) | V::UInt16(_) | V::UInt32(_) | V::UInt64(_) => "UInt64".to_string(),
        V::UInt8Opt(_) | V::UInt16Opt(_) | V::UInt32Opt(_) | V::UInt64Opt(_) => nullable("UInt64"),
        V::Real64(_) => "Float64".to_string(),
        V::Real64Opt(_) => nullable("Float64"),
        V::Real32(_) => "Float32".to_string(),
        V::Real32Opt(_) => nullable("Float32"),
        #[cfg(feature = "decimal")]
        V::Decimal(decimal) => format!("Decimal(38, {})", decimal.scale()),
        #[cfg(feature = "decimal")]
        V::DecimalOpt(decimal) => nullable(&format!(
            "Decimal(38, {})",
            decimal.map_or(0, |decimal| decimal.scale())
        )),
        #[cfg(feature = "uuid")]
        V::Uuid(_) => "UUID".to_string(),
        #[cfg(feature = "uuid")]
        V::UuidOpt(_) => nullable("UUID"),
        V::DateTime(_) | V::Now | V::NowPlus(_) => "DateTime64(6)".to_string(),
        V::DateTimeTz(_) => "DateTime64(6, 'UTC')".to_string(),
        V::DateTimeTzOpt(_) => nullable("DateTime64(6, 'UTC')"),
        V::Date(_) => "Date32".to_string(),
        V::DateOpt(_) => nullable("Date32"),
        // `ClickHouse`'s `Time` type is recent; text compares and converts.
        V::Time(_) | V::Other { .. } => "String".to_string(),
        V::TimeOpt(_) => nullable("String"),
        #[cfg(feature = "json")]
        V::Json(_) => "String".to_string(),
        #[cfg(feature = "json")]
        V::JsonOpt(_) => nullable("String"),
    }
}

/// A value in `ClickHouse`'s escaped text form, as query parameters are
/// parsed: `\N` is `NULL`, and backslashes, tabs and line breaks are
/// escaped.
fn escaped(value: &DatabaseValue) -> Vec<u8> {
    use DatabaseValue as V;

    let text = |text: &str| escape(text.as_bytes());
    let null = || b"\\N".to_vec();
    let or_null = |value: Option<String>| value.map_or_else(null, |value| text(&value));
    match value {
        V::Null | V::Now | V::NowPlus(_) => null(),
        V::String(value) => text(value),
        V::StringOpt(value) => or_null(value.clone()),
        V::Bytes(bytes) => escape(bytes),
        V::BytesOpt(bytes) => bytes.as_deref().map_or_else(null, escape),
        V::Bool(value) => text(&value.to_string()),
        V::BoolOpt(value) => or_null(value.map(|value| value.to_string())),
        V::Int8(value) => text(&value.to_string()),
        V::Int8Opt(value) => or_null(value.map(|value| value.to_string())),
        V::Int16(value) => text(&value.to_string()),
        V::Int16Opt(value) => or_null(value.map(|value| value.to_string())),
        V::Int32(value) => text(&value.to_string()),
        V::Int32Opt(value) => or_null(value.map(|value| value.to_string())),
        V::Int64(value) => text(&value.to_string()),
        V::Int64Opt(value) => or_null(value.map(|value| value.to_string())),
        V::UInt8(value) => text(&value.to_string()),
        V::UInt8Opt(value) => or_null(value.map(|value| value.to_string())),
        V::UInt16(value) => text(&value.to_string()),
        V::UInt16Opt(value) => or_null(value.map(|value| value.to_string())),
        V::UInt32(value) => text(&value.to_string()),
        V::UInt32Opt(value) => or_null(value.map(|value| value.to_string())),
        V::UInt64(value) => text(&value.to_string()),
        V::UInt64Opt(value) => or_null(value.map(|value| value.to_string())),
        V::Real64(value) => text(&float(*value)),
        V::Real64Opt(value) => or_null(value.map(float)),
        V::Real32(value) => text(&float(f64::from(*value))),
        V::Real32Opt(value) => or_null(value.map(|value| float(f64::from(value)))),
        #[cfg(feature = "decimal")]
        V::Decimal(value) => text(&value.to_string()),
        #[cfg(feature = "decimal")]
        V::DecimalOpt(value) => or_null(value.map(|value| value.to_string())),
        #[cfg(feature = "uuid")]
        V::Uuid(value) => text(&value.to_string()),
        #[cfg(feature = "uuid")]
        V::UuidOpt(value) => or_null(value.map(|value| value.to_string())),
        V::DateTime(value) => text(&value.format("%Y-%m-%d %H:%M:%S%.6f").to_string()),
        V::DateTimeTz(value) => text(&utc(value)),
        V::DateTimeTzOpt(value) => or_null(value.as_ref().map(utc)),
        V::Date(value) => text(&value.format("%Y-%m-%d").to_string()),
        V::DateOpt(value) => or_null(value.map(|value| value.format("%Y-%m-%d").to_string())),
        V::Time(value) => text(&value.format("%H:%M:%S%.f").to_string()),
        V::TimeOpt(value) => or_null(value.map(|value| value.format("%H:%M:%S%.f").to_string())),
        V::Other { text: value, .. } => text(value),
        #[cfg(feature = "json")]
        V::Json(value) => text(&value.to_string()),
        #[cfg(feature = "json")]
        V::JsonOpt(value) => or_null(value.as_ref().map(ToString::to_string)),
    }
}

/// A float as `ClickHouse` reads it (`nan`, `inf`, `-inf` included).
fn float(value: f64) -> String {
    if value.is_nan() {
        "nan".to_string()
    } else if value.is_infinite() {
        if value > 0.0 { "inf" } else { "-inf" }.to_string()
    } else {
        value.to_string()
    }
}

/// A zoned timestamp as UTC wall-clock time (sent as `DateTime64(6, 'UTC')`).
fn utc(value: &chrono::DateTime<chrono::FixedOffset>) -> String {
    value
        .naive_utc()
        .format("%Y-%m-%d %H:%M:%S%.6f")
        .to_string()
}

/// Escape bytes as `ClickHouse`'s escaped text format expects.
fn escape(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    for &byte in bytes {
        match byte {
            b'\\' => out.extend_from_slice(b"\\\\"),
            b'\t' => out.extend_from_slice(b"\\t"),
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            0 => out.extend_from_slice(b"\\0"),
            other => out.push(other),
        }
    }
    out
}

/// The index past the literal or identifier quoted by `quote` that starts at
/// `start` (a doubled quote or a backslash escapes, as in `ClickHouse`).
fn skip_quoted(bytes: &[u8], start: usize, quote: u8) -> Result<usize, DatabaseError> {
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            byte if byte == quote => {
                if bytes.get(index + 1) == Some(&quote) {
                    index += 2;
                } else {
                    return Ok(index + 1);
                }
            }
            _ => index += 1,
        }
    }
    Err(DatabaseError::InvalidQuery(format!(
        "unterminated {} in the statement",
        if quote == b'\'' {
            "string literal"
        } else {
            "quoted identifier"
        }
    )))
}

/// The index past the line comment starting at `start`.
fn skip_line(bytes: &[u8], start: usize) -> usize {
    bytes[start..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(bytes.len(), |offset| start + offset + 1)
}

/// The index past the (possibly nested) block comment starting at `start`.
fn skip_block_comment(bytes: &[u8], start: usize) -> Result<usize, DatabaseError> {
    let mut depth = 0_usize;
    let mut index = start;
    while index + 1 < bytes.len() {
        match (bytes[index], bytes[index + 1]) {
            (b'/', b'*') => {
                depth += 1;
                index += 2;
            }
            (b'*', b'/') => {
                depth -= 1;
                index += 2;
                if depth == 0 {
                    return Ok(index);
                }
            }
            _ => index += 1,
        }
    }
    Err(DatabaseError::InvalidQuery(
        "unterminated comment in the statement".to_string(),
    ))
}

/// The index past a heredoc string (`$tag$ ... $tag$`) starting at `start`,
/// or past the `$` when it doesn't start one.
fn skip_heredoc(bytes: &[u8], start: usize) -> Result<usize, DatabaseError> {
    let tag_len = bytes[start + 1..]
        .iter()
        .take_while(|byte| byte.is_ascii_alphanumeric() || **byte == b'_')
        .count();
    let close = start + 1 + tag_len;
    if bytes.get(close) != Some(&b'$') {
        return Ok(start + 1);
    }
    let delimiter = &bytes[start..=close];
    bytes[close + 1..]
        .windows(delimiter.len())
        .position(|window| window == delimiter)
        .map(|offset| close + 1 + offset + delimiter.len())
        .ok_or_else(|| {
            DatabaseError::InvalidQuery("unterminated heredoc string in the statement".to_string())
        })
}

#[cfg(test)]
// `{p1:String}` is a ClickHouse query parameter, not a format argument.
#[allow(clippy::literal_string_with_formatting_args)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn placeholders_become_typed_substitutions_and_values_stay_out() {
        let bound = bind(
            "SELECT * FROM albums WHERE title = $1 AND id > $2 AND note = '$1' -- $2\n AND x = $1",
            &[
                DatabaseValue::String("it's \\ a\ttab".to_string()),
                DatabaseValue::Int64(7),
            ],
        )
        .unwrap();
        assert_eq!(
            bound.sql,
            "SELECT * FROM albums WHERE title = {p1:String} AND id > {p2:Int64} AND note = '$1' -- $2\n AND x = {p1:String}"
        );
        assert_eq!(
            bound.params,
            vec![
                ("p1".to_string(), b"it's \\\\ a\\ttab".to_vec()),
                ("p2".to_string(), b"7".to_vec()),
            ]
        );
    }

    #[test]
    fn quoted_identifiers_comments_and_heredocs_keep_their_dollars() {
        let bound = bind(
            "SELECT `a$1`, \"b$1\", $$ $1 $$, $t$ $1 $t$ /* $1 /* $1 */ */ FROM t WHERE c = $1",
            &[DatabaseValue::Null],
        )
        .unwrap();
        assert_eq!(
            bound.sql,
            "SELECT `a$1`, \"b$1\", $$ $1 $$, $t$ $1 $t$ /* $1 /* $1 */ */ FROM t WHERE c = {p1:Nullable(String)}"
        );
        assert_eq!(bound.params, vec![("p1".to_string(), b"\\N".to_vec())]);
    }

    #[test]
    fn statements_without_parameters_are_sent_unchanged() {
        // `?` is ClickHouse's ternary operator, never a placeholder.
        let sql = "SELECT a ? 1 : 2, 'unterminated";
        assert_eq!(bind(sql, &[]).unwrap().sql, sql);
    }

    #[test]
    fn missing_parameters_and_unterminated_text_are_errors() {
        assert!(bind("SELECT $2", &[DatabaseValue::Int64(1)]).is_err());
        assert!(bind("SELECT $0", &[DatabaseValue::Int64(1)]).is_err());
        assert!(bind("SELECT 'a, $1", &[DatabaseValue::Int64(1)]).is_err());
    }

    #[test]
    fn now_is_an_expression() {
        let bound = bind(
            "SELECT $1, $2",
            &[
                DatabaseValue::Now,
                DatabaseValue::NowPlus(SqlInterval {
                    days: -1,
                    hours: 2,
                    ..SqlInterval::new()
                }),
            ],
        )
        .unwrap();
        assert_eq!(
            bound.sql,
            "SELECT now64(6), now64(6) + INTERVAL -1 DAY + INTERVAL 2 HOUR"
        );
        assert_eq!(bound.params.len(), 0);
    }

    #[test]
    fn floats_and_bytes_use_clickhouse_text() {
        assert_eq!(escaped(&DatabaseValue::Real64(f64::NAN)), b"nan".to_vec());
        assert_eq!(
            escaped(&DatabaseValue::Real64(f64::NEG_INFINITY)),
            b"-inf".to_vec()
        );
        assert_eq!(
            escaped(&DatabaseValue::Bytes(vec![0xff, b'\n', 0])),
            vec![0xff, b'\\', b'n', b'\\', b'0']
        );
    }
}
