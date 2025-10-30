//! Core assertions for validating FilterExpression structure.
//!
//! These assertions are used across multiple test files to validate
//! the AST structure of parsed filter expressions.

use clippier::package_filter::{FilterExpression, FilterOperator};

/// Validate that an expression is a Condition with specific property, operator, and value.
#[track_caller]
pub fn assert_condition(
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

/// Validate that an expression is an AND with exactly the expected number of children.
#[track_caller]
pub fn assert_and_with_n_children(
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

/// Validate that an expression is an OR with exactly the expected number of children.
#[track_caller]
pub fn assert_or_with_n_children(
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

/// Validate that an expression is a NOT node.
#[track_caller]
pub fn assert_not(expr: &FilterExpression) -> &FilterExpression {
    match expr {
        FilterExpression::Not(child) => child,
        _ => panic!("Expected Not expression, got: {expr:?}"),
    }
}
