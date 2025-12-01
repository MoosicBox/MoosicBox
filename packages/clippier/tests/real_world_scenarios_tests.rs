//! Real-world scenario tests for filter expressions.
//!
//! Tests actual use cases like CI/CD filtering, release preparation,
//! and package quality checks.

use clippier::package_filter::{
    FilterOperator, apply_filters, evaluate_expression, parse_expression,
};
use std::collections::BTreeMap;

// Import core expression assertions
mod expression_assertions;
use expression_assertions::*;

fn toml(s: &str) -> toml::Value {
    toml::from_str(s).unwrap()
}

fn create_test_package(dir: &std::path::Path, name: &str, toml_content: &str) {
    let pkg_dir = dir.join(name);
    switchy_fs::sync::create_dir_all(&pkg_dir).unwrap();
    switchy_fs::sync::write(pkg_dir.join("Cargo.toml"), toml_content).unwrap();
}

// ============================================================================
// CI/CD Filtering Scenarios
// ============================================================================

#[switchy_async::test]
async fn test_skip_unpublished_and_examples() {
    let cargo = toml(
        r#"[package]
        name = "test_example"
        publish = false"#,
    );
    let expr = parse_expression("package.publish=false OR package.name$=_example").unwrap();

    // Validate expression structure: OR(package.publish=false, package.name$=_example)
    let or_children = assert_or_with_n_children(&expr, 2);
    assert_condition(
        &or_children[0],
        &["package", "publish"],
        FilterOperator::Equals,
        "false",
    );
    assert_condition(
        &or_children[1],
        &["package", "name"],
        FilterOperator::EndsWith,
        "_example",
    );

    // Validate it evaluates to true (both conditions match)
    assert!(evaluate_expression(&expr, &cargo).unwrap());
}

#[switchy_async::test]
async fn test_include_only_production_packages() {
    let cargo = toml(
        r#"[package]
        name = "moosicbox_audio"
        publish = true
        version = "0.1.4""#,
    );
    let expr = parse_expression(
        "package.name^=moosicbox_ AND package.publish=true AND NOT package.name$=_example",
    )
    .unwrap();

    // Validate expression structure: AND(name^=moosicbox_, publish=true, NOT(name$=_example))
    let and_children = assert_and_with_n_children(&expr, 3);
    assert_condition(
        &and_children[0],
        &["package", "name"],
        FilterOperator::StartsWith,
        "moosicbox_",
    );
    assert_condition(
        &and_children[1],
        &["package", "publish"],
        FilterOperator::Equals,
        "true",
    );
    let not_child = assert_not(&and_children[2]);
    assert_condition(
        not_child,
        &["package", "name"],
        FilterOperator::EndsWith,
        "_example",
    );

    // Validate it evaluates correctly
    assert!(evaluate_expression(&expr, &cargo).unwrap());
}

#[switchy_async::test]
async fn test_quality_gate_well_documented() {
    let cargo = toml(
        r#"[package]
        name = "test"
        readme = "README.md"
        keywords = ["api", "audio", "music"]
        categories = ["multimedia"]"#,
    );
    let expr = parse_expression(
        "package.readme? AND package.keywords@#>2 AND (package.categories@=audio OR package.categories@=multimedia)",
    )
    .unwrap();

    // Validate structure: AND(readme?, keywords@#>2, OR(categories@=audio, categories@=multimedia))
    let and_children = assert_and_with_n_children(&expr, 3);
    assert_condition(
        &and_children[0],
        &["package", "readme"],
        FilterOperator::Exists,
        "",
    );
    assert_condition(
        &and_children[1],
        &["package", "keywords"],
        FilterOperator::ArrayLengthGreater,
        "2",
    );

    // Third child should be OR
    let or_children = assert_or_with_n_children(&and_children[2], 2);
    assert_condition(
        &or_children[0],
        &["package", "categories"],
        FilterOperator::ArrayContains,
        "audio",
    );
    assert_condition(
        &or_children[1],
        &["package", "categories"],
        FilterOperator::ArrayContains,
        "multimedia",
    );

    assert!(evaluate_expression(&expr, &cargo).unwrap());
}

#[switchy_async::test]
async fn test_component_isolation_by_prefix() {
    let cargo = toml(
        r#"[package]
        name = "moosicbox_player_core""#,
    );
    let expr = parse_expression("package.name^=moosicbox_player").unwrap();

    // Validate expression structure: package.name^=moosicbox_player
    assert_condition(
        &expr,
        &["package", "name"],
        FilterOperator::StartsWith,
        "moosicbox_player",
    );

    assert!(evaluate_expression(&expr, &cargo).unwrap());
}

// ============================================================================
// Release Preparation Scenarios
// ============================================================================

#[switchy_async::test]
async fn test_release_ready_packages() {
    let cargo = toml(
        r#"[package]
        name = "moosicbox_audio"
        version = "0.1.4"
        publish = true
        readme = "README.md"
        license = "MIT""#,
    );
    let expr = parse_expression(
        "package.publish=true AND package.readme? AND package.license? AND NOT package.name$=_example",
    )
    .unwrap();

    // Validate structure: AND(publish=true, readme?, license?, NOT(name$=_example))
    let and_children = assert_and_with_n_children(&expr, 4);
    assert_condition(
        &and_children[0],
        &["package", "publish"],
        FilterOperator::Equals,
        "true",
    );
    assert_condition(
        &and_children[1],
        &["package", "readme"],
        FilterOperator::Exists,
        "",
    );
    assert_condition(
        &and_children[2],
        &["package", "license"],
        FilterOperator::Exists,
        "",
    );
    let not_child = assert_not(&and_children[3]);
    assert_condition(
        not_child,
        &["package", "name"],
        FilterOperator::EndsWith,
        "_example",
    );

    assert!(evaluate_expression(&expr, &cargo).unwrap());
}

#[switchy_async::test]
async fn test_version_range_filtering() {
    let cargo = toml(
        r#"[package]
        version = "0.1.4""#,
    );
    let expr = parse_expression("package.version^=0.1").unwrap();

    // Validate expression structure: package.version^=0.1
    assert_condition(
        &expr,
        &["package", "version"],
        FilterOperator::StartsWith,
        "0.1",
    );

    assert!(evaluate_expression(&expr, &cargo).unwrap());
}

#[switchy_async::test]
async fn test_breaking_change_detection() {
    let cargo = toml(
        r#"[package]
        version = "1.0.0""#,
    );
    let expr = parse_expression("package.version^=1.").unwrap();

    // Validate expression structure: package.version^=1.
    assert_condition(
        &expr,
        &["package", "version"],
        FilterOperator::StartsWith,
        "1.",
    );

    assert!(evaluate_expression(&expr, &cargo).unwrap());
}

// ============================================================================
// Metadata-based Filtering
// ============================================================================

#[switchy_async::test]
async fn test_workspace_independent_packages() {
    let cargo = toml(
        r#"[package.metadata.workspaces]
        independent = true
        [package]
        name = "test""#,
    );
    let expr = parse_expression("package.metadata.workspaces.independent=true").unwrap();

    // Validate expression structure: metadata.workspaces.independent=true
    assert_condition(
        &expr,
        &["package", "metadata", "workspaces", "independent"],
        FilterOperator::Equals,
        "true",
    );

    assert!(evaluate_expression(&expr, &cargo).unwrap());
}

#[switchy_async::test]
async fn test_ci_skip_metadata() {
    let cargo = toml(
        r#"[package.metadata.ci]
        skip-tests = true
        [package]
        name = "test""#,
    );
    let expr = parse_expression("package.metadata.ci.skip-tests=true").unwrap();

    // Validate expression structure: metadata.ci.skip-tests=true
    assert_condition(
        &expr,
        &["package", "metadata", "ci", "skip-tests"],
        FilterOperator::Equals,
        "true",
    );

    assert!(evaluate_expression(&expr, &cargo).unwrap());
}

#[switchy_async::test]
async fn test_complex_metadata_filtering() {
    let cargo = toml(
        r#"[package.metadata]
        internal = true
        stage = "beta"
        [package]
        name = "test""#,
    );
    let expr =
        parse_expression("package.metadata.internal=true AND package.metadata.stage=beta").unwrap();

    // Validate structure: AND(metadata.internal=true, metadata.stage=beta)
    let and_children = assert_and_with_n_children(&expr, 2);
    assert_condition(
        &and_children[0],
        &["package", "metadata", "internal"],
        FilterOperator::Equals,
        "true",
    );
    assert_condition(
        &and_children[1],
        &["package", "metadata", "stage"],
        FilterOperator::Equals,
        "beta",
    );

    assert!(evaluate_expression(&expr, &cargo).unwrap());
}

// ============================================================================
// Category and Keyword Filtering
// ============================================================================

#[switchy_async::test]
async fn test_multimedia_packages() {
    let cargo = toml(
        r#"[package]
        categories = ["multimedia", "audio"]"#,
    );
    let expr =
        parse_expression("package.categories@=multimedia OR package.categories@=audio").unwrap();

    // Validate structure: OR(categories@=multimedia, categories@=audio)
    let or_children = assert_or_with_n_children(&expr, 2);
    assert_condition(
        &or_children[0],
        &["package", "categories"],
        FilterOperator::ArrayContains,
        "multimedia",
    );
    assert_condition(
        &or_children[1],
        &["package", "categories"],
        FilterOperator::ArrayContains,
        "audio",
    );

    assert!(evaluate_expression(&expr, &cargo).unwrap());
}

#[switchy_async::test]
async fn test_api_related_packages() {
    let cargo = toml(
        r#"[package]
        keywords = ["music-api", "rest-api"]"#,
    );
    let expr = parse_expression("package.keywords@*=api").unwrap();

    // Validate expression structure: keywords@*=api
    assert_condition(
        &expr,
        &["package", "keywords"],
        FilterOperator::ArrayContainsSubstring,
        "api",
    );

    assert!(evaluate_expression(&expr, &cargo).unwrap());
}

#[switchy_async::test]
async fn test_exclude_deprecated_packages() {
    let cargo = toml(
        r#"[package]
        keywords = ["music", "player"]"#,
    );
    let expr = parse_expression("package.keywords!@=deprecated").unwrap();

    // Validate expression structure: keywords!@=deprecated
    assert_condition(
        &expr,
        &["package", "keywords"],
        FilterOperator::ArrayNotContains,
        "deprecated",
    );

    assert!(evaluate_expression(&expr, &cargo).unwrap());
}

// ============================================================================
// apply_filters() with Expressions
// ============================================================================

#[switchy_async::test]
#[ignore] // TODO: Fix temp directory path handling
async fn test_apply_filters_with_complex_skip() {
    let temp_dir = switchy_fs::tempdir().unwrap();
    let temp_path = temp_dir.path();

    create_test_package(
        temp_path,
        "pkg1",
        r#"[package]
        name = "pkg1"
        publish = false"#,
    );
    create_test_package(
        temp_path,
        "pkg2",
        r#"[package]
        name = "pkg2_example"
        publish = true"#,
    );
    create_test_package(
        temp_path,
        "pkg3",
        r#"[package]
        name = "pkg3"
        publish = true"#,
    );

    let mut paths = BTreeMap::new();
    paths.insert("pkg1".to_string(), "pkg1".to_string());
    paths.insert("pkg2_example".to_string(), "pkg2_example".to_string());
    paths.insert("pkg3".to_string(), "pkg3".to_string());

    let packages = vec![
        "pkg1".to_string(),
        "pkg2_example".to_string(),
        "pkg3".to_string(),
    ];

    let skip_filters = vec!["package.publish=false OR package.name$=_example".to_string()];
    let result = apply_filters(&packages, &paths, temp_path, &skip_filters, &[]).unwrap();

    assert_eq!(result, vec!["pkg3"]);
}

#[switchy_async::test]
async fn test_apply_filters_with_complex_include() {
    let temp_dir = switchy_fs::tempdir().unwrap();
    let temp_path = temp_dir.path();

    create_test_package(
        temp_path,
        "moosicbox_audio",
        r#"[package]
        name = "moosicbox_audio"
        publish = true
        categories = ["multimedia"]"#,
    );
    create_test_package(
        temp_path,
        "moosicbox_video",
        r#"[package]
        name = "moosicbox_video"
        publish = true
        categories = ["video"]"#,
    );
    create_test_package(
        temp_path,
        "other_pkg",
        r#"[package]
        name = "other_pkg"
        publish = true
        categories = ["multimedia"]"#,
    );

    let mut paths = BTreeMap::new();
    paths.insert("moosicbox_audio".to_string(), "moosicbox_audio".to_string());
    paths.insert("moosicbox_video".to_string(), "moosicbox_video".to_string());
    paths.insert("other_pkg".to_string(), "other_pkg".to_string());

    let packages = vec![
        "moosicbox_audio".to_string(),
        "moosicbox_video".to_string(),
        "other_pkg".to_string(),
    ];

    let include_filters = vec![
        "package.name^=moosicbox_ AND (package.categories@=multimedia OR package.categories@=video)".to_string(),
    ];
    let result = apply_filters(&packages, &paths, temp_path, &[], &include_filters).unwrap();

    assert_eq!(result, vec!["moosicbox_audio", "moosicbox_video"]);
}

#[switchy_async::test]
async fn test_apply_filters_skip_and_include_together() {
    let temp_dir = switchy_fs::tempdir().unwrap();
    let temp_path = temp_dir.path();

    create_test_package(
        temp_path,
        "moosicbox_audio",
        r#"[package]
        name = "moosicbox_audio"
        publish = true"#,
    );
    create_test_package(
        temp_path,
        "moosicbox_example",
        r#"[package]
        name = "moosicbox_example"
        publish = false"#,
    );

    let mut paths = BTreeMap::new();
    paths.insert("moosicbox_audio".to_string(), "moosicbox_audio".to_string());
    paths.insert(
        "moosicbox_example".to_string(),
        "moosicbox_example".to_string(),
    );

    let packages = vec![
        "moosicbox_audio".to_string(),
        "moosicbox_example".to_string(),
    ];

    let skip_filters = vec!["package.publish=false OR package.name$=_example".to_string()];
    let include_filters = vec!["package.name^=moosicbox_".to_string()];
    let result = apply_filters(
        &packages,
        &paths,
        temp_path,
        &skip_filters,
        &include_filters,
    )
    .unwrap();

    assert_eq!(result, vec!["moosicbox_audio"]);
}

// ============================================================================
// Error Handling
// ============================================================================

#[switchy_async::test]
async fn test_invalid_regex_error_in_expression() {
    let cargo = toml(
        r#"[package]
        name = "test""#,
    );
    let expr = parse_expression(r"name~=[invalid AND version=0.1.0").unwrap();
    let result = evaluate_expression(&expr, &cargo);
    assert!(result.is_err());
}

#[switchy_async::test]
async fn test_invalid_array_length_value() {
    let cargo = toml(
        r#"[package]
        keywords = ["a", "b"]"#,
    );
    let expr = parse_expression("package.keywords@#=notanumber").unwrap();
    let result = evaluate_expression(&expr, &cargo);
    assert!(result.is_err());
}

// ============================================================================
// Backward Compatibility
// ============================================================================

#[switchy_async::test]
async fn test_simple_filter_still_works() {
    let cargo = toml(
        r#"[package]
        publish = false"#,
    );
    let expr = parse_expression("package.publish=false").unwrap();
    assert!(evaluate_expression(&expr, &cargo).unwrap());
}

#[switchy_async::test]
async fn test_all_existing_operators_work() {
    let cargo = toml(
        r#"[package]
        name = "test"
        version = "0.1.0"
        keywords = ["api", "audio"]
        readme = "README.md""#,
    );

    // Test each operator individually
    let operators = vec![
        ("package.name=test", true),
        ("package.name!=other", true),
        ("package.name^=test", true),
        ("package.name$=st", true),
        ("package.version*=0.1", true),
        ("package.keywords@=api", true),
        ("package.keywords@*=aud", true),
        ("package.keywords@^=api", true),
        ("package.keywords@#=2", true),
        ("package.keywords@#>1", true),
        ("package.keywords@#<5", true),
        ("package.keywords!@=video", true),
        ("package.readme?", true),
        ("package.homepage!?", true),
    ];

    for (filter_str, expected) in operators {
        let expr = parse_expression(filter_str).unwrap();
        assert_eq!(
            evaluate_expression(&expr, &cargo).unwrap(),
            expected,
            "Filter failed: {filter_str}"
        );
    }
}

// ============================================================================
// Performance / Stress Tests
// ============================================================================

#[switchy_async::test]
async fn test_wide_or_expression_matches_first() {
    let cargo = toml(
        r#"[package]
        name = "test""#,
    );
    let mut conditions: Vec<String> = vec!["package.name=test".to_string()];
    conditions.extend((0..49).map(|i| format!("package.name=other{i}")));
    let expr_str = conditions.join(" OR ");
    let expr = parse_expression(&expr_str).unwrap();
    // Should short-circuit on first match
    assert!(evaluate_expression(&expr, &cargo).unwrap());
}

#[switchy_async::test]
async fn test_wide_and_expression_fails_fast() {
    let cargo = toml(
        r#"[package]
        name = "test""#,
    );
    let mut conditions: Vec<String> = vec!["package.name=other".to_string()];
    conditions.extend((0..49).map(|_| "package.name=test".to_string()));
    let expr_str = conditions.join(" AND ");
    let expr = parse_expression(&expr_str).unwrap();
    // Should fail on first condition
    assert!(!evaluate_expression(&expr, &cargo).unwrap());
}

#[switchy_async::test]
async fn test_deeply_nested_expression_evaluates() {
    let cargo = toml(
        r#"[package]
        name = "test""#,
    );
    let mut expr_str = "(".repeat(20);
    expr_str.push_str("package.name=test");
    expr_str.push_str(&")".repeat(20));
    let expr = parse_expression(&expr_str).unwrap();
    assert!(evaluate_expression(&expr, &cargo).unwrap());
}
