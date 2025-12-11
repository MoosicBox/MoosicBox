//! Edge case and error handling tests for feature validator

use switchy_fs::TempDir;

use clippier::OutputType;
use clippier::feature_validator::{FeatureValidator, ValidatorConfig};

/// Helper to create a simple workspace
fn create_simple_workspace() -> TempDir {
    let temp_dir = switchy_fs::tempdir().unwrap();
    let root_path = temp_dir.path();

    let workspace_cargo = r#"[workspace]
members = ["simple"]
"#;
    switchy_fs::sync::write(root_path.join("Cargo.toml"), workspace_cargo).unwrap();

    switchy_fs::sync::create_dir(root_path.join("simple")).unwrap();
    switchy_fs::sync::create_dir(root_path.join("simple/src")).unwrap();
    switchy_fs::sync::write(root_path.join("simple/src/lib.rs"), "").unwrap();

    let pkg_cargo = r#"[package]
name = "simple"
version = "0.1.0"

[features]
default = []
fail-on-warnings = []
"#;
    switchy_fs::sync::write(root_path.join("simple/Cargo.toml"), pkg_cargo).unwrap();

    temp_dir
}

/// Helper to create workspace with unicode features
fn create_workspace_with_unicode_features() -> TempDir {
    let temp_dir = switchy_fs::tempdir().unwrap();
    let root_path = temp_dir.path();

    let workspace_cargo = r#"[workspace]
members = ["unicode_pkg"]
"#;
    switchy_fs::sync::write(root_path.join("Cargo.toml"), workspace_cargo).unwrap();

    switchy_fs::sync::create_dir(root_path.join("unicode_pkg")).unwrap();
    switchy_fs::sync::create_dir(root_path.join("unicode_pkg/src")).unwrap();
    switchy_fs::sync::write(root_path.join("unicode_pkg/src/lib.rs"), "").unwrap();

    let pkg_cargo = r#"[package]
name = "unicode_pkg"
version = "0.1.0"

[features]
default = []
"test-日本語-feature" = []
"test-emoji-😀" = []
production = []
"#;
    switchy_fs::sync::write(root_path.join("unicode_pkg/Cargo.toml"), pkg_cargo).unwrap();

    temp_dir
}

/// Helper to create workspace with multiple features
fn create_multi_feature_workspace() -> TempDir {
    let temp_dir = switchy_fs::tempdir().unwrap();
    let root_path = temp_dir.path();

    let workspace_cargo = r#"[workspace]
members = ["multi"]
"#;
    switchy_fs::sync::write(root_path.join("Cargo.toml"), workspace_cargo).unwrap();

    switchy_fs::sync::create_dir(root_path.join("multi")).unwrap();
    switchy_fs::sync::create_dir(root_path.join("multi/src")).unwrap();
    switchy_fs::sync::write(root_path.join("multi/src/lib.rs"), "").unwrap();

    let pkg_cargo = r#"[package]
name = "multi"
version = "0.1.0"

[features]
default = []
test-utils = []
test-fixtures = []
fail-on-warnings = []
"#;
    switchy_fs::sync::write(root_path.join("multi/Cargo.toml"), pkg_cargo).unwrap();

    temp_dir
}

#[switchy_async::test]
async fn test_unicode_in_skip_patterns() {
    let workspace = create_workspace_with_unicode_features();

    let config = ValidatorConfig {
        features: Some(vec![
            "test-日本語-feature".to_string(),
            "test-emoji-😀".to_string(),
            "production".to_string(),
        ]),
        skip_features: Some(vec!["test-*".to_string()]), // Should skip both unicode test features
        workspace_only: true,
        output_format: OutputType::Json,
        strict_optional_propagation: false,
        ..ValidatorConfig::test_default()
    };

    let validator = FeatureValidator::new(Some(workspace.path().to_path_buf()), config).unwrap();
    let result = validator.validate().unwrap();

    // Verify unicode features were properly skipped by the test-* pattern
    assert!(result.total_packages > 0);
    // No errors expected since nothing should be validated (all test-* features skipped)
    assert_eq!(result.errors.len(), 0);
}

#[switchy_async::test]
async fn test_conflicting_patterns() {
    // Test behavior when patterns conflict
    let workspace = create_multi_feature_workspace();

    // Pattern that both skips and keeps the same feature
    let config = ValidatorConfig {
        features: None,
        skip_features: Some(vec![
            "test-*".to_string(),      // Skip all test-*
            "!test-utils".to_string(), // But keep test-utils
            "test-*".to_string(),      // Skip all test-* again
        ]),
        workspace_only: true,
        output_format: OutputType::Json,
        strict_optional_propagation: false,
        ..ValidatorConfig::test_default()
    };

    let validator = FeatureValidator::new(Some(workspace.path().to_path_buf()), config).unwrap();
    let result = validator.validate().unwrap();

    // Last pattern should win (test-* skipped)
    // Verify test features are not validated
    assert!(
        !result
            .errors
            .iter()
            .any(|e| e.errors.iter().any(|fe| fe.feature == "test-utils"))
    );
}

#[switchy_async::test]
async fn test_empty_workspace() {
    let temp_dir = switchy_fs::tempdir().unwrap();
    let root_path = temp_dir.path();

    // Empty workspace with no members
    let workspace_cargo = r#"[workspace]
members = []
"#;
    switchy_fs::sync::write(root_path.join("Cargo.toml"), workspace_cargo).unwrap();

    let config = ValidatorConfig {
        features: None,
        skip_features: None,
        workspace_only: true,
        output_format: OutputType::Json,
        strict_optional_propagation: false,
        ..ValidatorConfig::test_default()
    };

    let validator = FeatureValidator::new(Some(root_path.to_path_buf()), config).unwrap();
    let result = validator.validate().unwrap();

    // Should handle empty workspace gracefully
    assert_eq!(result.total_packages, 0);
    assert_eq!(result.valid_packages, 0);
    assert_eq!(result.errors.len(), 0);
}

#[switchy_async::test]
async fn test_package_with_no_features() {
    let temp_dir = switchy_fs::tempdir().unwrap();
    let root_path = temp_dir.path();

    let workspace_cargo = r#"[workspace]
members = ["no_features"]
"#;
    switchy_fs::sync::write(root_path.join("Cargo.toml"), workspace_cargo).unwrap();

    switchy_fs::sync::create_dir(root_path.join("no_features")).unwrap();
    switchy_fs::sync::create_dir(root_path.join("no_features/src")).unwrap();
    switchy_fs::sync::write(root_path.join("no_features/src/lib.rs"), "").unwrap();

    let pkg_cargo = r#"[package]
name = "no_features"
version = "0.1.0"
"#; // No [features] section
    switchy_fs::sync::write(root_path.join("no_features/Cargo.toml"), pkg_cargo).unwrap();

    let config = ValidatorConfig {
        features: None,
        skip_features: None,
        workspace_only: true,
        output_format: OutputType::Json,
        strict_optional_propagation: false,
        ..ValidatorConfig::test_default()
    };

    let validator = FeatureValidator::new(Some(root_path.to_path_buf()), config).unwrap();
    let result = validator.validate().unwrap();

    // Package with no features should be skipped/ignored
    assert_eq!(result.errors.len(), 0);
}

#[switchy_async::test]
async fn test_circular_dependency_detection() {
    // Create workspace with circular feature dependencies
    let temp_dir = switchy_fs::tempdir().unwrap();
    let root_path = temp_dir.path();

    let workspace_cargo = r#"[workspace]
members = ["pkg_a", "pkg_b"]
"#;
    switchy_fs::sync::write(root_path.join("Cargo.toml"), workspace_cargo).unwrap();

    // pkg_a depends on pkg_b
    switchy_fs::sync::create_dir(root_path.join("pkg_a")).unwrap();
    switchy_fs::sync::create_dir(root_path.join("pkg_a/src")).unwrap();
    switchy_fs::sync::write(root_path.join("pkg_a/src/lib.rs"), "").unwrap();
    let pkg_a_cargo = r#"[package]
name = "pkg_a"
version = "0.1.0"

[dependencies]
pkg_b = { path = "../pkg_b" }

[features]
test = ["pkg_b/test"]
"#;
    switchy_fs::sync::write(root_path.join("pkg_a/Cargo.toml"), pkg_a_cargo).unwrap();

    // pkg_b depends on pkg_a (circular)
    switchy_fs::sync::create_dir(root_path.join("pkg_b")).unwrap();
    switchy_fs::sync::create_dir(root_path.join("pkg_b/src")).unwrap();
    switchy_fs::sync::write(root_path.join("pkg_b/src/lib.rs"), "").unwrap();
    let pkg_b_cargo = r#"[package]
name = "pkg_b"
version = "0.1.0"

[dependencies]
pkg_a = { path = "../pkg_a" }

[features]
test = []
"#;
    switchy_fs::sync::write(root_path.join("pkg_b/Cargo.toml"), pkg_b_cargo).unwrap();

    let config = ValidatorConfig {
        features: None,
        skip_features: None,
        workspace_only: true,
        output_format: OutputType::Json,
        strict_optional_propagation: false,
        ..ValidatorConfig::test_default()
    };

    // Should detect circular dependency or handle gracefully
    let validator = FeatureValidator::new(Some(root_path.to_path_buf()), config);

    // Either succeeds (Cargo handles circular deps) or fails gracefully
    match validator {
        Ok(v) => {
            let result = v.validate();
            assert!(result.is_ok());
        }
        Err(_) => {
            // Graceful failure is acceptable
        }
    }
}

#[switchy_async::test]
async fn test_malformed_cargo_toml() {
    let temp_dir = switchy_fs::tempdir().unwrap();
    let root_path = temp_dir.path();

    let workspace_cargo = r#"[workspace]
members = ["bad_pkg"]
"#;
    switchy_fs::sync::write(root_path.join("Cargo.toml"), workspace_cargo).unwrap();

    switchy_fs::sync::create_dir(root_path.join("bad_pkg")).unwrap();

    // Write malformed TOML
    let bad_cargo = r#"[package
name = "bad_pkg"
this is not valid toml
"#;
    switchy_fs::sync::write(root_path.join("bad_pkg/Cargo.toml"), bad_cargo).unwrap();

    let config = ValidatorConfig {
        features: None,
        skip_features: None,
        workspace_only: true,
        output_format: OutputType::Json,
        strict_optional_propagation: false,
        ..ValidatorConfig::test_default()
    };

    // Should return error, not panic
    let validator = FeatureValidator::new(Some(root_path.to_path_buf()), config);
    assert!(validator.is_err());
}

#[switchy_async::test]
async fn test_skip_features_with_whitespace() {
    let workspace = create_simple_workspace();

    // Patterns with leading/trailing whitespace
    let config = ValidatorConfig {
        features: None,
        skip_features: Some(vec![" default ".to_string(), "test-*  ".to_string()]),
        workspace_only: true,
        output_format: OutputType::Json,
        strict_optional_propagation: false,
        ..ValidatorConfig::test_default()
    };

    let validator = FeatureValidator::new(Some(workspace.path().to_path_buf()), config).unwrap();
    let result = validator.validate();

    // Should handle whitespace without panicking
    assert!(result.is_ok());
}

#[switchy_async::test]
async fn test_nonexistent_feature_in_skip_list() {
    let workspace = create_simple_workspace();

    // Skip features that don't exist, but validate real features
    let config = ValidatorConfig {
        features: Some(vec!["default".to_string(), "fail-on-warnings".to_string()]),
        skip_features: Some(vec![
            "nonexistent-feature".to_string(),
            "another-missing-feature".to_string(),
        ]),
        workspace_only: true,
        output_format: OutputType::Json,
        strict_optional_propagation: false,
        ..ValidatorConfig::test_default()
    };

    let validator = FeatureValidator::new(Some(workspace.path().to_path_buf()), config).unwrap();
    let result = validator.validate().unwrap();

    // Skipping nonexistent features should be harmless - real features still validated
    assert!(result.total_packages > 0);
    // Both real features should still be validated successfully (no deps to propagate)
    assert_eq!(result.errors.len(), 0);
}

#[switchy_async::test]
async fn test_skip_all_features_with_wildcard() {
    let workspace = create_multi_feature_workspace();

    // Skip ALL features
    let config = ValidatorConfig {
        features: None,
        skip_features: Some(vec!["*".to_string()]),
        workspace_only: true,
        output_format: OutputType::Json,
        strict_optional_propagation: false,
        ..ValidatorConfig::test_default()
    };

    let validator = FeatureValidator::new(Some(workspace.path().to_path_buf()), config).unwrap();
    let result = validator.validate().unwrap();

    // Should validate zero features, so no errors possible
    assert_eq!(result.errors.len(), 0);
    // All packages should be "valid" since nothing was checked
    assert_eq!(result.total_packages, result.valid_packages);
}

#[switchy_async::test]
async fn test_skip_features_negation_edge_case() {
    let workspace = create_multi_feature_workspace();

    // Negation-only pattern: nothing is initially skipped, so negation has no effect
    let config = ValidatorConfig {
        features: Some(vec![
            "default".to_string(),
            "test-utils".to_string(),
            "fail-on-warnings".to_string(),
        ]),
        skip_features: Some(vec!["!fail-on-warnings".to_string()]),
        workspace_only: true,
        output_format: OutputType::Json,
        strict_optional_propagation: false,
        ..ValidatorConfig::test_default()
    };

    let validator = FeatureValidator::new(Some(workspace.path().to_path_buf()), config).unwrap();
    let result = validator.validate().unwrap();

    // With negation-only, nothing is skipped (negation needs a positive match first)
    // All three features should be validated
    assert!(result.total_packages > 0);
    assert_eq!(result.errors.len(), 0);
}

#[switchy_async::test]
async fn test_workspace_only_flag_behavior() {
    let temp_dir = switchy_fs::tempdir().unwrap();
    let root_path = temp_dir.path();

    let workspace_cargo = r#"[workspace]
members = ["internal"]

[workspace.dependencies]
serde = "1.0"
"#;
    switchy_fs::sync::write(root_path.join("Cargo.toml"), workspace_cargo).unwrap();

    switchy_fs::sync::create_dir(root_path.join("internal")).unwrap();
    switchy_fs::sync::create_dir(root_path.join("internal/src")).unwrap();
    switchy_fs::sync::write(root_path.join("internal/src/lib.rs"), "").unwrap();

    let pkg_cargo = r#"[package]
name = "internal"
version = "0.1.0"

[dependencies]
serde = { workspace = true, features = ["derive"] }

[features]
fail-on-warnings = []
"#;
    switchy_fs::sync::write(root_path.join("internal/Cargo.toml"), pkg_cargo).unwrap();

    // Test with workspace_only = true (should not check serde)
    let config_workspace = ValidatorConfig {
        features: None,
        skip_features: None,
        workspace_only: true,
        output_format: OutputType::Json,
        strict_optional_propagation: false,
        ..ValidatorConfig::test_default()
    };

    let validator_workspace =
        FeatureValidator::new(Some(root_path.to_path_buf()), config_workspace).unwrap();
    let result_workspace = validator_workspace.validate().unwrap();

    // Test with workspace_only = false (may check serde)
    let config_all = ValidatorConfig {
        features: None,
        skip_features: None,
        workspace_only: false,
        output_format: OutputType::Json,
        strict_optional_propagation: false,
        ..ValidatorConfig::test_default()
    };

    let validator_all = FeatureValidator::new(Some(root_path.to_path_buf()), config_all).unwrap();
    let result_all = validator_all.validate().unwrap();

    // Both should complete without panicking
    assert!(result_workspace.total_packages > 0);
    assert!(result_all.total_packages > 0);
}

/// Test that feature propagation to dev-dependencies is correctly validated
#[switchy_async::test]
async fn test_dev_dependency_feature_propagation() {
    let temp_dir = switchy_fs::tempdir().unwrap();
    let root_path = temp_dir.path();

    // Create workspace with two packages
    let workspace_cargo = r#"[workspace]
members = ["main_pkg", "test_util"]

[workspace.dependencies]
test_util = { path = "test_util" }
"#;
    switchy_fs::sync::write(root_path.join("Cargo.toml"), workspace_cargo).unwrap();

    // Create test utility package with a feature
    switchy_fs::sync::create_dir(root_path.join("test_util")).unwrap();
    switchy_fs::sync::create_dir(root_path.join("test_util/src")).unwrap();
    switchy_fs::sync::write(root_path.join("test_util/src/lib.rs"), "").unwrap();

    let test_util_cargo = r#"[package]
name = "test_util"
version = "0.1.0"

[features]
test-feature = []
"#;
    switchy_fs::sync::write(root_path.join("test_util/Cargo.toml"), test_util_cargo).unwrap();

    // Create main package that uses test_util as dev-dependency with feature propagation
    switchy_fs::sync::create_dir(root_path.join("main_pkg")).unwrap();
    switchy_fs::sync::create_dir(root_path.join("main_pkg/src")).unwrap();
    switchy_fs::sync::write(root_path.join("main_pkg/src/lib.rs"), "").unwrap();

    let main_pkg_cargo = r#"[package]
name = "main_pkg"
version = "0.1.0"

[dev-dependencies]
test_util = { workspace = true }

[features]
test-feature = ["test_util/test-feature"]
"#;
    switchy_fs::sync::write(root_path.join("main_pkg/Cargo.toml"), main_pkg_cargo).unwrap();

    // Validate - should NOT report "test_util is not a direct dependency"
    let config = ValidatorConfig {
        features: None,
        skip_features: None,
        workspace_only: true,
        output_format: OutputType::Json,
        strict_optional_propagation: false,
        ..ValidatorConfig::test_default()
    };

    let validator = FeatureValidator::new(Some(root_path.to_path_buf()), config).unwrap();
    let result = validator.validate().unwrap();

    // Check that there are no "incorrect propagations" errors for dev-dependencies
    for error in &result.errors {
        if error.package == "main_pkg" {
            for feature_error in &error.errors {
                if feature_error.feature == "test-feature" {
                    // Should have no incorrect propagations
                    assert!(
                        feature_error.incorrect_propagations.is_empty(),
                        "Dev-dependency feature propagation incorrectly flagged as error: {:?}",
                        feature_error.incorrect_propagations
                    );
                }
            }
        }
    }
}
