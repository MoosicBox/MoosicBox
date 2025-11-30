//! Integration tests for parent package feature exposure validation
//!
//! These tests validate the parent package validation feature which ensures
//! that "umbrella" or "parent" packages properly expose all features from
//! their workspace dependencies.

use std::path::PathBuf;

use clippier::OutputType;
use clippier::feature_validator::{
    FeatureValidator, ParentValidationConfig, PrefixOverride, ValidatorConfig,
};
use insta::assert_snapshot;

/// Get the path to the parent-validation-test workspace
fn get_test_workspace_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("test-resources")
        .join("workspaces")
        .join("parent-validation-test")
}

/// Helper to create a config with parent validation settings
fn create_parent_config(
    packages: Vec<String>,
    depth: Option<u8>,
    skip_features: Vec<String>,
    prefix_overrides: Vec<PrefixOverride>,
) -> ValidatorConfig {
    ValidatorConfig {
        features: None,
        skip_features: None,
        workspace_only: true,
        output_format: OutputType::Json,
        strict_optional_propagation: false,
        cli_overrides: vec![],
        override_options: Default::default(),
        ignore_packages: vec![],
        ignore_features: vec![],
        parent_config: ParentValidationConfig {
            cli_packages: packages,
            cli_depth: depth,
            cli_skip_features: skip_features,
            cli_prefix_overrides: prefix_overrides,
            use_config: false, // Don't load from config files for tests
        },
    }
}

#[test]
fn test_parent_validation_detects_missing_features() {
    let workspace_path = get_test_workspace_path();

    let config = create_parent_config(vec!["parent".to_string()], None, vec![], vec![]);

    let validator = FeatureValidator::new(Some(workspace_path), config).unwrap();
    let result = validator.validate().unwrap();

    // Should have parent validation results
    assert!(
        !result.parent_results.is_empty(),
        "Expected parent validation results"
    );

    // Get the parent result
    let parent_result = result
        .parent_results
        .iter()
        .find(|r| r.package == "parent")
        .expect("Should have parent package result");

    // Should detect missing features from child_a (json, serde were not exposed)
    assert!(
        !parent_result.missing_exposures.is_empty(),
        "Expected missing feature exposures"
    );

    // Snapshot the results for verification
    let json = serde_json::to_string_pretty(&result.parent_results).unwrap();
    assert_snapshot!("parent_validation_missing_features", json);
}

#[test]
fn test_parent_validation_with_depth_limit() {
    let workspace_path = get_test_workspace_path();

    // Test with depth = 1 (only direct dependencies)
    let config = create_parent_config(
        vec!["parent".to_string()],
        Some(1), // Only direct dependencies
        vec![],
        vec![],
    );

    let validator = FeatureValidator::new(Some(workspace_path), config).unwrap();
    let result = validator.validate().unwrap();

    let parent_result = result
        .parent_results
        .iter()
        .find(|r| r.package == "parent")
        .expect("Should have parent package result");

    // With depth=1, should only check direct dependencies (child_a, child_b, nested_level1)
    // Should NOT check nested_level2 or nested_level3 transitively
    let nested_level2_missing: Vec<_> = parent_result
        .missing_exposures
        .iter()
        .filter(|e| e.dependency == "parent_nested_level2")
        .collect();

    assert!(
        nested_level2_missing.is_empty(),
        "With depth=1, should not check nested_level2"
    );

    let json = serde_json::to_string_pretty(&result.parent_results).unwrap();
    assert_snapshot!("parent_validation_depth_1", json);
}

#[test]
fn test_parent_validation_with_depth_2() {
    let workspace_path = get_test_workspace_path();

    // Test with depth = 2 (includes transitive dependencies one level deep)
    let config = create_parent_config(vec!["parent".to_string()], Some(2), vec![], vec![]);

    let validator = FeatureValidator::new(Some(workspace_path), config).unwrap();
    let result = validator.validate().unwrap();

    let parent_result = result
        .parent_results
        .iter()
        .find(|r| r.package == "parent")
        .expect("Should have parent package result");

    // With depth=2, should check nested_level2 (via nested_level1)
    // but NOT nested_level3
    let nested_level3_missing: Vec<_> = parent_result
        .missing_exposures
        .iter()
        .filter(|e| e.dependency == "parent_nested_level3")
        .collect();

    assert!(
        nested_level3_missing.is_empty(),
        "With depth=2, should not check nested_level3"
    );

    let json = serde_json::to_string_pretty(&result.parent_results).unwrap();
    assert_snapshot!("parent_validation_depth_2", json);
}

#[test]
fn test_parent_validation_with_custom_prefix() {
    let workspace_path = get_test_workspace_path();

    // Test with custom prefix overrides
    let config = create_parent_config(
        vec!["parent".to_string()],
        Some(1),
        vec![],
        vec![
            PrefixOverride {
                dependency: "parent_child_a".to_string(),
                prefix: "a".to_string(),
            },
            PrefixOverride {
                dependency: "parent_child_b".to_string(),
                prefix: "b".to_string(),
            },
        ],
    );

    let validator = FeatureValidator::new(Some(workspace_path), config).unwrap();
    let result = validator.validate().unwrap();

    let parent_result = result
        .parent_results
        .iter()
        .find(|r| r.package == "parent")
        .expect("Should have parent package result");

    // With custom prefixes, the expected feature names should use those prefixes
    // child_a's "api" feature should expect "a-api" (which exists)
    // child_a's "serde" feature should expect "a-serde" (missing)
    // child_a's "json" feature should expect "a-json" (missing)
    let child_a_missing: Vec<_> = parent_result
        .missing_exposures
        .iter()
        .filter(|e| e.dependency == "parent_child_a")
        .collect();

    // Should find missing a-serde and a-json
    assert!(
        child_a_missing.len() >= 2,
        "Should have at least 2 missing features for child_a"
    );

    let json = serde_json::to_string_pretty(&result.parent_results).unwrap();
    assert_snapshot!("parent_validation_custom_prefix", json);
}

#[test]
fn test_parent_validation_with_skip_features() {
    let workspace_path = get_test_workspace_path();

    // Test skipping features matching a pattern
    let config = create_parent_config(
        vec!["parent".to_string()],
        Some(1),
        vec!["internal-*".to_string()], // Skip internal features
        vec![PrefixOverride {
            dependency: "parent_child_a".to_string(),
            prefix: "a".to_string(),
        }],
    );

    let validator = FeatureValidator::new(Some(workspace_path), config).unwrap();
    let result = validator.validate().unwrap();

    let parent_result = result
        .parent_results
        .iter()
        .find(|r| r.package == "parent")
        .expect("Should have parent package result");

    // With skip-features = ["internal-*"], the "internal-debug" feature
    // from child_a should NOT be reported as missing
    let internal_missing: Vec<_> = parent_result
        .missing_exposures
        .iter()
        .filter(|e| e.dependency_feature.starts_with("internal-"))
        .collect();

    assert!(
        internal_missing.is_empty(),
        "Should skip internal-* features"
    );

    let json = serde_json::to_string_pretty(&result.parent_results).unwrap();
    assert_snapshot!("parent_validation_skip_features", json);
}

#[test]
fn test_parent_validation_no_missing_when_all_exposed() {
    let workspace_path = get_test_workspace_path();

    // Test child_b which has all features correctly exposed
    let config = create_parent_config(
        vec!["parent".to_string()],
        Some(1),
        vec![],
        vec![PrefixOverride {
            dependency: "parent_child_b".to_string(),
            prefix: "b".to_string(),
        }],
    );

    let validator = FeatureValidator::new(Some(workspace_path), config).unwrap();
    let result = validator.validate().unwrap();

    let parent_result = result
        .parent_results
        .iter()
        .find(|r| r.package == "parent")
        .expect("Should have parent package result");

    // child_b has api and serde, parent has b-api and b-serde, so no missing
    let child_b_missing: Vec<_> = parent_result
        .missing_exposures
        .iter()
        .filter(|e| e.dependency == "parent_child_b")
        .collect();

    assert!(
        child_b_missing.is_empty(),
        "child_b should have all features exposed"
    );
}

#[test]
fn test_parent_validation_json_output_structure() {
    let workspace_path = get_test_workspace_path();

    let config = create_parent_config(
        vec!["parent".to_string()],
        Some(1),
        vec![],
        vec![PrefixOverride {
            dependency: "parent_child_a".to_string(),
            prefix: "a".to_string(),
        }],
    );

    let validator = FeatureValidator::new(Some(workspace_path), config).unwrap();
    let result = validator.validate().unwrap();

    // Verify the full result serializes correctly
    let json = serde_json::to_string_pretty(&result).unwrap();

    // Verify expected fields are present
    assert!(json.contains("parent_results"));
    assert!(json.contains("missing_exposures"));

    assert_snapshot!("parent_validation_full_json_output", json);
}

#[test]
fn test_parent_validation_with_no_parent_packages() {
    let workspace_path = get_test_workspace_path();

    // Test when no parent packages are specified
    let config = create_parent_config(vec![], None, vec![], vec![]);

    let validator = FeatureValidator::new(Some(workspace_path), config).unwrap();
    let result = validator.validate().unwrap();

    // Should have no parent results
    assert!(
        result.parent_results.is_empty(),
        "Should have no parent results when no parent packages specified"
    );
}

#[test]
fn test_parent_validation_nonexistent_package() {
    let workspace_path = get_test_workspace_path();

    // Test with a package that doesn't exist
    let config = create_parent_config(
        vec!["nonexistent_package".to_string()],
        None,
        vec![],
        vec![],
    );

    let validator = FeatureValidator::new(Some(workspace_path), config).unwrap();
    let result = validator.validate().unwrap();

    // Should gracefully handle nonexistent package (empty results or warning)
    // The exact behavior depends on implementation
    assert!(
        result.parent_results.is_empty()
            || result
                .parent_results
                .iter()
                .all(|r| r.package != "nonexistent_package"),
        "Should handle nonexistent package gracefully"
    );
}
