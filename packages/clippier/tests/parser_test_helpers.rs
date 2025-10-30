//! Helper utilities specific to parser comprehensive tests.
//!
//! These are convenience functions for common patterns in parser testing.

use clippier::package_filter::{FilterError, FilterExpression, FilterOperator};

/// Validate that a result is a specific error type.
#[track_caller]
pub fn assert_error_type<T>(
    result: Result<T, FilterError>,
    check: impl FnOnce(&FilterError) -> bool,
    description: &str,
) {
    match result {
        Err(err) => {
            assert!(
                check(&err),
                "Error type check failed for {description}: {err:?}"
            );
        }
        Ok(_) => panic!("Expected {description} error but got Ok"),
    }
}

/// Validate AND expression with specific simple conditions (property=value).
///
/// This is a convenience wrapper. Requires expression_assertions to be imported in the test file.
#[track_caller]
pub fn assert_and_with_conditions(expr: &FilterExpression, expected_conditions: &[(&str, &str)]) {
    // Inline the dependency to avoid module loading issues
    fn assert_and_with_n_children_inline(
        expr: &FilterExpression,
        expected_count: usize,
    ) -> &Vec<FilterExpression> {
        match expr {
            FilterExpression::And(children) => {
                assert_eq!(
                    children.len(),
                    expected_count,
                    "AND node should have {expected_count} children, got {}",
                    children.len()
                );
                children
            }
            _ => panic!("Expected And expression, got: {expr:?}"),
        }
    }

    fn assert_condition_inline(
        expr: &FilterExpression,
        prop_path: &[&str],
        op: FilterOperator,
        val: &str,
    ) {
        match expr {
            FilterExpression::Condition(filter) => {
                let expected_path: Vec<String> = prop_path.iter().map(|s| s.to_string()).collect();
                assert_eq!(
                    filter.property_path, expected_path,
                    "Property path mismatch"
                );
                assert_eq!(filter.operator, op, "Operator mismatch");
                assert_eq!(filter.value, val, "Value mismatch");
            }
            _ => panic!("Expected Condition, got: {expr:?}"),
        }
    }

    let children = assert_and_with_n_children_inline(expr, expected_conditions.len());
    for (i, (prop, val)) in expected_conditions.iter().enumerate() {
        assert_condition_inline(&children[i], &[prop], FilterOperator::Equals, val);
    }
}

/// Validate OR expression with specific simple conditions (property=value).
///
/// This is a convenience wrapper. Requires expression_assertions to be imported in the test file.
#[track_caller]
pub fn assert_or_with_conditions(expr: &FilterExpression, expected_conditions: &[(&str, &str)]) {
    // Inline the dependency to avoid module loading issues
    fn assert_or_with_n_children_inline(
        expr: &FilterExpression,
        expected_count: usize,
    ) -> &Vec<FilterExpression> {
        match expr {
            FilterExpression::Or(children) => {
                assert_eq!(
                    children.len(),
                    expected_count,
                    "OR node should have {expected_count} children, got {}",
                    children.len()
                );
                children
            }
            _ => panic!("Expected Or expression, got: {expr:?}"),
        }
    }

    fn assert_condition_inline(
        expr: &FilterExpression,
        prop_path: &[&str],
        op: FilterOperator,
        val: &str,
    ) {
        match expr {
            FilterExpression::Condition(filter) => {
                let expected_path: Vec<String> = prop_path.iter().map(|s| s.to_string()).collect();
                assert_eq!(
                    filter.property_path, expected_path,
                    "Property path mismatch"
                );
                assert_eq!(filter.operator, op, "Operator mismatch");
                assert_eq!(filter.value, val, "Value mismatch");
            }
            _ => panic!("Expected Condition, got: {expr:?}"),
        }
    }

    let children = assert_or_with_n_children_inline(expr, expected_conditions.len());
    for (i, (prop, val)) in expected_conditions.iter().enumerate() {
        assert_condition_inline(&children[i], &[prop], FilterOperator::Equals, val);
    }
}
