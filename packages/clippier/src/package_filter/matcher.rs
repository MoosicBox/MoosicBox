//! Filter matching logic.
//!
//! This module contains all the comparison operators and matching logic
//! for filtering packages based on their Cargo.toml properties.

use super::types::{FilterError, FilterExpression, FilterOperator, PackageFilter};
use regex::Regex;
use toml::Value;

/// Evaluate a filter expression against a package's TOML data.
///
/// # Errors
///
/// * Returns error if regex pattern is invalid
/// * Returns error if value cannot be parsed for length comparisons
pub fn evaluate_expression(
    expr: &FilterExpression,
    cargo_toml: &Value,
) -> Result<bool, FilterError> {
    match expr {
        FilterExpression::Condition(filter) => matches(filter, cargo_toml),
        FilterExpression::And(children) => {
            for child in children {
                if !evaluate_expression(child, cargo_toml)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        FilterExpression::Or(children) => {
            for child in children {
                if evaluate_expression(child, cargo_toml)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        FilterExpression::Not(child) => {
            let result = evaluate_expression(child, cargo_toml)?;
            Ok(!result)
        }
    }
}

/// Check if a filter matches a package's TOML data.
///
/// # Errors
///
/// * Returns error if regex pattern is invalid
/// * Returns error if value cannot be parsed for length comparisons
pub fn matches(filter: &PackageFilter, cargo_toml: &Value) -> Result<bool, FilterError> {
    // Navigate to the nested property
    let property_value = navigate_to_property(cargo_toml, &filter.property_path);

    match filter.operator {
        FilterOperator::Equals => Ok(match_equals(property_value, &filter.value)),
        FilterOperator::NotEquals => Ok(!match_equals(property_value, &filter.value)),
        FilterOperator::StartsWith => Ok(match_starts_with(property_value, &filter.value)),
        FilterOperator::EndsWith => Ok(match_ends_with(property_value, &filter.value)),
        FilterOperator::Contains => Ok(match_contains(property_value, &filter.value)),
        FilterOperator::RegexMatch => match_regex(property_value, &filter.value),
        FilterOperator::ArrayContains => Ok(match_array_contains(property_value, &filter.value)),
        FilterOperator::ArrayContainsSubstring => Ok(match_array_contains_substring(
            property_value,
            &filter.value,
        )),
        FilterOperator::ArrayContainsStartsWith => Ok(match_array_contains_starts_with(
            property_value,
            &filter.value,
        )),
        FilterOperator::ArrayContainsRegex => {
            match_array_contains_regex(property_value, &filter.value)
        }
        FilterOperator::ArrayEmpty => Ok(match_array_empty(property_value)),
        FilterOperator::ArrayLengthEquals => match_array_length_eq(property_value, &filter.value),
        FilterOperator::ArrayLengthGreater => match_array_length_gt(property_value, &filter.value),
        FilterOperator::ArrayLengthLess => match_array_length_lt(property_value, &filter.value),
        FilterOperator::ArrayNotContains => {
            Ok(!match_array_contains(property_value, &filter.value))
        }
        FilterOperator::Exists => Ok(match_exists(property_value)),
        FilterOperator::NotExists => Ok(!match_exists(property_value)),
    }
}

/// Navigate to a nested property in TOML structure.
///
/// # Arguments
///
/// * `toml` - The root TOML value
/// * `path` - Path segments to navigate (e.g., `["package", "metadata", "workspaces", "independent"]`)
///
/// # Returns
///
/// The value at the property path, or None if not found
#[must_use]
fn navigate_to_property<'a>(toml: &'a Value, path: &[String]) -> Option<&'a Value> {
    if path.is_empty() {
        return Some(toml);
    }

    let mut current = toml;

    for segment in path {
        match current.get(segment) {
            Some(value) => current = value,
            None => return None,
        }
    }

    Some(current)
}

// Scalar matchers

#[must_use]
fn match_equals(value: Option<&Value>, target: &str) -> bool {
    match value {
        Some(Value::Boolean(b)) => *b == (target == "true"),
        Some(Value::String(s)) => s == target,
        Some(Value::Integer(i)) => i.to_string() == target,
        None => target == "null",
        _ => false,
    }
}

#[must_use]
fn match_starts_with(value: Option<&Value>, target: &str) -> bool {
    match value {
        Some(Value::String(s)) => s.starts_with(target),
        _ => false,
    }
}

#[must_use]
fn match_ends_with(value: Option<&Value>, target: &str) -> bool {
    match value {
        Some(Value::String(s)) => s.ends_with(target),
        _ => false,
    }
}

#[must_use]
fn match_contains(value: Option<&Value>, target: &str) -> bool {
    match value {
        Some(Value::String(s)) => s.contains(target),
        _ => false,
    }
}

fn match_regex(value: Option<&Value>, pattern: &str) -> Result<bool, FilterError> {
    let re = Regex::new(pattern).map_err(|e| FilterError::InvalidRegex(e.to_string()))?;

    Ok(match value {
        Some(Value::String(s)) => re.is_match(s),
        _ => false,
    })
}

// Array matchers

#[must_use]
fn match_array_contains(value: Option<&Value>, target: &str) -> bool {
    match value {
        Some(Value::Array(arr)) => arr.iter().any(|v| {
            if let Value::String(s) = v {
                s == target
            } else {
                false
            }
        }),
        _ => false,
    }
}

#[must_use]
fn match_array_contains_substring(value: Option<&Value>, target: &str) -> bool {
    match value {
        Some(Value::Array(arr)) => arr.iter().any(|v| {
            if let Value::String(s) = v {
                s.contains(target)
            } else {
                false
            }
        }),
        _ => false,
    }
}

#[must_use]
fn match_array_contains_starts_with(value: Option<&Value>, target: &str) -> bool {
    match value {
        Some(Value::Array(arr)) => arr.iter().any(|v| {
            if let Value::String(s) = v {
                s.starts_with(target)
            } else {
                false
            }
        }),
        _ => false,
    }
}

fn match_array_contains_regex(value: Option<&Value>, pattern: &str) -> Result<bool, FilterError> {
    let re = Regex::new(pattern).map_err(|e| FilterError::InvalidRegex(e.to_string()))?;

    Ok(match value {
        Some(Value::Array(arr)) => arr.iter().any(|v| {
            if let Value::String(s) = v {
                re.is_match(s)
            } else {
                false
            }
        }),
        _ => false,
    })
}

const fn match_array_empty(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Array(arr)) => arr.is_empty(),
        None => true, // Missing property = empty
        _ => false,
    }
}

fn match_array_length_eq(value: Option<&Value>, target: &str) -> Result<bool, FilterError> {
    let target_len: usize = target
        .parse()
        .map_err(|_| FilterError::InvalidValue(format!("'{target}' is not a valid number")))?;

    Ok(match value {
        Some(Value::Array(arr)) => arr.len() == target_len,
        _ => false,
    })
}

fn match_array_length_gt(value: Option<&Value>, target: &str) -> Result<bool, FilterError> {
    let target_len: usize = target
        .parse()
        .map_err(|_| FilterError::InvalidValue(format!("'{target}' is not a valid number")))?;

    Ok(match value {
        Some(Value::Array(arr)) => arr.len() > target_len,
        _ => false,
    })
}

fn match_array_length_lt(value: Option<&Value>, target: &str) -> Result<bool, FilterError> {
    let target_len: usize = target
        .parse()
        .map_err(|_| FilterError::InvalidValue(format!("'{target}' is not a valid number")))?;

    Ok(match value {
        Some(Value::Array(arr)) => arr.len() < target_len,
        _ => false,
    })
}

// Existence matcher

const fn match_exists(value: Option<&Value>) -> bool {
    value.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_equals_boolean() {
        let toml = r#"
            [package]
            name = "test"
            publish = false
        "#;
        let value: Value = toml::from_str(toml).unwrap();
        let filter = super::super::parser::parse_filter("package.publish=false").unwrap();
        assert!(matches(&filter, &value).unwrap());
    }

    #[test]
    fn test_starts_with() {
        let toml = r#"
            [package]
            name = "test"
            version = "0.1.0"
        "#;
        let value: Value = toml::from_str(toml).unwrap();
        let filter = super::super::parser::parse_filter("package.version^=0.1").unwrap();
        assert!(matches(&filter, &value).unwrap());
    }

    #[test]
    fn test_array_contains() {
        let toml = r#"
            [package]
            name = "test"
            categories = ["audio", "multimedia"]
        "#;
        let value: Value = toml::from_str(toml).unwrap();
        let filter = super::super::parser::parse_filter("package.categories@=audio").unwrap();
        assert!(matches(&filter, &value).unwrap());
    }

    #[test]
    fn test_array_empty() {
        let toml = r#"
            [package]
            name = "test"
            keywords = []
        "#;
        let value: Value = toml::from_str(toml).unwrap();
        let filter = super::super::parser::parse_filter("package.keywords@!").unwrap();
        assert!(matches(&filter, &value).unwrap());
    }

    #[test]
    fn test_property_exists() {
        let toml = r#"
            [package]
            name = "test"
            readme = "README.md"
        "#;
        let value: Value = toml::from_str(toml).unwrap();
        let filter = super::super::parser::parse_filter("package.readme?").unwrap();
        assert!(matches(&filter, &value).unwrap());
    }

    #[test]
    fn test_property_not_exists() {
        let toml = r#"
            [package]
            name = "test"
        "#;
        let value: Value = toml::from_str(toml).unwrap();
        let filter = super::super::parser::parse_filter("package.homepage!?").unwrap();
        assert!(matches(&filter, &value).unwrap());
    }

    #[test]
    fn test_nested_property() {
        let toml = r"
            [package.metadata.workspaces]
            independent = true
        ";
        let value: Value = toml::from_str(toml).unwrap();
        let filter =
            super::super::parser::parse_filter("package.metadata.workspaces.independent=true")
                .unwrap();
        assert!(matches(&filter, &value).unwrap());
    }
}
