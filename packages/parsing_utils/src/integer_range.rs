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
/// # Errors
///
/// * If a number fails to parse to a u64
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
/// # Errors
///
/// * If a number fails to parse to a u64
/// * If a range is unmatched (odd number of range separators)
/// * If a range is too large (> 100,000)
///
/// # Panics
///
/// * If the input string contains empty comma-separated segments that result in empty vectors
/// * If indexing operations on internal vectors fail (should not happen with valid input)
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
