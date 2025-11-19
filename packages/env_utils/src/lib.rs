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

    use crate::parse_isize;

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
}
