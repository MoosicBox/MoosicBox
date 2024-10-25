#![allow(clippy::module_name_repetitions)]

use thiserror::Error;

use crate::{Calculation, Number};

#[derive(Debug, Error)]
pub enum GetNumberError {
    #[error("Failed to parse number '{0}'")]
    Parse(String),
}

pub fn parse_calculation(calc: &str) -> Result<Calculation, GetNumberError> {
    Ok(
        if let Some((left, right)) = calc.split_once('+').map(|(x, y)| (x.trim(), y.trim())) {
            Calculation::Add(
                Box::new(parse_calculation(left)?),
                Box::new(parse_calculation(right)?),
            )
        } else if let Some((left, right)) = calc.split_once('-').map(|(x, y)| (x.trim(), y.trim()))
        {
            Calculation::Subtract(
                Box::new(parse_calculation(left)?),
                Box::new(parse_calculation(right)?),
            )
        } else if let Some((left, right)) = calc.split_once('*').map(|(x, y)| (x.trim(), y.trim()))
        {
            Calculation::Multiply(
                Box::new(parse_calculation(left)?),
                Box::new(parse_calculation(right)?),
            )
        } else if let Some((left, right)) = calc.split_once('/').map(|(x, y)| (x.trim(), y.trim()))
        {
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
