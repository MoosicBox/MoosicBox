//! Feature propagation validation for workspace dependencies.
//!
//! This module validates that Cargo features are correctly propagated across workspace
//! dependencies to ensure consistent builds and prevent feature-related compilation failures.
//!
//! # Purpose
//!
//! When a workspace package depends on another workspace package that has a specific feature,
//! that feature should be propagated in the dependent package's feature definition. This
//! validator ensures such propagation is correct and complete.
//!
//! # Features
//!
//! * Auto-detect features that need validation across the workspace
//! * Validate specific features or all matching features
//! * Detect missing feature propagations
//! * Detect incorrect feature propagations
//! * Support for optional dependencies with `?` syntax
//! * JSON and human-readable output formats
//!
//! # Example
//!
//! ```rust
//! use clippier::feature_validator::{FeatureValidator, ValidatorConfig};
//! use clippier::OutputType;
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = ValidatorConfig {
//!     features: Some(vec!["fail-on-warnings".to_string()]),
//!     skip_features: None,
//!     workspace_only: true,
//!     output_format: OutputType::Json,
//!     strict_optional_propagation: false,
//! };
//!
//! let validator = FeatureValidator::new(None, config)?;
//! let result = validator.validate()?;
//!
//! if result.errors.is_empty() {
//!     println!("All features correctly propagated!");
//! }
//! # Ok(())
//! # }
//! ```

#![allow(clippy::similar_names)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use serde::Serialize;
use toml::Value;

use crate::{OutputType, should_skip_feature};

/// Type aliases for complex types
type WorkspacePackages = BTreeSet<String>;
type PackagePaths = BTreeMap<String, String>;
type PackageCargoValues = BTreeMap<String, Value>;
type WorkspaceData = (WorkspacePackages, PackagePaths, PackageCargoValues);

/// Validation results for feature propagation
#[derive(Debug, Serialize)]
pub struct ValidationResult {
    /// Total number of packages validated
    pub total_packages: usize,
    /// Number of packages that passed validation
    pub valid_packages: usize,
    /// Validation errors found across packages
    pub errors: Vec<PackageValidationError>,
    /// Non-critical warnings found during validation
    pub warnings: Vec<PackageValidationWarning>,
}

/// Validation errors for a single package
#[derive(Debug, Serialize)]
pub struct PackageValidationError {
    /// Name of the package with validation errors
    pub package: String,
    /// List of feature-specific validation errors
    pub errors: Vec<FeatureError>,
}

/// Feature validation error details
#[derive(Debug, Serialize)]
pub struct FeatureError {
    /// Name of the feature with validation errors
    pub feature: String,
    /// Missing feature propagations that should be added
    pub missing_propagations: Vec<MissingPropagation>,
    /// Incorrect feature propagations that need correction
    pub incorrect_propagations: Vec<IncorrectPropagation>,
}

/// A feature propagation that is missing from the package definition
#[derive(Debug, Serialize)]
pub struct MissingPropagation {
    /// Name of the dependency that should have the feature propagated
    pub dependency: String,
    /// Expected feature propagation entry (e.g., "dep:feature-name")
    pub expected: String,
    /// Explanation of why this propagation is required
    pub reason: String,
}

/// An incorrect feature propagation in the package definition
#[derive(Debug, Serialize)]
pub struct IncorrectPropagation {
    /// The problematic feature propagation entry
    pub entry: String,
    /// Explanation of why this propagation is incorrect
    pub reason: String,
}

/// A non-critical validation warning for a package
#[derive(Debug, Serialize)]
pub struct PackageValidationWarning {
    /// Name of the package with the warning
    pub package: String,
    /// Warning message
    pub message: String,
}

/// Configuration for feature validation
pub struct ValidatorConfig {
    /// Specific features to validate (None = validate all matching features)
    pub features: Option<Vec<String>>,
    /// Features to skip during validation (defaults to `["default", "_*"]`)
    pub skip_features: Option<Vec<String>>,
    /// Whether to validate workspace packages only
    pub workspace_only: bool,
    /// Output format
    pub output_format: OutputType,
    /// Require strict `?` syntax for optional dependencies (default: false)
    ///
    /// When false (lenient mode), accepts both `dep?/feature` and `dep/feature` for optional deps.
    /// When true (strict mode), only accepts `dep?/feature` for optional deps.
    pub strict_optional_propagation: bool,
}

/// Main validator struct
pub struct FeatureValidator {
    workspace_packages: BTreeSet<String>,
    package_cargo_values: BTreeMap<String, Value>,
    config: ValidatorConfig,
}

impl FeatureValidator {
    /// Create a new validator from the current directory or specified path
    ///
    /// # Errors
    ///
    /// * Returns an error if the workspace root cannot be found
    /// * Returns an error if workspace data cannot be loaded (invalid TOML, missing files, etc.)
    pub fn new(path: Option<PathBuf>, config: ValidatorConfig) -> Result<Self> {
        let workspace_root = find_workspace_root(path)?;
        let (workspace_packages, _package_paths, package_cargo_values) =
            load_workspace_data(&workspace_root)?;

        Ok(Self {
            workspace_packages,
            package_cargo_values,
            config,
        })
    }

    /// Run the validation
    ///
    /// # Errors
    ///
    /// * Returns an error if package validation fails due to invalid TOML structure
    /// * Returns an error if feature validation encounters unexpected data format
    pub fn validate(&self) -> Result<ValidationResult> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut valid_count = 0;

        let packages_to_check: Vec<(&String, &Value)> = if self.config.workspace_only {
            self.package_cargo_values
                .iter()
                .filter(|(name, _)| self.workspace_packages.contains(*name))
                .collect()
        } else {
            self.package_cargo_values.iter().collect()
        };

        for (package_name, cargo_value) in packages_to_check {
            match self.validate_package(package_name, cargo_value) {
                Ok(Some(error)) => errors.push(error),
                Ok(None) => valid_count += 1,
                Err(e) => warnings.push(PackageValidationWarning {
                    package: package_name.clone(),
                    message: format!("Failed to validate: {e}"),
                }),
            }
        }

        Ok(ValidationResult {
            total_packages: valid_count + errors.len(),
            valid_packages: valid_count,
            errors,
            warnings,
        })
    }

    /// Validate a single package
    fn validate_package(
        &self,
        package_name: &str,
        cargo_value: &Value,
    ) -> Result<Option<PackageValidationError>> {
        let features_to_check = self.get_features_to_check(package_name, cargo_value);

        if features_to_check.is_empty() {
            return Ok(None);
        }

        let mut feature_errors = Vec::new();

        for feature in features_to_check {
            let (missing, incorrect) =
                self.validate_feature(package_name, &feature, cargo_value)?;

            if !missing.is_empty() || !incorrect.is_empty() {
                feature_errors.push(FeatureError {
                    feature: feature.clone(),
                    missing_propagations: missing,
                    incorrect_propagations: incorrect,
                });
            }
        }

        if feature_errors.is_empty() {
            Ok(None)
        } else {
            Ok(Some(PackageValidationError {
                package: package_name.to_string(),
                errors: feature_errors,
            }))
        }
    }

    /// Get features to check for a package
    fn get_features_to_check(&self, _package_name: &str, cargo_value: &Value) -> Vec<String> {
        let Some(features_table) = cargo_value.get("features").and_then(|f| f.as_table()) else {
            return Vec::new();
        };

        // Build skip list: use provided skip_features, or default to ["default", "_*"]
        // Keep as Vec<String> for glob pattern matching
        let skip_features_vec: Vec<String> = self
            .config
            .skip_features
            .clone()
            .unwrap_or_else(|| vec!["default".to_string(), "_*".to_string()]);

        self.config.features.as_ref().map_or_else(
            || {
                // Auto-detect mode: Check all features that have matching names in dependencies
                let mut features_to_check = Vec::new();

                for feature_name in features_table.keys() {
                    // Skip features using glob pattern matching (supports wildcards and negation)
                    if should_skip_feature(feature_name, &skip_features_vec) {
                        continue;
                    }

                    // Check if any dependency has the same feature
                    if self.any_dependency_has_feature(cargo_value, feature_name) {
                        features_to_check.push(feature_name.clone());
                    }
                }

                features_to_check
            },
            |specific_features| {
                // Specific features mode: Only check specified features that exist in this package
                specific_features
                    .iter()
                    .filter(|f| !should_skip_feature(f, &skip_features_vec))
                    .filter(|f| features_table.contains_key(*f))
                    .cloned()
                    .collect()
            },
        )
    }

    /// Check if any dependency has a specific feature
    fn any_dependency_has_feature(&self, cargo_value: &Value, feature_name: &str) -> bool {
        let deps = extract_all_dependencies(cargo_value, false);

        for (dep_name, _) in deps {
            if self.config.workspace_only && !self.workspace_packages.contains(&dep_name) {
                continue;
            }

            if self.dependency_has_feature(&dep_name, feature_name) {
                return true;
            }
        }

        false
    }

    /// Check if a dependency has a specific feature
    fn dependency_has_feature(&self, dep_name: &str, feature_name: &str) -> bool {
        self.package_cargo_values
            .get(dep_name)
            .and_then(|v| v.get("features"))
            .and_then(|f| f.as_table())
            .is_some_and(|t| t.contains_key(feature_name))
    }

    /// Validate a specific feature in a package
    fn validate_feature(
        &self,
        _package_name: &str,
        feature_name: &str,
        cargo_value: &Value,
    ) -> Result<(Vec<MissingPropagation>, Vec<IncorrectPropagation>)> {
        let mut missing = Vec::new();
        let mut incorrect = Vec::new();

        // Get the feature definition
        let feature_def = cargo_value
            .get("features")
            .and_then(|f| f.get(feature_name))
            .and_then(|f| f.as_array())
            .ok_or_else(|| anyhow!("Feature {feature_name} not found"))?;

        // Get expected propagations
        let expected = self.get_expected_propagations(cargo_value, feature_name);

        // Parse actual propagations from feature definition
        let actual = parse_feature_propagations(feature_def);

        // Find missing propagations
        for (dep_name, expected_entry) in &expected {
            let is_propagated = if self.config.strict_optional_propagation {
                // Strict mode: require exact match (dep?/feature for optional deps)
                actual.contains(expected_entry)
            } else {
                // Lenient mode: accept both dep?/feature and dep/feature for optional deps
                if expected_entry.contains('?') {
                    let without_question = expected_entry.replace("?/", "/");
                    actual.contains(expected_entry) || actual.contains(&without_question)
                } else {
                    actual.contains(expected_entry)
                }
            };

            if !is_propagated {
                missing.push(MissingPropagation {
                    dependency: dep_name.clone(),
                    expected: expected_entry.clone(),
                    reason: format!(
                        "Dependency '{dep_name}' has feature '{feature_name}' but it's not propagated"
                    ),
                });
            }
        }

        // Find incorrect propagations
        for entry in &actual {
            if let Some(dep_name) = extract_dependency_name(entry) {
                if self.config.workspace_only && !self.workspace_packages.contains(&dep_name) {
                    continue;
                }

                // Extract the feature name from the entry
                let entry_feature = entry.split('/').nth(1).unwrap_or(feature_name);

                if !expected.values().any(|e| e == entry) {
                    // Include dev-dependencies when checking if a dependency is direct
                    // because features CAN propagate to dev-dependencies (used in tests, examples, etc.)
                    let all_deps = extract_all_dependencies(cargo_value, true);
                    let is_direct_dep = all_deps.iter().any(|(n, _)| n == &dep_name);

                    if !is_direct_dep {
                        incorrect.push(IncorrectPropagation {
                            entry: entry.clone(),
                            reason: format!(
                                "'{dep_name}' is not a direct dependency of this package"
                            ),
                        });
                    } else if !self.dependency_has_feature(&dep_name, entry_feature) {
                        incorrect.push(IncorrectPropagation {
                            entry: entry.clone(),
                            reason: format!(
                                "Dependency '{dep_name}' doesn't have feature '{entry_feature}'"
                            ),
                        });
                    }
                }
            }
        }

        Ok((missing, incorrect))
    }

    /// Get expected propagations for a feature
    fn get_expected_propagations(
        &self,
        cargo_value: &Value,
        feature_name: &str,
    ) -> BTreeMap<String, String> {
        let mut expected = BTreeMap::new();

        // Get all dependencies (excluding dev-dependencies)
        let deps = extract_all_dependencies(cargo_value, false);

        for (dep_name, is_optional) in deps {
            // Skip if workspace_only and not a workspace package
            if self.config.workspace_only && !self.workspace_packages.contains(&dep_name) {
                continue;
            }

            // Check if the dependency has this feature
            if self.dependency_has_feature(&dep_name, feature_name) {
                let propagation = if is_optional {
                    format!("{dep_name}?/{feature_name}")
                } else {
                    format!("{dep_name}/{feature_name}")
                };

                expected.insert(dep_name.clone(), propagation);
            }
        }

        expected
    }
}

/// Find workspace root from a given path
fn find_workspace_root(path: Option<PathBuf>) -> Result<PathBuf> {
    let start_dir = path.unwrap_or_else(|| std::env::current_dir().unwrap());

    let mut current = start_dir.as_path();
    loop {
        let cargo_toml = current.join("Cargo.toml");
        if cargo_toml.exists() {
            let content = fs::read_to_string(&cargo_toml)?;
            let value: Value = toml::from_str(&content)?;
            if value.get("workspace").is_some() {
                return Ok(current.to_path_buf());
            }
        }

        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }

    // If no workspace found, use start directory if it has Cargo.toml
    let cargo_toml = start_dir.join("Cargo.toml");
    if cargo_toml.exists() {
        Ok(start_dir)
    } else {
        Err(anyhow!("Could not find Cargo.toml or workspace root"))
    }
}

/// Load workspace data - packages, paths, and cargo values
fn load_workspace_data(workspace_root: &Path) -> Result<WorkspaceData> {
    let workspace_cargo_path = workspace_root.join("Cargo.toml");
    let workspace_source = fs::read_to_string(&workspace_cargo_path)?;
    let workspace_value: Value = toml::from_str(&workspace_source)?;

    // Handle both workspace and single-package projects
    let workspace_members = workspace_value
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .and_then(|a| a.iter().map(|v| v.as_str()).collect::<Option<Vec<_>>>())
        .map_or_else(|| vec!["."], |members| members);

    let mut workspace_packages = BTreeSet::new();
    let mut package_paths = BTreeMap::new();
    let mut package_cargo_values = BTreeMap::new();

    for member_path in workspace_members {
        let full_path = if member_path == "." {
            workspace_root.to_path_buf()
        } else {
            workspace_root.join(member_path)
        };
        let cargo_path = full_path.join("Cargo.toml");

        if !cargo_path.exists() {
            continue;
        }

        let source = fs::read_to_string(&cargo_path)?;
        let value: Value = toml::from_str(&source)?;

        if let Some(package_name) = value
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
        {
            workspace_packages.insert(package_name.to_string());
            package_paths.insert(package_name.to_string(), member_path.to_string());
            package_cargo_values.insert(package_name.to_string(), value);
        }
    }

    Ok((workspace_packages, package_paths, package_cargo_values))
}

/// Extract all dependencies from a Cargo.toml value (excluding dev-dependencies by default)
/// Returns tuples of (name, `is_optional`)
fn extract_all_dependencies(cargo_value: &Value, include_dev: bool) -> Vec<(String, bool)> {
    let mut deps = Vec::new();

    // Helper to extract from a section
    let extract_from_section = |section: &Value| -> Vec<(String, bool)> {
        let mut section_deps = Vec::new();
        if let Some(dependencies) = section.as_table() {
            for (dep_name, dep_value) in dependencies {
                let is_optional = if let Value::Table(table) = dep_value {
                    table.get("optional") == Some(&Value::Boolean(true))
                } else {
                    false
                };
                section_deps.push((dep_name.clone(), is_optional));
            }
        }
        section_deps
    };

    // Regular dependencies
    if let Some(dependencies) = cargo_value.get("dependencies") {
        deps.extend(extract_from_section(dependencies));
    }

    // Build dependencies
    if let Some(build_dependencies) = cargo_value.get("build-dependencies") {
        deps.extend(extract_from_section(build_dependencies));
    }

    // Dev dependencies (optional)
    if include_dev && let Some(dev_dependencies) = cargo_value.get("dev-dependencies") {
        deps.extend(extract_from_section(dev_dependencies));
    }

    // Remove duplicates while preserving the most permissive optional status
    let mut deduped = BTreeMap::new();
    for (name, is_optional) in deps {
        deduped
            .entry(name)
            .and_modify(|opt| *opt = *opt && is_optional)
            .or_insert(is_optional);
    }

    deduped.into_iter().collect()
}

/// Parse feature propagations from a feature definition array
fn parse_feature_propagations(feature_def: &[Value]) -> BTreeSet<String> {
    feature_def
        .iter()
        .filter_map(|v| v.as_str())
        .filter(|s| s.contains('/'))
        .map(std::string::ToString::to_string)
        .collect()
}

/// Extract dependency name from a feature propagation entry
fn extract_dependency_name(entry: &str) -> Option<String> {
    if entry.contains('/') {
        entry
            .split('/')
            .next()
            .map(|s| s.trim_end_matches('?').to_string())
    } else {
        None
    }
}

/// Print human-readable output
pub fn print_human_output(result: &ValidationResult) {
    println!("🔍 Feature Propagation Validation Results");
    println!("=========================================");
    println!("Total packages checked: {}", result.total_packages);
    println!("Valid packages: {}", result.valid_packages);

    if !result.warnings.is_empty() {
        println!("\n⚠️  Warnings:");
        for warning in &result.warnings {
            println!("  - {}: {}", warning.package, warning.message);
        }
    }

    if result.errors.is_empty() {
        println!("\n✅ All packages correctly propagate features!");
    } else {
        println!(
            "\n❌ Found {} packages with incorrect feature propagation:",
            result.errors.len()
        );

        for error in &result.errors {
            println!("\n📦 Package: {}", error.package);

            for feature_error in &error.errors {
                println!("  Feature: {}", feature_error.feature);

                if !feature_error.missing_propagations.is_empty() {
                    println!("    Missing propagations:");
                    for missing in &feature_error.missing_propagations {
                        println!("      - {} ({})", missing.expected, missing.reason);
                    }
                }

                if !feature_error.incorrect_propagations.is_empty() {
                    println!("    Incorrect entries:");
                    for incorrect in &feature_error.incorrect_propagations {
                        println!("      - {} ({})", incorrect.entry, incorrect.reason);
                    }
                }
            }
        }
    }
}

/// Print GitHub Actions format output
pub fn print_github_output(result: &ValidationResult) {
    for error in &result.errors {
        for feature_error in &error.errors {
            for missing in &feature_error.missing_propagations {
                println!(
                    "::error file=packages/{}/Cargo.toml::Missing feature propagation '{}' for feature '{}'",
                    error.package, missing.expected, feature_error.feature
                );
            }

            for incorrect in &feature_error.incorrect_propagations {
                println!(
                    "::error file=packages/{}/Cargo.toml::Incorrect feature propagation '{}' for feature '{}'",
                    error.package, incorrect.entry, feature_error.feature
                );
            }
        }
    }

    for warning in &result.warnings {
        println!(
            "::warning file=packages/{}/Cargo.toml::{}",
            warning.package, warning.message
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;
    use tempfile::TempDir;

    /// Helper to create a temporary workspace for testing
    #[allow(clippy::similar_names)]
    fn create_test_workspace() -> TempDir {
        let temp_dir = tempfile::tempdir().unwrap();
        let root_path = temp_dir.path();

        // Create workspace Cargo.toml
        let workspace_cargo = r#"[workspace]
members = ["pkg_a", "pkg_b", "pkg_c"]

[workspace.dependencies]
anyhow = "1.0"
serde = "1.0"
"#;
        fs::write(root_path.join("Cargo.toml"), workspace_cargo).unwrap();

        // Create pkg_a with fail-on-warnings feature
        fs::create_dir(root_path.join("pkg_a")).unwrap();
        let package_a_cargo = r#"[package]
name = "pkg_a"
version = "0.1.0"

[dependencies]
pkg_b = { path = "../pkg_b" }
anyhow = { workspace = true }

[features]
fail-on-warnings = ["pkg_b/fail-on-warnings"]
test-feature = ["pkg_b/test-feature"]
"#;
        fs::write(root_path.join("pkg_a/Cargo.toml"), package_a_cargo).unwrap();

        // Create pkg_b with fail-on-warnings feature
        fs::create_dir(root_path.join("pkg_b")).unwrap();
        let package_b_cargo = r#"[package]
name = "pkg_b"
version = "0.1.0"

[dependencies]
pkg_c = { path = "../pkg_c", optional = true }
serde = { workspace = true }

[features]
fail-on-warnings = ["pkg_c?/fail-on-warnings"]
test-feature = []
"#;
        fs::write(root_path.join("pkg_b/Cargo.toml"), package_b_cargo).unwrap();

        // Create pkg_c with fail-on-warnings feature
        fs::create_dir(root_path.join("pkg_c")).unwrap();
        let package_c_cargo = r#"[package]
name = "pkg_c"
version = "0.1.0"

[dependencies]
anyhow = { workspace = true }

[features]
fail-on-warnings = []
other-feature = []
"#;
        fs::write(root_path.join("pkg_c/Cargo.toml"), package_c_cargo).unwrap();

        temp_dir
    }

    /// Helper to create a workspace with errors
    fn create_test_workspace_with_errors() -> TempDir {
        let temp_dir = tempfile::tempdir().unwrap();
        let root_path = temp_dir.path();

        // Create workspace Cargo.toml
        let workspace_cargo = r#"[workspace]
members = ["pkg_error"]

[workspace.dependencies]
anyhow = "1.0"
"#;
        fs::write(root_path.join("Cargo.toml"), workspace_cargo).unwrap();

        // Create pkg_error with missing and incorrect feature propagations
        fs::create_dir(root_path.join("pkg_error")).unwrap();
        let pkg_error_cargo = r#"[package]
name = "pkg_error"
version = "0.1.0"

[dependencies]
anyhow = { workspace = true }
external_dep = "1.0"

[features]
# Missing propagation to anyhow
fail-on-warnings = ["external_dep/nonexistent-feature"]
# Has feature but anyhow doesn't
test-feature = []
"#;
        fs::write(root_path.join("pkg_error/Cargo.toml"), pkg_error_cargo).unwrap();

        temp_dir
    }

    #[test]
    fn test_find_workspace_root_valid() {
        let temp_workspace = create_test_workspace();
        let root_path = temp_workspace.path().to_path_buf();

        let found_root = find_workspace_root(Some(root_path.clone())).unwrap();
        assert_eq!(found_root, root_path);
    }

    #[test]
    fn test_find_workspace_root_from_subdirectory() {
        let temp_workspace = create_test_workspace();
        let root_path = temp_workspace.path().to_path_buf();
        let subdir = root_path.join("pkg_a");

        let found_root = find_workspace_root(Some(subdir)).unwrap();
        assert_eq!(found_root, root_path);
    }

    #[test]
    fn test_find_workspace_root_no_workspace() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root_path = temp_dir.path();

        // Create a single package Cargo.toml without workspace
        let cargo_toml = r#"[package]
name = "single_package"
version = "0.1.0"
"#;
        fs::write(root_path.join("Cargo.toml"), cargo_toml).unwrap();

        let found_root = find_workspace_root(Some(root_path.to_path_buf())).unwrap();
        assert_eq!(found_root, root_path);
    }

    #[test]
    fn test_load_workspace_data() {
        let temp_workspace = create_test_workspace();
        let root_path = temp_workspace.path();

        let (workspace_packages, package_paths, package_cargo_values) =
            load_workspace_data(root_path).unwrap();

        // Check workspace packages
        assert_eq!(workspace_packages.len(), 3);
        assert!(workspace_packages.contains("pkg_a"));
        assert!(workspace_packages.contains("pkg_b"));
        assert!(workspace_packages.contains("pkg_c"));

        // Check package paths
        assert_eq!(package_paths.len(), 3);
        assert_eq!(package_paths.get("pkg_a").unwrap(), "pkg_a");

        // Check cargo values
        assert_eq!(package_cargo_values.len(), 3);
        let package_a_cargo = package_cargo_values.get("pkg_a").unwrap();
        assert_eq!(
            package_a_cargo
                .get("package")
                .unwrap()
                .get("name")
                .unwrap()
                .as_str()
                .unwrap(),
            "pkg_a"
        );
    }

    #[test]
    fn test_extract_all_dependencies() {
        let cargo_toml = r#"[package]
name = "test_pkg"
version = "0.1.0"

[dependencies]
regular_dep = "1.0"
optional_dep = { version = "1.0", optional = true }

[build-dependencies]
build_dep = "1.0"

[dev-dependencies]
dev_dep = "1.0"
"#;
        let value: Value = toml::from_str(cargo_toml).unwrap();

        // Without dev dependencies
        let deps = extract_all_dependencies(&value, false);
        assert_eq!(deps.len(), 3);

        let deps_map: BTreeMap<String, bool> = deps.into_iter().collect();
        assert_eq!(deps_map.get("regular_dep"), Some(&false));
        assert_eq!(deps_map.get("optional_dep"), Some(&true));
        assert_eq!(deps_map.get("build_dep"), Some(&false));
        assert!(!deps_map.contains_key("dev_dep"));

        // With dev dependencies
        let deps_with_dev = extract_all_dependencies(&value, true);
        assert_eq!(deps_with_dev.len(), 4);

        let deps_with_dev_map: BTreeMap<String, bool> = deps_with_dev.into_iter().collect();
        assert!(deps_with_dev_map.contains_key("dev_dep"));
    }

    #[test]
    fn test_parse_feature_propagations() {
        let feature_def = vec![
            Value::String("dep1/feature1".to_string()),
            Value::String("dep2?/feature2".to_string()),
            Value::String("standalone_feature".to_string()),
            Value::String("dep3/feature3".to_string()),
        ];

        let propagations = parse_feature_propagations(&feature_def);
        assert_eq!(propagations.len(), 3);
        assert!(propagations.contains("dep1/feature1"));
        assert!(propagations.contains("dep2?/feature2"));
        assert!(propagations.contains("dep3/feature3"));
        assert!(!propagations.contains("standalone_feature"));
    }

    #[test]
    fn test_extract_dependency_name() {
        assert_eq!(
            extract_dependency_name("dep1/feature1"),
            Some("dep1".to_string())
        );
        assert_eq!(
            extract_dependency_name("dep2?/feature2"),
            Some("dep2".to_string())
        );
        assert_eq!(
            extract_dependency_name("dep3/feature3/extra"),
            Some("dep3".to_string())
        );
        assert_eq!(extract_dependency_name("standalone"), None);
    }

    #[test]
    fn test_validator_creation() {
        let temp_workspace = create_test_workspace();
        let root_path = temp_workspace.path().to_path_buf();

        let config = ValidatorConfig {
            features: None,
            skip_features: None,
            workspace_only: true,
            output_format: OutputType::Raw,
            strict_optional_propagation: false,
        };

        let validator = FeatureValidator::new(Some(root_path), config).unwrap();
        assert_eq!(validator.workspace_packages.len(), 3);
        assert_eq!(validator.package_cargo_values.len(), 3);
    }

    #[test]
    fn test_validator_validation_success() {
        let temp_workspace = create_test_workspace();
        let root_path = temp_workspace.path().to_path_buf();

        let config = ValidatorConfig {
            features: Some(vec!["fail-on-warnings".to_string()]),
            skip_features: None,
            workspace_only: true,
            output_format: OutputType::Raw,
            strict_optional_propagation: false,
        };

        let validator = FeatureValidator::new(Some(root_path), config).unwrap();
        let result = validator.validate().unwrap();

        // Should have no errors for this valid workspace
        assert_eq!(result.errors.len(), 0);
        assert!(result.valid_packages > 0);
    }

    #[test]
    fn test_validator_validation_with_errors() {
        let temp_workspace = create_test_workspace_with_errors();
        let root_path = temp_workspace.path().to_path_buf();

        let config = ValidatorConfig {
            features: Some(vec!["fail-on-warnings".to_string()]),
            skip_features: None,
            workspace_only: false, // Include external deps to catch the error
            output_format: OutputType::Raw,
            strict_optional_propagation: false,
        };

        let validator = FeatureValidator::new(Some(root_path), config).unwrap();
        let result = validator.validate().unwrap();

        // Should have errors for the incorrect workspace
        assert!(!result.errors.is_empty());

        let pkg_error = result
            .errors
            .iter()
            .find(|e| e.package == "pkg_error")
            .expect("Should find pkg_error");

        assert!(!pkg_error.errors.is_empty());

        // Check for incorrect propagation
        let fail_on_warnings_error = pkg_error
            .errors
            .iter()
            .find(|e| e.feature == "fail-on-warnings")
            .expect("Should find fail-on-warnings error");

        assert!(!fail_on_warnings_error.incorrect_propagations.is_empty());
    }

    #[test]
    #[allow(clippy::similar_names)]
    fn test_get_features_to_check_specific() {
        let temp_workspace = create_test_workspace();
        let root_path = temp_workspace.path().to_path_buf();

        let config = ValidatorConfig {
            features: Some(vec!["fail-on-warnings".to_string()]),
            skip_features: None,
            workspace_only: true,

            output_format: OutputType::Raw,
            strict_optional_propagation: false,
        };

        let validator = FeatureValidator::new(Some(root_path), config).unwrap();
        let package_a_cargo = validator.package_cargo_values.get("pkg_a").unwrap();

        let features = validator.get_features_to_check("pkg_a", package_a_cargo);
        assert_eq!(features.len(), 1);
        assert_eq!(features[0], "fail-on-warnings");
    }

    #[test]
    #[allow(clippy::similar_names)]
    fn test_get_features_to_check_auto_detect() {
        let temp_workspace = create_test_workspace();
        let root_path = temp_workspace.path().to_path_buf();

        let config = ValidatorConfig {
            features: None, // Auto-detect
            skip_features: None,
            workspace_only: true,

            output_format: OutputType::Raw,
            strict_optional_propagation: false,
        };

        let validator = FeatureValidator::new(Some(root_path), config).unwrap();
        let package_a_cargo = validator.package_cargo_values.get("pkg_a").unwrap();

        let features = validator.get_features_to_check("pkg_a", package_a_cargo);
        assert!(!features.is_empty());
        assert!(features.contains(&"fail-on-warnings".to_string()));
        assert!(features.contains(&"test-feature".to_string()));
    }

    #[test]
    fn test_dependency_has_feature() {
        let temp_workspace = create_test_workspace();
        let root_path = temp_workspace.path().to_path_buf();

        let config = ValidatorConfig {
            features: None,
            skip_features: None,
            workspace_only: true,

            output_format: OutputType::Raw,
            strict_optional_propagation: false,
        };

        let validator = FeatureValidator::new(Some(root_path), config).unwrap();

        // pkg_b should have fail-on-warnings
        assert!(validator.dependency_has_feature("pkg_b", "fail-on-warnings"));
        assert!(validator.dependency_has_feature("pkg_b", "test-feature"));
        assert!(!validator.dependency_has_feature("pkg_b", "nonexistent-feature"));

        // pkg_c should have fail-on-warnings and other-feature
        assert!(validator.dependency_has_feature("pkg_c", "fail-on-warnings"));
        assert!(validator.dependency_has_feature("pkg_c", "other-feature"));
        assert!(!validator.dependency_has_feature("pkg_c", "nonexistent-feature"));
    }

    #[test]
    #[allow(clippy::similar_names)]
    fn test_any_dependency_has_feature() {
        let temp_workspace = create_test_workspace();
        let root_path = temp_workspace.path().to_path_buf();

        let config = ValidatorConfig {
            features: None,
            skip_features: None,
            workspace_only: true,

            output_format: OutputType::Raw,
            strict_optional_propagation: false,
        };

        let validator = FeatureValidator::new(Some(root_path), config).unwrap();
        let package_a_cargo = validator.package_cargo_values.get("pkg_a").unwrap();

        // pkg_a depends on pkg_b, which has fail-on-warnings
        assert!(validator.any_dependency_has_feature(package_a_cargo, "fail-on-warnings"));
        assert!(validator.any_dependency_has_feature(package_a_cargo, "test-feature"));
        assert!(!validator.any_dependency_has_feature(package_a_cargo, "nonexistent-feature"));
    }

    #[test]
    #[allow(clippy::similar_names)]
    fn test_get_expected_propagations() {
        let temp_workspace = create_test_workspace();
        let root_path = temp_workspace.path().to_path_buf();

        let config = ValidatorConfig {
            features: None,
            skip_features: None,
            workspace_only: true,

            output_format: OutputType::Raw,
            strict_optional_propagation: false,
        };

        let validator = FeatureValidator::new(Some(root_path), config).unwrap();
        let package_a_cargo = validator.package_cargo_values.get("pkg_a").unwrap();

        let expected = validator.get_expected_propagations(package_a_cargo, "fail-on-warnings");

        // pkg_a should expect pkg_b/fail-on-warnings (regular dependency)
        assert_eq!(
            expected.get("pkg_b"),
            Some(&"pkg_b/fail-on-warnings".to_string())
        );

        let package_b_cargo = validator.package_cargo_values.get("pkg_b").unwrap();
        let expected_b = validator.get_expected_propagations(package_b_cargo, "fail-on-warnings");

        // pkg_b should expect pkg_c?/fail-on-warnings (optional dependency)
        assert_eq!(
            expected_b.get("pkg_c"),
            Some(&"pkg_c?/fail-on-warnings".to_string())
        );
    }

    #[test]
    fn test_validation_result_serialization() {
        let result = ValidationResult {
            total_packages: 3,
            valid_packages: 2,
            errors: vec![PackageValidationError {
                package: "test_pkg".to_string(),
                errors: vec![FeatureError {
                    feature: "test-feature".to_string(),
                    missing_propagations: vec![MissingPropagation {
                        dependency: "dep1".to_string(),
                        expected: "dep1/test-feature".to_string(),
                        reason: "Test reason".to_string(),
                    }],
                    incorrect_propagations: vec![IncorrectPropagation {
                        entry: "nonexistent/feature".to_string(),
                        reason: "Test incorrect reason".to_string(),
                    }],
                }],
            }],
            warnings: vec![PackageValidationWarning {
                package: "warn_pkg".to_string(),
                message: "Test warning".to_string(),
            }],
        };

        // Should be able to serialize to JSON
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("test_pkg"));
        assert!(json.contains("test-feature"));
        assert!(json.contains("warn_pkg"));
    }

    /// Helper to create a test workspace with default feature
    fn create_test_workspace_with_default_feature() -> TempDir {
        let temp_dir = tempfile::tempdir().unwrap();
        let root_path = temp_dir.path();

        let workspace_cargo = r#"[workspace]
members = ["test_pkg"]
"#;
        fs::write(root_path.join("Cargo.toml"), workspace_cargo).unwrap();

        fs::create_dir(root_path.join("test_pkg")).unwrap();
        fs::create_dir(root_path.join("test_pkg/src")).unwrap();
        fs::write(root_path.join("test_pkg/src/lib.rs"), "").unwrap();

        let pkg_cargo = r#"[package]
name = "test_pkg"
version = "0.1.0"

[features]
default = []
fail-on-warnings = []
"#;
        fs::write(root_path.join("test_pkg/Cargo.toml"), pkg_cargo).unwrap();

        temp_dir
    }

    /// Helper to create a test workspace with multiple features
    fn create_test_workspace_with_multiple_features() -> TempDir {
        let temp_dir = tempfile::tempdir().unwrap();
        let root_path = temp_dir.path();

        let workspace_cargo = r#"[workspace]
members = ["test_pkg"]
"#;
        fs::write(root_path.join("Cargo.toml"), workspace_cargo).unwrap();

        fs::create_dir(root_path.join("test_pkg")).unwrap();
        fs::create_dir(root_path.join("test_pkg/src")).unwrap();
        fs::write(root_path.join("test_pkg/src/lib.rs"), "").unwrap();

        let pkg_cargo = r#"[package]
name = "test_pkg"
version = "0.1.0"

[features]
default = []
fail-on-warnings = []
test-utils = []
"#;
        fs::write(root_path.join("test_pkg/Cargo.toml"), pkg_cargo).unwrap();

        temp_dir
    }

    /// Helper to create a test workspace with underscore features
    fn create_test_workspace_with_underscore_features() -> TempDir {
        let temp_dir = tempfile::tempdir().unwrap();
        let root_path = temp_dir.path();

        let workspace_cargo = r#"[workspace]
members = ["test_pkg"]
"#;
        fs::write(root_path.join("Cargo.toml"), workspace_cargo).unwrap();

        fs::create_dir(root_path.join("test_pkg")).unwrap();
        fs::create_dir(root_path.join("test_pkg/src")).unwrap();
        fs::write(root_path.join("test_pkg/src/lib.rs"), "").unwrap();

        let pkg_cargo = r#"[package]
name = "test_pkg"
version = "0.1.0"

[features]
default = []
fail-on-warnings = []
"_internal" = []
"_private" = []
"_debug" = []
"#;
        fs::write(root_path.join("test_pkg/Cargo.toml"), pkg_cargo).unwrap();

        temp_dir
    }

    #[test]
    fn test_skip_features_default_behavior() {
        // When skip_features is None, should default to skipping "default" and "_*"
        let temp_workspace = create_test_workspace_with_underscore_features();
        let config = ValidatorConfig {
            features: Some(vec![
                "default".to_string(),
                "fail-on-warnings".to_string(),
                "_internal".to_string(),
            ]),
            skip_features: None, // Should default to vec!["default", "_*"]
            workspace_only: true,
            output_format: OutputType::Raw,
            strict_optional_propagation: false,
        };

        let validator =
            FeatureValidator::new(Some(temp_workspace.path().to_path_buf()), config).unwrap();

        // Should skip both "default" and "_internal" (underscore features)
        let pkg_cargo = validator.package_cargo_values.get("test_pkg").unwrap();
        let features = validator.get_features_to_check("test_pkg", pkg_cargo);

        assert!(!features.contains(&"default".to_string()));
        assert!(!features.contains(&"_internal".to_string()));
        assert!(features.contains(&"fail-on-warnings".to_string()));
    }

    #[test]
    fn test_skip_features_empty_validates_all() {
        // When skip_features is Some(vec![]), should validate ALL features including default
        let temp_workspace = create_test_workspace_with_default_feature();
        let config = ValidatorConfig {
            features: Some(vec!["default".to_string(), "fail-on-warnings".to_string()]),
            skip_features: Some(vec![]),
            workspace_only: true,
            output_format: OutputType::Raw,
            strict_optional_propagation: false,
        };

        let validator =
            FeatureValidator::new(Some(temp_workspace.path().to_path_buf()), config).unwrap();

        let pkg_cargo = validator.package_cargo_values.get("test_pkg").unwrap();
        let features = validator.get_features_to_check("test_pkg", pkg_cargo);

        assert!(features.contains(&"default".to_string()));
        assert!(features.contains(&"fail-on-warnings".to_string()));
    }

    #[test]
    fn test_skip_features_explicit_list() {
        // Should skip only specified features
        let temp_workspace = create_test_workspace_with_multiple_features();
        let config = ValidatorConfig {
            features: Some(vec![
                "default".to_string(),
                "test-utils".to_string(),
                "fail-on-warnings".to_string(),
            ]),
            skip_features: Some(vec!["default".to_string(), "test-utils".to_string()]),
            workspace_only: true,
            output_format: OutputType::Raw,
            strict_optional_propagation: false,
        };

        let validator =
            FeatureValidator::new(Some(temp_workspace.path().to_path_buf()), config).unwrap();

        let pkg_cargo = validator.package_cargo_values.get("test_pkg").unwrap();
        let features = validator.get_features_to_check("test_pkg", pkg_cargo);

        assert!(!features.contains(&"default".to_string()));
        assert!(!features.contains(&"test-utils".to_string()));
        assert!(features.contains(&"fail-on-warnings".to_string()));
    }

    #[test]
    fn test_skip_features_with_specific_features_list() {
        // skip_features should filter the specific features list too
        let temp_workspace = create_test_workspace_with_default_feature();
        let config = ValidatorConfig {
            features: Some(vec!["default".to_string(), "fail-on-warnings".to_string()]),
            skip_features: Some(vec!["default".to_string()]),
            workspace_only: true,
            output_format: OutputType::Raw,
            strict_optional_propagation: false,
        };

        let validator =
            FeatureValidator::new(Some(temp_workspace.path().to_path_buf()), config).unwrap();

        let pkg_cargo = validator.package_cargo_values.get("test_pkg").unwrap();
        let features = validator.get_features_to_check("test_pkg", pkg_cargo);

        // Should only get fail-on-warnings, not default (even though both were in features list)
        assert_eq!(features.len(), 1);
        assert_eq!(features[0], "fail-on-warnings");
    }

    #[test]
    fn test_underscore_features_skipped_by_default() {
        // All underscore features should be skipped by default
        let temp_workspace = create_test_workspace_with_underscore_features();
        let config = ValidatorConfig {
            features: Some(vec![
                "_internal".to_string(),
                "_private".to_string(),
                "_debug".to_string(),
                "fail-on-warnings".to_string(),
            ]),
            skip_features: None, // Uses default: ["default", "_*"]
            workspace_only: true,
            output_format: OutputType::Raw,
            strict_optional_propagation: false,
        };

        let validator =
            FeatureValidator::new(Some(temp_workspace.path().to_path_buf()), config).unwrap();

        let pkg_cargo = validator.package_cargo_values.get("test_pkg").unwrap();
        let features = validator.get_features_to_check("test_pkg", pkg_cargo);

        // All underscore features should be skipped by the "_*" pattern
        assert!(!features.contains(&"_internal".to_string()));
        assert!(!features.contains(&"_private".to_string()));
        assert!(!features.contains(&"_debug".to_string()));
        // Public feature should NOT be skipped
        assert!(features.contains(&"fail-on-warnings".to_string()));
    }

    #[test]
    fn test_skip_features_glob_patterns() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root_path = temp_dir.path();

        let workspace_cargo = r#"[workspace]
members = ["test_pkg"]
"#;
        fs::write(root_path.join("Cargo.toml"), workspace_cargo).unwrap();

        fs::create_dir(root_path.join("test_pkg")).unwrap();
        fs::create_dir(root_path.join("test_pkg/src")).unwrap();
        fs::write(root_path.join("test_pkg/src/lib.rs"), "").unwrap();

        let pkg_cargo = r#"[package]
name = "test_pkg"
version = "0.1.0"

[features]
default = []
test-utils = []
test-fixtures = []
fail-on-warnings = []
"#;
        fs::write(root_path.join("test_pkg/Cargo.toml"), pkg_cargo).unwrap();

        // Skip all test-* features using glob pattern
        let config = ValidatorConfig {
            features: Some(vec![
                "default".to_string(),
                "test-utils".to_string(),
                "test-fixtures".to_string(),
                "fail-on-warnings".to_string(),
            ]),
            skip_features: Some(vec!["test-*".to_string()]),
            workspace_only: true,
            output_format: OutputType::Raw,
            strict_optional_propagation: false,
        };

        let validator = FeatureValidator::new(Some(root_path.to_path_buf()), config).unwrap();

        let pkg_cargo = validator.package_cargo_values.get("test_pkg").unwrap();
        let features = validator.get_features_to_check("test_pkg", pkg_cargo);

        // Should skip test-utils and test-fixtures, but keep default and fail-on-warnings
        assert!(!features.contains(&"test-utils".to_string()));
        assert!(!features.contains(&"test-fixtures".to_string()));
        assert!(features.contains(&"default".to_string()));
        assert!(features.contains(&"fail-on-warnings".to_string()));
    }

    #[test]
    fn test_lenient_optional_propagation_accepts_both_styles() {
        // Lenient mode (default) should accept both dep?/feature and dep/feature for optional deps
        let temp_dir = tempfile::tempdir().unwrap();
        let root_path = temp_dir.path();

        let workspace_cargo = r#"[workspace]
members = ["pkg_a", "pkg_b"]
"#;
        fs::write(root_path.join("Cargo.toml"), workspace_cargo).unwrap();

        // Create pkg_b (dependency)
        fs::create_dir(root_path.join("pkg_b")).unwrap();
        fs::create_dir(root_path.join("pkg_b/src")).unwrap();
        fs::write(root_path.join("pkg_b/src/lib.rs"), "").unwrap();
        let pkg_b_cargo = r#"[package]
name = "pkg_b"
version = "0.1.0"

[features]
test-feature = []
"#;
        fs::write(root_path.join("pkg_b/Cargo.toml"), pkg_b_cargo).unwrap();

        // Create pkg_a (depends on pkg_b)
        fs::create_dir(root_path.join("pkg_a")).unwrap();
        fs::create_dir(root_path.join("pkg_a/src")).unwrap();
        fs::write(root_path.join("pkg_a/src/lib.rs"), "").unwrap();

        // Package with optional dependency using dep/feature syntax (without ?)
        let pkg_a_cargo = r#"[package]
name = "pkg_a"
version = "0.1.0"

[dependencies]
pkg_b = { path = "../pkg_b", optional = true }

[features]
test-feature = ["pkg_b/test-feature"]
"#;
        fs::write(root_path.join("pkg_a/Cargo.toml"), pkg_a_cargo).unwrap();

        let config = ValidatorConfig {
            features: Some(vec!["test-feature".to_string()]),
            skip_features: None,
            workspace_only: true, // Workspace only
            output_format: OutputType::Raw,
            strict_optional_propagation: false, // Lenient mode
        };

        let validator = FeatureValidator::new(Some(root_path.to_path_buf()), config).unwrap();
        let result = validator.validate().unwrap();

        // In lenient mode, dep/feature should be accepted for optional deps
        assert_eq!(
            result.errors.len(),
            0,
            "Lenient mode should accept dep/feature syntax"
        );
    }

    #[test]
    fn test_strict_optional_propagation_requires_question_mark() {
        // Strict mode should only accept dep?/feature for optional deps
        let temp_dir = tempfile::tempdir().unwrap();
        let root_path = temp_dir.path();

        let workspace_cargo = r#"[workspace]
members = ["pkg_a", "pkg_b"]
"#;
        fs::write(root_path.join("Cargo.toml"), workspace_cargo).unwrap();

        // Create pkg_b (dependency)
        fs::create_dir(root_path.join("pkg_b")).unwrap();
        fs::create_dir(root_path.join("pkg_b/src")).unwrap();
        fs::write(root_path.join("pkg_b/src/lib.rs"), "").unwrap();
        let pkg_b_cargo = r#"[package]
name = "pkg_b"
version = "0.1.0"

[features]
test-feature = []
"#;
        fs::write(root_path.join("pkg_b/Cargo.toml"), pkg_b_cargo).unwrap();

        // Create pkg_a (depends on pkg_b)
        fs::create_dir(root_path.join("pkg_a")).unwrap();
        fs::create_dir(root_path.join("pkg_a/src")).unwrap();
        fs::write(root_path.join("pkg_a/src/lib.rs"), "").unwrap();

        // Package with optional dependency using dep/feature syntax (without ?)
        let pkg_a_cargo = r#"[package]
name = "pkg_a"
version = "0.1.0"

[dependencies]
pkg_b = { path = "../pkg_b", optional = true }

[features]
test-feature = ["pkg_b/test-feature"]
"#;
        fs::write(root_path.join("pkg_a/Cargo.toml"), pkg_a_cargo).unwrap();

        let config = ValidatorConfig {
            features: Some(vec!["test-feature".to_string()]),
            skip_features: None,
            workspace_only: true, // Workspace only
            output_format: OutputType::Raw,
            strict_optional_propagation: true, // Strict mode
        };

        let validator = FeatureValidator::new(Some(root_path.to_path_buf()), config).unwrap();
        let result = validator.validate().unwrap();

        // In strict mode, dep/feature should NOT be accepted for optional deps
        assert!(
            !result.errors.is_empty(),
            "Strict mode should reject dep/feature syntax"
        );
        assert_eq!(result.errors[0].package, "pkg_a");
        assert!(!result.errors[0].errors[0].missing_propagations.is_empty());
    }

    #[test]
    fn test_strict_mode_accepts_question_mark_syntax() {
        // Strict mode should accept dep?/feature for optional deps
        let temp_dir = tempfile::tempdir().unwrap();
        let root_path = temp_dir.path();

        let workspace_cargo = r#"[workspace]
members = ["pkg_a", "pkg_b"]
"#;
        fs::write(root_path.join("Cargo.toml"), workspace_cargo).unwrap();

        // Create pkg_b (dependency)
        fs::create_dir(root_path.join("pkg_b")).unwrap();
        fs::create_dir(root_path.join("pkg_b/src")).unwrap();
        fs::write(root_path.join("pkg_b/src/lib.rs"), "").unwrap();
        let pkg_b_cargo = r#"[package]
name = "pkg_b"
version = "0.1.0"

[features]
test-feature = []
"#;
        fs::write(root_path.join("pkg_b/Cargo.toml"), pkg_b_cargo).unwrap();

        // Create pkg_a (depends on pkg_b)
        fs::create_dir(root_path.join("pkg_a")).unwrap();
        fs::create_dir(root_path.join("pkg_a/src")).unwrap();
        fs::write(root_path.join("pkg_a/src/lib.rs"), "").unwrap();

        // Package with optional dependency using dep?/feature syntax (with ?)
        let pkg_a_cargo = r#"[package]
name = "pkg_a"
version = "0.1.0"

[dependencies]
pkg_b = { path = "../pkg_b", optional = true }

[features]
test-feature = ["dep:pkg_b", "pkg_b?/test-feature"]
"#;
        fs::write(root_path.join("pkg_a/Cargo.toml"), pkg_a_cargo).unwrap();

        let config = ValidatorConfig {
            features: Some(vec!["test-feature".to_string()]),
            skip_features: None,
            workspace_only: true, // Workspace only
            output_format: OutputType::Raw,
            strict_optional_propagation: true, // Strict mode
        };

        let validator = FeatureValidator::new(Some(root_path.to_path_buf()), config).unwrap();
        let result = validator.validate().unwrap();

        // In strict mode, dep?/feature should be accepted
        assert_eq!(
            result.errors.len(),
            0,
            "Strict mode should accept dep?/feature syntax"
        );
    }
}
