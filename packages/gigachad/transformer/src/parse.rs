#![allow(clippy::module_name_repetitions)]

use thiserror::Error;

use crate::{Calculation, Number};

#[derive(Debug, Error)]
pub enum GetNumberError {
    #[error("Failed to parse number '{0}'")]
    Parse(String),
}

pub fn split_on_char(haystack: &str, needle: char) -> Result<Option<(&str, &str)>, GetNumberError> {
    let mut pop_stack = vec![];

    for (i, char) in haystack.chars().enumerate() {
        if pop_stack.is_empty() && char == needle {
            let (a, b) = haystack.split_at(i);
            return Ok(Some((a, &b[1..])));
        }

        match char {
            '{' => {
                pop_stack.insert(0, '}');
            }
            '}' => {
                moosicbox_assert::assert_or_err!(
                    pop_stack.first() == Some(&'}'),
                    GetNumberError::Parse("Failed to find ending match to {".to_string()),
                );
                pop_stack.remove(0);
            }
            '(' => {
                pop_stack.insert(0, ')');
            }
            ')' => {
                moosicbox_assert::assert_or_err!(
                    pop_stack.first() == Some(&')'),
                    GetNumberError::Parse("Failed to find ending match to (".to_string()),
                );
                if pop_stack.first() == Some(&')') {
                    pop_stack.remove(0);
                }
            }
            _ => {}
        }
    }

    Ok(None)
}

pub fn split_on_char_trimmed(
    haystack: &str,
    needle: char,
) -> Result<Option<(&str, &str)>, GetNumberError> {
    Ok(split_on_char(haystack, needle)?.map(|(x, y)| (x.trim(), y.trim())))
}

pub fn parse_calculation(calc: &str) -> Result<Calculation, GetNumberError> {
    Ok(
        if let Some((left, right)) = split_on_char_trimmed(calc, '+')? {
            Calculation::Add(
                Box::new(parse_calculation(left)?),
                Box::new(parse_calculation(right)?),
            )
        } else if let Some((left, right)) = split_on_char_trimmed(calc, '-')? {
            Calculation::Subtract(
                Box::new(parse_calculation(left)?),
                Box::new(parse_calculation(right)?),
            )
        } else if let Some((left, right)) = split_on_char_trimmed(calc, '*')? {
            Calculation::Multiply(
                Box::new(parse_calculation(left)?),
                Box::new(parse_calculation(right)?),
            )
        } else if let Some((left, right)) = split_on_char_trimmed(calc, '/')? {
            Calculation::Divide(
                Box::new(parse_calculation(left)?),
                Box::new(parse_calculation(right)?),
            )
        } else {
            Calculation::Number(Box::new(parse_number(calc)?))
        },
    )
}

pub fn parse_number(number: &str) -> Result<Number, GetNumberError> {
    Ok(
        if let Some(calc) = number
            .strip_prefix("calc(")
            .and_then(|x| x.strip_suffix(")"))
            .map(str::trim)
        {
            Number::Calc(parse_calculation(calc)?)
        } else if let Some((number, _)) = number.split_once('%') {
            if number.contains('.') {
                Number::RealPercent(
                    number
                        .parse::<f32>()
                        .map_err(|_| GetNumberError::Parse(number.to_string()))?,
                )
            } else {
                Number::IntegerPercent(
                    number
                        .parse::<u64>()
                        .map_err(|_| GetNumberError::Parse(number.to_string()))?,
                )
            }
        } else if number.contains('.') {
            let number = number.strip_suffix("px").unwrap_or(number);
            Number::Real(
                number
                    .parse::<f32>()
                    .map_err(|_| GetNumberError::Parse(number.to_string()))?,
            )
        } else {
            let number = number.strip_suffix("px").unwrap_or(number);
            Number::Integer(
                number
                    .parse::<u64>()
                    .map_err(|_| GetNumberError::Parse(number.to_string()))?,
            )
        },
    )
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use crate::parse::{split_on_char, split_on_char_trimmed};

    #[test_log::test]
    fn split_on_char_returns_none_for_basic_floating_point_number() {
        assert_eq!(split_on_char("123.5", '+').unwrap(), None);
    }

    #[test_log::test]
    fn split_on_char_returns_none_for_basic_integer_number() {
        assert_eq!(split_on_char("123", '+').unwrap(), None);
    }

    #[test_log::test]
    fn split_on_char_returns_splits_on_plus_sign_with_floating_point_numbers() {
        assert_eq!(
            split_on_char("123.5 + 131.2", '+').unwrap(),
            Some(("123.5 ", " 131.2"))
        );
    }

    #[test_log::test]
    fn split_on_char_returns_splits_on_plus_sign_with_integer_numbers() {
        assert_eq!(
            split_on_char("123 + 131", '+').unwrap(),
            Some(("123 ", " 131"))
        );
    }

    #[test_log::test]
    fn split_on_char_trimmed_returns_splits_on_plus_sign_with_floating_point_numbers() {
        assert_eq!(
            split_on_char_trimmed("123.5 + 131.2", '+').unwrap(),
            Some(("123.5", "131.2"))
        );
    }

    #[test_log::test]
    fn split_on_char_trimmed_returns_splits_on_plus_sign_with_integer_numbers() {
        assert_eq!(
            split_on_char_trimmed("123 + 131", '+').unwrap(),
            Some(("123", "131"))
        );
    }
}
