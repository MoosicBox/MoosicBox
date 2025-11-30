//! Byte range parsing for HTTP partial content requests.
//!
//! Supports parsing RFC 7233 byte range specifications (e.g., "0-1023", "-100", "500-") for
//! streaming media with partial content support. Used primarily for HTTP Range header processing.

#![allow(clippy::module_name_repetitions)]

use thiserror::Error;

/// Represents a byte range with optional start and end positions.
#[derive(Debug, Clone)]
pub struct Range {
    /// Starting byte position (inclusive)
    pub start: Option<usize>,
    /// Ending byte position (inclusive)
    pub end: Option<usize>,
}

/// Errors that can occur when parsing byte range specifications.
#[derive(Debug, Error)]
pub enum ParseRangesError {
    /// Failed to parse a range value as a number
    #[error("Could not parse range value: {0}")]
    Parse(String),
    /// Range specification has too few values (expected 2)
    #[error("Too few range values: {0}")]
    TooFewValues(String),
    /// Range specification has too many values (expected 2)
    #[error("Too many range values: {0}")]
    TooManyValues(String),
}

/// Parses a single byte range specification (e.g., "0-1023" or "-100" or "500-").
///
/// # Errors
///
/// * `ParseRangesError::Parse` - If fails to parse a `usize`
/// * `ParseRangesError::TooFewValues` - If too few values in the range
/// * `ParseRangesError::TooManyValues` - If too many values in the range
pub fn parse_range(range: &str) -> std::result::Result<Range, ParseRangesError> {
    let ends = range
        .split('-')
        .map(|id| {
            if id.is_empty() {
                Ok(None)
            } else {
                Some(
                    id.parse::<usize>()
                        .map_err(|_| ParseRangesError::Parse(id.into())),
                )
                .transpose()
            }
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;

    match ends.len() {
        2 => Ok(Range {
            start: ends[0],
            end: ends[1],
        }),
        0 | 1 => Err(ParseRangesError::TooFewValues(range.to_string())),
        _ => Err(ParseRangesError::TooManyValues(range.to_string())),
    }
}

/// Parses multiple comma-separated byte range specifications (e.g., "0-1023,2048-4095").
///
/// # Errors
///
/// * `ParseRangesError::Parse` - If fails to parse a `usize`
/// * `ParseRangesError::TooFewValues` - If too few values in any range
/// * `ParseRangesError::TooManyValues` - If too many values in any range
pub fn parse_ranges(ranges: &str) -> std::result::Result<Vec<Range>, ParseRangesError> {
    ranges.split(',').map(parse_range).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_parse_range_with_both_bounds() {
        let range = parse_range("100-200").unwrap();
        assert_eq!(range.start, Some(100));
        assert_eq!(range.end, Some(200));
    }

    #[test_log::test]
    fn test_parse_range_with_start_only() {
        let range = parse_range("500-").unwrap();
        assert_eq!(range.start, Some(500));
        assert_eq!(range.end, None);
    }

    #[test_log::test]
    fn test_parse_range_with_end_only() {
        let range = parse_range("-1023").unwrap();
        assert_eq!(range.start, None);
        assert_eq!(range.end, Some(1023));
    }

    #[test_log::test]
    fn test_parse_range_zero_values() {
        let range = parse_range("0-0").unwrap();
        assert_eq!(range.start, Some(0));
        assert_eq!(range.end, Some(0));

        let range = parse_range("0-100").unwrap();
        assert_eq!(range.start, Some(0));
        assert_eq!(range.end, Some(100));
    }

    #[test_log::test]
    fn test_parse_range_invalid_number() {
        let result = parse_range("abc-123");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ParseRangesError::Parse(_)));
    }

    #[test_log::test]
    fn test_parse_range_too_few_values() {
        let result = parse_range("100");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ParseRangesError::TooFewValues(_)
        ));
    }

    #[test_log::test]
    fn test_parse_range_too_many_values() {
        let result = parse_range("100-200-300");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ParseRangesError::TooManyValues(_)
        ));
    }

    #[test_log::test]
    fn test_parse_range_empty_string() {
        let result = parse_range("");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ParseRangesError::TooFewValues(_)
        ));
    }

    #[test_log::test]
    fn test_parse_ranges_multiple_ranges() {
        let ranges = parse_ranges("0-100,200-300,400-").unwrap();
        assert_eq!(ranges.len(), 3);
        assert_eq!(ranges[0].start, Some(0));
        assert_eq!(ranges[0].end, Some(100));
        assert_eq!(ranges[1].start, Some(200));
        assert_eq!(ranges[1].end, Some(300));
        assert_eq!(ranges[2].start, Some(400));
        assert_eq!(ranges[2].end, None);
    }

    #[test_log::test]
    fn test_parse_ranges_single_range() {
        let ranges = parse_ranges("0-1023").unwrap();
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start, Some(0));
        assert_eq!(ranges[0].end, Some(1023));
    }

    #[test_log::test]
    fn test_parse_ranges_with_invalid_range() {
        let result = parse_ranges("0-100,invalid,200-300");
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_parse_ranges_empty_string() {
        let ranges = parse_ranges("").unwrap_err();
        assert!(matches!(ranges, ParseRangesError::TooFewValues(_)));
    }

    #[test_log::test]
    fn test_parse_range_large_values() {
        let range = parse_range("1000000-9999999").unwrap();
        assert_eq!(range.start, Some(1_000_000));
        assert_eq!(range.end, Some(9_999_999));
    }

    #[test_log::test]
    fn test_parse_range_open_range() {
        // An open range "-" (no start, no end) is valid per the parser logic
        // though semantically unusual for HTTP byte ranges
        let range = parse_range("-").unwrap();
        assert_eq!(range.start, None);
        assert_eq!(range.end, None);
    }

    #[test_log::test]
    fn test_parse_range_whitespace_in_numbers() {
        // Whitespace in numbers should fail parsing
        let result = parse_range(" 100-200");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ParseRangesError::Parse(_)));

        let result = parse_range("100- 200");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ParseRangesError::Parse(_)));
    }

    #[test_log::test]
    fn test_parse_ranges_empty_segments() {
        // Empty segments between commas should produce TooFewValues errors
        let result = parse_ranges("0-100,,200-300");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ParseRangesError::TooFewValues(_)
        ));
    }

    #[test_log::test]
    fn test_parse_range_negative_looking_number() {
        // "-100-200" splits as ["", "100", "200"] which has 3 elements (too many)
        let result = parse_range("-100-200");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ParseRangesError::TooManyValues(_)
        ));
    }
}
