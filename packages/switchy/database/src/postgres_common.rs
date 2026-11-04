//! Shared `PostgreSQL` helpers used by both the raw (`tokio-postgres`) and
//! sqlx backends: catalog SQL text, introspection logic, type mapping and
//! driver-independent decoding of binary wire values.
//!
//! # Value decoding
//!
//! Both drivers fetch results in the binary wire format. Types the backends
//! decode natively are handled in the backend; everything else is passed to
//! [`decode_binary_value`], which never fails: types without a dedicated
//! [`DatabaseValue`] variant become [`DatabaseValue::Other`] carrying the
//! canonical type name and a text rendering that matches `PostgreSQL`'s own
//! text output for common types (arrays, intervals, network types, ranges,
//! enums, numerics, ...). Unrecognized binary payloads are rendered as
//! `\x`-prefixed hex.
//!
//! `TIMESTAMP WITH TIME ZONE` values decode to [`DatabaseValue::DateTime`]
//! normalized to UTC (existing behaviour): `PostgreSQL` stores `timestamptz`
//! as a UTC instant, so no offset information is lost.

#![allow(dead_code)]

use std::fmt::Write as _;

use crate::DatabaseValue;

// ---------------------------------------------------------------------------
// Type OIDs and names
// ---------------------------------------------------------------------------

/// Built-in type OIDs used for decoding.
pub mod oid {
    #![allow(missing_docs)]
    pub const BOOL: u32 = 16;
    pub const BYTEA: u32 = 17;
    pub const CHAR: u32 = 18;
    pub const NAME: u32 = 19;
    pub const INT8: u32 = 20;
    pub const INT2: u32 = 21;
    pub const INT4: u32 = 23;
    pub const REGPROC: u32 = 24;
    pub const TEXT: u32 = 25;
    pub const OID: u32 = 26;
    pub const XID: u32 = 28;
    pub const CID: u32 = 29;
    pub const JSON: u32 = 114;
    pub const XML: u32 = 142;
    pub const POINT: u32 = 600;
    pub const CIDR: u32 = 650;
    pub const FLOAT4: u32 = 700;
    pub const FLOAT8: u32 = 701;
    pub const UNKNOWN: u32 = 705;
    pub const MACADDR8: u32 = 774;
    pub const MONEY: u32 = 790;
    pub const MACADDR: u32 = 829;
    pub const INET: u32 = 869;
    pub const BPCHAR: u32 = 1042;
    pub const VARCHAR: u32 = 1043;
    pub const DATE: u32 = 1082;
    pub const TIME: u32 = 1083;
    pub const TIMESTAMP: u32 = 1114;
    pub const TIMESTAMPTZ: u32 = 1184;
    pub const INTERVAL: u32 = 1186;
    pub const TIMETZ: u32 = 1266;
    pub const BIT: u32 = 1560;
    pub const VARBIT: u32 = 1562;
    pub const NUMERIC: u32 = 1700;
    pub const REGCLASS: u32 = 2205;
    pub const UUID: u32 = 2950;
    pub const JSONB: u32 = 3802;
    pub const INT4RANGE: u32 = 3904;
    pub const NUMRANGE: u32 = 3906;
    pub const TSRANGE: u32 = 3908;
    pub const TSTZRANGE: u32 = 3910;
    pub const DATERANGE: u32 = 3912;
    pub const INT8RANGE: u32 = 3926;
}

/// `pg_type.typname` of well-known built-in scalar types.
const BUILTIN_TYPES: &[(u32, &str)] = &[
    (oid::BOOL, "bool"),
    (oid::BYTEA, "bytea"),
    (oid::CHAR, "char"),
    (oid::NAME, "name"),
    (oid::INT8, "int8"),
    (oid::INT2, "int2"),
    (oid::INT4, "int4"),
    (oid::REGPROC, "regproc"),
    (oid::TEXT, "text"),
    (oid::OID, "oid"),
    (oid::XID, "xid"),
    (oid::CID, "cid"),
    (oid::JSON, "json"),
    (oid::XML, "xml"),
    (oid::POINT, "point"),
    (oid::CIDR, "cidr"),
    (oid::FLOAT4, "float4"),
    (oid::FLOAT8, "float8"),
    (oid::UNKNOWN, "unknown"),
    (oid::MACADDR8, "macaddr8"),
    (oid::MONEY, "money"),
    (oid::MACADDR, "macaddr"),
    (oid::INET, "inet"),
    (oid::BPCHAR, "bpchar"),
    (oid::VARCHAR, "varchar"),
    (oid::DATE, "date"),
    (oid::TIME, "time"),
    (oid::TIMESTAMP, "timestamp"),
    (oid::TIMESTAMPTZ, "timestamptz"),
    (oid::INTERVAL, "interval"),
    (oid::TIMETZ, "timetz"),
    (oid::BIT, "bit"),
    (oid::VARBIT, "varbit"),
    (oid::NUMERIC, "numeric"),
    (oid::REGCLASS, "regclass"),
    (oid::UUID, "uuid"),
    (oid::JSONB, "jsonb"),
    (oid::INT4RANGE, "int4range"),
    (oid::NUMRANGE, "numrange"),
    (oid::TSRANGE, "tsrange"),
    (oid::TSTZRANGE, "tstzrange"),
    (oid::DATERANGE, "daterange"),
    (oid::INT8RANGE, "int8range"),
];

/// Built-in array type OIDs and their element type OIDs.
const BUILTIN_ARRAYS: &[(u32, u32)] = &[
    (1000, oid::BOOL),
    (1001, oid::BYTEA),
    (1002, oid::CHAR),
    (1003, oid::NAME),
    (1016, oid::INT8),
    (1005, oid::INT2),
    (1007, oid::INT4),
    (1009, oid::TEXT),
    (1028, oid::OID),
    (199, oid::JSON),
    (143, oid::XML),
    (1017, oid::POINT),
    (651, oid::CIDR),
    (1021, oid::FLOAT4),
    (1022, oid::FLOAT8),
    (775, oid::MACADDR8),
    (791, oid::MONEY),
    (1040, oid::MACADDR),
    (1041, oid::INET),
    (1014, oid::BPCHAR),
    (1015, oid::VARCHAR),
    (1182, oid::DATE),
    (1183, oid::TIME),
    (1115, oid::TIMESTAMP),
    (1185, oid::TIMESTAMPTZ),
    (1187, oid::INTERVAL),
    (1270, oid::TIMETZ),
    (1561, oid::BIT),
    (1563, oid::VARBIT),
    (1231, oid::NUMERIC),
    (2951, oid::UUID),
    (3807, oid::JSONB),
    (3905, oid::INT4RANGE),
    (3907, oid::NUMRANGE),
    (3909, oid::TSRANGE),
    (3911, oid::TSTZRANGE),
    (3913, oid::DATERANGE),
    (3927, oid::INT8RANGE),
];

fn builtin_type_name(type_oid: u32) -> Option<&'static str> {
    BUILTIN_TYPES
        .iter()
        .find(|(o, _)| *o == type_oid)
        .map(|(_, name)| *name)
}

fn builtin_array_element(type_oid: u32) -> Option<u32> {
    BUILTIN_ARRAYS
        .iter()
        .find(|(o, _)| *o == type_oid)
        .map(|(_, elem)| *elem)
}

/// Canonical type name for a result column or [`DatabaseValue::Other`] value.
///
/// Built-in types use their `pg_type.typname` (e.g. `int4`, `timestamptz`);
/// arrays are rendered as `<element>[]` (e.g. `int4[]`); other types use the
/// driver-reported name (user-defined enums, domains, extension types).
#[must_use]
pub fn canonical_type_name(type_oid: Option<u32>, driver_name: &str) -> String {
    if let Some(type_oid) = type_oid {
        if let Some(name) = builtin_type_name(type_oid) {
            return name.to_string();
        }
        if let Some(elem) = builtin_array_element(type_oid)
            && let Some(name) = builtin_type_name(elem)
        {
            return format!("{name}[]");
        }
    }
    driver_name
        .strip_prefix('_')
        .filter(|rest| !rest.is_empty())
        .map_or_else(|| driver_name.to_string(), |rest| format!("{rest}[]"))
}

// ---------------------------------------------------------------------------
// Binary value decoding
// ---------------------------------------------------------------------------

/// Days between 1970-01-01 and the `PostgreSQL` epoch (2000-01-01).
const PG_EPOCH_DAYS_FROM_UNIX: i64 = 10_957;

fn pg_epoch() -> chrono::NaiveDateTime {
    chrono::NaiveDate::from_ymd_opt(2000, 1, 1)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .unwrap_or_default()
}

fn read_i16(bytes: &[u8], at: usize) -> Option<i16> {
    bytes
        .get(at..at + 2)
        .map(|b| i16::from_be_bytes([b[0], b[1]]))
}

fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
    bytes
        .get(at..at + 2)
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
}

fn read_i32(bytes: &[u8], at: usize) -> Option<i32> {
    bytes
        .get(at..at + 4)
        .map(|b| i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    bytes
        .get(at..at + 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn read_i64(bytes: &[u8], at: usize) -> Option<i64> {
    bytes.get(at..at + 8).map(|b| {
        let mut buf = [0u8; 8];
        buf.copy_from_slice(b);
        i64::from_be_bytes(buf)
    })
}

fn exact<T>(bytes: &[u8], len: usize, value: Option<T>) -> Option<T> {
    if bytes.len() == len { value } else { None }
}

/// Render bytes as `PostgreSQL` hex `bytea` text (`\x0102ff`).
#[must_use]
pub fn hex_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(2 + bytes.len() * 2);
    out.push_str("\\x");
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Decode a binary `date` value.
#[must_use]
pub fn binary_date(bytes: &[u8]) -> Option<chrono::NaiveDate> {
    let days = exact(bytes, 4, read_i32(bytes, 0))?;
    if days == i32::MAX || days == i32::MIN {
        return None;
    }
    pg_epoch()
        .date()
        .checked_add_signed(chrono::TimeDelta::days(i64::from(days)))
}

/// Decode a binary `time` value.
#[must_use]
pub fn binary_time(bytes: &[u8]) -> Option<chrono::NaiveTime> {
    let micros = exact(bytes, 8, read_i64(bytes, 0))?;
    let secs = u32::try_from(micros.div_euclid(1_000_000)).ok()?;
    let nanos = u32::try_from(micros.rem_euclid(1_000_000) * 1_000).ok()?;
    chrono::NaiveTime::from_num_seconds_from_midnight_opt(secs, nanos)
}

/// Decode a binary `timestamp`/`timestamptz` value (UTC for `timestamptz`).
#[must_use]
pub fn binary_timestamp(bytes: &[u8]) -> Option<chrono::NaiveDateTime> {
    let micros = exact(bytes, 8, read_i64(bytes, 0))?;
    if micros == i64::MAX || micros == i64::MIN {
        return None;
    }
    pg_epoch().checked_add_signed(chrono::TimeDelta::microseconds(micros))
}

fn format_time(time: chrono::NaiveTime) -> String {
    use chrono::Timelike as _;
    if time.nanosecond() == 0 {
        time.format("%H:%M:%S").to_string()
    } else {
        let micros = time.format("%H:%M:%S%.6f").to_string();
        micros.trim_end_matches('0').to_string()
    }
}

fn format_timestamp(ts: chrono::NaiveDateTime) -> String {
    format!(
        "{} {}",
        ts.date().format("%Y-%m-%d"),
        format_time(ts.time())
    )
}

fn render_date(bytes: &[u8]) -> Option<String> {
    match exact(bytes, 4, read_i32(bytes, 0))? {
        i32::MAX => Some("infinity".to_string()),
        i32::MIN => Some("-infinity".to_string()),
        _ => binary_date(bytes).map(|d| d.format("%Y-%m-%d").to_string()),
    }
}

fn render_timestamp(bytes: &[u8], tz: bool) -> Option<String> {
    match exact(bytes, 8, read_i64(bytes, 0))? {
        i64::MAX => Some("infinity".to_string()),
        i64::MIN => Some("-infinity".to_string()),
        _ => binary_timestamp(bytes).map(|ts| {
            let mut out = format_timestamp(ts);
            if tz {
                out.push_str("+00");
            }
            out
        }),
    }
}

fn render_timetz(bytes: &[u8]) -> Option<String> {
    if bytes.len() != 12 {
        return None;
    }
    let time = binary_time(&bytes[..8])?;
    // PostgreSQL stores the zone as seconds *west* of UTC.
    let zone = -read_i32(bytes, 8)?;
    let sign = if zone < 0 { '-' } else { '+' };
    let zone = zone.unsigned_abs();
    let (hours, minutes, seconds) = (zone / 3600, (zone % 3600) / 60, zone % 60);
    let mut out = format!("{}{sign}{hours:02}", format_time(time));
    if minutes != 0 || seconds != 0 {
        let _ = write!(out, ":{minutes:02}");
    }
    if seconds != 0 {
        let _ = write!(out, ":{seconds:02}");
    }
    Some(out)
}

/// Render an interval from its binary components in `PostgreSQL`'s default
/// (`postgres`) interval style, e.g. `1 year 2 mons 3 days 04:05:06.5`.
#[must_use]
pub fn format_interval(months: i32, days: i32, micros: i64) -> String {
    fn unit(out: &mut Vec<String>, value: i64, singular: &str, plural: &str) {
        if value != 0 {
            out.push(format!(
                "{value} {}",
                if value.abs() == 1 { singular } else { plural }
            ));
        }
    }
    let mut parts = Vec::new();
    unit(&mut parts, i64::from(months / 12), "year", "years");
    unit(&mut parts, i64::from(months % 12), "mon", "mons");
    unit(&mut parts, i64::from(days), "day", "days");
    if micros != 0 || parts.is_empty() {
        let sign = if micros < 0 { "-" } else { "" };
        let total = micros.unsigned_abs();
        let (secs, frac) = (total / 1_000_000, total % 1_000_000);
        let mut time = format!(
            "{sign}{:02}:{:02}:{:02}",
            secs / 3600,
            (secs % 3600) / 60,
            secs % 60
        );
        if frac != 0 {
            let frac = format!("{frac:06}");
            let _ = write!(time, ".{}", frac.trim_end_matches('0'));
        }
        parts.push(time);
    }
    parts.join(" ")
}

fn render_interval(bytes: &[u8]) -> Option<String> {
    if bytes.len() != 16 {
        return None;
    }
    Some(format_interval(
        read_i32(bytes, 12)?,
        read_i32(bytes, 8)?,
        read_i64(bytes, 0)?,
    ))
}

/// Render a binary `numeric` value as decimal text (`NaN`, `Infinity` included).
#[must_use]
pub fn render_numeric(bytes: &[u8]) -> Option<String> {
    let ndigits = usize::try_from(read_i16(bytes, 0)?).ok()?;
    let weight = i64::from(read_i16(bytes, 2)?);
    let sign = read_u16(bytes, 4)?;
    let dscale = usize::from(read_u16(bytes, 6)?);
    if bytes.len() != 8 + ndigits * 2 {
        return None;
    }
    match sign {
        0xC000 => return Some("NaN".to_string()),
        0xD000 => return Some("Infinity".to_string()),
        0xF000 => return Some("-Infinity".to_string()),
        0x0000 | 0x4000 => {}
        _ => return None,
    }
    let digit = |index: i64| -> i16 {
        usize::try_from(index)
            .ok()
            .filter(|i| *i < ndigits)
            .and_then(|i| read_i16(bytes, 8 + i * 2))
            .unwrap_or(0)
    };
    let mut out = String::new();
    if sign == 0x4000 {
        out.push('-');
    }
    if weight < 0 {
        out.push('0');
    } else {
        for group in 0..=weight {
            if group == 0 {
                let _ = write!(out, "{}", digit(group));
            } else {
                let _ = write!(out, "{:04}", digit(group));
            }
        }
    }
    if dscale > 0 {
        let mut frac = String::new();
        let mut group = weight + 1;
        while frac.len() < dscale {
            let _ = write!(frac, "{:04}", digit(group));
            group += 1;
        }
        frac.truncate(dscale);
        out.push('.');
        out.push_str(&frac);
    }
    Some(out)
}

fn render_inet(bytes: &[u8], is_cidr_type: bool) -> Option<String> {
    let family = *bytes.first()?;
    let bits = *bytes.get(1)?;
    let len = usize::from(*bytes.get(3)?);
    let addr = bytes.get(4..4 + len)?;
    if bytes.len() != 4 + len {
        return None;
    }
    let (text, max_bits) = match (family, len) {
        (2, 4) => (
            std::net::Ipv4Addr::new(addr[0], addr[1], addr[2], addr[3]).to_string(),
            32,
        ),
        (3, 16) => {
            let mut octets = [0u8; 16];
            octets.copy_from_slice(addr);
            (std::net::Ipv6Addr::from(octets).to_string(), 128)
        }
        _ => return None,
    };
    if is_cidr_type || bits != max_bits {
        Some(format!("{text}/{bits}"))
    } else {
        Some(text)
    }
}

fn render_mac(bytes: &[u8]) -> Option<String> {
    if bytes.len() != 6 && bytes.len() != 8 {
        return None;
    }
    Some(
        bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(":"),
    )
}

/// Render a `money` value from its binary cents representation (locale-neutral).
#[must_use]
pub fn format_money(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.unsigned_abs();
    format!("{sign}{}.{:02}", abs / 100, abs % 100)
}

fn render_bits(bytes: &[u8]) -> Option<String> {
    let len = usize::try_from(read_i32(bytes, 0)?).ok()?;
    let data = bytes.get(4..)?;
    if data.len() != len.div_ceil(8) {
        return None;
    }
    Some(
        (0..len)
            .map(|i| {
                if data[i / 8] & (0x80 >> (i % 8)) == 0 {
                    '0'
                } else {
                    '1'
                }
            })
            .collect(),
    )
}

fn render_float4(bytes: &[u8]) -> Option<String> {
    exact(bytes, 4, read_u32(bytes, 0)).map(|b| f32::from_bits(b).to_string())
}

fn render_float8(bytes: &[u8]) -> Option<String> {
    exact(bytes, 8, read_i64(bytes, 0)).map(|b| f64::from_bits(b.cast_unsigned()).to_string())
}

fn render_uuid(bytes: &[u8]) -> Option<String> {
    if bytes.len() != 16 {
        return None;
    }
    let hex = bytes.iter().fold(String::with_capacity(32), |mut acc, b| {
        let _ = write!(acc, "{b:02x}");
        acc
    });
    Some(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}

fn utf8_text(bytes: &[u8]) -> Option<String> {
    std::str::from_utf8(bytes).ok().map(ToString::to_string)
}

fn jsonb_text(bytes: &[u8]) -> Option<String> {
    match bytes.split_first() {
        Some((1, rest)) => utf8_text(rest),
        _ => None,
    }
}

const fn range_element(range_oid: u32) -> Option<u32> {
    match range_oid {
        oid::INT4RANGE => Some(oid::INT4),
        oid::INT8RANGE => Some(oid::INT8),
        oid::NUMRANGE => Some(oid::NUMERIC),
        oid::TSRANGE => Some(oid::TIMESTAMP),
        oid::TSTZRANGE => Some(oid::TIMESTAMPTZ),
        oid::DATERANGE => Some(oid::DATE),
        _ => None,
    }
}

fn render_range(bytes: &[u8], element: u32) -> Option<String> {
    let flags = *bytes.first()?;
    if flags & 0x01 != 0 {
        return Some("empty".to_string());
    }
    let mut at = 1;
    let mut bound = |infinite: bool| -> Option<String> {
        if infinite {
            return Some(String::new());
        }
        let len = usize::try_from(read_i32(bytes, at)?).ok()?;
        let value = bytes.get(at + 4..at + 4 + len)?;
        at += 4 + len;
        let text = render_binary(element, value)?;
        Some(quote_composite_element(&text))
    };
    let lower = bound(flags & 0x08 != 0)?;
    let upper = bound(flags & 0x10 != 0)?;
    if at != bytes.len() {
        return None;
    }
    Some(format!(
        "{}{lower},{upper}{}",
        if flags & 0x02 != 0 { '[' } else { '(' },
        if flags & 0x04 != 0 { ']' } else { ')' }
    ))
}

fn quote_array_element(text: &str) -> String {
    let needs_quotes = text.is_empty()
        || text.eq_ignore_ascii_case("null")
        || text
            .chars()
            .any(|c| matches!(c, ',' | '{' | '}' | '"' | '\\') || c.is_whitespace());
    if needs_quotes {
        format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        text.to_string()
    }
}

fn quote_composite_element(text: &str) -> String {
    let needs_quotes = text.is_empty()
        || text
            .chars()
            .any(|c| matches!(c, ',' | '(' | ')' | '[' | ']' | '"' | '\\') || c.is_whitespace());
    if needs_quotes {
        format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        text.to_string()
    }
}

/// Render an element of unknown type: valid UTF-8 (e.g. enum labels) as text, else hex.
fn render_unknown(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text)
            if !text
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t') =>
        {
            text.to_string()
        }
        _ => hex_bytes(bytes),
    }
}

/// Render a binary array value as a `PostgreSQL` array literal (`{1,2,NULL}`).
///
/// Returns `None` if `bytes` is not a well-formed binary array.
#[must_use]
pub fn render_binary_array(bytes: &[u8]) -> Option<String> {
    fn nest(dims: &[usize], elements: &mut std::vec::IntoIter<String>) -> String {
        let Some((first, rest)) = dims.split_first() else {
            return elements.next().unwrap_or_default();
        };
        let parts: Vec<String> = (0..*first).map(|_| nest(rest, elements)).collect();
        format!("{{{}}}", parts.join(","))
    }
    let ndim = usize::try_from(read_i32(bytes, 0)?).ok()?;
    let flags = read_i32(bytes, 4)?;
    let element = read_u32(bytes, 8)?;
    if ndim > 6 || !(flags == 0 || flags == 1) {
        return None;
    }
    let mut at = 12;
    let mut dims = Vec::with_capacity(ndim);
    for _ in 0..ndim {
        dims.push(usize::try_from(read_i32(bytes, at)?).ok()?);
        at += 8;
    }
    let total: usize = if ndim == 0 { 0 } else { dims.iter().product() };
    let mut elements = Vec::with_capacity(total);
    for _ in 0..total {
        let len = read_i32(bytes, at)?;
        at += 4;
        if len < 0 {
            elements.push("NULL".to_string());
            continue;
        }
        let len = usize::try_from(len).ok()?;
        let value = bytes.get(at..at + len)?;
        at += len;
        let text = render_binary(element, value).unwrap_or_else(|| render_unknown(value));
        elements.push(quote_array_element(&text));
    }
    if at != bytes.len() {
        return None;
    }
    if ndim == 0 {
        return Some("{}".to_string());
    }
    Some(nest(&dims, &mut elements.into_iter()))
}

fn render_record(bytes: &[u8]) -> Option<String> {
    let count = usize::try_from(read_i32(bytes, 0)?).ok()?;
    let mut at = 4;
    let mut fields = Vec::with_capacity(count);
    for _ in 0..count {
        let field_oid = read_u32(bytes, at)?;
        let len = read_i32(bytes, at + 4)?;
        at += 8;
        if len < 0 {
            fields.push(String::new());
            continue;
        }
        let len = usize::try_from(len).ok()?;
        let value = bytes.get(at..at + len)?;
        at += len;
        let text = render_binary(field_oid, value).unwrap_or_else(|| render_unknown(value));
        fields.push(quote_composite_element(&text));
    }
    if at != bytes.len() {
        return None;
    }
    Some(format!("({})", fields.join(",")))
}

/// Render a binary wire value of a known built-in type as `PostgreSQL` text.
///
/// Returns `None` for unknown type OIDs or malformed payloads.
#[must_use]
pub fn render_binary(type_oid: u32, bytes: &[u8]) -> Option<String> {
    match type_oid {
        oid::BOOL => match bytes {
            [0] => Some("f".to_string()),
            [_] => Some("t".to_string()),
            _ => None,
        },
        oid::CHAR => match bytes {
            [b] => Some(char::from(*b).to_string()),
            _ => None,
        },
        oid::INT2 => exact(bytes, 2, read_i16(bytes, 0)).map(|v| v.to_string()),
        oid::INT4 => exact(bytes, 4, read_i32(bytes, 0)).map(|v| v.to_string()),
        oid::INT8 => exact(bytes, 8, read_i64(bytes, 0)).map(|v| v.to_string()),
        oid::OID | oid::XID | oid::CID | oid::REGPROC | oid::REGCLASS => {
            exact(bytes, 4, read_u32(bytes, 0)).map(|v| v.to_string())
        }
        oid::FLOAT4 => render_float4(bytes),
        oid::FLOAT8 => render_float8(bytes),
        oid::TEXT
        | oid::VARCHAR
        | oid::BPCHAR
        | oid::NAME
        | oid::XML
        | oid::JSON
        | oid::UNKNOWN => utf8_text(bytes),
        oid::JSONB => jsonb_text(bytes),
        oid::BYTEA => Some(hex_bytes(bytes)),
        oid::NUMERIC => render_numeric(bytes),
        oid::UUID => render_uuid(bytes),
        oid::DATE => render_date(bytes),
        oid::TIME => binary_time(bytes).map(format_time),
        oid::TIMETZ => render_timetz(bytes),
        oid::TIMESTAMP => render_timestamp(bytes, false),
        oid::TIMESTAMPTZ => render_timestamp(bytes, true),
        oid::INTERVAL => render_interval(bytes),
        oid::INET => render_inet(bytes, false),
        oid::CIDR => render_inet(bytes, true),
        oid::MACADDR | oid::MACADDR8 => render_mac(bytes),
        oid::MONEY => exact(bytes, 8, read_i64(bytes, 0)).map(format_money),
        oid::BIT | oid::VARBIT => render_bits(bytes),
        oid::POINT => {
            if bytes.len() == 16 {
                Some(format!(
                    "({},{})",
                    render_float8(&bytes[..8])?,
                    render_float8(&bytes[8..])?
                ))
            } else {
                None
            }
        }
        // Anonymous records
        2249 => render_record(bytes),
        other => range_element(other).map_or_else(
            || builtin_array_element(other).and_then(|_| render_binary_array(bytes)),
            |element| render_range(bytes, element),
        ),
    }
}

/// Type-category hints a driver can supply for non-built-in types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeShape {
    /// Unknown or scalar user-defined type (enum, domain, extension type)
    Scalar,
    /// Array type
    Array,
    /// Composite (row) type
    Composite,
    /// Range type
    Range,
}

/// Decode a non-NULL binary wire value into a [`DatabaseValue`]; never fails.
///
/// `type_name` should be the canonical name from [`canonical_type_name`]; it
/// is used as [`DatabaseValue::Other::type_name`] for types without a
/// dedicated variant.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn decode_binary_value(
    type_oid: u32,
    type_name: &str,
    shape: TypeShape,
    bytes: &[u8],
) -> DatabaseValue {
    let other = |text: String| DatabaseValue::Other {
        type_name: type_name.to_string(),
        text,
    };
    let fallback = || {
        let text = match shape {
            TypeShape::Array => render_binary_array(bytes),
            TypeShape::Composite => render_record(bytes),
            TypeShape::Scalar | TypeShape::Range => None,
        };
        other(
            text.or_else(|| render_binary(type_oid, bytes))
                .unwrap_or_else(|| render_unknown(bytes)),
        )
    };
    let decoded = match type_oid {
        oid::BOOL => match bytes {
            [b] => Some(DatabaseValue::Bool(*b != 0)),
            _ => None,
        },
        oid::CHAR => match bytes {
            [b] => Some(DatabaseValue::Int8(i8::from_be_bytes([*b]))),
            _ => None,
        },
        oid::INT2 => exact(bytes, 2, read_i16(bytes, 0)).map(DatabaseValue::Int16),
        oid::INT4 => exact(bytes, 4, read_i32(bytes, 0)).map(DatabaseValue::Int32),
        oid::INT8 => exact(bytes, 8, read_i64(bytes, 0)).map(DatabaseValue::Int64),
        oid::OID | oid::XID | oid::CID => {
            exact(bytes, 4, read_u32(bytes, 0)).map(|v| DatabaseValue::Int64(i64::from(v)))
        }
        oid::FLOAT4 => {
            exact(bytes, 4, read_u32(bytes, 0)).map(|v| DatabaseValue::Real32(f32::from_bits(v)))
        }
        oid::FLOAT8 => exact(bytes, 8, read_i64(bytes, 0))
            .map(|v| DatabaseValue::Real64(f64::from_bits(v.cast_unsigned()))),
        oid::TEXT | oid::VARCHAR | oid::BPCHAR | oid::NAME | oid::UNKNOWN => {
            utf8_text(bytes).map(DatabaseValue::String)
        }
        oid::BYTEA => Some(DatabaseValue::Bytes(bytes.to_vec())),
        oid::DATE => binary_date(bytes).map(DatabaseValue::Date),
        oid::TIME => binary_time(bytes).map(DatabaseValue::Time),
        oid::TIMESTAMP | oid::TIMESTAMPTZ => binary_timestamp(bytes).map(DatabaseValue::DateTime),
        oid::JSON => utf8_text(bytes).map(json_value),
        oid::JSONB => jsonb_text(bytes).map(json_value),
        oid::UUID => {
            #[cfg(feature = "uuid")]
            {
                uuid::Uuid::from_slice(bytes).ok().map(DatabaseValue::Uuid)
            }
            #[cfg(not(feature = "uuid"))]
            {
                render_uuid(bytes).map(DatabaseValue::String)
            }
        }
        oid::NUMERIC => render_numeric(bytes).map(|text| {
            #[cfg(feature = "decimal")]
            {
                text.parse::<rust_decimal::Decimal>()
                    .map_or_else(|_| other(text.clone()), DatabaseValue::Decimal)
            }
            #[cfg(not(feature = "decimal"))]
            {
                other(text)
            }
        }),
        _ => None,
    };
    decoded.unwrap_or_else(fallback)
}

/// Decode a non-NULL text-format wire value into a [`DatabaseValue`]; never fails.
#[must_use]
pub fn decode_text_value(type_oid: u32, type_name: &str, text: &str) -> DatabaseValue {
    let parsed = match type_oid {
        oid::BOOL => match text {
            "t" | "true" => Some(DatabaseValue::Bool(true)),
            "f" | "false" => Some(DatabaseValue::Bool(false)),
            _ => None,
        },
        oid::INT2 => text.parse().ok().map(DatabaseValue::Int16),
        oid::INT4 => text.parse().ok().map(DatabaseValue::Int32),
        oid::INT8 | oid::OID => text.parse().ok().map(DatabaseValue::Int64),
        oid::FLOAT4 => text.parse().ok().map(DatabaseValue::Real32),
        oid::FLOAT8 => text.parse().ok().map(DatabaseValue::Real64),
        oid::TEXT | oid::VARCHAR | oid::BPCHAR | oid::NAME | oid::UNKNOWN => {
            Some(DatabaseValue::String(text.to_string()))
        }
        oid::DATE => chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
            .ok()
            .map(DatabaseValue::Date),
        oid::TIME => chrono::NaiveTime::parse_from_str(text, "%H:%M:%S%.f")
            .ok()
            .map(DatabaseValue::Time),
        oid::TIMESTAMP => chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.f")
            .ok()
            .map(DatabaseValue::DateTime),
        oid::JSON | oid::JSONB => Some(json_value(text.to_string())),
        #[cfg(feature = "uuid")]
        oid::UUID => text.parse().ok().map(DatabaseValue::Uuid),
        #[cfg(feature = "decimal")]
        oid::NUMERIC => text.parse().ok().map(DatabaseValue::Decimal),
        _ => None,
    };
    parsed.unwrap_or_else(|| DatabaseValue::Other {
        type_name: type_name.to_string(),
        text: text.to_string(),
    })
}

/// JSON text as [`DatabaseValue::Json`] (with the `json` feature) or `String`.
#[must_use]
#[allow(clippy::missing_const_for_fn)] // only const without the `json` feature
pub fn json_value(text: String) -> DatabaseValue {
    #[cfg(feature = "json")]
    {
        serde_json::from_str(&text).map_or(DatabaseValue::String(text), DatabaseValue::Json)
    }
    #[cfg(not(feature = "json"))]
    {
        DatabaseValue::String(text)
    }
}

// ---------------------------------------------------------------------------
// SQL literal rendering for value variants without dedicated handling
// ---------------------------------------------------------------------------

/// Escape text for use inside a single-quoted SQL literal.
#[must_use]
pub fn escape_literal(text: &str) -> String {
    text.replace('\'', "''")
}

/// Render a SQL literal for [`DatabaseValue`] variants added for lossless
/// decoding (`Bytes`, `Date`, `Time`, `DateTimeTz`, `Json`, `Other`).
///
/// Returns `None` for variants the backends render themselves.
/// [`DatabaseValue::Other`] renders as an untyped literal so `PostgreSQL`
/// coerces it to the target column type.
#[must_use]
pub fn extended_value_literal(value: &DatabaseValue) -> Option<String> {
    Some(match value {
        DatabaseValue::BytesOpt(None)
        | DatabaseValue::DateOpt(None)
        | DatabaseValue::TimeOpt(None)
        | DatabaseValue::DateTimeTzOpt(None) => "NULL".to_string(),
        #[cfg(feature = "json")]
        DatabaseValue::JsonOpt(None) => "NULL".to_string(),
        DatabaseValue::Bytes(bytes) | DatabaseValue::BytesOpt(Some(bytes)) => {
            format!("'{}'::bytea", hex_bytes(bytes))
        }
        DatabaseValue::Date(date) | DatabaseValue::DateOpt(Some(date)) => {
            format!("DATE '{}'", date.format("%Y-%m-%d"))
        }
        DatabaseValue::Time(time) | DatabaseValue::TimeOpt(Some(time)) => {
            format!("TIME '{}'", time.format("%H:%M:%S%.f"))
        }
        DatabaseValue::DateTimeTz(dt) | DatabaseValue::DateTimeTzOpt(Some(dt)) => {
            format!("TIMESTAMPTZ '{}'", dt.to_rfc3339())
        }
        #[cfg(feature = "json")]
        DatabaseValue::Json(json) | DatabaseValue::JsonOpt(Some(json)) => {
            format!("'{}'::jsonb", escape_literal(&json.to_string()))
        }
        DatabaseValue::Other { text, .. } => format!("'{}'", escape_literal(text)),
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Introspection: type mapping and default parsing
// ---------------------------------------------------------------------------

#[cfg(feature = "schema")]
pub use introspection::*;

#[cfg(feature = "schema")]
mod introspection {
    use std::collections::BTreeMap;

    use async_trait::async_trait;

    use crate::schema::{
        ColumnInfo, DataType, DatabaseInfo, ForeignKeyInfo, GeneratedKind, IndexInfo, RelationKind,
        RelationSummary, SchemaInfo, TableInfo,
    };
    use crate::{DatabaseError, DatabaseValue};

    /// Separator used to aggregate lists into a single text catalog column.
    const LIST_SEPARATOR: char = '\u{1f}';

    /// A connection able to run catalog queries whose parameters and result
    /// columns are all (nullable) `text`.
    #[async_trait]
    pub trait PgCatalog: Send {
        /// Run `sql` with text parameters and return all rows as nullable strings.
        ///
        /// # Errors
        ///
        /// * If the query fails
        async fn catalog_rows(
            &mut self,
            sql: &str,
            params: &[Option<&str>],
        ) -> Result<Vec<Vec<Option<String>>>, DatabaseError>;
    }

    /// Relation kinds treated as "tables" by the table-oriented methods
    /// (matches `information_schema.tables`: tables, partitioned tables,
    /// views and foreign tables).
    const TABLE_LIKE_KINDS: &str = "'r','p','v','f'";

    /// Relation kinds returned by `list_relations` / `get_relation_info`.
    const RELATION_KINDS: &str = "'r','p','v','m','f'";

    /// Resolves `$1` (nullable schema) / `$2` (name) to a `pg_class` row.
    fn relation_filter(kinds: &str) -> String {
        format!(
            "n.nspname = COALESCE($1::text, current_schema())::name \
             AND c.relname = $2::text::name AND c.relkind IN ({kinds})"
        )
    }

    /// `SELECT name, is_default, is_system` for every schema.
    pub const LIST_SCHEMAS_SQL: &str = "SELECT n.nspname::text, \
         (n.nspname = current_schema())::text, \
         (n.nspname IN ('pg_catalog', 'information_schema') \
          OR n.nspname LIKE 'pg\\_toast%' OR n.nspname LIKE 'pg\\_temp\\_%')::text \
         FROM pg_namespace n ORDER BY n.nspname";

    /// `SELECT name, is_current` for every connectable, non-template database.
    pub const LIST_DATABASES_SQL: &str = "SELECT datname::text, \
         (datname = current_database())::text \
         FROM pg_database WHERE datallowconn AND NOT datistemplate ORDER BY datname";

    /// Relations of a schema (`$1`, nullable = current schema):
    /// `schema, name, relkind, comment, reltuples`.
    #[must_use]
    pub fn list_relations_sql(kinds: &str) -> String {
        format!(
            "SELECT n.nspname::text, c.relname::text, c.relkind::text, \
             obj_description(c.oid, 'pg_class'), c.reltuples::bigint::text \
             FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = COALESCE($1::text, current_schema())::name \
             AND c.relkind IN ({kinds}) ORDER BY c.relname"
        )
    }

    /// `schema, relkind, comment` of a relation.
    #[must_use]
    pub fn relation_meta_sql(kinds: &str) -> String {
        format!(
            "SELECT n.nspname::text, c.relkind::text, obj_description(c.oid, 'pg_class') \
             FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE {}",
            relation_filter(kinds)
        )
    }

    fn relation_oid_subquery(kinds: &str) -> String {
        format!(
            "(SELECT c.oid FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE {})",
            relation_filter(kinds)
        )
    }

    /// Columns: `name, native_type, nullable, default, attnum, is_identity, is_pk,
    /// generated (`s`/`v` or `NULL`), enum_labels`.
    ///
    /// `enum_labels` is `NULL` unless the column's type is an enum (`typtype = 'e'`;
    /// arrays of enums are base types), otherwise the labels in `enumsortorder`
    /// separated by U+001F (empty for an enum without labels).
    #[must_use]
    pub fn columns_sql(kinds: &str) -> String {
        format!(
            "SELECT a.attname::text, format_type(a.atttypid, a.atttypmod), \
             (NOT a.attnotnull)::text, pg_get_expr(ad.adbin, ad.adrelid), a.attnum::text, \
             (a.attidentity <> '')::text, \
             EXISTS (SELECT 1 FROM pg_index i WHERE i.indrelid = a.attrelid \
                     AND i.indisprimary AND a.attnum = ANY (i.indkey))::text, \
             NULLIF(a.attgenerated::text, ''), \
             CASE WHEN t.typtype = 'e' THEN COALESCE( \
                 (SELECT string_agg(e.enumlabel::text, chr(31) ORDER BY e.enumsortorder) \
                  FROM pg_enum e WHERE e.enumtypid = t.oid), '') END \
             FROM pg_attribute a \
             JOIN pg_type t ON t.oid = a.atttypid \
             LEFT JOIN pg_attrdef ad ON ad.adrelid = a.attrelid AND ad.adnum = a.attnum \
             WHERE a.attrelid = {} AND a.attnum > 0 AND NOT a.attisdropped \
             ORDER BY a.attnum",
            relation_oid_subquery(kinds)
        )
    }

    /// Indexes: `name, is_unique, is_primary, columns (separated by U+001F)`.
    #[must_use]
    pub fn indexes_sql(kinds: &str) -> String {
        format!(
            "SELECT ic.relname::text, i.indisunique::text, i.indisprimary::text, \
             (SELECT string_agg(pg_get_indexdef(i.indexrelid, k, true), chr(31) ORDER BY k) \
              FROM generate_series(1, i.indnkeyatts::int) AS k) \
             FROM pg_index i JOIN pg_class ic ON ic.oid = i.indexrelid \
             WHERE i.indrelid = {} ORDER BY ic.relname",
            relation_oid_subquery(kinds)
        )
    }

    /// Foreign keys: `name, column, referenced_table, referenced_column,
    /// on_update, on_delete` (one row per column pair).
    #[must_use]
    pub fn foreign_keys_sql(kinds: &str) -> String {
        format!(
            "SELECT con.conname::text, a.attname::text, rc.relname::text, ra.attname::text, \
             con.confupdtype::text, con.confdeltype::text \
             FROM pg_constraint con \
             CROSS JOIN LATERAL unnest(con.conkey, con.confkey) WITH ORDINALITY AS k(att, ref_att, ord) \
             JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k.att \
             JOIN pg_class rc ON rc.oid = con.confrelid \
             JOIN pg_attribute ra ON ra.attrelid = con.confrelid AND ra.attnum = k.ref_att \
             WHERE con.contype = 'f' AND con.conrelid = {} \
             ORDER BY con.conname, k.ord",
            relation_oid_subquery(kinds)
        )
    }

    /// View definition of a view or materialized view.
    #[must_use]
    pub fn view_definition_sql() -> String {
        format!(
            "SELECT pg_get_viewdef(c.oid, true) FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace WHERE {}",
            relation_filter("'v','m'")
        )
    }

    /// Map a `pg_class.relkind` to a [`RelationKind`].
    #[must_use]
    pub fn relation_kind(relkind: &str) -> RelationKind {
        match relkind {
            "r" | "p" => RelationKind::Table,
            "v" => RelationKind::View,
            "m" => RelationKind::MaterializedView,
            "f" => RelationKind::ForeignTable,
            other => RelationKind::Other(other.to_string()),
        }
    }

    /// Map a `pg_constraint` referential action code to its SQL name.
    #[must_use]
    pub fn referential_action(code: &str) -> Option<String> {
        Some(
            match code {
                "a" => "NO ACTION",
                "r" => "RESTRICT",
                "c" => "CASCADE",
                "n" => "SET NULL",
                "d" => "SET DEFAULT",
                _ => return None,
            }
            .to_string(),
        )
    }

    /// Remove parenthesized modifiers: `time(3) without time zone` -> `time without time zone`.
    fn strip_modifiers(native: &str) -> (String, Vec<u32>) {
        let mut base = String::with_capacity(native.len());
        let mut args = String::new();
        let mut depth = 0usize;
        for c in native.chars() {
            match c {
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                _ if depth > 0 => args.push(c),
                _ => base.push(c),
            }
        }
        let args = args
            .split(',')
            .filter_map(|a| a.trim().parse::<u32>().ok())
            .collect();
        (base.split_whitespace().collect::<Vec<_>>().join(" "), args)
    }

    /// Map a `PostgreSQL` type name (as produced by `format_type`, or a short
    /// alias such as `int4`) to a [`DataType`]. Never fails: unknown types
    /// (enums, domains, composites, extensions) map to [`DataType::Custom`].
    #[must_use]
    pub fn pg_type_to_data_type(native: &str) -> DataType {
        let trimmed = native.trim();
        if let Some(inner) = trimmed.strip_suffix("[]") {
            return DataType::Array(Box::new(pg_type_to_data_type(inner)));
        }
        if let Some(inner) = trimmed.strip_prefix('_')
            && !inner.is_empty()
            && !matches!(pg_type_to_data_type(inner), DataType::Custom(_))
        {
            return DataType::Array(Box::new(pg_type_to_data_type(inner)));
        }
        let (base, args) = strip_modifiers(&trimmed.to_ascii_lowercase());
        let len = || args.first().and_then(|n| u16::try_from(*n).ok());
        match base.as_str() {
            "smallint" | "int2" => DataType::SmallInt,
            "integer" | "int" | "int4" => DataType::Int,
            "bigint" | "int8" => DataType::BigInt,
            "serial" | "serial4" => DataType::Serial,
            "bigserial" | "serial8" => DataType::BigSerial,
            "character varying" | "varchar" => DataType::VarChar(len().unwrap_or(255)),
            "character" | "char" | "bpchar" => DataType::Char(len().unwrap_or(1)),
            "text" => DataType::Text,
            "boolean" | "bool" => DataType::Bool,
            "real" | "float4" => DataType::Real,
            "double precision" | "float8" => DataType::Double,
            "numeric" | "decimal" => match args.as_slice() {
                [p, s] => match (u8::try_from(*p), u8::try_from(*s)) {
                    (Ok(p), Ok(s)) => DataType::Decimal(p, s),
                    _ => DataType::Custom(trimmed.to_string()),
                },
                [p] => u8::try_from(*p).map_or_else(
                    |_| DataType::Custom(trimmed.to_string()),
                    |p| DataType::Decimal(p, 0),
                ),
                _ => DataType::Decimal(38, 10),
            },
            "money" => DataType::Money,
            "date" => DataType::Date,
            "time" | "time without time zone" => DataType::Time,
            // Matches the DDL mapping: `DataType::Timestamp` <-> `TIMESTAMP`,
            // `DataType::DateTime` <-> `TIMESTAMP WITH TIME ZONE`.
            "timestamp" | "timestamp without time zone" => DataType::Timestamp,
            "timestamptz" | "timestamp with time zone" => DataType::DateTime,
            "bytea" => DataType::Blob,
            "json" => DataType::Json,
            "jsonb" => DataType::Jsonb,
            "uuid" => DataType::Uuid,
            "xml" => DataType::Xml,
            "inet" | "cidr" => DataType::Inet,
            "macaddr" | "macaddr8" => DataType::MacAddr,
            _ => DataType::Custom(trimmed.to_string()),
        }
    }

    /// Parse `PostgreSQL` default value expressions (`pg_get_expr` output).
    #[must_use]
    pub fn parse_default_value(default_str: &str) -> Option<DatabaseValue> {
        if default_str.starts_with('\'') && default_str.contains("'::") {
            // Format: 'value'::type
            if let Some(end_quote) = default_str[1..].find('\'') {
                let value = &default_str[1..=end_quote];
                return Some(DatabaseValue::String(value.to_string()));
            }
        }

        if default_str.starts_with("nextval(") {
            // Sequence default - not representable as simple value
            return None;
        }

        match default_str.to_uppercase().as_str() {
            "TRUE" => Some(DatabaseValue::Bool(true)),
            "FALSE" => Some(DatabaseValue::Bool(false)),
            "NULL" => None,
            _ => default_str.parse::<i64>().map_or_else(
                |_| {
                    default_str.parse::<f64>().map_or_else(
                        |_| Some(DatabaseValue::String(default_str.to_string())),
                        |float_val| Some(DatabaseValue::Real64(float_val)),
                    )
                },
                |int_val| Some(DatabaseValue::Int64(int_val)),
            ),
        }
    }

    fn col(row: &[Option<String>], index: usize) -> Option<&str> {
        row.get(index).and_then(Option::as_deref)
    }

    fn col_string(row: &[Option<String>], index: usize) -> String {
        col(row, index).unwrap_or_default().to_string()
    }

    fn col_bool(row: &[Option<String>], index: usize) -> bool {
        matches!(col(row, index), Some("true" | "t"))
    }

    /// Split `columns_sql` enum labels (U+001F separated; empty = no labels).
    fn split_enum_labels(labels: &str) -> Vec<String> {
        if labels.is_empty() {
            Vec::new()
        } else {
            labels
                .split(LIST_SEPARATOR)
                .map(ToString::to_string)
                .collect()
        }
    }

    /// List all schemas.
    ///
    /// # Errors
    ///
    /// * If the catalog query fails
    pub async fn pg_list_schemas<C: PgCatalog + ?Sized>(
        client: &mut C,
    ) -> Result<Vec<SchemaInfo>, DatabaseError> {
        Ok(client
            .catalog_rows(LIST_SCHEMAS_SQL, &[])
            .await?
            .iter()
            .map(|row| SchemaInfo {
                name: col_string(row, 0),
                is_default: col_bool(row, 1),
                is_system: col_bool(row, 2),
            })
            .collect())
    }

    /// List the connectable, non-template databases of the server.
    ///
    /// # Errors
    ///
    /// * If the catalog query fails
    pub async fn pg_list_databases<C: PgCatalog + ?Sized>(
        client: &mut C,
    ) -> Result<Vec<DatabaseInfo>, DatabaseError> {
        Ok(client
            .catalog_rows(LIST_DATABASES_SQL, &[])
            .await?
            .iter()
            .map(|row| DatabaseInfo {
                name: col_string(row, 0),
                is_current: col_bool(row, 1),
                is_system: false,
            })
            .collect())
    }

    /// List relations in `schema` (`None` = current schema).
    ///
    /// # Errors
    ///
    /// * If the catalog query fails
    pub async fn pg_list_relations<C: PgCatalog + ?Sized>(
        client: &mut C,
        schema: Option<&str>,
    ) -> Result<Vec<RelationSummary>, DatabaseError> {
        Ok(client
            .catalog_rows(&list_relations_sql(RELATION_KINDS), &[schema])
            .await?
            .iter()
            .map(|row| RelationSummary {
                schema: col(row, 0).map(ToString::to_string),
                name: col_string(row, 1),
                kind: relation_kind(col(row, 2).unwrap_or_default()),
                comment: col(row, 3).map(ToString::to_string),
                estimated_rows: col(row, 4)
                    .and_then(|n| n.parse::<i64>().ok())
                    .and_then(|n| u64::try_from(n).ok()),
            })
            .collect())
    }

    /// List table names (tables and partitioned tables) in the current schema.
    ///
    /// # Errors
    ///
    /// * If the catalog query fails
    pub async fn pg_list_tables<C: PgCatalog + ?Sized>(
        client: &mut C,
    ) -> Result<Vec<String>, DatabaseError> {
        Ok(client
            .catalog_rows(&list_relations_sql("'r','p'"), &[None])
            .await?
            .iter()
            .map(|row| col_string(row, 1))
            .collect())
    }

    /// Whether a table-like relation exists in the current schema.
    ///
    /// # Errors
    ///
    /// * If the catalog query fails
    pub async fn pg_table_exists<C: PgCatalog + ?Sized>(
        client: &mut C,
        table_name: &str,
    ) -> Result<bool, DatabaseError> {
        Ok(!client
            .catalog_rows(
                &relation_meta_sql(TABLE_LIKE_KINDS),
                &[None, Some(table_name)],
            )
            .await?
            .is_empty())
    }

    async fn pg_columns<C: PgCatalog + ?Sized>(
        client: &mut C,
        schema: Option<&str>,
        name: &str,
        kinds: &str,
    ) -> Result<Vec<ColumnInfo>, DatabaseError> {
        Ok(client
            .catalog_rows(&columns_sql(kinds), &[schema, Some(name)])
            .await?
            .iter()
            .map(|row| {
                let native_type = col_string(row, 1);
                let generated = match col(row, 7) {
                    Some("s") => Some(GeneratedKind::Stored),
                    Some("v") => Some(GeneratedKind::Virtual),
                    _ => None,
                };
                // pg_attrdef holds the generation expression for generated columns
                let default_expr = col(row, 3).filter(|_| generated.is_none());
                let is_identity = col_bool(row, 5);
                ColumnInfo {
                    name: col_string(row, 0),
                    data_type: pg_type_to_data_type(&native_type),
                    nullable: col_bool(row, 2),
                    is_primary_key: col_bool(row, 6),
                    auto_increment: is_identity
                        || default_expr.is_some_and(|d| d.starts_with("nextval(")),
                    default_value: default_expr.and_then(parse_default_value),
                    ordinal_position: col(row, 4).and_then(|n| n.parse::<u32>().ok()).unwrap_or(0),
                    native_type,
                    enum_values: col(row, 8).map(split_enum_labels),
                    generated,
                }
            })
            .collect())
    }

    /// Columns of a table-like relation in the current schema.
    ///
    /// # Errors
    ///
    /// * If the catalog query fails
    pub async fn pg_get_table_columns<C: PgCatalog + ?Sized>(
        client: &mut C,
        table_name: &str,
    ) -> Result<Vec<ColumnInfo>, DatabaseError> {
        pg_columns(client, None, table_name, TABLE_LIKE_KINDS).await
    }

    /// Whether a column exists on a table-like relation in the current schema.
    ///
    /// # Errors
    ///
    /// * If the catalog query fails
    pub async fn pg_column_exists<C: PgCatalog + ?Sized>(
        client: &mut C,
        table_name: &str,
        column_name: &str,
    ) -> Result<bool, DatabaseError> {
        Ok(pg_get_table_columns(client, table_name)
            .await?
            .iter()
            .any(|c| c.name == column_name))
    }

    async fn pg_relation_info<C: PgCatalog + ?Sized>(
        client: &mut C,
        schema: Option<&str>,
        name: &str,
        kinds: &str,
    ) -> Result<Option<TableInfo>, DatabaseError> {
        let meta = client
            .catalog_rows(&relation_meta_sql(kinds), &[schema, Some(name)])
            .await?;
        let Some(meta) = meta.first() else {
            return Ok(None);
        };

        let columns = pg_columns(client, schema, name, kinds)
            .await?
            .into_iter()
            .map(|c| (c.name.clone(), c))
            .collect();

        let mut indexes = BTreeMap::new();
        for row in client
            .catalog_rows(&indexes_sql(kinds), &[schema, Some(name)])
            .await?
        {
            let index_name = col_string(&row, 0);
            indexes.insert(
                index_name.clone(),
                IndexInfo {
                    name: index_name,
                    unique: col_bool(&row, 1),
                    is_primary: col_bool(&row, 2),
                    columns: col(&row, 3)
                        .map(|c| c.split(LIST_SEPARATOR).map(ToString::to_string).collect())
                        .unwrap_or_default(),
                },
            );
        }

        let mut foreign_keys = BTreeMap::new();
        for row in client
            .catalog_rows(&foreign_keys_sql(kinds), &[schema, Some(name)])
            .await?
        {
            let constraint = col_string(&row, 0);
            // Multi-column keys report their first column pair.
            foreign_keys
                .entry(constraint.clone())
                .or_insert_with(|| ForeignKeyInfo {
                    name: constraint,
                    column: col_string(&row, 1),
                    referenced_table: col_string(&row, 2),
                    referenced_column: col_string(&row, 3),
                    on_update: col(&row, 4).and_then(referential_action),
                    on_delete: col(&row, 5).and_then(referential_action),
                });
        }

        Ok(Some(TableInfo {
            name: name.to_string(),
            schema: col(meta, 0).map(ToString::to_string),
            kind: relation_kind(col(meta, 1).unwrap_or_default()),
            comment: col(meta, 2).map(ToString::to_string),
            columns,
            indexes,
            foreign_keys,
        }))
    }

    /// Full information about a table-like relation in the current schema.
    ///
    /// # Errors
    ///
    /// * If a catalog query fails
    pub async fn pg_get_table_info<C: PgCatalog + ?Sized>(
        client: &mut C,
        table_name: &str,
    ) -> Result<Option<TableInfo>, DatabaseError> {
        pg_relation_info(client, None, table_name, TABLE_LIKE_KINDS).await
    }

    /// Full information about any relation (tables, views, materialized
    /// views, foreign tables) in any schema.
    ///
    /// # Errors
    ///
    /// * If a catalog query fails
    pub async fn pg_get_relation_info<C: PgCatalog + ?Sized>(
        client: &mut C,
        relation: &crate::schema::RelationRef,
    ) -> Result<Option<TableInfo>, DatabaseError> {
        pg_relation_info(
            client,
            relation.schema.as_deref(),
            &relation.name,
            RELATION_KINDS,
        )
        .await
    }

    /// Definition of a view or materialized view.
    ///
    /// # Errors
    ///
    /// * If the catalog query fails
    pub async fn pg_get_view_definition<C: PgCatalog + ?Sized>(
        client: &mut C,
        relation: &crate::schema::RelationRef,
    ) -> Result<Option<String>, DatabaseError> {
        Ok(client
            .catalog_rows(
                &view_definition_sql(),
                &[relation.schema.as_deref(), Some(relation.name.as_str())],
            )
            .await?
            .first()
            .and_then(|row| col(row, 0).map(|d| d.trim().to_string())))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn maps_format_type_output() {
            assert_eq!(pg_type_to_data_type("integer"), DataType::Int);
            assert_eq!(
                pg_type_to_data_type("character varying(50)"),
                DataType::VarChar(50)
            );
            assert_eq!(pg_type_to_data_type("character(3)"), DataType::Char(3));
            assert_eq!(
                pg_type_to_data_type("numeric(10,2)"),
                DataType::Decimal(10, 2)
            );
            assert_eq!(pg_type_to_data_type("numeric"), DataType::Decimal(38, 10));
            assert_eq!(
                pg_type_to_data_type("time(3) without time zone"),
                DataType::Time
            );
            assert_eq!(
                pg_type_to_data_type("timestamp(6) with time zone"),
                DataType::DateTime
            );
            assert_eq!(
                pg_type_to_data_type("timestamp without time zone"),
                DataType::Timestamp
            );
            assert_eq!(
                pg_type_to_data_type("integer[]"),
                DataType::Array(Box::new(DataType::Int))
            );
            assert_eq!(
                pg_type_to_data_type("_int4"),
                DataType::Array(Box::new(DataType::Int))
            );
            assert_eq!(pg_type_to_data_type("cidr"), DataType::Inet);
            assert_eq!(pg_type_to_data_type("bytea"), DataType::Blob);
            assert_eq!(
                pg_type_to_data_type("mood"),
                DataType::Custom("mood".to_string())
            );
            assert_eq!(
                pg_type_to_data_type("time with time zone"),
                DataType::Custom("time with time zone".to_string())
            );
            assert_eq!(
                pg_type_to_data_type("mood[]"),
                DataType::Array(Box::new(DataType::Custom("mood".to_string())))
            );
        }

        #[test]
        fn maps_relkinds_and_actions() {
            assert_eq!(relation_kind("p"), RelationKind::Table);
            assert_eq!(relation_kind("m"), RelationKind::MaterializedView);
            assert_eq!(relation_kind("f"), RelationKind::ForeignTable);
            assert_eq!(referential_action("c").as_deref(), Some("CASCADE"));
            assert_eq!(referential_action("x"), None);
        }

        #[test]
        fn parses_defaults() {
            assert_eq!(
                parse_default_value("'abc'::text"),
                Some(DatabaseValue::String("abc".into()))
            );
            assert_eq!(parse_default_value("nextval('s'::regclass)"), None);
            assert_eq!(parse_default_value("42"), Some(DatabaseValue::Int64(42)));
            assert_eq!(parse_default_value("true"), Some(DatabaseValue::Bool(true)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn be32(v: i32) -> [u8; 4] {
        v.to_be_bytes()
    }

    #[test]
    fn canonical_names() {
        assert_eq!(canonical_type_name(Some(oid::INT4), "INT4"), "int4");
        assert_eq!(canonical_type_name(Some(1007), "INT4[]"), "int4[]");
        assert_eq!(canonical_type_name(Some(99_999), "mood"), "mood");
        assert_eq!(canonical_type_name(Some(99_999), "_mood"), "mood[]");
        assert_eq!(canonical_type_name(None, "x"), "x");
    }

    #[test]
    fn renders_numeric() {
        // 12345.678 => weight 1, digits [1, 2345, 6780], dscale 3
        let mut bytes = vec![];
        for v in [3i16, 1, 0, 3, 1, 2345, 6780] {
            bytes.extend_from_slice(&v.to_be_bytes());
        }
        // Fix sign/dscale as u16
        bytes[4..6].copy_from_slice(&0u16.to_be_bytes());
        bytes[6..8].copy_from_slice(&3u16.to_be_bytes());
        assert_eq!(render_numeric(&bytes).as_deref(), Some("12345.678"));

        // -0.05 => weight -1, digits [500], dscale 2
        let mut bytes = vec![];
        bytes.extend_from_slice(&1i16.to_be_bytes());
        bytes.extend_from_slice(&(-1i16).to_be_bytes());
        bytes.extend_from_slice(&0x4000u16.to_be_bytes());
        bytes.extend_from_slice(&2u16.to_be_bytes());
        bytes.extend_from_slice(&500i16.to_be_bytes());
        assert_eq!(render_numeric(&bytes).as_deref(), Some("-0.05"));

        // 0 with dscale 0
        let bytes = [0u8, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(render_numeric(&bytes).as_deref(), Some("0"));
    }

    #[test]
    fn renders_interval() {
        assert_eq!(
            format_interval(14, 3, 14_706_500_000),
            "1 year 2 mons 3 days 04:05:06.5"
        );
        assert_eq!(format_interval(0, 0, 0), "00:00:00");
        assert_eq!(format_interval(0, 1, 0), "1 day");
        assert_eq!(format_interval(0, 0, -3_600_000_000), "-01:00:00");
    }

    #[test]
    fn renders_network_and_misc() {
        assert_eq!(
            render_binary(oid::INET, &[2, 32, 0, 4, 192, 168, 0, 1]).as_deref(),
            Some("192.168.0.1")
        );
        assert_eq!(
            render_binary(oid::CIDR, &[2, 24, 1, 4, 10, 0, 0, 0]).as_deref(),
            Some("10.0.0.0/24")
        );
        assert_eq!(
            render_binary(oid::MACADDR, &[8, 0, 0x2b, 1, 2, 3]).as_deref(),
            Some("08:00:2b:01:02:03")
        );
        assert_eq!(format_money(-12_345), "-123.45");
        assert_eq!(hex_bytes(&[0, 255]), "\\x00ff");
        assert_eq!(
            render_binary(oid::BIT, &[0, 0, 0, 3, 0b1010_0000]).as_deref(),
            Some("101")
        );
    }

    #[test]
    fn renders_arrays() {
        // int4[] {1,NULL,3}
        let mut bytes = vec![];
        bytes.extend_from_slice(&be32(1));
        bytes.extend_from_slice(&be32(1));
        bytes.extend_from_slice(&oid::INT4.to_be_bytes());
        bytes.extend_from_slice(&be32(3));
        bytes.extend_from_slice(&be32(1));
        for v in [Some(1), None, Some(3)] {
            match v {
                Some(v) => {
                    bytes.extend_from_slice(&be32(4));
                    bytes.extend_from_slice(&be32(v));
                }
                None => bytes.extend_from_slice(&be32(-1)),
            }
        }
        assert_eq!(render_binary_array(&bytes).as_deref(), Some("{1,NULL,3}"));
        assert_eq!(render_binary(1007, &bytes).as_deref(), Some("{1,NULL,3}"));

        // text[] with quoting, 2-D
        let mut bytes = vec![];
        bytes.extend_from_slice(&be32(2));
        bytes.extend_from_slice(&be32(0));
        bytes.extend_from_slice(&oid::TEXT.to_be_bytes());
        for _ in 0..2 {
            bytes.extend_from_slice(&be32(2));
            bytes.extend_from_slice(&be32(1));
        }
        for s in ["a", "b c", "", "d\"e"] {
            bytes.extend_from_slice(&be32(i32::try_from(s.len()).unwrap()));
            bytes.extend_from_slice(s.as_bytes());
        }
        assert_eq!(
            render_binary_array(&bytes).as_deref(),
            Some("{{a,\"b c\"},{\"\",\"d\\\"e\"}}")
        );

        // empty array
        let mut bytes = vec![];
        bytes.extend_from_slice(&be32(0));
        bytes.extend_from_slice(&be32(0));
        bytes.extend_from_slice(&oid::INT4.to_be_bytes());
        assert_eq!(render_binary_array(&bytes).as_deref(), Some("{}"));
    }

    #[test]
    fn renders_ranges() {
        // [1,10)
        let mut bytes = vec![0x02];
        for v in [1, 10] {
            bytes.extend_from_slice(&be32(4));
            bytes.extend_from_slice(&be32(v));
        }
        assert_eq!(
            render_binary(oid::INT4RANGE, &bytes).as_deref(),
            Some("[1,10)")
        );
        assert_eq!(
            render_binary(oid::INT4RANGE, &[0x01]).as_deref(),
            Some("empty")
        );
    }

    #[test]
    fn decodes_values_without_failing() {
        let date = decode_binary_value(oid::DATE, "date", TypeShape::Scalar, &be32(1));
        assert_eq!(
            date,
            DatabaseValue::Date(chrono::NaiveDate::from_ymd_opt(2000, 1, 2).unwrap())
        );
        let time = decode_binary_value(
            oid::TIME,
            "time",
            TypeShape::Scalar,
            &3_600_000_000i64.to_be_bytes(),
        );
        assert_eq!(
            time,
            DatabaseValue::Time(chrono::NaiveTime::from_hms_opt(1, 0, 0).unwrap())
        );
        assert_eq!(
            decode_binary_value(oid::BYTEA, "bytea", TypeShape::Scalar, &[1, 2]),
            DatabaseValue::Bytes(vec![1, 2])
        );
        // Enum label
        assert_eq!(
            decode_binary_value(77_777, "mood", TypeShape::Scalar, b"happy"),
            DatabaseValue::Other {
                type_name: "mood".into(),
                text: "happy".into()
            }
        );
        // Malformed known type degrades to Other instead of failing
        assert_eq!(
            decode_binary_value(oid::INT4, "int4", TypeShape::Scalar, &[1]),
            DatabaseValue::Other {
                type_name: "int4".into(),
                text: "\\x01".into()
            }
        );
        // Interval
        let mut interval = vec![];
        interval.extend_from_slice(&0i64.to_be_bytes());
        interval.extend_from_slice(&be32(2));
        interval.extend_from_slice(&be32(0));
        assert_eq!(
            decode_binary_value(oid::INTERVAL, "interval", TypeShape::Scalar, &interval),
            DatabaseValue::Other {
                type_name: "interval".into(),
                text: "2 days".into()
            }
        );
        let jsonb = decode_binary_value(oid::JSONB, "jsonb", TypeShape::Scalar, b"\x01{\"a\":1}");
        #[cfg(feature = "json")]
        assert_eq!(jsonb, DatabaseValue::Json(serde_json::json!({"a": 1})));
        #[cfg(not(feature = "json"))]
        assert_eq!(jsonb, DatabaseValue::String("{\"a\":1}".into()));
    }

    #[test]
    fn renders_literals() {
        assert_eq!(
            extended_value_literal(&DatabaseValue::Bytes(vec![1, 0xab])).as_deref(),
            Some("'\\x01ab'::bytea")
        );
        assert_eq!(
            extended_value_literal(&DatabaseValue::Other {
                type_name: "interval".into(),
                text: "1 day's".into()
            })
            .as_deref(),
            Some("'1 day''s'")
        );
        assert_eq!(
            extended_value_literal(&DatabaseValue::DateOpt(None)).as_deref(),
            Some("NULL")
        );
        assert_eq!(extended_value_literal(&DatabaseValue::Int32(1)), None);
    }
}
