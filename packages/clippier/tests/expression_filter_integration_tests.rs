//! Integration tests for filter expression parsing and evaluation.

use clippier::package_filter::{evaluate_expression, parse_expression};

/// Helper to create TOML Value from string
fn toml(s: &str) -> toml::Value {
    toml::from_str(s).unwrap()
}

#[test]
fn test_and_expression_both_match() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test_package"
        version = "0.1.0"
        publish = false
    "#,
    );

    let expr = parse_expression("publish=false AND version^=0.1").unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[test]
fn test_and_expression_one_fails() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test_package"
        version = "0.1.0"
        publish = false
    "#,
    );

    let expr = parse_expression("publish=false AND version^=1.0").unwrap();
    assert!(!evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[test]
fn test_or_expression_one_matches() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test_package"
        version = "0.1.0"
        publish = true
    "#,
    );

    let expr = parse_expression("publish=false OR version^=0.1").unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[test]
fn test_or_expression_both_fail() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test_package"
        version = "0.1.0"
        publish = true
    "#,
    );

    let expr = parse_expression("publish=false OR version^=1.0").unwrap();
    assert!(!evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[test]
fn test_not_expression() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test_package"
        version = "0.1.0"
        publish = false
    "#,
    );

    let expr = parse_expression("NOT publish=true").unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());

    let expr = parse_expression("NOT publish=false").unwrap();
    assert!(!evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[test]
fn test_complex_expression_with_grouping() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test_example"
        version = "0.1.0"
        publish = false
        categories = ["audio", "multimedia"]
    "#,
    );

    // (publish=false OR name$=_example) AND categories@=audio
    let expr = parse_expression("(publish=false OR name$=_example) AND categories@=audio").unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());

    // Should still match even if publish is true (because name matches)
    let cargo_toml = toml(
        r#"
        [package]
        name = "test_example"
        version = "0.1.0"
        publish = true
        categories = ["audio", "multimedia"]
    "#,
    );
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());

    // Should fail if categories doesn't match
    let cargo_toml = toml(
        r#"
        [package]
        name = "test_example"
        version = "0.1.0"
        publish = false
        categories = ["video"]
    "#,
    );
    assert!(!evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[test]
fn test_precedence_not_and_or() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test"
        version = "0.1.0"
        publish = false
    "#,
    );

    // NOT publish=false AND version^=0.1 OR name=other
    // Should parse as: ((NOT publish=false) AND version^=0.1) OR name=other
    let expr = parse_expression("NOT publish=false AND version^=0.1 OR name=other").unwrap();

    // publish=false evaluates to TRUE (because publish IS false in the toml)
    // NOT (publish=false) = NOT true = false
    // false AND version^=0.1 = false AND true = false
    // false OR name=other = false OR false = false
    assert!(!evaluate_expression(&expr, &cargo_toml).unwrap());

    // But if name=test, then it should be true
    let cargo_toml_with_test = toml(
        r#"
        [package]
        name = "test"
        version = "0.1.0"
        publish = false
    "#,
    );
    // false OR name=other = false OR false = false (still false, name is "test" not "other")
    assert!(!evaluate_expression(&expr, &cargo_toml_with_test).unwrap());
}

#[test]
fn test_quoted_value_with_spaces() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "my cool package"
        version = "0.1.0"
    "#,
    );

    let expr = parse_expression(r#"name="my cool package""#).unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[test]
fn test_quoted_value_with_keyword() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test"
        description = "This AND that"
    "#,
    );

    let expr = parse_expression(r#"description="This AND that""#).unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[test]
fn test_complex_nested_expression() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "moosicbox_audio"
        version = "0.1.4"
        publish = true
        categories = ["audio", "multimedia"]
        keywords = ["music", "player"]
    "#,
    );

    // (name^=moosicbox AND publish=true) AND (categories@=audio OR keywords@=music)
    let expr = parse_expression(
        "(name^=moosicbox AND publish=true) AND (categories@=audio OR keywords@=music)",
    )
    .unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());

    // Change to make it fail
    let cargo_toml = toml(
        r#"
        [package]
        name = "other_audio"
        version = "0.1.4"
        publish = true
        categories = ["audio", "multimedia"]
        keywords = ["music", "player"]
    "#,
    );
    assert!(!evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[test]
fn test_multiple_and_conditions() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test"
        version = "0.1.0"
        publish = false
        categories = ["audio"]
    "#,
    );

    let expr = parse_expression("publish=false AND version^=0.1 AND categories@=audio").unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());

    let expr = parse_expression("publish=false AND version^=0.1 AND categories@=video").unwrap();
    assert!(!evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[test]
fn test_multiple_or_conditions() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test"
        version = "0.1.0"
    "#,
    );

    let expr = parse_expression("publish=false OR version^=0.1 OR name=test").unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());

    let expr = parse_expression("publish=false OR version^=1.0 OR name=other").unwrap();
    assert!(!evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[test]
fn test_double_negation() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test"
        publish = false
    "#,
    );

    let expr = parse_expression("NOT NOT publish=false").unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[test]
fn test_nested_property_in_expression() {
    let cargo_toml = toml(
        r#"
        [package.metadata.workspaces]
        independent = true
        
        [package]
        name = "test"
        publish = false
    "#,
    );

    let expr = parse_expression("metadata.workspaces.independent=true AND publish=false").unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[test]
fn test_array_operators_in_expression() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test"
        keywords = ["music", "audio", "player"]
    "#,
    );

    // Array length and contains
    let expr = parse_expression("keywords@#=3 AND keywords@=music").unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());

    // Array not contains with OR
    let expr = parse_expression("keywords!@=video OR keywords@*=mus").unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[test]
fn test_existence_operators_in_expression() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test"
        readme = "README.md"
    "#,
    );

    let expr = parse_expression("readme? AND homepage!?").unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());

    let expr = parse_expression("readme? AND homepage?").unwrap();
    assert!(!evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[test]
fn test_case_insensitive_keywords() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test"
        version = "0.1.0"
    "#,
    );

    let expr = parse_expression("name=test and version^=0.1").unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());

    let expr = parse_expression("name=test AND version^=0.1").unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());

    let expr = parse_expression("name=test AnD version^=0.1").unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());
}
