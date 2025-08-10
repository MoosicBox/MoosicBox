#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[derive(Clone, Copy, Debug)]
pub enum ParseIntError {
    InvalidDigit,
}

const fn parse_byte(b: u8, pow10: u128) -> Result<u128, ParseIntError> {
    let r = b.wrapping_sub(48);

    if r > 9 {
        Err(ParseIntError::InvalidDigit)
    } else {
        Ok((r as u128) * pow10)
    }
}

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

/// # Errors
///
/// * If encounters an invalid digit in the `&str`
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

/// # Errors
///
/// * If encounters an invalid digit in the `&str`
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

#[macro_export]
macro_rules! env_usize {
    ($name:expr $(,)?) => {
        match $crate::parse_usize(env!($name)) {
            Ok(v) => v,
            Err(_e) => panic!("Environment variable not set"),
        }
    };
}

#[macro_export]
macro_rules! default_env_usize {
    ($name:expr, $default:expr $(,)?) => {
        match $crate::option_env_usize!($name) {
            Some(v) => v,
            None => $default,
        }
    };
}

#[macro_export]
macro_rules! default_env_u64 {
    ($name:expr, $default:expr $(,)?) => {
        match $crate::option_env_u64!($name) {
            Some(v) => v,
            None => $default,
        }
    };
}

#[macro_export]
macro_rules! default_env_u32 {
    ($name:expr, $default:expr $(,)?) => {
        match $crate::option_env_u32!($name) {
            Some(v) => v,
            None => $default,
        }
    };
}

#[macro_export]
macro_rules! default_env_u16 {
    ($name:expr, $default:expr $(,)?) => {
        match $crate::option_env_u16!($name) {
            Some(v) => v,
            None => $default,
        }
    };
}

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
