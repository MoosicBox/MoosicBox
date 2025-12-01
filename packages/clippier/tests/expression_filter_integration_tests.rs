//! Integration tests for filter expression parsing and evaluation.

use clippier::package_filter::{evaluate_expression, parse_expression};

/// Helper to create TOML Value from string
fn toml(s: &str) -> toml::Value {
    toml::from_str(s).unwrap()
}

#[switchy_async::test]
async fn test_and_expression_both_match() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test_package"
        version = "0.1.0"
        publish = false
    "#,
    );

    let expr = parse_expression("package.publish=false AND package.version^=0.1").unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[switchy_async::test]
async fn test_and_expression_one_fails() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test_package"
        version = "0.1.0"
        publish = false
    "#,
    );

    let expr = parse_expression("package.publish=false AND package.version^=1.0").unwrap();
    assert!(!evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[switchy_async::test]
async fn test_or_expression_one_matches() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test_package"
        version = "0.1.0"
        publish = true
    "#,
    );

    let expr = parse_expression("package.publish=false OR package.version^=0.1").unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[switchy_async::test]
async fn test_or_expression_both_fail() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test_package"
        version = "0.1.0"
        publish = true
    "#,
    );

    let expr = parse_expression("package.publish=false OR package.version^=1.0").unwrap();
    assert!(!evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[switchy_async::test]
async fn test_not_expression() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test_package"
        version = "0.1.0"
        publish = false
    "#,
    );

    let expr = parse_expression("NOT package.publish=true").unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());

    let expr = parse_expression("NOT package.publish=false").unwrap();
    assert!(!evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[switchy_async::test]
async fn test_complex_expression_with_grouping() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test_example"
        version = "0.1.0"
        publish = false
        categories = ["audio", "multimedia"]
    "#,
    );

    // (package.publish=false OR package.name$=_example) AND package.categories@=audio
    let expr = parse_expression(
        "(package.publish=false OR package.name$=_example) AND package.categories@=audio",
    )
    .unwrap();
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

#[switchy_async::test]
async fn test_precedence_not_and_or() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test"
        version = "0.1.0"
        publish = false
    "#,
    );

    // NOT package.publish=false AND package.version^=0.1 OR package.name=other
    // Should parse as: ((NOT package.publish=false) AND package.version^=0.1) OR package.name=other
    let expr = parse_expression(
        "NOT package.publish=false AND package.version^=0.1 OR package.name=other",
    )
    .unwrap();

    // package.publish=false evaluates to TRUE (because publish IS false in the toml)
    // NOT (package.publish=false) = NOT true = false
    // false AND package.version^=0.1 = false AND true = false
    // false OR package.name=other = false OR false = false
    assert!(!evaluate_expression(&expr, &cargo_toml).unwrap());

    // But if package.name=test, then it should be true
    let cargo_toml_with_test = toml(
        r#"
        [package]
        name = "test"
        version = "0.1.0"
        publish = false
    "#,
    );
    // false OR package.name=other = false OR false = false (still false, name is "test" not "other")
    assert!(!evaluate_expression(&expr, &cargo_toml_with_test).unwrap());
}

#[switchy_async::test]
async fn test_quoted_value_with_spaces() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "my cool package"
        version = "0.1.0"
    "#,
    );

    let expr = parse_expression(r#"package.name="my cool package""#).unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[switchy_async::test]
async fn test_quoted_value_with_keyword() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test"
        description = "This AND that"
    "#,
    );

    let expr = parse_expression(r#"package.description="This AND that""#).unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[switchy_async::test]
async fn test_complex_nested_expression() {
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

    // (package.name^=moosicbox AND package.publish=true) AND (package.categories@=audio OR package.keywords@=music)
    let expr = parse_expression(
        "(package.name^=moosicbox AND package.publish=true) AND (package.categories@=audio OR package.keywords@=music)",
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

#[switchy_async::test]
async fn test_multiple_and_conditions() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test"
        version = "0.1.0"
        publish = false
        categories = ["audio"]
    "#,
    );

    let expr = parse_expression(
        "package.publish=false AND package.version^=0.1 AND package.categories@=audio",
    )
    .unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());

    let expr = parse_expression(
        "package.publish=false AND package.version^=0.1 AND package.categories@=video",
    )
    .unwrap();
    assert!(!evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[switchy_async::test]
async fn test_multiple_or_conditions() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test"
        version = "0.1.0"
    "#,
    );

    let expr =
        parse_expression("package.publish=false OR package.version^=0.1 OR package.name=test")
            .unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());

    let expr =
        parse_expression("package.publish=false OR package.version^=1.0 OR package.name=other")
            .unwrap();
    assert!(!evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[switchy_async::test]
async fn test_double_negation() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test"
        publish = false
    "#,
    );

    let expr = parse_expression("NOT NOT package.publish=false").unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[switchy_async::test]
async fn test_nested_property_in_expression() {
    let cargo_toml = toml(
        r#"
        [package.metadata.workspaces]
        independent = true
        
        [package]
        name = "test"
        publish = false
    "#,
    );

    let expr =
        parse_expression("package.metadata.workspaces.independent=true AND package.publish=false")
            .unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[switchy_async::test]
async fn test_array_operators_in_expression() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test"
        keywords = ["music", "audio", "player"]
    "#,
    );

    // Array length and contains
    let expr = parse_expression("package.keywords@#=3 AND package.keywords@=music").unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());

    // Array not contains with OR
    let expr = parse_expression("package.keywords!@=video OR keywords@*=mus").unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[switchy_async::test]
async fn test_existence_operators_in_expression() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test"
        readme = "README.md"
    "#,
    );

    let expr = parse_expression("package.readme? AND package.homepage!?").unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());

    let expr = parse_expression("package.readme? AND package.homepage?").unwrap();
    assert!(!evaluate_expression(&expr, &cargo_toml).unwrap());
}

#[switchy_async::test]
async fn test_case_insensitive_keywords() {
    let cargo_toml = toml(
        r#"
        [package]
        name = "test"
        version = "0.1.0"
    "#,
    );

    let expr = parse_expression("package.name=test and package.version^=0.1").unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());

    let expr = parse_expression("package.name=test AND package.version^=0.1").unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());

    let expr = parse_expression("package.name=test AnD package.version^=0.1").unwrap();
    assert!(evaluate_expression(&expr, &cargo_toml).unwrap());
}
