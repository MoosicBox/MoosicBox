//! Date and time parsing utilities using the `chrono` crate.
//!
//! This module re-exports all types from the `chrono` crate and provides
//! the [`parse_date_time`] function for flexible date/time string parsing.

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
/// Returns `chrono::ParseError` if:
/// * The input string doesn't match any of the supported formats
/// * The year string cannot be parsed as a valid 16-bit unsigned integer (for year-only input)
/// * The date or time components are invalid (e.g., month > 12, day > 31, hour > 23)
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

#[cfg(test)]
mod tests {
    use super::*;

    // Tests for year-only parsing
    #[test_log::test]
    fn test_parse_year_only_valid() {
        let dt = parse_date_time("2024").unwrap();
        assert_eq!(dt.to_string(), "2024-01-01 00:00:00");
    }

    #[test_log::test]
    fn test_parse_year_only_single_digit() {
        let dt = parse_date_time("1").unwrap();
        assert_eq!(dt.to_string(), "0001-01-01 00:00:00");
    }

    #[test_log::test]
    fn test_parse_year_only_with_leading_zeros() {
        let dt = parse_date_time("0001").unwrap();
        assert_eq!(dt.to_string(), "0001-01-01 00:00:00");
    }

    #[test_log::test]
    fn test_parse_year_only_max_valid() {
        let dt = parse_date_time("9999").unwrap();
        assert_eq!(dt.to_string(), "9999-01-01 00:00:00");
    }

    #[test_log::test]
    fn test_parse_year_only_invalid_non_numeric() {
        // String with 4 chars but not a number should fail
        let result = parse_date_time("abcd");
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_parse_year_only_too_long() {
        // More than 4 chars shouldn't use year-only path
        let result = parse_date_time("12345");
        assert!(result.is_err());
    }

    // Tests for ISO date parsing (10 characters)
    #[test_log::test]
    fn test_parse_iso_date_valid() {
        let dt = parse_date_time("2024-10-24").unwrap();
        assert_eq!(dt.to_string(), "2024-10-24 00:00:00");
    }

    #[test_log::test]
    fn test_parse_iso_date_leap_year_valid() {
        // Feb 29 on a leap year should succeed
        let dt = parse_date_time("2024-02-29").unwrap();
        assert_eq!(dt.to_string(), "2024-02-29 00:00:00");
    }

    #[test_log::test]
    fn test_parse_iso_date_leap_year_invalid() {
        // Feb 29 on a non-leap year should fail
        let result = parse_date_time("2023-02-29");
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_parse_iso_date_invalid_month_zero() {
        let result = parse_date_time("2024-00-15");
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_parse_iso_date_invalid_month_thirteen() {
        let result = parse_date_time("2024-13-15");
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_parse_iso_date_invalid_day_zero() {
        let result = parse_date_time("2024-10-00");
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_parse_iso_date_invalid_day_too_high() {
        let result = parse_date_time("2024-10-32");
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_parse_iso_date_invalid_day_for_month() {
        // April only has 30 days
        let result = parse_date_time("2024-04-31");
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_parse_iso_date_ten_chars_not_date() {
        // String that's 10 chars but not a valid date format
        let result = parse_date_time("abcdefghij");
        assert!(result.is_err());
    }

    // Tests for ISO datetime with Z suffix
    #[test_log::test]
    fn test_parse_iso_datetime_z_valid() {
        let dt = parse_date_time("2024-10-24T12:30:45Z").unwrap();
        assert_eq!(dt.to_string(), "2024-10-24 12:30:45");
    }

    #[test_log::test]
    fn test_parse_iso_datetime_z_midnight() {
        let dt = parse_date_time("2024-10-24T00:00:00Z").unwrap();
        assert_eq!(dt.to_string(), "2024-10-24 00:00:00");
    }

    #[test_log::test]
    fn test_parse_iso_datetime_z_end_of_day() {
        let dt = parse_date_time("2024-10-24T23:59:59Z").unwrap();
        assert_eq!(dt.to_string(), "2024-10-24 23:59:59");
    }

    #[test_log::test]
    fn test_parse_iso_datetime_z_invalid_hour() {
        let result = parse_date_time("2024-10-24T24:00:00Z");
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_parse_iso_datetime_z_invalid_minute() {
        let result = parse_date_time("2024-10-24T12:60:00Z");
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_parse_iso_datetime_z_invalid_second() {
        // Note: Second 60 is actually valid for leap seconds in chrono
        // So we test with 61 instead
        let result = parse_date_time("2024-10-24T12:30:61Z");
        assert!(result.is_err());
    }

    // Tests for ISO datetime with timezone offset
    #[test_log::test]
    fn test_parse_iso_datetime_with_timezone_valid() {
        let dt = parse_date_time("2024-10-24T12:30:45+00:00").unwrap();
        assert_eq!(dt.to_string(), "2024-10-24 12:30:45");
    }

    #[test_log::test]
    fn test_parse_iso_datetime_with_timezone_fractional_seconds() {
        let dt = parse_date_time("2024-10-24T12:30:45.123+00:00").unwrap();
        assert_eq!(dt.to_string(), "2024-10-24 12:30:45.123");
    }

    #[test_log::test]
    fn test_parse_iso_datetime_with_timezone_nanoseconds() {
        let dt = parse_date_time("2024-10-24T12:30:45.123456789+00:00").unwrap();
        assert_eq!(dt.to_string(), "2024-10-24 12:30:45.123456789");
    }

    #[test_log::test]
    fn test_parse_iso_datetime_with_timezone_invalid_hour() {
        let result = parse_date_time("2024-10-24T25:00:00+00:00");
        assert!(result.is_err());
    }

    // Tests for ISO datetime with fractional seconds (no timezone)
    #[test_log::test]
    fn test_parse_iso_datetime_fractional_valid() {
        let dt = parse_date_time("2024-10-24T12:30:45.123").unwrap();
        assert_eq!(dt.to_string(), "2024-10-24 12:30:45.123");
    }

    #[test_log::test]
    fn test_parse_iso_datetime_fractional_single_digit() {
        let dt = parse_date_time("2024-10-24T12:30:45.1").unwrap();
        assert_eq!(dt.to_string(), "2024-10-24 12:30:45.100");
    }

    #[test_log::test]
    fn test_parse_iso_datetime_fractional_nanoseconds() {
        let dt = parse_date_time("2024-10-24T12:30:45.123456789").unwrap();
        assert_eq!(dt.to_string(), "2024-10-24 12:30:45.123456789");
    }

    #[test_log::test]
    fn test_parse_iso_datetime_no_fractional() {
        // This falls through to the final parser which expects fractional
        // but should still work without it
        let dt = parse_date_time("2024-10-24T12:30:45").unwrap();
        assert_eq!(dt.to_string(), "2024-10-24 12:30:45");
    }

    #[test_log::test]
    fn test_parse_iso_datetime_fractional_invalid_hour() {
        let result = parse_date_time("2024-10-24T24:30:45.123");
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_parse_iso_datetime_fractional_invalid_minute() {
        let result = parse_date_time("2024-10-24T12:61:45.123");
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_parse_iso_datetime_fractional_invalid_second() {
        let result = parse_date_time("2024-10-24T12:30:61.123");
        assert!(result.is_err());
    }

    // Tests for format mismatches and edge cases
    #[test_log::test]
    fn test_parse_empty_string() {
        let result = parse_date_time("");
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_parse_wrong_date_separator() {
        let result = parse_date_time("2024/10/24");
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_parse_wrong_datetime_separator() {
        let result = parse_date_time("2024-10-24 12:30:45");
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_parse_mixed_format() {
        let result = parse_date_time("24-10-2024");
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_parse_with_extra_whitespace() {
        let result = parse_date_time(" 2024-10-24 ");
        assert!(result.is_err());
    }

    // Tests for boundary years
    #[test_log::test]
    fn test_parse_year_boundary_year_1() {
        let dt = parse_date_time("1").unwrap();
        assert_eq!(dt.year(), 1);
    }

    #[test_log::test]
    fn test_parse_date_boundary_year_9999() {
        let dt = parse_date_time("9999-12-31").unwrap();
        assert_eq!(dt.to_string(), "9999-12-31 00:00:00");
    }
}
