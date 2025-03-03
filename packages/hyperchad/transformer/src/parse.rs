#![allow(clippy::module_name_repetitions)]

use thiserror::Error;

use crate::{Calculation, Number};

#[derive(Debug, Error)]
pub enum GetNumberError {
    #[error("Failed to parse number '{0}'")]
    Parse(String),
}

/// # Errors
///
/// * If there is an unmatched ending ')'
/// * If there is an unmatched ending '}'
pub fn split_on_char(
    haystack: &str,
    needle: char,
    start: usize,
) -> Result<Option<(&str, &str)>, GetNumberError> {
    let mut pop_stack = vec![];

    for (i, char) in haystack.chars().enumerate().skip(start) {
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
                    GetNumberError::Parse(format!(
                        "Failed to find ending match to '{{' in \"{haystack}\""
                    )),
                );
                pop_stack.remove(0);
            }
            '(' => {
                pop_stack.insert(0, ')');
            }
            ')' => {
                moosicbox_assert::assert_or_err!(
                    pop_stack.first() == Some(&')'),
                    GetNumberError::Parse(format!(
                        "Failed to find ending match to '(' in \"{haystack}\""
                    )),
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

/// # Errors
///
/// * If the `split_on_char` fn failed.
pub fn split_on_char_trimmed(
    haystack: &str,
    needle: char,
    start: usize,
) -> Result<Option<(&str, &str)>, GetNumberError> {
    Ok(split_on_char(haystack, needle, start)?.map(|(x, y)| (x.trim(), y.trim())))
}

/// # Errors
///
/// * If the input is not a grouping.
/// * If the contents fails to parse.
pub fn parse_grouping(calc: &str) -> Result<Calculation, GetNumberError> {
    log::trace!("parse_grouping: '{calc}'");
    if let Some(contents) = calc.strip_prefix('(').and_then(|x| x.strip_suffix(')')) {
        log::trace!("parse_grouping: contents='{contents}'");
        Ok(Calculation::Grouping(Box::new(parse_calculation(
            contents,
        )?)))
    } else {
        let message = format!("Invalid grouping: '{calc}'");
        log::trace!("parse_grouping: failed='{message}'");
        Err(GetNumberError::Parse(message))
    }
}

/// # Errors
///
/// * If the input is not a `min` function.
/// * If the contents fails to parse.
pub fn parse_min(calc: &str) -> Result<Calculation, GetNumberError> {
    log::trace!("parse_min: '{calc}'");
    if let Some(contents) = calc
        .strip_prefix("min")
        .and_then(|x| x.trim_start().strip_prefix('('))
        .and_then(|x| x.strip_suffix(')'))
    {
        log::trace!("parse_min: contents='{contents}'");
        if let Some((left, right)) = split_on_char_trimmed(contents, ',', 0)? {
            log::trace!("parse_min: left='{left}' right='{right}'");
            return Ok(Calculation::Min(
                Box::new(parse_calculation(left)?),
                Box::new(parse_calculation(right)?),
            ));
        }
    }

    let message = format!("Invalid min: '{calc}'");
    log::trace!("parse_min: failed='{message}'");
    Err(GetNumberError::Parse(message))
}

/// # Errors
///
/// * If the input is not a `max` function.
/// * If the contents fails to parse.
pub fn parse_max(calc: &str) -> Result<Calculation, GetNumberError> {
    log::trace!("parse_max: '{calc}'");
    if let Some(contents) = calc
        .strip_prefix("max")
        .and_then(|x| x.trim_start().strip_prefix('('))
        .and_then(|x| x.strip_suffix(')'))
    {
        log::trace!("parse_max: contents='{contents}'");
        if let Some((left, right)) = split_on_char_trimmed(contents, ',', 0)? {
            log::trace!("parse_max: left='{left}' right='{right}'");
            return Ok(Calculation::Max(
                Box::new(parse_calculation(left)?),
                Box::new(parse_calculation(right)?),
            ));
        }
    }

    let message = format!("Invalid max: '{calc}'");
    log::trace!("parse_max: failed='{message}'");
    Err(GetNumberError::Parse(message))
}

/// # Errors
///
/// * If the input is not a `calc` function.
/// * If the contents fails to parse.
pub fn parse_calc(calc: &str) -> Result<Number, GetNumberError> {
    log::trace!("parse_calc: '{calc}'");
    if let Some(contents) = calc
        .strip_prefix("calc")
        .and_then(|x| x.trim().strip_prefix('('))
        .and_then(|x| x.strip_suffix(')'))
        .map(str::trim)
    {
        log::trace!("parse_calc: contents='{contents}'");
        return Ok(Number::Calc(parse_calculation(contents)?));
    }

    let message = format!("Invalid calc: '{calc}'");
    log::trace!("parse_calc: failed='{message}'");
    Err(GetNumberError::Parse(message))
}

/// # Errors
///
/// * If the `calc` fails to parse.
pub fn parse_calculation(calc: &str) -> Result<Calculation, GetNumberError> {
    if let Ok(min) = parse_min(calc) {
        return Ok(min);
    }
    if let Ok(max) = parse_max(calc) {
        return Ok(max);
    }
    if let Ok(grouping) = parse_grouping(calc) {
        return Ok(grouping);
    }
    if let Ok((left, right)) = parse_operation(calc, '*') {
        return Ok(Calculation::Multiply(Box::new(left), Box::new(right)));
    }
    if let Ok((left, right)) = parse_operation(calc, '/') {
        return Ok(Calculation::Divide(Box::new(left), Box::new(right)));
    }
    if let Ok((left, right)) = parse_signed_operation(calc, '+') {
        return Ok(Calculation::Add(Box::new(left), Box::new(right)));
    }
    if let Ok((left, right)) = parse_signed_operation(calc, '-') {
        return Ok(Calculation::Subtract(Box::new(left), Box::new(right)));
    }

    Ok(Calculation::Number(Box::new(parse_number(calc)?)))
}

fn parse_operation(
    calc: &str,
    operator: char,
) -> Result<(Calculation, Calculation), GetNumberError> {
    log::trace!("parse_operation: '{calc}' operator={operator}");
    if let Some((left, right)) = split_on_char_trimmed(calc, operator, 0)? {
        log::trace!("parse_operation: left='{left}' right='{right}'");
        return Ok((parse_calculation(left)?, parse_calculation(right)?));
    }

    let message = format!("Invalid operation: '{calc}'");
    log::trace!("parse_operation: failed='{message}'");
    Err(GetNumberError::Parse(message))
}

fn parse_signed_operation(
    calc: &str,
    operator: char,
) -> Result<(Calculation, Calculation), GetNumberError> {
    log::trace!("parse_signed_operation: '{calc}' operator={operator}");
    if let Some((left, right)) = split_on_char_trimmed(calc, operator, 0)? {
        if left.is_empty() {
            if let Some((left, right)) = split_on_char_trimmed(calc, operator, 1)? {
                log::trace!("parse_signed_operation: left='{left}' right='{right}'");
                if !left.is_empty() && !right.is_empty() {
                    return Ok((parse_calculation(left)?, parse_calculation(right)?));
                }
            }
        } else if !right.is_empty() {
            log::trace!("parse_signed_operation: left='{left}' right='{right}'");
            return Ok((parse_calculation(left)?, parse_calculation(right)?));
        }
    }

    let message = format!("Invalid signed operation: '{calc}'");
    log::trace!("parse_signed_operation: failed='{message}'");
    Err(GetNumberError::Parse(message))
}

/// # Errors
///
/// * If the input string is not a valid number.
#[allow(clippy::too_many_lines)]
pub fn parse_number(number: &str) -> Result<Number, GetNumberError> {
    static EPSILON: f32 = 0.00001;

    let mut number = if let Ok(calc) = parse_calc(number) {
        calc
    } else if let Some((number, _)) = number.split_once("dvw") {
        if number.contains('.') {
            Number::RealDvw(
                number
                    .parse::<f32>()
                    .map_err(|_| GetNumberError::Parse(number.to_string()))?,
            )
        } else {
            number
                .parse::<i64>()
                .ok()
                .map(Number::IntegerDvw)
                .or_else(|| number.parse::<f32>().ok().map(Number::RealDvw))
                .ok_or_else(|| GetNumberError::Parse(number.to_string()))?
        }
    } else if let Some((number, _)) = number.split_once("dvh") {
        if number.contains('.') {
            Number::RealDvh(
                number
                    .parse::<f32>()
                    .map_err(|_| GetNumberError::Parse(number.to_string()))?,
            )
        } else {
            number
                .parse::<i64>()
                .ok()
                .map(Number::IntegerDvh)
                .or_else(|| number.parse::<f32>().ok().map(Number::RealDvh))
                .ok_or_else(|| GetNumberError::Parse(number.to_string()))?
        }
    } else if let Some((number, _)) = number.split_once("vw") {
        if number.contains('.') {
            Number::RealVw(
                number
                    .parse::<f32>()
                    .map_err(|_| GetNumberError::Parse(number.to_string()))?,
            )
        } else {
            number
                .parse::<i64>()
                .ok()
                .map(Number::IntegerVw)
                .or_else(|| number.parse::<f32>().ok().map(Number::RealVw))
                .ok_or_else(|| GetNumberError::Parse(number.to_string()))?
        }
    } else if let Some((number, _)) = number.split_once("vh") {
        if number.contains('.') {
            Number::RealVh(
                number
                    .parse::<f32>()
                    .map_err(|_| GetNumberError::Parse(number.to_string()))?,
            )
        } else {
            number
                .parse::<i64>()
                .ok()
                .map(Number::IntegerVh)
                .or_else(|| number.parse::<f32>().ok().map(Number::RealVh))
                .ok_or_else(|| GetNumberError::Parse(number.to_string()))?
        }
    } else if let Some((number, _)) = number.split_once('%') {
        if number.contains('.') {
            Number::RealPercent(
                number
                    .parse::<f32>()
                    .map_err(|_| GetNumberError::Parse(number.to_string()))?,
            )
        } else {
            number
                .parse::<i64>()
                .ok()
                .map(Number::IntegerPercent)
                .or_else(|| number.parse::<f32>().ok().map(Number::RealPercent))
                .ok_or_else(|| GetNumberError::Parse(number.to_string()))?
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
        number
            .parse::<i64>()
            .ok()
            .map(Number::Integer)
            .or_else(|| number.parse::<f32>().ok().map(Number::Real))
            .ok_or_else(|| GetNumberError::Parse(number.to_string()))?
    };

    match &mut number {
        Number::Real(x)
        | Number::RealPercent(x)
        | Number::RealVw(x)
        | Number::RealVh(x)
        | Number::RealDvw(x)
        | Number::RealDvh(x) => {
            if x.is_sign_negative() && x.abs() < EPSILON {
                *x = 0.0;
            }
        }
        Number::Integer(..)
        | Number::IntegerPercent(..)
        | Number::Calc(..)
        | Number::IntegerVw(..)
        | Number::IntegerVh(..)
        | Number::IntegerDvw(..)
        | Number::IntegerDvh(..) => {}
    }

    Ok(number)
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use crate::{
        Calculation, Number,
        parse::{parse_calculation, split_on_char, split_on_char_trimmed},
    };

    #[test_log::test]
    fn split_on_char_returns_none_for_basic_floating_point_number() {
        assert_eq!(split_on_char("123.5", '+', 0).unwrap(), None);
    }

    #[test_log::test]
    fn split_on_char_returns_none_for_basic_integer_number() {
        assert_eq!(split_on_char("123", '+', 0).unwrap(), None);
    }

    #[test_log::test]
    fn split_on_char_returns_splits_on_plus_sign_with_floating_point_numbers() {
        assert_eq!(
            split_on_char("123.5 + 131.2", '+', 0).unwrap(),
            Some(("123.5 ", " 131.2"))
        );
    }

    #[test_log::test]
    fn split_on_char_returns_splits_on_plus_sign_with_integer_numbers() {
        assert_eq!(
            split_on_char("123 + 131", '+', 0).unwrap(),
            Some(("123 ", " 131"))
        );
    }

    #[test_log::test]
    fn split_on_char_trimmed_returns_splits_on_plus_sign_with_floating_point_numbers() {
        assert_eq!(
            split_on_char_trimmed("123.5 + 131.2", '+', 0).unwrap(),
            Some(("123.5", "131.2"))
        );
    }

    #[test_log::test]
    fn split_on_char_trimmed_returns_splits_on_plus_sign_with_integer_numbers() {
        assert_eq!(
            split_on_char_trimmed("123 + 131", '+', 0).unwrap(),
            Some(("123", "131"))
        );
    }

    #[test_log::test]
    fn split_on_char_trimmed_skips_char_in_parens_scope() {
        assert_eq!(
            split_on_char_trimmed("(123 + 131) + 100", '+', 0).unwrap(),
            Some(("(123 + 131)", "100"))
        );
    }

    #[test_log::test]
    fn split_on_char_trimmed_skips_char_in_nested_parens_scope() {
        assert_eq!(
            split_on_char_trimmed("(123 + (131 * 99)) + 100", '+', 0).unwrap(),
            Some(("(123 + (131 * 99))", "100"))
        );
    }

    #[test_log::test]
    fn parse_calculation_can_parse_basic_floating_point_number() {
        assert_eq!(
            parse_calculation("123.5").unwrap(),
            Calculation::Number(Box::new(Number::Real(123.5)))
        );
    }

    #[test_log::test]
    fn parse_calculation_can_parse_basic_integer_number() {
        assert_eq!(
            parse_calculation("123").unwrap(),
            Calculation::Number(Box::new(Number::Integer(123)))
        );
    }

    #[test_log::test]
    fn parse_calculation_can_parse_plus_sign_with_floating_point_numbers() {
        assert_eq!(
            parse_calculation("123.5 + 131.2").unwrap(),
            Calculation::Add(
                Box::new(Calculation::Number(Box::new(Number::Real(123.5)))),
                Box::new(Calculation::Number(Box::new(Number::Real(131.2))))
            )
        );
    }

    #[test_log::test]
    fn parse_calculation_can_parse_plus_sign_with_integer_numbers() {
        assert_eq!(
            parse_calculation("123 + 131").unwrap(),
            Calculation::Add(
                Box::new(Calculation::Number(Box::new(Number::Integer(123)))),
                Box::new(Calculation::Number(Box::new(Number::Integer(131))))
            )
        );
    }

    #[test_log::test]
    fn parse_calculation_can_parse_parens_scope() {
        assert_eq!(
            parse_calculation("(123 + 131) + 100").unwrap(),
            Calculation::Add(
                Box::new(Calculation::Grouping(Box::new(Calculation::Add(
                    Box::new(Calculation::Number(Box::new(Number::Integer(123)))),
                    Box::new(Calculation::Number(Box::new(Number::Integer(131))))
                )))),
                Box::new(Calculation::Number(Box::new(Number::Integer(100))))
            )
        );
    }

    #[test_log::test]
    fn parse_calculation_can_parse_nested_parens_scope() {
        assert_eq!(
            parse_calculation("(123 + (131 * 99)) + 100").unwrap(),
            Calculation::Add(
                Box::new(Calculation::Grouping(Box::new(Calculation::Add(
                    Box::new(Calculation::Number(Box::new(Number::Integer(123)))),
                    Box::new(Calculation::Grouping(Box::new(Calculation::Multiply(
                        Box::new(Calculation::Number(Box::new(Number::Integer(131)))),
                        Box::new(Calculation::Number(Box::new(Number::Integer(99))))
                    )))),
                )))),
                Box::new(Calculation::Number(Box::new(Number::Integer(100))))
            )
        );
    }

    #[test_log::test]
    fn parse_calculation_can_parse_min_with_two_integers() {
        assert_eq!(
            parse_calculation("min(123, 131)").unwrap(),
            Calculation::Min(
                Box::new(Calculation::Number(Box::new(Number::Integer(123)))),
                Box::new(Calculation::Number(Box::new(Number::Integer(131))))
            )
        );
    }

    #[test_log::test]
    fn parse_calculation_can_parse_min_with_a_space_before_paren() {
        assert_eq!(
            parse_calculation("min (123, 131)").unwrap(),
            Calculation::Min(
                Box::new(Calculation::Number(Box::new(Number::Integer(123)))),
                Box::new(Calculation::Number(Box::new(Number::Integer(131))))
            )
        );
    }

    #[test_log::test]
    fn parse_calculation_can_parse_min_with_two_floats() {
        assert_eq!(
            parse_calculation("min(123.5, 131.2)").unwrap(),
            Calculation::Min(
                Box::new(Calculation::Number(Box::new(Number::Real(123.5)))),
                Box::new(Calculation::Number(Box::new(Number::Real(131.2))))
            )
        );
    }

    #[test_log::test]
    fn parse_calculation_can_parse_max_with_two_integers() {
        assert_eq!(
            parse_calculation("max(123, 131)").unwrap(),
            Calculation::Max(
                Box::new(Calculation::Number(Box::new(Number::Integer(123)))),
                Box::new(Calculation::Number(Box::new(Number::Integer(131))))
            )
        );
    }

    #[test_log::test]
    fn parse_calculation_can_parse_max_with_a_space_before_paren() {
        assert_eq!(
            parse_calculation("max (123, 131)").unwrap(),
            Calculation::Max(
                Box::new(Calculation::Number(Box::new(Number::Integer(123)))),
                Box::new(Calculation::Number(Box::new(Number::Integer(131))))
            )
        );
    }

    #[test_log::test]
    fn parse_calculation_can_parse_max_with_two_floats() {
        assert_eq!(
            parse_calculation("max(123.5, 131.2)").unwrap(),
            Calculation::Max(
                Box::new(Calculation::Number(Box::new(Number::Real(123.5)))),
                Box::new(Calculation::Number(Box::new(Number::Real(131.2))))
            )
        );
    }

    #[test_log::test]
    fn parse_calculation_can_parse_nested_parens_scope_with_min_and_max_calls() {
        assert_eq!(
            parse_calculation("(123 + min(131 * max(100, 100%), 25)) + 100").unwrap(),
            Calculation::Add(
                Box::new(Calculation::Grouping(Box::new(Calculation::Add(
                    Box::new(Calculation::Number(Box::new(Number::Integer(123)))),
                    Box::new(Calculation::Min(
                        Box::new(Calculation::Multiply(
                            Box::new(Calculation::Number(Box::new(Number::Integer(131)))),
                            Box::new(Calculation::Max(
                                Box::new(Calculation::Number(Box::new(Number::Integer(100)))),
                                Box::new(Calculation::Number(Box::new(Number::IntegerPercent(
                                    100
                                ))))
                            )),
                        )),
                        Box::new(Calculation::Number(Box::new(Number::Integer(25)))),
                    )),
                )))),
                Box::new(Calculation::Number(Box::new(Number::Integer(100))))
            )
        );
    }

    #[test_log::test]
    fn parse_calculation_can_parse_negative_number_on_left_and_subtract() {
        assert_eq!(
            parse_calculation("-123 - 10").unwrap(),
            Calculation::Subtract(
                Box::new(Calculation::Number(Box::new(Number::Integer(-123)))),
                Box::new(Calculation::Number(Box::new(Number::Integer(10))))
            )
        );
    }

    #[test_log::test]
    fn parse_calculation_can_parse_negative_number_on_right_and_subtract() {
        assert_eq!(
            parse_calculation("123 - -10").unwrap(),
            Calculation::Subtract(
                Box::new(Calculation::Number(Box::new(Number::Integer(123)))),
                Box::new(Calculation::Number(Box::new(Number::Integer(-10))))
            )
        );
    }

    #[test_log::test]
    fn parse_calculation_can_parse_positive_number_on_left_and_subtract() {
        assert_eq!(
            parse_calculation("+123 - 10").unwrap(),
            Calculation::Subtract(
                Box::new(Calculation::Number(Box::new(Number::Integer(123)))),
                Box::new(Calculation::Number(Box::new(Number::Integer(10))))
            )
        );
    }

    #[test_log::test]
    fn parse_calculation_can_parse_positive_number_on_right_and_subtract() {
        assert_eq!(
            parse_calculation("123 - +10").unwrap(),
            Calculation::Subtract(
                Box::new(Calculation::Number(Box::new(Number::Integer(123)))),
                Box::new(Calculation::Number(Box::new(Number::Integer(10))))
            )
        );
    }

    #[test_log::test]
    fn parse_calculation_can_parse_negative_number_on_left_and_add() {
        assert_eq!(
            parse_calculation("-123 + 10").unwrap(),
            Calculation::Add(
                Box::new(Calculation::Number(Box::new(Number::Integer(-123)))),
                Box::new(Calculation::Number(Box::new(Number::Integer(10))))
            )
        );
    }

    #[test_log::test]
    fn parse_calculation_can_parse_negative_number_on_right_and_add() {
        assert_eq!(
            parse_calculation("123 + -10").unwrap(),
            Calculation::Add(
                Box::new(Calculation::Number(Box::new(Number::Integer(123)))),
                Box::new(Calculation::Number(Box::new(Number::Integer(-10))))
            )
        );
    }

    #[test_log::test]
    fn parse_calculation_can_parse_positive_number_on_left_and_add() {
        assert_eq!(
            parse_calculation("+123 + 10").unwrap(),
            Calculation::Add(
                Box::new(Calculation::Number(Box::new(Number::Integer(123)))),
                Box::new(Calculation::Number(Box::new(Number::Integer(10))))
            )
        );
    }

    #[test_log::test]
    fn parse_calculation_can_parse_positive_number_on_right_and_add() {
        assert_eq!(
            parse_calculation("123 + +10").unwrap(),
            Calculation::Add(
                Box::new(Calculation::Number(Box::new(Number::Integer(123)))),
                Box::new(Calculation::Number(Box::new(Number::Integer(10))))
            )
        );
    }
}
