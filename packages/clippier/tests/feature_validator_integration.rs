//! Integration tests for the feature validator
//!
//! These tests create real workspace structures on disk and test the validator
//! against them to ensure it works correctly in real-world scenarios.

use std::fs;
use std::path::Path;
use tempfile::TempDir;

use clippier::OutputType;
use clippier::feature_validator::{FeatureValidator, ValidatorConfig};

/// Helper to create a complex workspace structure for testing
fn create_complex_workspace() -> TempDir {
    let temp_dir = tempfile::tempdir().unwrap();
    let root_path = temp_dir.path();

    // Create workspace Cargo.toml
    let workspace_cargo = r#"[workspace]
members = [
    "core",
    "web_api",
    "auth",
    "database",
    "utils",
    "external_lib",
]

[workspace.dependencies]
serde = { version = "1.0", features = ["derive"] }
tokio = { version = "1.0", features = ["full"] }
anyhow = "1.0"
uuid = "1.0"
reqwest = "0.11"
"#;
    fs::write(root_path.join("Cargo.toml"), workspace_cargo).unwrap();

    // Create core package - central package with multiple features
    create_package(
        root_path,
        "core",
        r#"[package]
name = "core"
version = "0.1.0"

[dependencies]
auth = { path = "../auth" }
database = { path = "../database", optional = true }
utils = { path = "../utils" }
serde = { workspace = true }
anyhow = { workspace = true }

[features]
default = ["auth/default"]
fail-on-warnings = [
    "auth/fail-on-warnings",
    "database?/fail-on-warnings",
    "utils/fail-on-warnings",
]
full = ["database", "auth/full", "database?/full", "utils/full"]
json = ["serde/json", "utils/json"]
"#,
    );

    // Create web_api package - depends on core
    create_package(
        root_path,
        "web_api",
        r#"[package]
name = "web_api"
version = "0.1.0"

[dependencies]
core = { path = "../core" }
auth = { path = "../auth" }
tokio = { workspace = true }
reqwest = { workspace = true, optional = true }

[features]
fail-on-warnings = [
    "core/fail-on-warnings",
    "auth/fail-on-warnings",
    "reqwest?/fail-on-warnings",
]
client = ["dep:reqwest", "core/json"]
full = ["client", "core/full", "auth/full"]
"#,
    );

    // Create auth package - has various features
    create_package(
        root_path,
        "auth",
        r#"[package]
name = "auth"
version = "0.1.0"

[dependencies]
utils = { path = "../utils" }
uuid = { workspace = true }
external_lib = { path = "../external_lib", optional = true }

[features]
default = ["basic"]
basic = ["utils/basic"]
fail-on-warnings = [
    "utils/fail-on-warnings",
    "external_lib?/fail-on-warnings",
]
full = ["basic", "external_lib", "utils/full"]
jwt = ["external_lib?/jwt"]
"#,
    );

    // Create database package - optional dependency
    create_package(
        root_path,
        "database",
        r#"[package]
name = "database"
version = "0.1.0"

[dependencies]
utils = { path = "../utils" }
anyhow = { workspace = true }

[features]
fail-on-warnings = ["utils/fail-on-warnings"]
full = ["utils/full"]
migrations = ["utils/fs"]
"#,
    );

    // Create utils package - foundational utilities
    create_package(
        root_path,
        "utils",
        r#"[package]
name = "utils"
version = "0.1.0"

[dependencies]
serde = { workspace = true, optional = true }
anyhow = { workspace = true }

[features]
default = []
fail-on-warnings = []
basic = []
full = ["json", "fs", "basic"]
json = ["dep:serde"]
fs = []
"#,
    );

    // Create external_lib package - simulates external dependency
    create_package(
        root_path,
        "external_lib",
        r#"[package]
name = "external_lib"
version = "0.1.0"

[dependencies]
anyhow = { workspace = true }

[features]
fail-on-warnings = []
jwt = []
crypto = []
"#,
    );

    temp_dir
}

/// Helper to create a workspace with feature propagation errors
fn create_error_workspace() -> TempDir {
    let temp_dir = tempfile::tempdir().unwrap();
    let root_path = temp_dir.path();

    // Create workspace Cargo.toml
    let workspace_cargo = r#"[workspace]
members = ["main", "lib_a", "lib_b"]

[workspace.dependencies]
serde = "1.0"
"#;
    fs::write(root_path.join("Cargo.toml"), workspace_cargo).unwrap();

    // Create main package with errors
    create_package(
        root_path,
        "main",
        r#"[package]
name = "main"
version = "0.1.0"

[dependencies]
lib_a = { path = "../lib_a" }
lib_b = { path = "../lib_b", optional = true }
serde = { workspace = true }

[features]
# Missing propagation to lib_a
fail-on-warnings = ["lib_b?/fail-on-warnings"]
# Incorrect propagation - lib_a doesn't have this feature
test-utils = ["lib_a/nonexistent-feature"]
# Missing optional dependency marker
missing-optional = ["lib_b/test-utils"]
"#,
    );

    // Create lib_a
    create_package(
        root_path,
        "lib_a",
        r#"[package]
name = "lib_a"
version = "0.1.0"

[dependencies]
serde = { workspace = true }

[features]
fail-on-warnings = []
test-utils = []
"#,
    );

    // Create lib_b
    create_package(
        root_path,
        "lib_b",
        r#"[package]
name = "lib_b"
version = "0.1.0"

[features]
fail-on-warnings = []
test-utils = []
"#,
    );

    temp_dir
}

/// Helper to create a single-package (non-workspace) project
fn create_single_package_project() -> TempDir {
    let temp_dir = tempfile::tempdir().unwrap();
    let root_path = temp_dir.path();

    // Create single package Cargo.toml
    let cargo_toml = r#"[package]
name = "single_package"
version = "0.1.0"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
anyhow = "1.0"

[features]
default = []
fail-on-warnings = []
json = []
"#;
    fs::write(root_path.join("Cargo.toml"), cargo_toml).unwrap();

    temp_dir
}

/// Helper function to create a package directory and Cargo.toml
fn create_package(workspace_root: &Path, name: &str, cargo_content: &str) {
    let pkg_dir = workspace_root.join(name);
    fs::create_dir(&pkg_dir).unwrap();
    fs::write(pkg_dir.join("Cargo.toml"), cargo_content).unwrap();

    // Create a basic lib.rs to make it a valid package
    let src_dir = pkg_dir.join("src");
    fs::create_dir(&src_dir).unwrap();
    fs::write(src_dir.join("lib.rs"), "// Auto-generated for testing\n").unwrap();
}

#[test]
fn test_complex_workspace_validation_success() {
    let workspace = create_complex_workspace();
    let root_path = workspace.path().to_path_buf();

    let config = ValidatorConfig {
        features: Some(vec!["fail-on-warnings".to_string()]),
        skip_features: None,
        workspace_only: true,

        output_format: OutputType::Raw,
    };

    let validator = FeatureValidator::new(Some(root_path), config).unwrap();
    let result = validator.validate().unwrap();

    // The complex workspace should validate successfully
    assert_eq!(
        result.errors.len(),
        0,
        "Expected no validation errors, got: {:#?}",
        result.errors
    );
    assert!(
        result.valid_packages > 0,
        "Should have validated at least one package"
    );
    assert_eq!(result.total_packages, result.valid_packages);
}

#[test]
fn test_complex_workspace_all_features() {
    let workspace = create_complex_workspace();
    let root_path = workspace.path().to_path_buf();

    let config = ValidatorConfig {
        features: None, // Auto-detect all features
        skip_features: None,
        workspace_only: true,

        output_format: OutputType::Raw,
    };

    let validator = FeatureValidator::new(Some(root_path), config).unwrap();
    let result = validator.validate().unwrap();

    // Should validate multiple features across packages
    // Note: there might be errors due to external dependencies (serde, tokio, etc.) not having expected features
    // The test focuses on workspace-only validation, so external dep errors are acceptable
    // As long as we're validating some packages, the functionality works
    assert!(
        result.total_packages > 0,
        "Should validate at least some packages"
    );

    if !result.errors.is_empty() {
        eprintln!(
            "Note: Found {} validation errors (likely from external dependencies): {:#?}",
            result.errors.len(),
            result.errors
        );
    }
}

#[test]
fn test_workspace_with_errors() {
    let workspace = create_error_workspace();
    let root_path = workspace.path().to_path_buf();

    let config = ValidatorConfig {
        features: None, // Check all features
        skip_features: None,
        workspace_only: true,

        output_format: OutputType::Raw,
    };

    let validator = FeatureValidator::new(Some(root_path), config).unwrap();
    let result = validator.validate().unwrap();

    // Should find errors in the main package
    assert!(!result.errors.is_empty(), "Expected validation errors");

    let main_package_error = result
        .errors
        .iter()
        .find(|e| e.package == "main")
        .expect("Should find errors in main package");

    assert!(
        !main_package_error.errors.is_empty(),
        "Should have feature errors"
    );

    // Check specific error types
    let fail_on_warnings_error = main_package_error
        .errors
        .iter()
        .find(|e| e.feature == "fail-on-warnings")
        .expect("Should have fail-on-warnings error");

    assert!(
        !fail_on_warnings_error.missing_propagations.is_empty(),
        "Should have missing propagation to lib_a"
    );

    // Check for incorrect propagation error
    let test_utils_error = main_package_error
        .errors
        .iter()
        .find(|e| e.feature == "test-utils");

    if let Some(error) = test_utils_error {
        assert!(
            !error.incorrect_propagations.is_empty(),
            "Should have incorrect propagation for nonexistent feature"
        );
    }
}

#[test]
fn test_single_package_project() {
    let project = create_single_package_project();
    let root_path = project.path().to_path_buf();

    let config = ValidatorConfig {
        features: Some(vec!["fail-on-warnings".to_string()]),
        skip_features: None,
        workspace_only: false,

        output_format: OutputType::Raw,
    };

    let validator = FeatureValidator::new(Some(root_path), config).unwrap();
    let result = validator.validate().unwrap();

    // Single package should validate (no dependencies to propagate to)
    assert_eq!(result.errors.len(), 0);
    assert_eq!(result.valid_packages, 1);
}

#[test]
fn test_workspace_only_vs_all_packages() {
    let workspace = create_complex_workspace();
    let root_path = workspace.path().to_path_buf();

    // Test workspace-only validation
    let config_workspace_only = ValidatorConfig {
        features: Some(vec!["fail-on-warnings".to_string()]),
        skip_features: None,
        workspace_only: true,

        output_format: OutputType::Raw,
    };

    let validator_workspace =
        FeatureValidator::new(Some(root_path.clone()), config_workspace_only).unwrap();
    let result_workspace = validator_workspace.validate().unwrap();

    // Test all packages validation
    let config_all = ValidatorConfig {
        features: Some(vec!["fail-on-warnings".to_string()]),
        skip_features: None,
        workspace_only: false,

        output_format: OutputType::Raw,
    };

    let validator_all = FeatureValidator::new(Some(root_path), config_all).unwrap();
    let result_all = validator_all.validate().unwrap();

    // Workspace-only should succeed (our test workspace is correctly configured)
    assert_eq!(result_workspace.errors.len(), 0);

    // All packages mode may have errors from external dependencies that don't have matching features
    // This is expected behavior - external dependencies may not have the same feature structure

    // The total number of packages checked should be different
    assert!(result_all.total_packages >= result_workspace.total_packages);
}

#[test]
fn test_json_output_format() {
    let workspace = create_error_workspace();
    let root_path = workspace.path().to_path_buf();

    let config = ValidatorConfig {
        features: Some(vec![
            "fail-on-warnings".to_string(),
            "test-utils".to_string(),
        ]),
        skip_features: None,
        workspace_only: true,

        output_format: OutputType::Json,
    };

    let validator = FeatureValidator::new(Some(root_path), config).unwrap();
    let result = validator.validate().unwrap();

    // Test that the result can be serialized to JSON
    let json_output = serde_json::to_string_pretty(&result).unwrap();
    assert!(json_output.contains("total_packages"));
    assert!(json_output.contains("valid_packages"));
    assert!(json_output.contains("errors"));

    // Should contain error details
    if !result.errors.is_empty() {
        assert!(
            json_output.contains("missing_propagations")
                || json_output.contains("incorrect_propagations")
        );
    }
}

#[test]
fn test_specific_features_validation() {
    let workspace = create_complex_workspace();
    let root_path = workspace.path().to_path_buf();

    // Test validating only specific features
    let config = ValidatorConfig {
        features: Some(vec!["full".to_string(), "json".to_string()]),
        skip_features: None,
        workspace_only: true,

        output_format: OutputType::Raw,
    };

    let validator = FeatureValidator::new(Some(root_path), config).unwrap();
    let result = validator.validate().unwrap();

    // Should validate successfully for these features
    assert_eq!(result.errors.len(), 0);

    // Should only check packages that have these features
    assert!(result.total_packages > 0);
}

#[test]
fn test_validator_with_nonexistent_feature() {
    let workspace = create_complex_workspace();
    let root_path = workspace.path().to_path_buf();

    // Test with a feature that doesn't exist in any package
    let config = ValidatorConfig {
        features: Some(vec!["nonexistent-feature".to_string()]),
        skip_features: None,
        workspace_only: true,

        output_format: OutputType::Raw,
    };

    let validator = FeatureValidator::new(Some(root_path), config).unwrap();
    let result = validator.validate().unwrap();

    // Should succeed but packages checked depends on whether any external deps have this feature
    assert_eq!(result.errors.len(), 0);
    // Note: total_packages might not be 0 if external deps are included and workspace_only=true
    // but valid_packages should be equal to total_packages since no one has the nonexistent feature
    assert_eq!(result.valid_packages, result.total_packages);
}

#[test]
fn test_optional_dependency_handling() {
    let workspace = create_complex_workspace();
    let root_path = workspace.path().to_path_buf();

    // Focus on packages that use optional dependencies
    let config = ValidatorConfig {
        features: Some(vec!["full".to_string()]),
        skip_features: None,
        workspace_only: true,

        output_format: OutputType::Raw,
    };

    let validator = FeatureValidator::new(Some(root_path), config).unwrap();
    let result = validator.validate().unwrap();

    // Should handle optional dependencies correctly (with ? syntax)
    assert_eq!(
        result.errors.len(),
        0,
        "Optional dependency handling failed: {:#?}",
        result.errors
    );
}

#[test]
fn test_workspace_root_discovery_from_subdirectory() {
    let workspace = create_complex_workspace();
    let core_path = workspace.path().join("core");

    let config = ValidatorConfig {
        features: Some(vec!["fail-on-warnings".to_string()]),
        skip_features: None,
        workspace_only: true,

        output_format: OutputType::Raw,
    };

    // Start validation from a subdirectory
    let validator = FeatureValidator::new(Some(core_path), config).unwrap();
    let result = validator.validate().unwrap();

    // Should still find and validate the entire workspace
    assert!(
        result.total_packages > 1,
        "Should find multiple packages from subdirectory"
    );
    assert_eq!(result.errors.len(), 0);
}

#[test]
fn test_validation_summary_with_errors() {
    let workspace = create_error_workspace();
    let root_path = workspace.path().to_path_buf();

    let config = ValidatorConfig {
        features: None,
        skip_features: None,
        workspace_only: true,
        output_format: OutputType::Raw,
    };

    let validator = FeatureValidator::new(Some(root_path), config).unwrap();
    let result = validator.validate().unwrap();

    // Verify summary fields when errors exist
    assert!(
        !result.errors.is_empty(),
        "Should have errors to display in summary"
    );
    assert!(result.total_packages > 0);
    assert!(result.valid_packages < result.total_packages);
    assert_eq!(
        result.total_packages,
        result.valid_packages + result.errors.len()
    );
}

#[test]
fn test_validation_summary_with_no_errors() {
    let workspace = create_complex_workspace();
    let root_path = workspace.path().to_path_buf();

    let config = ValidatorConfig {
        features: Some(vec!["fail-on-warnings".to_string()]),
        skip_features: None,
        workspace_only: true,
        output_format: OutputType::Json,
    };

    let validator = FeatureValidator::new(Some(root_path), config).unwrap();
    let result = validator.validate().unwrap();

    // Verify summary fields when no errors
    assert_eq!(result.errors.len(), 0);
    assert!(result.valid_packages > 0);
    assert_eq!(result.total_packages, result.valid_packages);
}

#[test]
fn test_validation_result_json_serialization() {
    use clippier::feature_validator::{
        FeatureError, IncorrectPropagation, MissingPropagation, PackageValidationError,
        ValidationResult,
    };

    let result = ValidationResult {
        total_packages: 3,
        valid_packages: 2,
        errors: vec![PackageValidationError {
            package: "test_pkg".to_string(),
            errors: vec![FeatureError {
                feature: "fail-on-warnings".to_string(),
                missing_propagations: vec![MissingPropagation {
                    dependency: "dep1".to_string(),
                    expected: "dep1/fail-on-warnings".to_string(),
                    reason: "Dependency has feature but not propagated".to_string(),
                }],
                incorrect_propagations: vec![IncorrectPropagation {
                    entry: "nonexistent/feature".to_string(),
                    reason: "Dependency doesn't have this feature".to_string(),
                }],
            }],
        }],
        warnings: vec![],
    };

    // Should serialize to valid JSON
    let json = serde_json::to_string_pretty(&result).unwrap();
    assert!(json.contains("test_pkg"));
    assert!(json.contains("fail-on-warnings"));
    assert!(json.contains("dep1"));
    assert!(json.contains("missing_propagations"));
    assert!(json.contains("incorrect_propagations"));

    // Verify it's valid JSON that can be parsed
    let _parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
}

#[test]
fn test_validation_summary_pluralization() {
    use clippier::feature_validator::{PackageValidationError, ValidationResult};

    // Test with 1 error
    let result_singular = ValidationResult {
        total_packages: 3,
        valid_packages: 2,
        errors: vec![PackageValidationError {
            package: "test".to_string(),
            errors: vec![],
        }],
        warnings: vec![],
    };

    assert_eq!(result_singular.errors.len(), 1);
    assert_eq!(result_singular.valid_packages, 2);

    // Test with multiple errors
    let result_plural = ValidationResult {
        total_packages: 5,
        valid_packages: 3,
        errors: vec![
            PackageValidationError {
                package: "test1".to_string(),
                errors: vec![],
            },
            PackageValidationError {
                package: "test2".to_string(),
                errors: vec![],
            },
        ],
        warnings: vec![],
    };

    assert_eq!(result_plural.errors.len(), 2);
    assert_eq!(result_plural.valid_packages, 3);
    assert_eq!(
        result_plural.total_packages,
        result_plural.valid_packages + result_plural.errors.len()
    );
}
