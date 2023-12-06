#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[derive(Clone, Copy, Debug)]
pub enum ParseIntError {
    InvalidDigit,
}

const fn parse_byte(b: u8, pow10: usize) -> Result<usize, ParseIntError> {
    let r = b.wrapping_sub(48);

    if r > 9 {
        Err(ParseIntError::InvalidDigit)
    } else {
        Ok((r as usize) * pow10)
    }
}

pub(crate) const POW10: [usize; 20] = {
    let mut array = [0; 20];
    let mut current = 1;

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

pub const fn parse(b: &str) -> Result<usize, ParseIntError> {
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

        result = result.wrapping_add(r);

        index += 1;
        index_const_table += 1;
    }

    Ok(result)
}

#[macro_export]
macro_rules! env_usize {
    ($name:expr $(,)?) => {
        match $crate::parse(env!($name)) {
            Ok(v) => v,
            Err(_e) => panic!("Environment variable not set"),
        }
    };
}

#[macro_export]
macro_rules! default_env_usize {
    ($name:expr, $default:expr) => {
        match $crate::option_env_usize!($name) {
            Some(v) => v,
            None => $default,
        }
    };
}

#[macro_export]
macro_rules! option_env_usize {
    ($name:expr $(,)?) => {
        match option_env!($name) {
            Some(v) => match $crate::parse(v) {
                Ok(v) => Some(v),
                Err(_e) => panic!("Invalid environment variable value"),
            },
            None => None,
        }
    };
}
