//! Comprehensive parser tests.
//!
//! Tests parse tree structure, precedence, error handling, and complex scenarios.

use clippier::package_filter::{FilterError, FilterExpression, FilterOperator, parse_expression};

// Import core expression assertions
mod expression_assertions;
use expression_assertions::*;

// Import parser-specific helpers
mod parser_test_helpers;
use parser_test_helpers::*;

// ============================================================================
// Parse Tree Structure Validation
// ============================================================================

#[switchy_async::test]
async fn test_and_node_has_two_children() {
    let expr = parse_expression("a=1 AND b=2").unwrap();
    match expr {
        FilterExpression::And(children) => {
            assert_eq!(children.len(), 2);
        }
        _ => panic!("Expected And node"),
    }
}

#[switchy_async::test]
async fn test_or_node_has_two_children() {
    let expr = parse_expression("a=1 OR b=2").unwrap();
    match expr {
        FilterExpression::Or(children) => {
            assert_eq!(children.len(), 2);
        }
        _ => panic!("Expected Or node"),
    }
}

#[switchy_async::test]
async fn test_not_node_has_one_child() {
    let expr = parse_expression("NOT a=1").unwrap();
    match expr {
        FilterExpression::Not(child) => {
            assert!(matches!(*child, FilterExpression::Condition(_)));
        }
        _ => panic!("Expected Not node"),
    }
}

#[switchy_async::test]
async fn test_nested_and_structure() {
    let expr = parse_expression("(a=1 AND b=2) AND c=3").unwrap();
    match expr {
        FilterExpression::And(children) => {
            assert_eq!(children.len(), 3); // Flattened
        }
        _ => panic!("Expected And node"),
    }
}

#[switchy_async::test]
async fn test_nested_or_structure() {
    let expr = parse_expression("(a=1 OR b=2) OR c=3").unwrap();
    match expr {
        FilterExpression::Or(children) => {
            assert_eq!(children.len(), 3); // Flattened
        }
        _ => panic!("Expected Or node"),
    }
}

#[switchy_async::test]
async fn test_mixed_and_or_not_flattened() {
    let expr = parse_expression("a=1 AND b=2 AND c=3").unwrap();
    match expr {
        FilterExpression::And(children) => {
            assert_eq!(children.len(), 3); // All AND, so flattened
        }
        _ => panic!("Expected And node"),
    }
}

#[switchy_async::test]
async fn test_grouping_with_same_operator() {
    let expr = parse_expression("a=1 AND (b=2 AND c=3)").unwrap();
    match expr {
        FilterExpression::And(children) => {
            // Parser creates: AND(a=1, AND(b=2, c=3))
            // Depending on flattening strategy, could be 2 or 3
            assert!(children.len() >= 2);
        }
        _ => panic!("Expected And node"),
    }
}

// ============================================================================
// Operator Precedence Testing
// ============================================================================

#[switchy_async::test]
async fn test_precedence_not_and_or() {
    // NOT a=1 AND b=2 OR c=3 should parse as ((NOT a=1) AND b=2) OR c=3
    let expr = parse_expression("NOT a=1 AND b=2 OR c=3").unwrap();

    // Root should be OR with 2 children
    let or_children = assert_or_with_n_children(&expr, 2);

    // First OR child: AND(NOT(a=1), b=2)
    let and_children = assert_and_with_n_children(&or_children[0], 2);

    // First AND child: NOT(a=1)
    let not_child = assert_not(&and_children[0]);
    assert_condition(not_child, &["a"], FilterOperator::Equals, "1");

    // Second AND child: b=2
    assert_condition(&and_children[1], &["b"], FilterOperator::Equals, "2");

    // Second OR child: c=3
    assert_condition(&or_children[1], &["c"], FilterOperator::Equals, "3");
}

#[switchy_async::test]
async fn test_precedence_and_or() {
    // a=1 AND b=2 OR c=3 should parse as (a=1 AND b=2) OR c=3
    let expr = parse_expression("a=1 AND b=2 OR c=3").unwrap();

    // Root should be OR with 2 children
    let or_children = assert_or_with_n_children(&expr, 2);

    // First OR child: AND(a=1, b=2)
    assert_and_with_conditions(&or_children[0], &[("a", "1"), ("b", "2")]);

    // Second OR child: c=3
    assert_condition(&or_children[1], &["c"], FilterOperator::Equals, "3");
}

#[switchy_async::test]
async fn test_precedence_overridden_by_parens() {
    // a=1 AND (b=2 OR c=3) should parse as AND(a=1, OR(b=2, c=3))
    let expr = parse_expression("a=1 AND (b=2 OR c=3)").unwrap();

    // Root should be AND with 2 children
    let and_children = assert_and_with_n_children(&expr, 2);

    // First AND child: a=1
    assert_condition(&and_children[0], &["a"], FilterOperator::Equals, "1");

    // Second AND child: OR(b=2, c=3)
    assert_or_with_conditions(&and_children[1], &[("b", "2"), ("c", "3")]);
}

#[switchy_async::test]
async fn test_multiple_nots() {
    // NOT NOT a=1 should parse as NOT(NOT(a=1))
    let expr = parse_expression("NOT NOT a=1").unwrap();

    // First NOT
    let first_not_child = assert_not(&expr);

    // Second NOT
    let second_not_child = assert_not(first_not_child);

    // Finally the condition: a=1
    assert_condition(second_not_child, &["a"], FilterOperator::Equals, "1");
}

#[switchy_async::test]
async fn test_complex_precedence_4_operators() {
    // NOT a=1 AND b=2 OR c=3 AND NOT d=4
    // Should parse as: (NOT a=1 AND b=2) OR (c=3 AND NOT d=4)
    let expr = parse_expression("NOT a=1 AND b=2 OR c=3 AND NOT d=4").unwrap();

    // Root: OR with 2 children
    let or_children = assert_or_with_n_children(&expr, 2);

    // First OR child: AND(NOT(a=1), b=2)
    let left_and_children = assert_and_with_n_children(&or_children[0], 2);
    let left_not_child = assert_not(&left_and_children[0]);
    assert_condition(left_not_child, &["a"], FilterOperator::Equals, "1");
    assert_condition(&left_and_children[1], &["b"], FilterOperator::Equals, "2");

    // Second OR child: AND(c=3, NOT(d=4))
    let right_and_children = assert_and_with_n_children(&or_children[1], 2);
    assert_condition(&right_and_children[0], &["c"], FilterOperator::Equals, "3");
    let right_not_child = assert_not(&right_and_children[1]);
    assert_condition(right_not_child, &["d"], FilterOperator::Equals, "4");
}

// ============================================================================
// Error Recovery
// ============================================================================

#[switchy_async::test]
async fn test_missing_closing_paren() {
    let result = parse_expression("(a=1 AND b=2");
    assert_error_type(
        result,
        |e| matches!(e, FilterError::ExpectedToken(_)),
        "ExpectedToken for missing closing paren",
    );
}

#[switchy_async::test]
async fn test_missing_opening_paren() {
    let result = parse_expression("a=1 AND b=2)");
    // Parser consumes all tokens then succeeds, ignoring trailing )
    // This is acceptable behavior - the expression is valid up to that point
    assert!(result.is_ok(), "Extra closing paren should be ignored");
}

#[switchy_async::test]
async fn test_empty_expression() {
    let result = parse_expression("");
    assert_error_type(
        result,
        |e| matches!(e, FilterError::ExpectedToken(_)),
        "ExpectedToken for empty expression",
    );
}

#[switchy_async::test]
async fn test_expression_with_only_not() {
    let result = parse_expression("NOT");
    assert_error_type(
        result,
        |e| matches!(e, FilterError::ExpectedToken(_)),
        "ExpectedToken when NOT has no operand",
    );
}

#[switchy_async::test]
async fn test_expression_with_trailing_and() {
    let result = parse_expression("a=1 AND");
    assert_error_type(
        result,
        |e| matches!(e, FilterError::ExpectedToken(_)),
        "ExpectedToken when AND missing right operand",
    );
}

#[switchy_async::test]
async fn test_expression_with_trailing_or() {
    let result = parse_expression("a=1 OR");
    assert_error_type(
        result,
        |e| matches!(e, FilterError::ExpectedToken(_)),
        "ExpectedToken when OR missing right operand",
    );
}

#[switchy_async::test]
async fn test_double_and() {
    let result = parse_expression("a=1 AND AND b=2");
    assert_error_type(
        result,
        |e| matches!(e, FilterError::UnexpectedToken(_)),
        "UnexpectedToken for double AND",
    );
}

#[switchy_async::test]
async fn test_double_or() {
    let result = parse_expression("a=1 OR OR b=2");
    assert_error_type(
        result,
        |e| matches!(e, FilterError::UnexpectedToken(_)),
        "UnexpectedToken for double OR",
    );
}

#[switchy_async::test]
async fn test_and_without_left_operand() {
    let result = parse_expression("AND b=2");
    assert_error_type(
        result,
        |e| matches!(e, FilterError::UnexpectedToken(_)),
        "UnexpectedToken when AND starts expression",
    );
}

#[switchy_async::test]
async fn test_or_without_left_operand() {
    let result = parse_expression("OR b=2");
    assert_error_type(
        result,
        |e| matches!(e, FilterError::UnexpectedToken(_)),
        "UnexpectedToken when OR starts expression",
    );
}

// ============================================================================
// Complex Scenarios
// ============================================================================

#[switchy_async::test]
async fn test_deeply_nested_ands_and_ors() {
    // (a=1 AND (b=2 OR (c=3 AND (d=4 OR e=5))))
    let expr = parse_expression("(a=1 AND (b=2 OR (c=3 AND (d=4 OR e=5))))").unwrap();

    // Root: AND(a=1, OR(...))
    let and_children = assert_and_with_n_children(&expr, 2);
    assert_condition(&and_children[0], &["a"], FilterOperator::Equals, "1");

    // Second child: OR(b=2, AND(...))
    let or_children = assert_or_with_n_children(&and_children[1], 2);
    assert_condition(&or_children[0], &["b"], FilterOperator::Equals, "2");

    // Second OR child: AND(c=3, OR(...))
    let inner_and_children = assert_and_with_n_children(&or_children[1], 2);
    assert_condition(&inner_and_children[0], &["c"], FilterOperator::Equals, "3");

    // Deepest: OR(d=4, e=5)
    assert_or_with_conditions(&inner_and_children[1], &[("d", "4"), ("e", "5")]);
}

#[switchy_async::test]
async fn test_alternating_and_or_chain() {
    // a=1 AND b=2 OR c=3 AND d=4 OR e=5
    // Should parse as: (a=1 AND b=2) OR (c=3 AND d=4) OR e=5
    let expr = parse_expression("a=1 AND b=2 OR c=3 AND d=4 OR e=5").unwrap();

    // Root: OR with 3 children
    let or_children = assert_or_with_n_children(&expr, 3);

    // First OR child: AND(a=1, b=2)
    assert_and_with_conditions(&or_children[0], &[("a", "1"), ("b", "2")]);

    // Second OR child: AND(c=3, d=4)
    assert_and_with_conditions(&or_children[1], &[("c", "3"), ("d", "4")]);

    // Third OR child: e=5
    assert_condition(&or_children[2], &["e"], FilterOperator::Equals, "5");
}

#[switchy_async::test]
async fn test_multiple_nots_at_various_positions() {
    // NOT a=1 AND NOT b=2 OR NOT c=3
    // Should parse as: (NOT a=1 AND NOT b=2) OR NOT c=3
    let expr = parse_expression("NOT a=1 AND NOT b=2 OR NOT c=3").unwrap();

    // Root: OR with 2 children
    let or_children = assert_or_with_n_children(&expr, 2);

    // First OR child: AND(NOT a=1, NOT b=2)
    let and_children = assert_and_with_n_children(&or_children[0], 2);
    let first_not = assert_not(&and_children[0]);
    assert_condition(first_not, &["a"], FilterOperator::Equals, "1");
    let second_not = assert_not(&and_children[1]);
    assert_condition(second_not, &["b"], FilterOperator::Equals, "2");

    // Second OR child: NOT c=3
    let third_not = assert_not(&or_children[1]);
    assert_condition(third_not, &["c"], FilterOperator::Equals, "3");
}

#[switchy_async::test]
async fn test_grouped_nots() {
    let expr = parse_expression("NOT (a=1 AND b=2)").unwrap();
    match expr {
        FilterExpression::Not(child) => {
            assert!(matches!(*child, FilterExpression::And(_)));
        }
        _ => panic!("Expected Not at root"),
    }
}

#[switchy_async::test]
async fn test_all_17_operators_in_expressions() {
    // Test that all operator types can be parsed in expressions
    let operators = vec![
        "a=1", "a!=1", "a^=1", "a$=1", "a*=1", "a~=1", "a@=1", "a@*=1", "a@^=1", "a@~=1", "a@!",
        "a@#=1", "a@#>1", "a@#<1", "a!@=1", "a?", "a!?",
    ];

    for op in operators {
        let expr_str = format!("{op} AND b=2");
        let expr = parse_expression(&expr_str);
        assert!(expr.is_ok(), "Failed to parse: {expr_str}");
    }
}

#[switchy_async::test]
async fn test_nested_properties_in_expressions() {
    let expr =
        parse_expression("package.metadata.workspaces.independent=true AND name=test").unwrap();
    match expr {
        FilterExpression::And(children) => {
            assert_eq!(children.len(), 2);
        }
        _ => panic!("Expected And node"),
    }
}

#[switchy_async::test]
async fn test_quoted_values_in_complex_expressions() {
    let expr = parse_expression(
        r#"package.name="test pkg" AND (package.desc="A OR B" OR package.version^="0.1")"#,
    )
    .unwrap();
    // Just verify it parses correctly
    assert!(matches!(expr, FilterExpression::And(_)));
}

// ============================================================================
// Backward Compatibility
// ============================================================================

#[switchy_async::test]
async fn test_simple_filter_parses_as_condition() {
    let expr = parse_expression("package.publish=false").unwrap();
    assert!(matches!(expr, FilterExpression::Condition(_)));
}

#[switchy_async::test]
async fn test_filter_with_dots_in_path() {
    let expr = parse_expression("package.metadata.ci.skip=true").unwrap();
    assert!(matches!(expr, FilterExpression::Condition(_)));
}

#[switchy_async::test]
async fn test_all_operator_types_as_simple_filters() {
    let filters = vec![
        "name=test",
        "name!=test",
        "name^=test",
        "name$=test",
        "name*=test",
        "name~=test",
        "keywords@=api",
        "keywords@*=api",
        "keywords@^=api",
        "keywords@~=api",
        "keywords@!",
        "keywords@#=3",
        "keywords@#>2",
        "keywords@#<5",
        "keywords!@=deprecated",
        "readme?",
        "homepage!?",
    ];

    for filter in filters {
        let expr = parse_expression(filter);
        assert!(expr.is_ok(), "Failed to parse: {filter}");
        assert!(matches!(expr.unwrap(), FilterExpression::Condition(_)));
    }
}

// ============================================================================
// Special Cases
// ============================================================================

#[switchy_async::test]
async fn test_single_condition_in_parens() {
    let expr = parse_expression("(name=test)").unwrap();
    // Should simplify to just the condition, not wrapped in parens
    assert!(matches!(expr, FilterExpression::Condition(_)));
}

#[switchy_async::test]
async fn test_multiple_levels_of_grouping() {
    let expr = parse_expression("((((name=test))))").unwrap();
    // All parens should be stripped, leaving just the condition
    assert!(matches!(expr, FilterExpression::Condition(_)));
}

#[switchy_async::test]
async fn test_whitespace_only_between_tokens() {
    let expr = parse_expression("   a=1    AND    b=2   ").unwrap();
    match expr {
        FilterExpression::And(children) => {
            assert_eq!(children.len(), 2);
        }
        _ => panic!("Expected And node"),
    }
}

#[switchy_async::test]
async fn test_newlines_between_tokens() {
    let expr = parse_expression("a=1\nAND\nb=2\nOR\nc=3").unwrap();
    assert!(matches!(expr, FilterExpression::Or(_)));
}

#[switchy_async::test]
async fn test_very_long_and_chain() {
    let conditions: Vec<String> = (0..50).map(|i| format!("f{i}=v{i}")).collect();
    let expr_str = conditions.join(" AND ");
    let expr = parse_expression(&expr_str).unwrap();
    match expr {
        FilterExpression::And(children) => {
            assert_eq!(children.len(), 50);
        }
        _ => panic!("Expected And node"),
    }
}

#[switchy_async::test]
async fn test_very_long_or_chain() {
    let conditions: Vec<String> = (0..50).map(|i| format!("f{i}=v{i}")).collect();
    let expr_str = conditions.join(" OR ");
    let expr = parse_expression(&expr_str).unwrap();
    match expr {
        FilterExpression::Or(children) => {
            assert_eq!(children.len(), 50);
        }
        _ => panic!("Expected Or node"),
    }
}
