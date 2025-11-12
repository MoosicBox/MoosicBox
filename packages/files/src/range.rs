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
