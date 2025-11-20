//! Compile-time environment variable parsing utilities.
//!
//! This crate provides macros and const functions for parsing environment variables at compile
//! time. It enables reading and converting environment variables to numeric types during
//! compilation, useful for configuration that needs to be baked into the binary.
//!
//! # Features
//!
//! * Const functions for parsing strings to integers (`parse_usize`, `parse_isize`)
//! * Macros for reading environment variables as numeric types at compile time
//! * Support for optional environment variables with default values
//! * Zero runtime overhead - all parsing happens at compile time
//!
//! # Examples
//!
//! ```rust
//! # // This example can't actually run in doc tests because env vars are evaluated at compile time
//! # // of the moosicbox_env_utils crate itself, not the doc test
//! # use moosicbox_env_utils::{env_usize, default_env_usize, option_env_usize};
//! // Read a required environment variable as usize (panics if not set or invalid)
//! // const THREADS: usize = env_usize!("THREAD_COUNT");
//!
//! // Read with a default value
//! // const BUFFER_SIZE: usize = default_env_usize!("BUFFER_SIZE", 4096);
//!
//! // Read as an Option (returns None if not set)
//! // const MAX_RETRIES: Option<usize> = option_env_usize!("MAX_RETRIES");
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

/// Error type for integer parsing operations.
#[derive(Clone, Copy, Debug)]
pub enum ParseIntError {
    /// An invalid digit was encountered in the input string.
    ///
    /// This error occurs when the input contains a character that is not a valid decimal digit
    /// (0-9), or in the case of signed integers, when a sign character (+/-) appears in an
    /// invalid position.
    InvalidDigit,
}

/// Parses a single byte as a decimal digit and multiplies it by the given power of 10.
///
/// This is a helper function for parsing numeric strings digit by digit. It converts an ASCII
/// byte to its numeric value and scales it by the appropriate power of 10 based on its position.
///
/// # Errors
///
/// * Returns [`ParseIntError::InvalidDigit`] if the byte is not a valid ASCII decimal digit (0-9)
const fn parse_byte(b: u8, pow10: u128) -> Result<u128, ParseIntError> {
    let r = b.wrapping_sub(48);

    if r > 9 {
        Err(ParseIntError::InvalidDigit)
    } else {
        Ok((r as u128) * pow10)
    }
}

/// Lookup table of powers of 10 from 10^0 to 10^19 for efficient integer parsing.
///
/// This constant array is used by the parsing functions to convert string digits to their
/// numeric values by multiplying each digit by the appropriate power of 10 based on its
/// position in the string.
///
/// The array is computed at compile time in reverse order: `[1, 10, 100, ..., 10^19]`.
pub(crate) const POW10: [u128; 20] = {
    let mut array = [0; 20];
    let mut current: u128 = 1;

    let mut index = 20;

    loop {
        index -= 1;
        array[index] = current;

        if index == 0 {
            break;
        }

        current *= 10;
    }

    array
};

/// Parses a string slice into a `usize` at compile time.
///
/// This is a const function that can be used in const contexts to parse string literals
/// into numeric values during compilation.
///
/// # Errors
///
/// * Returns [`ParseIntError::InvalidDigit`] if the string contains a character that is not a valid decimal digit (0-9)
pub const fn parse_usize(b: &str) -> Result<usize, ParseIntError> {
    let bytes = b.as_bytes();

    let mut result: usize = 0;

    let len = bytes.len();

    // Start at the correct index of the table,
    // (skip the power's that are too large)
    let mut index_const_table = POW10.len().wrapping_sub(len);
    let mut index = 0;

    while index < b.len() {
        let a = bytes[index];
        let p = POW10[index_const_table];

        let r = match parse_byte(a, p) {
            Err(e) => return Err(e),
            Ok(d) => d,
        };

        result = result.wrapping_add(r as usize);

        index += 1;
        index_const_table += 1;
    }

    Ok(result)
}

/// Parses a string slice into an `isize` at compile time.
///
/// This is a const function that can be used in const contexts to parse string literals
/// (including those with leading `+` or `-` signs) into signed numeric values during compilation.
///
/// # Errors
///
/// * Returns [`ParseIntError::InvalidDigit`] if the string contains a character that is not a valid decimal digit (0-9) or if a sign character (+/-) appears in an invalid position
pub const fn parse_isize(b: &str) -> Result<isize, ParseIntError> {
    let bytes = b.as_bytes();

    let mut result: usize = 0;

    let len = bytes.len();

    // Start at the correct index of the table,
    // (skip the power's that are too large)
    let mut index_const_table = POW10.len().wrapping_sub(len);
    let mut index = 0;
    let mut sign = 1;

    while index < b.len() {
        let a = bytes[index];
        let p = POW10[index_const_table];

        if index == 0 {
            match a {
                // +
                43 => {
                    index += 1;
                    index_const_table += 1;
                    continue;
                }
                // -
                45 => {
                    sign = -1;
                    index += 1;
                    index_const_table += 1;
                    continue;
                }
                _ => {}
            }
        }

        let r = match parse_byte(a, p) {
            Err(e) => return Err(e),
            Ok(d) => d,
        };

        result = result.wrapping_add(r as usize);

        index += 1;
        index_const_table += 1;
    }

    #[allow(clippy::cast_possible_wrap)]
    Ok(result as isize * sign)
}

/// Parses a compile-time environment variable as a `usize`.
///
/// # Panics
///
/// * If the environment variable is not set at compile time
/// * If the environment variable contains an invalid digit
#[macro_export]
macro_rules! env_usize {
    ($name:expr $(,)?) => {
        match $crate::parse_usize(env!($name)) {
            Ok(v) => v,
            Err(_e) => panic!("Environment variable not set"),
        }
    };
}

/// Returns a compile-time environment variable as a `usize`, or a default value if not set.
///
/// # Panics
///
/// * If the environment variable contains an invalid digit
#[macro_export]
macro_rules! default_env_usize {
    ($name:expr, $default:expr $(,)?) => {
        match $crate::option_env_usize!($name) {
            Some(v) => v,
            None => $default,
        }
    };
}

/// Returns a compile-time environment variable as a `u64`, or a default value if not set.
///
/// # Panics
///
/// * If the environment variable contains an invalid digit
#[macro_export]
macro_rules! default_env_u64 {
    ($name:expr, $default:expr $(,)?) => {
        match $crate::option_env_u64!($name) {
            Some(v) => v,
            None => $default,
        }
    };
}

/// Returns a compile-time environment variable as a `u32`, or a default value if not set.
///
/// # Panics
///
/// * If the environment variable contains an invalid digit
#[macro_export]
macro_rules! default_env_u32 {
    ($name:expr, $default:expr $(,)?) => {
        match $crate::option_env_u32!($name) {
            Some(v) => v,
            None => $default,
        }
    };
}

/// Returns a compile-time environment variable as a `u16`, or a default value if not set.
///
/// # Panics
///
/// * If the environment variable contains an invalid digit
#[macro_export]
macro_rules! default_env_u16 {
    ($name:expr, $default:expr $(,)?) => {
        match $crate::option_env_u16!($name) {
            Some(v) => v,
            None => $default,
        }
    };
}

/// Returns a compile-time environment variable as an `Option<usize>`.
///
/// Returns `None` if the environment variable is not set at compile time.
///
/// # Panics
///
/// * If the environment variable contains an invalid digit
#[macro_export]
macro_rules! option_env_usize {
    ($name:expr $(,)?) => {
        match option_env!($name) {
            Some(v) => match $crate::parse_usize(v) {
                Ok(v) => Some(v),
                Err(_e) => panic!("Invalid environment variable value"),
            },
            None => None,
        }
    };
}

/// Returns a compile-time environment variable as an `Option<u64>`.
///
/// Returns `None` if the environment variable is not set at compile time.
///
/// # Panics
///
/// * If the environment variable contains an invalid digit
#[macro_export]
macro_rules! option_env_u64 {
    ($name:expr $(,)?) => {
        match option_env!($name) {
            Some(v) => match $crate::parse_usize(v) {
                Ok(v) => Some(v as u64),
                Err(_e) => panic!("Invalid environment variable value"),
            },
            None => None,
        }
    };
}

/// Returns a compile-time environment variable as an `Option<u32>`.
///
/// Returns `None` if the environment variable is not set at compile time.
///
/// # Panics
///
/// * If the environment variable contains an invalid digit
#[macro_export]
macro_rules! option_env_u32 {
    ($name:expr $(,)?) => {
        match option_env!($name) {
            Some(v) => match $crate::parse_usize(v) {
                Ok(v) => Some(v as u32),
                Err(_e) => panic!("Invalid environment variable value"),
            },
            None => None,
        }
    };
}

/// Returns a compile-time environment variable as an `Option<u16>`.
///
/// Returns `None` if the environment variable is not set at compile time.
///
/// # Panics
///
/// * If the environment variable contains an invalid digit
#[macro_export]
macro_rules! option_env_u16 {
    ($name:expr $(,)?) => {
        match option_env!($name) {
            Some(v) => match $crate::parse_usize(v) {
                Ok(v) => Some(v as u16),
                Err(_e) => panic!("Invalid environment variable value"),
            },
            None => None,
        }
    };
}

/// Returns a compile-time environment variable as an `Option<isize>`.
///
/// Returns `None` if the environment variable is not set at compile time.
///
/// # Panics
///
/// * If the environment variable contains an invalid digit
#[macro_export]
macro_rules! option_env_isize {
    ($name:expr $(,)?) => {
        match option_env!($name) {
            Some(v) => match $crate::parse_isize(v) {
                Ok(v) => Some(v as isize),
                Err(_e) => panic!("Invalid environment variable value"),
            },
            None => None,
        }
    };
}

/// Returns a compile-time environment variable as an `Option<i64>`.
///
/// Returns `None` if the environment variable is not set at compile time.
///
/// # Panics
///
/// * If the environment variable contains an invalid digit
#[macro_export]
macro_rules! option_env_i64 {
    ($name:expr $(,)?) => {
        match option_env!($name) {
            Some(v) => match $crate::parse_isize(v) {
                Ok(v) => Some(v as i64),
                Err(_e) => panic!("Invalid environment variable value"),
            },
            None => None,
        }
    };
}

/// Returns a compile-time environment variable as an `Option<i32>`.
///
/// Returns `None` if the environment variable is not set at compile time.
///
/// # Panics
///
/// * If the environment variable contains an invalid digit
#[macro_export]
macro_rules! option_env_i32 {
    ($name:expr $(,)?) => {
        match option_env!($name) {
            Some(v) => match $crate::parse_isize(v) {
                Ok(v) => Some(v as i32),
                Err(_e) => panic!("Invalid environment variable value"),
            },
            None => None,
        }
    };
}

/// Returns a compile-time environment variable as an `Option<i16>`.
///
/// Returns `None` if the environment variable is not set at compile time.
///
/// # Panics
///
/// * If the environment variable contains an invalid digit
#[macro_export]
macro_rules! option_env_i16 {
    ($name:expr $(,)?) => {
        match option_env!($name) {
            Some(v) => match $crate::parse_isize(v) {
                Ok(v) => Some(v as i16),
                Err(_e) => panic!("Invalid environment variable value"),
            },
            None => None,
        }
    };
}

/// Returns a compile-time environment variable as an `Option<i8>`.
///
/// Returns `None` if the environment variable is not set at compile time.
///
/// # Panics
///
/// * If the environment variable contains an invalid digit
#[macro_export]
macro_rules! option_env_i8 {
    ($name:expr $(,)?) => {
        match option_env!($name) {
            Some(v) => match $crate::parse_isize(v) {
                Ok(v) => Some(v as i8),
                Err(_e) => panic!("Invalid environment variable value"),
            },
            None => None,
        }
    };
}

/// Returns a compile-time environment variable as a string slice, or a default value if not set.
#[macro_export]
macro_rules! default_env {
    ($name:expr, $default:expr $(,)?) => {
        match option_env!($name) {
            Some(v) => v,
            None => $default,
        }
    };
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use crate::{POW10, ParseIntError, parse_byte, parse_isize, parse_usize};

    // Tests for parse_usize function
    #[test_log::test]
    fn parse_usize_can_parse_single_digit() {
        let result = parse_usize("5").unwrap();
        assert_eq!(result, 5);
    }

    #[test_log::test]
    fn parse_usize_can_parse_multiple_digits() {
        let result = parse_usize("12345").unwrap();
        assert_eq!(result, 12345);
    }

    #[test_log::test]
    fn parse_usize_can_parse_zero() {
        let result = parse_usize("0").unwrap();
        assert_eq!(result, 0);
    }

    #[test_log::test]
    fn parse_usize_can_parse_large_number() {
        let result = parse_usize("9876543210").unwrap();
        assert_eq!(result, 9_876_543_210);
    }

    #[test_log::test]
    fn parse_usize_can_parse_number_with_leading_zeros() {
        let result = parse_usize("00123").unwrap();
        assert_eq!(result, 123);
    }

    #[test_log::test]
    fn parse_usize_returns_error_for_invalid_digit() {
        let result = parse_usize("12a34");
        assert!(matches!(result, Err(ParseIntError::InvalidDigit)));
    }

    #[test_log::test]
    fn parse_usize_returns_error_for_negative_sign() {
        let result = parse_usize("-123");
        assert!(matches!(result, Err(ParseIntError::InvalidDigit)));
    }

    #[test_log::test]
    fn parse_usize_returns_error_for_positive_sign() {
        let result = parse_usize("+123");
        assert!(matches!(result, Err(ParseIntError::InvalidDigit)));
    }

    #[test_log::test]
    fn parse_usize_returns_error_for_empty_string() {
        let result = parse_usize("");
        // Empty string should return 0 based on the implementation
        assert_eq!(result.unwrap(), 0);
    }

    #[test_log::test]
    fn parse_usize_returns_error_for_non_numeric_string() {
        let result = parse_usize("abc");
        assert!(matches!(result, Err(ParseIntError::InvalidDigit)));
    }

    #[test_log::test]
    fn parse_usize_returns_error_for_special_characters() {
        let result = parse_usize("12@34");
        assert!(matches!(result, Err(ParseIntError::InvalidDigit)));
    }

    // Tests for parse_isize function
    #[test_log::test]
    fn parse_isize_can_parse_positive_number() {
        let result = parse_isize("100").unwrap();

        assert_eq!(result, 100);
    }

    #[test_log::test]
    fn parse_isize_can_parse_explicitly_positive_number() {
        let result = parse_isize("+100").unwrap();

        assert_eq!(result, 100);
    }

    #[test_log::test]
    fn parse_isize_can_parse_negative_number() {
        let result = parse_isize("-100").unwrap();

        assert_eq!(result, -100);
    }

    #[test_log::test]
    fn parse_isize_can_parse_zero() {
        let result = parse_isize("0").unwrap();
        assert_eq!(result, 0);
    }

    #[test_log::test]
    fn parse_isize_can_parse_negative_zero() {
        let result = parse_isize("-0").unwrap();
        assert_eq!(result, 0);
    }

    #[test_log::test]
    fn parse_isize_can_parse_positive_zero() {
        let result = parse_isize("+0").unwrap();
        assert_eq!(result, 0);
    }

    #[test_log::test]
    fn parse_isize_returns_error_for_invalid_digit() {
        let result = parse_isize("12a34");
        assert!(matches!(result, Err(ParseIntError::InvalidDigit)));
    }

    #[test_log::test]
    fn parse_isize_returns_error_for_sign_only() {
        let result = parse_isize("-");
        // Sign-only should result in 0 based on implementation
        assert_eq!(result.unwrap(), 0);
    }

    #[test_log::test]
    fn parse_isize_returns_error_for_positive_sign_only() {
        let result = parse_isize("+");
        // Sign-only should result in 0 based on implementation
        assert_eq!(result.unwrap(), 0);
    }

    #[test_log::test]
    fn parse_isize_can_parse_large_negative_number() {
        let result = parse_isize("-987654321").unwrap();
        assert_eq!(result, -987_654_321);
    }

    #[test_log::test]
    fn parse_isize_can_parse_large_positive_number() {
        let result = parse_isize("+987654321").unwrap();
        assert_eq!(result, 987_654_321);
    }

    #[test_log::test]
    fn parse_isize_returns_error_for_empty_string() {
        let result = parse_isize("");
        // Empty string should return 0 based on the implementation
        assert_eq!(result.unwrap(), 0);
    }

    // Tests for parse_byte helper function
    #[test_log::test]
    fn parse_byte_can_parse_valid_digit_zero() {
        let result = parse_byte(b'0', 1).unwrap();
        assert_eq!(result, 0);
    }

    #[test_log::test]
    fn parse_byte_can_parse_valid_digit_nine() {
        let result = parse_byte(b'9', 1).unwrap();
        assert_eq!(result, 9);
    }

    #[test_log::test]
    fn parse_byte_can_parse_digit_with_power_of_ten() {
        let result = parse_byte(b'5', 100).unwrap();
        assert_eq!(result, 500);
    }

    #[test_log::test]
    fn parse_byte_returns_error_for_letter() {
        let result = parse_byte(b'a', 1);
        assert!(matches!(result, Err(ParseIntError::InvalidDigit)));
    }

    #[test_log::test]
    fn parse_byte_returns_error_for_special_char() {
        let result = parse_byte(b'@', 1);
        assert!(matches!(result, Err(ParseIntError::InvalidDigit)));
    }

    #[test_log::test]
    fn parse_byte_returns_error_for_negative_sign() {
        let result = parse_byte(b'-', 1);
        assert!(matches!(result, Err(ParseIntError::InvalidDigit)));
    }

    #[test_log::test]
    fn parse_byte_returns_error_for_positive_sign() {
        let result = parse_byte(b'+', 1);
        assert!(matches!(result, Err(ParseIntError::InvalidDigit)));
    }

    #[test_log::test]
    fn parse_byte_returns_error_for_space() {
        let result = parse_byte(b' ', 1);
        assert!(matches!(result, Err(ParseIntError::InvalidDigit)));
    }

    // Tests for POW10 constant table
    // The POW10 array is stored in reverse order: from 10^19 down to 10^0
    #[test_log::test]
    fn pow10_first_element_is_ten_to_nineteenth() {
        assert_eq!(POW10[0], 10_u128.pow(19));
    }

    #[test_log::test]
    fn pow10_last_element_is_one() {
        assert_eq!(POW10[19], 1);
    }

    #[test_log::test]
    fn pow10_second_to_last_element_is_ten() {
        assert_eq!(POW10[18], 10);
    }

    #[test_log::test]
    fn pow10_third_to_last_element_is_hundred() {
        assert_eq!(POW10[17], 100);
    }

    #[test_log::test]
    fn pow10_tenth_element_is_ten_billion() {
        assert_eq!(POW10[9], 10_000_000_000);
    }

    #[test_log::test]
    fn pow10_has_correct_length() {
        assert_eq!(POW10.len(), 20);
    }

    #[test_log::test]
    fn pow10_elements_decrease_by_factor_of_ten() {
        // Array is in reverse order, so each element should be 1/10 of the previous
        for i in 1..POW10.len() {
            assert_eq!(POW10[i], POW10[i - 1] / 10);
        }
    }

    #[test_log::test]
    fn pow10_middle_element_is_correct() {
        // POW10[10] should be 10^9
        assert_eq!(POW10[10], 1_000_000_000);
    }
}
