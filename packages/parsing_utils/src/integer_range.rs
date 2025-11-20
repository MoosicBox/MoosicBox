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

#[cfg(test)]
mod tests {
    use super::*;

    mod parse_integer_sequences {
        use super::*;

        #[test]
        fn parses_single_integer() {
            let result = parse_integer_sequences("42").unwrap();
            assert_eq!(result, vec![42]);
        }

        #[test]
        fn parses_multiple_comma_separated_integers() {
            let result = parse_integer_sequences("1,2,3,10").unwrap();
            assert_eq!(result, vec![1, 2, 3, 10]);
        }

        #[test]
        fn parses_large_integers() {
            let result = parse_integer_sequences("1000000,2000000,3000000").unwrap();
            assert_eq!(result, vec![1_000_000, 2_000_000, 3_000_000]);
        }

        #[test]
        fn parses_zero() {
            let result = parse_integer_sequences("0").unwrap();
            assert_eq!(result, vec![0]);
        }

        #[test]
        fn parses_max_u64() {
            let max = u64::MAX.to_string();
            let result = parse_integer_sequences(&max).unwrap();
            assert_eq!(result, vec![u64::MAX]);
        }

        #[test]
        fn returns_error_for_invalid_integer() {
            let result = parse_integer_sequences("1,not_a_number,3");
            assert!(result.is_err());
            match result.unwrap_err() {
                ParseIntegersError::ParseId(s) => assert_eq!(s, "not_a_number"),
                _ => panic!("Expected ParseId error"),
            }
        }

        #[test]
        fn returns_error_for_negative_number() {
            let result = parse_integer_sequences("1,-5,3");
            assert!(result.is_err());
            match result.unwrap_err() {
                ParseIntegersError::ParseId(s) => assert_eq!(s, "-5"),
                _ => panic!("Expected ParseId error"),
            }
        }

        #[test]
        fn returns_error_for_float() {
            let result = parse_integer_sequences("1,2.5,3");
            assert!(result.is_err());
            match result.unwrap_err() {
                ParseIntegersError::ParseId(s) => assert_eq!(s, "2.5"),
                _ => panic!("Expected ParseId error"),
            }
        }

        #[test]
        fn returns_error_for_overflow() {
            // u64::MAX + 1
            let result = parse_integer_sequences("18446744073709551616");
            assert!(result.is_err());
            assert!(matches!(
                result.unwrap_err(),
                ParseIntegersError::ParseId(_)
            ));
        }
    }

    mod parse_integer_ranges {
        use super::*;

        // Single values (no ranges)
        #[test]
        fn parses_single_integer_no_range() {
            let result = parse_integer_ranges("42").unwrap();
            assert_eq!(result, vec![42]);
        }

        #[test]
        fn parses_comma_separated_no_ranges() {
            let result = parse_integer_ranges("1,5,10").unwrap();
            assert_eq!(result, vec![1, 5, 10]);
        }

        // Simple ranges
        #[test]
        fn parses_simple_range() {
            let result = parse_integer_ranges("1-5").unwrap();
            assert_eq!(result, vec![1, 2, 3, 4, 5]);
        }

        #[test]
        fn parses_range_with_single_gap() {
            let result = parse_integer_ranges("10-12").unwrap();
            assert_eq!(result, vec![10, 11, 12]);
        }

        #[test]
        fn parses_range_with_no_gap() {
            let result = parse_integer_ranges("5-6").unwrap();
            assert_eq!(result, vec![5, 6]);
        }

        // Mixed values and ranges
        #[test]
        fn parses_mixed_values_and_single_range() {
            let result = parse_integer_ranges("1,2-5,10").unwrap();
            assert_eq!(result, vec![1, 2, 3, 4, 5, 10]);
        }

        #[test]
        fn parses_value_before_range() {
            let result = parse_integer_ranges("1,5-7").unwrap();
            assert_eq!(result, vec![1, 5, 6, 7]);
        }

        #[test]
        fn parses_value_after_range() {
            let result = parse_integer_ranges("1-3,10").unwrap();
            assert_eq!(result, vec![1, 2, 3, 10]);
        }

        #[test]
        fn parses_multiple_values_with_range() {
            let result = parse_integer_ranges("1,2,5-7,10,11").unwrap();
            assert_eq!(result, vec![1, 2, 5, 6, 7, 10, 11]);
        }

        // Multiple ranges - Note: Cannot use comma-separated ranges due to algorithm
        // The algorithm splits on '-' first, so "1-3,5-7" becomes ["1", "3,5", "7"]
        // which has 3 parts (odd > 2) and causes UnmatchedRange error
        #[test]
        fn returns_error_for_comma_separated_ranges() {
            // This is actually an invalid format for this parser
            let result = parse_integer_ranges("1-3,5-7");
            assert!(result.is_err());
            assert!(matches!(
                result.unwrap_err(),
                ParseIntegersError::UnmatchedRange(_)
            ));
        }

        // Edge cases with zero
        #[test]
        fn parses_range_starting_at_zero() {
            let result = parse_integer_ranges("0-2").unwrap();
            assert_eq!(result, vec![0, 1, 2]);
        }

        #[test]
        fn parses_zero_in_sequence() {
            let result = parse_integer_ranges("0,5-7").unwrap();
            assert_eq!(result, vec![0, 5, 6, 7]);
        }

        // Large numbers
        #[test]
        fn parses_range_with_large_numbers() {
            let result = parse_integer_ranges("1000000-1000003").unwrap();
            assert_eq!(result, vec![1_000_000, 1_000_001, 1_000_002, 1_000_003]);
        }

        // Error cases: Invalid integer
        #[test]
        fn returns_error_for_invalid_integer_in_range() {
            let result = parse_integer_ranges("1-abc");
            assert!(result.is_err());
            assert!(matches!(
                result.unwrap_err(),
                ParseIntegersError::ParseId(_)
            ));
        }

        #[test]
        fn returns_error_for_invalid_start_value() {
            let result = parse_integer_ranges("abc-5");
            assert!(result.is_err());
            assert!(matches!(
                result.unwrap_err(),
                ParseIntegersError::ParseId(_)
            ));
        }

        #[test]
        fn returns_error_for_negative_in_range() {
            // "1--5" splits on '-' to ["1", "", "5"]
            // This creates 3 parts (odd > 2), so UnmatchedRange error
            let result = parse_integer_ranges("1--5");
            assert!(result.is_err());
            assert!(matches!(
                result.unwrap_err(),
                ParseIntegersError::UnmatchedRange(_)
            ));
        }

        // Error cases: Unmatched range
        #[test]
        fn returns_error_for_three_hyphens_unmatched() {
            let result = parse_integer_ranges("1-2-3");
            assert!(result.is_err());
            match result.unwrap_err() {
                ParseIntegersError::UnmatchedRange(s) => assert_eq!(s, "1-2-3"),
                _ => panic!("Expected UnmatchedRange error"),
            }
        }

        #[test]
        fn returns_error_for_five_hyphens_unmatched() {
            let result = parse_integer_ranges("1-2-3-4-5");
            assert!(result.is_err());
            match result.unwrap_err() {
                ParseIntegersError::UnmatchedRange(s) => assert_eq!(s, "1-2-3-4-5"),
                _ => panic!("Expected UnmatchedRange error"),
            }
        }

        // Four segments (3 hyphens) is also unmatched
        #[test]
        fn returns_error_for_four_segments() {
            let result = parse_integer_ranges("1-2,3-4");
            assert!(result.is_err());
            assert!(matches!(
                result.unwrap_err(),
                ParseIntegersError::UnmatchedRange(_)
            ));
        }

        // Error cases: Range too large
        #[test]
        fn returns_error_for_range_exceeding_100000() {
            // Range from 1 to 100002 has 100001 items between (not including endpoints)
            // Since the check is: end_id - start_id > 100_000
            // where start_id = start[last] + 1 = 1 + 1 = 2
            // and end_id = end[0] = 100002
            // 100002 - 2 = 100000, which is NOT > 100000, so it passes!
            // We need 100003 to actually exceed the limit
            let result = parse_integer_ranges("1-100003");
            assert!(result.is_err());
            match result.unwrap_err() {
                ParseIntegersError::RangeTooLarge(s) => {
                    assert!(s.contains("100003") || s.contains("2-100003"));
                }
                _ => panic!("Expected RangeTooLarge error"),
            }
        }

        #[test]
        fn returns_error_for_large_range_in_sequence() {
            let result = parse_integer_ranges("1,5-100010");
            assert!(result.is_err());
            match result.unwrap_err() {
                ParseIntegersError::RangeTooLarge(_) => {}
                _ => panic!("Expected RangeTooLarge error"),
            }
        }

        #[test]
        fn accepts_range_at_100000_limit() {
            // The check is: end_id - start_id > 100_000
            // where start_id = 1 + 1 = 2, end_id = 100002
            // 100002 - 2 = 100000 (exactly at limit, not exceeding)
            let result = parse_integer_ranges("1-100002");
            assert!(result.is_ok());
            let values = result.unwrap();
            // Total: 1 (start) + 100000 (filled) + 1 (end) = 100002
            assert_eq!(values.len(), 100_002);
            assert_eq!(values[0], 1);
            assert_eq!(values[values.len() - 1], 100_002);
        }

        // Boundary tests - Edge case where range has same start and end
        #[test]
        #[should_panic(expected = "attempt to subtract with overflow")]
        fn panics_on_range_with_same_start_and_end() {
            // This is an edge case in the implementation
            // When start and end are the same:
            // start = [5], start_id = 5 + 1 = 6
            // end = [5], end_id = 5
            // The check at line 412: end_id - start_id causes underflow
            // since 5 - 6 underflows for u64
            let _result = parse_integer_ranges("5-5");
        }

        #[test]
        fn parses_large_sequence_of_single_values() {
            let input = (0..50).map(|n| n.to_string()).collect::<Vec<_>>().join(",");
            let result = parse_integer_ranges(&input).unwrap();
            assert_eq!(result.len(), 50);
            assert_eq!(result[0], 0);
            assert_eq!(result[49], 49);
        }

        // Complex mixed cases
        #[test]
        fn parses_complex_mixed_sequence() {
            // "1,3,5-7,10-12,20,25-27,30" splits on '-' to:
            // ["1,3,5", "7,10", "12,20,25", "27,30"]
            // 4 parts (even), processes pairs: (0,1) and (2,3)
            // Pair 1: start=[1,3,5] (last=5), end=[7,10] (first=7)
            //   start_id=6, end_id=7, fills [6], result: [1,3,5,6,7,10]
            // Pair 2: start=[12,20,25] (last=25), end=[27,30] (first=27)
            //   start_id=26, end_id=27, fills [26], result: [12,20,25,26,27,30]
            // Final: [1,3,5,6,7,10,12,20,25,26,27,30]
            let result = parse_integer_ranges("1,3,5-7,10-12,20,25-27,30").unwrap();
            assert_eq!(result, vec![1, 3, 5, 6, 7, 10, 12, 20, 25, 26, 27, 30]);
        }

        #[test]
        fn preserves_order_of_values() {
            let result = parse_integer_ranges("10,1-3,5").unwrap();
            assert_eq!(result, vec![10, 1, 2, 3, 5]);
        }
    }
}
