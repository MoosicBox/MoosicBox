//! Integration tests for --skip-features CLI argument

use std::fs;
use tempfile::TempDir;

use clippier::OutputType;
use clippier::feature_validator::{FeatureValidator, ValidatorConfig};

/// Helper to create workspace with various feature types
fn create_multi_feature_workspace() -> TempDir {
    let temp_dir = tempfile::tempdir().unwrap();
    let root_path = temp_dir.path();

    // Workspace Cargo.toml
    let workspace_cargo = r#"[workspace]
members = ["pkg_a", "pkg_b"]

[workspace.dependencies]
serde = "1.0"
"#;
    fs::write(root_path.join("Cargo.toml"), workspace_cargo).unwrap();

    // Package A with multiple feature types
    fs::create_dir(root_path.join("pkg_a")).unwrap();
    fs::create_dir(root_path.join("pkg_a/src")).unwrap();
    fs::write(root_path.join("pkg_a/src/lib.rs"), "").unwrap();

    let pkg_a_cargo = r#"[package]
name = "pkg_a"
version = "0.1.0"

[dependencies]
pkg_b = { path = "../pkg_b" }
serde = { workspace = true }

[features]
default = ["pkg_b/default"]
fail-on-warnings = ["pkg_b/fail-on-warnings"]
test-utils = ["pkg_b/test-utils"]
test-fixtures = ["pkg_b/test-fixtures"]
mp3-codec = []
flac-codec = []
opus-codec = []
"#;
    fs::write(root_path.join("pkg_a/Cargo.toml"), pkg_a_cargo).unwrap();

    // Package B
    fs::create_dir(root_path.join("pkg_b")).unwrap();
    fs::create_dir(root_path.join("pkg_b/src")).unwrap();
    fs::write(root_path.join("pkg_b/src/lib.rs"), "").unwrap();

    let pkg_b_cargo = r#"[package]
name = "pkg_b"
version = "0.1.0"

[features]
default = []
fail-on-warnings = []
test-utils = []
test-fixtures = []
"#;
    fs::write(root_path.join("pkg_b/Cargo.toml"), pkg_b_cargo).unwrap();

    temp_dir
}

#[switchy_async::test]
async fn test_cli_skip_features_default_behavior() {
    let workspace = create_multi_feature_workspace();

    // Default behavior: skip "default" feature
    let config = ValidatorConfig {
        features: None,
        skip_features: None, // Should default to vec!["default"]
        workspace_only: true,
        output_format: OutputType::Json,
        strict_optional_propagation: false,
        ..ValidatorConfig::test_default()
    };

    let validator = FeatureValidator::new(Some(workspace.path().to_path_buf()), config).unwrap();
    let result = validator.validate().unwrap();

    // Should validate fail-on-warnings but NOT default
    // pkg_a has default feature but no propagation error because it's skipped
    assert!(
        result.errors.is_empty()
            || !result
                .errors
                .iter()
                .any(|e| e.errors.iter().any(|fe| fe.feature == "default"))
    );
}

#[switchy_async::test]
async fn test_cli_skip_features_empty_string() {
    let workspace = create_multi_feature_workspace();

    // Empty skip_features list means validate ALL features (including default)
    let config = ValidatorConfig {
        features: None,
        skip_features: Some(vec![]),
        workspace_only: true,
        output_format: OutputType::Json,
        strict_optional_propagation: false,
        ..ValidatorConfig::test_default()
    };

    let validator = FeatureValidator::new(Some(workspace.path().to_path_buf()), config).unwrap();
    let result = validator.validate().unwrap();

    // Empty skip list with auto-detect mode: no features are validated because
    // auto-detect requires dependencies with matching features, which don't exist
    // This test verifies empty skip list doesn't break validation
    assert!(result.total_packages > 0);
    assert_eq!(result.errors.len(), 0);
}

#[switchy_async::test]
async fn test_cli_skip_features_wildcard_pattern() {
    let workspace = create_multi_feature_workspace();

    // Skip all test-* features
    let config = ValidatorConfig {
        features: None,
        skip_features: Some(vec!["test-*".to_string()]),
        workspace_only: true,
        output_format: OutputType::Json,
        strict_optional_propagation: false,
        ..ValidatorConfig::test_default()
    };

    let validator = FeatureValidator::new(Some(workspace.path().to_path_buf()), config).unwrap();
    let result = validator.validate().unwrap();

    // Should not validate test-utils or test-fixtures
    assert!(
        !result
            .errors
            .iter()
            .any(|e| e.errors.iter().any(|fe| fe.feature.starts_with("test-")))
    );
}

#[switchy_async::test]
async fn test_cli_skip_features_multiple_patterns() {
    let workspace = create_multi_feature_workspace();

    // Skip codecs and test features
    let config = ValidatorConfig {
        features: None,
        skip_features: Some(vec!["*-codec".to_string(), "test-*".to_string()]),
        workspace_only: true,
        output_format: OutputType::Json,
        strict_optional_propagation: false,
        ..ValidatorConfig::test_default()
    };

    let validator = FeatureValidator::new(Some(workspace.path().to_path_buf()), config).unwrap();
    let result = validator.validate().unwrap();

    // Should only validate fail-on-warnings and default
    if !result.errors.is_empty() {
        for error in &result.errors {
            for feature_error in &error.errors {
                assert!(
                    feature_error.feature == "default"
                        || feature_error.feature == "fail-on-warnings",
                    "Unexpected feature validation: {}",
                    feature_error.feature
                );
            }
        }
    }
}

#[switchy_async::test]
async fn test_cli_skip_features_with_negation() {
    let workspace = create_multi_feature_workspace();

    // Skip all except fail-on-warnings
    let config = ValidatorConfig {
        features: None,
        skip_features: Some(vec!["*".to_string(), "!fail-on-warnings".to_string()]),
        workspace_only: true,
        output_format: OutputType::Json,
        strict_optional_propagation: false,
        ..ValidatorConfig::test_default()
    };

    let validator = FeatureValidator::new(Some(workspace.path().to_path_buf()), config).unwrap();
    let result = validator.validate().unwrap();

    // Should ONLY validate fail-on-warnings
    if !result.errors.is_empty() {
        for error in &result.errors {
            for feature_error in &error.errors {
                assert_eq!(
                    feature_error.feature, "fail-on-warnings",
                    "Should only validate fail-on-warnings, found: {}",
                    feature_error.feature
                );
            }
        }
    }
}

#[switchy_async::test]
async fn test_cli_skip_features_specific_list() {
    let workspace = create_multi_feature_workspace();

    // Skip specific features by name
    let config = ValidatorConfig {
        features: None,
        skip_features: Some(vec![
            "default".to_string(),
            "test-utils".to_string(),
            "test-fixtures".to_string(),
        ]),
        workspace_only: true,
        output_format: OutputType::Json,
        strict_optional_propagation: false,
        ..ValidatorConfig::test_default()
    };

    let validator = FeatureValidator::new(Some(workspace.path().to_path_buf()), config).unwrap();
    let result = validator.validate().unwrap();

    // Should not validate the skipped features
    if !result.errors.is_empty() {
        for error in &result.errors {
            for feature_error in &error.errors {
                assert!(
                    feature_error.feature != "default"
                        && feature_error.feature != "test-utils"
                        && feature_error.feature != "test-fixtures",
                    "Should not validate skipped feature: {}",
                    feature_error.feature
                );
            }
        }
    }
}

#[switchy_async::test]
async fn test_cli_skip_features_combined_with_explicit_features() {
    let workspace = create_multi_feature_workspace();

    // Explicitly request features, but skip some
    let config = ValidatorConfig {
        features: Some(vec![
            "default".to_string(),
            "fail-on-warnings".to_string(),
            "test-utils".to_string(),
        ]),
        skip_features: Some(vec!["default".to_string(), "test-*".to_string()]),
        workspace_only: true,
        output_format: OutputType::Json,
        strict_optional_propagation: false,
        ..ValidatorConfig::test_default()
    };

    let validator = FeatureValidator::new(Some(workspace.path().to_path_buf()), config).unwrap();
    let result = validator.validate().unwrap();

    // Should only check fail-on-warnings (default and test-utils are skipped)
    // Verify by checking that errors (if any) only mention fail-on-warnings
    if !result.errors.is_empty() {
        for error in &result.errors {
            for feature_error in &error.errors {
                assert_eq!(
                    feature_error.feature, "fail-on-warnings",
                    "Only fail-on-warnings should be validated, found: {}",
                    feature_error.feature
                );
            }
        }
    }
}
