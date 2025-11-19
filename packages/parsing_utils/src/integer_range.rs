//! Integer parsing utilities for sequences and ranges.
//!
//! This module provides functions to parse comma-separated integers and hyphen-separated ranges
//! from strings into vectors of `u64` values.

use thiserror::Error;

/// Errors that can occur when parsing integer sequences or ranges.
#[derive(Debug, Error)]
pub enum ParseIntegersError {
    /// Failed to parse a string segment as a `u64` integer.
    ///
    /// Contains the invalid string that could not be parsed.
    #[error("Could not parse integers: {0}")]
    ParseId(String),
    /// Range specification has an invalid format (odd number of range separators).
    ///
    /// Contains the invalid range string.
    #[error("Unmatched range: {0}")]
    UnmatchedRange(String),
    /// Range span exceeds the maximum allowed size of 100,000 items.
    ///
    /// Contains the range specification that was too large.
    #[error("Range too large: {0}")]
    RangeTooLarge(String),
}

/// Parses a comma-separated string of integers into a vector of `u64` values.
///
/// # Examples
///
/// ```rust
/// use moosicbox_parsing_utils::integer_range::parse_integer_sequences;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let result = parse_integer_sequences("1,2,3,10")?;
/// assert_eq!(result, vec![1, 2, 3, 10]);
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// * `ParseIntegersError::ParseId` - If any segment cannot be parsed as a valid `u64` integer
pub fn parse_integer_sequences(
    integers: &str,
) -> std::result::Result<Vec<u64>, ParseIntegersError> {
    integers
        .split(',')
        .map(|id| {
            id.parse::<u64>()
                .map_err(|_| ParseIntegersError::ParseId(id.into()))
        })
        .collect::<std::result::Result<Vec<_>, _>>()
}

/// Parses a string containing comma-separated integers and hyphen-separated ranges into a vector of `u64` values.
///
/// # Examples
///
/// ```rust
/// use moosicbox_parsing_utils::integer_range::parse_integer_ranges;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // Parse a mix of individual integers and ranges
/// let result = parse_integer_ranges("1,2-5,10")?;
/// assert_eq!(result, vec![1, 2, 3, 4, 5, 10]);
///
/// // Parse just individual integers (no ranges)
/// let result = parse_integer_ranges("1,5,10")?;
/// assert_eq!(result, vec![1, 5, 10]);
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// * `ParseIntegersError::ParseId` - If any segment cannot be parsed as a valid `u64` integer
/// * `ParseIntegersError::UnmatchedRange` - If the range specification has an invalid format (more than 2 range separators and an odd count)
/// * `ParseIntegersError::RangeTooLarge` - If any range span exceeds 100,000 items
///
/// # Panics
///
/// Panics if range segments contain empty values between commas (e.g., `"1,,3-5"` or `"1-,3"`), which would result in indexing empty vectors
pub fn parse_integer_ranges(
    integer_ranges: &str,
) -> std::result::Result<Vec<u64>, ParseIntegersError> {
    let ranges = integer_ranges.split('-').collect::<Vec<_>>();

    if ranges.len() == 1 {
        parse_integer_sequences(ranges[0])
    } else if ranges.len() > 2 && ranges.len() % 2 == 1 {
        Err(ParseIntegersError::UnmatchedRange(integer_ranges.into()))
    } else {
        let mut i = 0;
        let mut ids = Vec::new();

        while i < ranges.len() {
            let mut start = parse_integer_sequences(ranges[i])?;
            let mut start_id = start[start.len() - 1] + 1;
            let mut end = parse_integer_sequences(ranges[i + 1])?;
            let end_id = end[0];

            if end_id - start_id > 100_000 {
                return Err(ParseIntegersError::RangeTooLarge(format!(
                    "{}-{}",
                    start_id - 1,
                    end_id,
                )));
            }

            ids.append(&mut start);

            while start_id < end_id {
                ids.push(start_id);
                start_id += 1;
            }

            ids.append(&mut end);

            i += 2;
        }

        Ok(ids)
    }
}
