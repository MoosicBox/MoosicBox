pub use chrono::*;

/// Parses a date/time string into a `NaiveDateTime`.
///
/// Supports multiple input formats:
/// * Year only (4 digits or less): `"2024"`
/// * ISO date: `"2024-10-24"`
/// * ISO datetime with Z suffix: `"2024-10-24T12:30:45Z"`
/// * ISO datetime with timezone: `"2024-10-24T12:30:45.123+00:00"`
/// * ISO datetime with fractional seconds: `"2024-10-24T12:30:45.123"`
///
/// # Examples
///
/// ```rust
/// use moosicbox_date_utils::chrono::parse_date_time;
///
/// // Parse an ISO datetime
/// let dt = parse_date_time("2024-10-24T12:30:45Z").unwrap();
/// assert_eq!(dt.to_string(), "2024-10-24 12:30:45");
///
/// // Parse just a date
/// let dt = parse_date_time("2024-10-24").unwrap();
/// assert_eq!(dt.to_string(), "2024-10-24 00:00:00");
///
/// // Parse just a year
/// let dt = parse_date_time("2024").unwrap();
/// assert_eq!(dt.to_string(), "2024-01-01 00:00:00");
/// ```
///
/// # Errors
///
/// * If the datetime fails to parse
pub fn parse_date_time(value: &str) -> Result<NaiveDateTime, chrono::ParseError> {
    if value.len() <= 4
        && let Ok(year) = value.parse::<u16>()
        && let Some(date) = NaiveDate::default().with_year(i32::from(year))
    {
        return Ok(date.into());
    }
    if value.len() == 10 {
        return NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .inspect_err(|&e| {
                log::error!("Error parsing 10 {value}: {e:?}");
            })
            .map(Into::into);
    }
    if value.ends_with('Z') {
        return NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%SZ").inspect_err(|&e| {
            log::error!("Error parsing full z {value}: {e:?}");
        });
    }
    if value.ends_with("+00:00") {
        return NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f%z").inspect_err(|&e| {
            log::error!("Error parsing full %.f%z {value}: {e:?}");
        });
    }

    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f").inspect_err(|&e| {
        log::error!("Error parsing full {value}: {e:?}");
    })
}
