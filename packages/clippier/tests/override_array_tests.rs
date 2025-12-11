use clippier::OutputType;
use clippier::feature_validator::{FeatureValidator, ValidatorConfig};
use switchy_fs::TempDir;

/// Helper to create a test workspace for override testing
fn create_override_test_workspace() -> TempDir {
    let temp_dir = switchy_fs::tempdir().unwrap();
    let root_path = temp_dir.path();

    // Create workspace Cargo.toml
    let workspace_cargo = r#"[workspace]
members = ["pkg_a", "pkg_b", "pkg_c"]

[workspace.dependencies]
anyhow = "1.0"
"#;
    switchy_fs::sync::write(root_path.join("Cargo.toml"), workspace_cargo).unwrap();

    // Create pkg_b with test-feature
    switchy_fs::sync::create_dir(root_path.join("pkg_b")).unwrap();
    switchy_fs::sync::create_dir(root_path.join("pkg_b/src")).unwrap();
    switchy_fs::sync::write(root_path.join("pkg_b/src/lib.rs"), "").unwrap();
    let pkg_b_cargo = r#"[package]
name = "pkg_b"
version = "0.1.0"

[features]
test-feature = []
"#;
    switchy_fs::sync::write(root_path.join("pkg_b/Cargo.toml"), pkg_b_cargo).unwrap();

    // Create pkg_c with test-feature
    switchy_fs::sync::create_dir(root_path.join("pkg_c")).unwrap();
    switchy_fs::sync::create_dir(root_path.join("pkg_c/src")).unwrap();
    switchy_fs::sync::write(root_path.join("pkg_c/src/lib.rs"), "").unwrap();
    let pkg_c_cargo = r#"[package]
name = "pkg_c"
version = "0.1.0"

[features]
test-feature = []
"#;
    switchy_fs::sync::write(root_path.join("pkg_c/Cargo.toml"), pkg_c_cargo).unwrap();

    // Create pkg_a (depends on pkg_b and pkg_c but doesn't propagate test-feature)
    switchy_fs::sync::create_dir(root_path.join("pkg_a")).unwrap();
    switchy_fs::sync::create_dir(root_path.join("pkg_a/src")).unwrap();
    switchy_fs::sync::write(root_path.join("pkg_a/src/lib.rs"), "").unwrap();
    let pkg_a_cargo = r#"[package]
name = "pkg_a"
version = "0.1.0"

[dependencies]
pkg_b = { path = "../pkg_b" }
pkg_c = { path = "../pkg_c" }

[features]
test-feature = []
"#;
    switchy_fs::sync::write(root_path.join("pkg_a/Cargo.toml"), pkg_a_cargo).unwrap();

    temp_dir
}

#[switchy_async::test]
async fn test_workspace_clippier_toml_array_expansion() {
    let temp_dir = create_override_test_workspace();
    let root_path = temp_dir.path();

    // Create workspace-level clippier.toml with array syntax
    let clippier_config = r#"
[[feature-validation.override]]
feature = "test-feature"
dependencies = ["pkg_b", "pkg_c"]
type = "allow-missing"
reason = "Testing array expansion in workspace config"
"#;
    switchy_fs::sync::write(root_path.join("clippier.toml"), clippier_config).unwrap();

    let config = ValidatorConfig {
        features: Some(vec!["test-feature".to_string()]),
        skip_features: None,
        workspace_only: true,
        output_format: OutputType::Raw,
        ..ValidatorConfig::default()
    };

    let validator = FeatureValidator::new(Some(root_path.to_path_buf()), config).unwrap();
    let result = validator.validate().unwrap();

    // Should have no errors because the array override suppresses both missing propagations
    assert_eq!(
        result.errors.len(),
        0,
        "Workspace-level array override should suppress errors"
    );

    // Verify override summary
    let summary = result.override_summary.as_ref().unwrap();
    assert_eq!(
        summary.total_applied, 2,
        "Should have 2 overrides applied (array expanded)"
    );
}

#[switchy_async::test]
async fn test_package_clippier_toml_array_expansion() {
    let temp_dir = create_override_test_workspace();
    let root_path = temp_dir.path();

    // Create package-level clippier.toml with array syntax
    let clippier_config = r#"
[[feature-validation.override]]
feature = "test-feature"
dependencies = ["pkg_b", "pkg_c"]
type = "allow-missing"
reason = "Testing array expansion in package config"
"#;
    switchy_fs::sync::write(root_path.join("pkg_a/clippier.toml"), clippier_config).unwrap();

    let config = ValidatorConfig {
        features: Some(vec!["test-feature".to_string()]),
        skip_features: None,
        workspace_only: true,
        output_format: OutputType::Raw,
        ..ValidatorConfig::default()
    };

    let validator = FeatureValidator::new(Some(root_path.to_path_buf()), config).unwrap();
    let result = validator.validate().unwrap();

    // Should have no errors
    assert_eq!(
        result.errors.len(),
        0,
        "Package-level array override should suppress errors"
    );

    // Verify override summary
    let summary = result.override_summary.as_ref().unwrap();
    assert_eq!(summary.total_applied, 2, "Should have 2 overrides applied");

    // Verify overridden errors count
    assert_eq!(
        result.overridden_errors.len(),
        2,
        "Should have 2 overridden errors"
    );
}

#[switchy_async::test]
async fn test_cargo_metadata_array_expansion() {
    let temp_dir = create_override_test_workspace();
    let root_path = temp_dir.path();

    // Create pkg_a with Cargo.toml metadata override using array syntax
    let pkg_a_cargo = r#"[package]
name = "pkg_a"
version = "0.1.0"

[package.metadata.clippier.feature-validation]
override = [
    { feature = "test-feature", dependencies = ["pkg_b", "pkg_c"], type = "allow-missing", reason = "Testing array in Cargo.toml metadata" }
]

[dependencies]
pkg_b = { path = "../pkg_b" }
pkg_c = { path = "../pkg_c" }

[features]
test-feature = []
"#;
    switchy_fs::sync::write(root_path.join("pkg_a/Cargo.toml"), pkg_a_cargo).unwrap();

    let config = ValidatorConfig {
        features: Some(vec!["test-feature".to_string()]),
        skip_features: None,
        workspace_only: true,
        output_format: OutputType::Raw,
        ..ValidatorConfig::default()
    };

    let validator = FeatureValidator::new(Some(root_path.to_path_buf()), config).unwrap();
    let result = validator.validate().unwrap();

    // Should have no errors
    assert_eq!(
        result.errors.len(),
        0,
        "Cargo.toml metadata array override should suppress errors"
    );

    // Verify 2 overrides were applied (expanded from array)
    let summary = result.override_summary.as_ref().unwrap();
    assert_eq!(
        summary.total_applied, 2,
        "Should have 2 overrides applied from Cargo.toml metadata"
    );
}

#[switchy_async::test]
async fn test_single_vs_array_equivalence() {
    let temp_dir = create_override_test_workspace();
    let root_path = temp_dir.path();

    // Test 1: Single dependency syntax
    let clippier_single = r#"
[[feature-validation.override]]
feature = "test-feature"
dependency = "pkg_b"
type = "allow-missing"
reason = "Single syntax"

[[feature-validation.override]]
feature = "test-feature"
dependency = "pkg_c"
type = "allow-missing"
reason = "Single syntax"
"#;
    switchy_fs::sync::write(root_path.join("clippier.toml"), clippier_single).unwrap();

    let config1 = ValidatorConfig {
        features: Some(vec!["test-feature".to_string()]),
        skip_features: None,
        workspace_only: true,
        output_format: OutputType::Raw,
        ..ValidatorConfig::default()
    };

    let validator1 = FeatureValidator::new(Some(root_path.to_path_buf()), config1).unwrap();
    let result1 = validator1.validate().unwrap();

    // Test 2: Array syntax
    let clippier_array = r#"
[[feature-validation.override]]
feature = "test-feature"
dependencies = ["pkg_b", "pkg_c"]
type = "allow-missing"
reason = "Array syntax"
"#;
    switchy_fs::sync::write(root_path.join("clippier.toml"), clippier_array).unwrap();

    let config2 = ValidatorConfig {
        features: Some(vec!["test-feature".to_string()]),
        skip_features: None,
        workspace_only: true,
        output_format: OutputType::Raw,
        ..ValidatorConfig::default()
    };

    let validator2 = FeatureValidator::new(Some(root_path.to_path_buf()), config2).unwrap();
    let result2 = validator2.validate().unwrap();

    // Both should produce the same result
    assert_eq!(result1.errors.len(), result2.errors.len());
    assert_eq!(
        result1.override_summary.as_ref().unwrap().total_applied,
        result2.override_summary.as_ref().unwrap().total_applied
    );
    assert_eq!(result1.valid_packages, result2.valid_packages);
}

#[switchy_async::test]
async fn test_array_with_wildcards() {
    let temp_dir = create_override_test_workspace();
    let root_path = temp_dir.path();

    // Use wildcard pattern in array
    let clippier_config = r#"
[[feature-validation.override]]
feature = "test-feature"
dependencies = ["pkg_*"]
type = "allow-missing"
reason = "Wildcard pattern in array"
"#;
    switchy_fs::sync::write(root_path.join("clippier.toml"), clippier_config).unwrap();

    let config = ValidatorConfig {
        features: Some(vec!["test-feature".to_string()]),
        skip_features: None,
        workspace_only: true,
        output_format: OutputType::Raw,
        ..ValidatorConfig::default()
    };

    let validator = FeatureValidator::new(Some(root_path.to_path_buf()), config).unwrap();
    let result = validator.validate().unwrap();

    // Should suppress errors for pkg_b and pkg_c via wildcard
    assert_eq!(
        result.errors.len(),
        0,
        "Wildcard in array should match multiple packages"
    );
}

#[switchy_async::test]
async fn test_mixed_array_and_single_entries() {
    let temp_dir = create_override_test_workspace();
    let root_path = temp_dir.path();

    // Mix array and single syntax in same config
    let clippier_config = r#"
[[feature-validation.override]]
feature = "test-feature"
dependencies = ["pkg_b", "pkg_c"]
type = "allow-missing"
reason = "Array syntax"
"#;
    switchy_fs::sync::write(root_path.join("clippier.toml"), clippier_config).unwrap();

    let config = ValidatorConfig {
        features: Some(vec!["test-feature".to_string()]),
        skip_features: None,
        workspace_only: true,
        output_format: OutputType::Raw,
        ..ValidatorConfig::default()
    };

    let validator = FeatureValidator::new(Some(root_path.to_path_buf()), config).unwrap();
    let result = validator.validate().unwrap();

    assert_eq!(result.errors.len(), 0);
    let summary = result.override_summary.as_ref().unwrap();
    assert_eq!(summary.total_applied, 2);
}

#[switchy_async::test]
async fn test_empty_array_handling() {
    // Test that empty arrays don't cause crashes (though they're not useful)
    let toml_str = r#"
feature = "test-feature"
dependencies = []
type = "allow-missing"
reason = "Empty array test"
"#;

    let result: Result<clippier::feature_validator::OverrideConfigEntry, _> =
        toml::from_str(toml_str);
    assert!(result.is_ok(), "Empty array should parse successfully");

    let entry = result.unwrap();
    assert_eq!(
        entry.dependency.to_vec().len(),
        0,
        "Empty array should produce empty vec"
    );
}

#[switchy_async::test]
async fn test_validation_without_overrides() {
    let temp_dir = create_override_test_workspace();
    let root_path = temp_dir.path();

    // Don't create any override configs
    let config = ValidatorConfig {
        features: Some(vec!["test-feature".to_string()]),
        skip_features: None,
        workspace_only: true,
        output_format: OutputType::Raw,
        ..ValidatorConfig::test_default() // Use test_default which disables overrides
    };

    let validator = FeatureValidator::new(Some(root_path.to_path_buf()), config).unwrap();
    let result = validator.validate().unwrap();

    // Should have errors for pkg_a not propagating to pkg_b and pkg_c
    assert!(
        !result.errors.is_empty(),
        "Should have validation errors without overrides"
    );

    // Find pkg_a errors
    let pkg_a_errors = result.errors.iter().find(|e| e.package == "pkg_a");
    assert!(pkg_a_errors.is_some(), "pkg_a should have errors");

    let errors = &pkg_a_errors.unwrap().errors;
    assert!(!errors.is_empty(), "pkg_a should have feature errors");

    let test_feature_error = errors.iter().find(|e| e.feature == "test-feature");
    assert!(
        test_feature_error.is_some(),
        "Should have error for test-feature"
    );

    let missing = &test_feature_error.unwrap().missing_propagations;
    assert_eq!(missing.len(), 2, "Should have 2 missing propagations");
}

#[switchy_async::test]
async fn test_validation_with_overrides_enabled() {
    let temp_dir = create_override_test_workspace();
    let root_path = temp_dir.path();

    // Create override with array syntax
    let clippier_config = r#"
[[feature-validation.override]]
feature = "test-feature"
dependencies = ["pkg_b", "pkg_c"]
type = "allow-missing"
reason = "Integration test for array overrides"
"#;
    switchy_fs::sync::write(root_path.join("pkg_a/clippier.toml"), clippier_config).unwrap();

    let config = ValidatorConfig {
        features: Some(vec!["test-feature".to_string()]),
        skip_features: None,
        workspace_only: true,
        output_format: OutputType::Raw,
        ..ValidatorConfig::default() // Default enables overrides
    };

    let validator = FeatureValidator::new(Some(root_path.to_path_buf()), config).unwrap();
    let result = validator.validate().unwrap();

    // Should have no errors (all suppressed)
    assert_eq!(
        result.errors.len(),
        0,
        "All errors should be suppressed by overrides"
    );

    // Should have override summary
    assert!(
        result.override_summary.is_some(),
        "Should have override summary"
    );
    let summary = result.override_summary.as_ref().unwrap();
    assert_eq!(summary.total_applied, 2, "Should apply 2 overrides");

    // Should have overridden errors listed
    assert!(
        !result.overridden_errors.is_empty(),
        "Should have overridden errors"
    );
    let overridden = &result.overridden_errors;
    assert_eq!(overridden.len(), 2, "Should have 2 overridden errors");

    // Verify the overridden errors are for the right dependencies
    let has_pkg_b = overridden.iter().any(|e| e.dependency == "pkg_b");
    let has_pkg_c = overridden.iter().any(|e| e.dependency == "pkg_c");
    assert!(has_pkg_b, "Should have overridden error for pkg_b");
    assert!(has_pkg_c, "Should have overridden error for pkg_c");
}
