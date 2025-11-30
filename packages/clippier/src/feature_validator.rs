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
//!     output_format: OutputType::Json,
//!     ..Default::default()
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
use chrono;
use serde::{Deserialize, Serialize};
use toml::Value;

use crate::{OutputType, matches_pattern, should_skip_feature};

/// Type aliases for complex types
type WorkspacePackages = BTreeSet<String>;
type PackagePaths = BTreeMap<String, String>;
type PackageCargoValues = BTreeMap<String, Value>;
type WorkspaceData = (WorkspacePackages, PackagePaths, PackageCargoValues);

/// Default features to skip during validation.
///
/// These patterns are used when no explicit skip-features are configured:
/// - `"default"` - The default feature is typically a convenience aggregate
/// - `"_*"` - Features starting with underscore are conventionally internal/private
pub const DEFAULT_SKIP_FEATURES: &[&str] = &["default", "_*"];

/// Resolve skip features from optional configuration.
///
/// - `None` → use [`DEFAULT_SKIP_FEATURES`]
/// - `Some(empty vec)` → skip nothing (validate all features)
/// - `Some(vec with patterns)` → use the provided patterns
#[must_use]
pub fn resolve_skip_features(configured: &Option<Vec<String>>) -> Vec<String> {
    configured.as_ref().map_or_else(
        || {
            DEFAULT_SKIP_FEATURES
                .iter()
                .map(|s| (*s).to_string())
                .collect()
        },
        Clone::clone,
    )
}

/// Source of an override (for precedence tracking)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverrideSource {
    /// CLI arguments (highest priority)
    Cli = 0,
    /// Package-level clippier.toml
    PackageClippierToml = 1,
    /// Package-level Cargo.toml metadata
    CargoTomlMetadata = 2,
    /// Workspace-level clippier.toml (lowest priority)
    WorkspaceClippierToml = 3,
}

/// Type of override to apply
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverrideType {
    /// Allow a specific missing propagation
    AllowMissing,
    /// Allow a specific incorrect propagation
    AllowIncorrect,
    /// Suppress all validation for matching cases
    Suppress,
}

/// A validation override with its source
#[derive(Debug, Clone)]
pub struct ValidationOverride {
    /// Feature name (supports wildcards)
    pub feature: String,
    /// Dependency name (supports wildcards)
    pub dependency: String,
    /// Optional package name filter (None = applies to all packages)
    pub package: Option<String>,
    /// Type of override
    pub override_type: OverrideType,
    /// Human-readable reason for the override
    pub reason: Option<String>,
    /// Optional expiration date (RFC 3339 format)
    pub expires: Option<String>,
    /// Source of this override (for precedence)
    pub source: OverrideSource,
}

/// Helper type to support both single string and array of strings in TOML
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum StringOrArray {
    /// Single dependency
    Single(String),
    /// Multiple dependencies
    Multiple(Vec<String>),
}

impl StringOrArray {
    /// Convert to a vector of strings (always returns a vec for uniform processing)
    #[must_use]
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            Self::Single(s) => vec![s.clone()],
            Self::Multiple(v) => v.clone(),
        }
    }
}

/// Configuration entry for overrides (from TOML files)
#[derive(Debug, Clone, Deserialize)]
pub struct OverrideConfigEntry {
    /// Feature name (supports wildcards)
    pub feature: String,
    /// Dependency name or names (supports wildcards)
    /// Can be a single string or an array of strings
    ///
    /// Examples:
    /// - Single: `dependency = "some_dep"`
    /// - Array: `dependencies = ["dep1", "dep2", "dep3"]`
    /// - Alias: Both `dependency` and `dependencies` are accepted
    #[serde(alias = "dependencies")]
    pub dependency: StringOrArray,
    /// Type of override
    #[serde(rename = "type")]
    pub override_type: OverrideType,
    /// Human-readable reason for the override
    pub reason: String,
    /// Optional expiration date (RFC 3339 format)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
}

/// Feature validation configuration section (for clippier.toml and Cargo.toml)
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct FeatureValidationConfig {
    /// List of validation overrides
    #[serde(default, rename = "override")]
    pub overrides: Vec<OverrideConfigEntry>,

    /// Parent package config (package-level: declares this package as a parent)
    #[serde(default)]
    pub parent: Option<ParentPackageConfig>,

    /// Parent packages list (workspace-level: declares which packages are parents)
    #[serde(default)]
    pub parent_packages: Vec<WorkspaceParentPackage>,

    /// Global prefix overrides (workspace-level)
    #[serde(default)]
    pub parent_prefix: Vec<PrefixOverride>,
}

/// Package-level parent config (from package clippier.toml)
///
/// Declares this package as a "parent" package that re-exports features
/// from its workspace dependencies.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ParentPackageConfig {
    /// Enable parent validation for this package
    #[serde(default)]
    pub enabled: bool,

    /// Maximum depth for nested dependency checking
    /// None = unlimited (follow full dependency chain)
    /// Some(n) = limit to n levels
    #[serde(default)]
    pub depth: Option<u8>,

    /// Features to skip when checking if they're exposed
    /// If None, defaults to `["default", "_*"]`
    #[serde(default)]
    pub skip_features: Option<Vec<String>>,

    /// Prefix overrides for specific dependencies
    #[serde(default)]
    pub prefix: Vec<PrefixOverride>,
}

/// Workspace-level parent package entry
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct WorkspaceParentPackage {
    /// Package name
    pub package: String,

    /// Maximum depth for nested dependency checking
    /// None = unlimited (follow full dependency chain)
    #[serde(default)]
    pub depth: Option<u8>,

    /// Features to skip when checking if they're exposed
    /// If None, defaults to `["default", "_*"]`
    #[serde(default)]
    pub skip_features: Option<Vec<String>>,
}

/// Prefix override entry for parent package validation
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PrefixOverride {
    /// Dependency package name
    pub dependency: String,
    /// Prefix to use for features from this dependency
    pub prefix: String,
}

/// Result of parent package validation
#[derive(Debug, Serialize)]
pub struct ParentValidationResult {
    /// Parent package name
    pub package: String,
    /// Missing feature exposures
    pub missing_exposures: Vec<MissingFeatureExposure>,
    /// Total features checked across all dependencies
    pub features_checked: usize,
    /// Features correctly exposed
    pub features_exposed: usize,
}

/// Error for missing feature exposure in parent package
#[derive(Debug, Serialize)]
pub struct MissingFeatureExposure {
    /// The parent package
    pub parent_package: String,
    /// The dependency whose feature isn't exposed
    pub dependency: String,
    /// The feature in the dependency that isn't exposed
    pub dependency_feature: String,
    /// Expected feature name in parent (e.g., `database-api`)
    pub expected_parent_feature: String,
    /// Expected propagation entry (e.g., `switchy_database?/api`)
    pub expected_propagation: String,
    /// Depth in dependency chain (1 = direct dep, 2 = dep of dep, etc.)
    pub depth: u8,
    /// Chain from parent to dependency (for depth > 1)
    pub chain: Vec<String>,
}

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
    /// Errors that were overridden and suppressed
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub overridden_errors: Vec<OverriddenError>,
    /// Summary of applied overrides
    #[serde(skip_serializing_if = "Option::is_none")]
    pub override_summary: Option<OverrideSummary>,
    /// Parent package validation results
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parent_results: Vec<ParentValidationResult>,
}

/// An error that was overridden by configuration
#[derive(Debug, Serialize)]
pub struct OverriddenError {
    /// Package name
    pub package: String,
    /// Feature name
    pub feature: String,
    /// Dependency name
    pub dependency: String,
    /// Expected propagation
    pub expected: String,
    /// Original error reason
    pub original_reason: String,
    /// Override information
    pub override_info: OverrideInfo,
}

/// Information about an applied override
#[derive(Debug, Serialize)]
pub struct OverrideInfo {
    /// Type of override
    #[serde(rename = "type")]
    pub override_type: OverrideType,
    /// Reason for the override
    pub reason: Option<String>,
    /// Source of the override
    pub source: OverrideSource,
    /// Expiration date if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
}

/// Summary of overrides applied during validation
#[derive(Debug, Serialize)]
pub struct OverrideSummary {
    /// Total number of overrides applied
    pub total_applied: usize,
    /// Breakdown by source
    pub by_source: BTreeMap<String, usize>,
    /// Breakdown by type
    pub by_type: BTreeMap<String, usize>,
    /// Number of expired overrides
    pub expired: usize,
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
#[derive(Debug, Serialize, Clone)]
pub struct MissingPropagation {
    /// Name of the dependency that should have the feature propagated
    pub dependency: String,
    /// Expected feature propagation entry (e.g., "dep:feature-name")
    pub expected: String,
    /// Explanation of why this propagation is required
    pub reason: String,
}

/// An incorrect feature propagation in the package definition
#[derive(Debug, Serialize, Clone)]
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

/// Override behavior options
#[derive(Debug, Clone, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct OverrideOptions {
    /// Load overrides from clippier.toml files
    pub use_config_overrides: bool,
    /// Load overrides from Cargo.toml metadata
    pub use_cargo_metadata_overrides: bool,
    /// Warn about expired overrides
    pub warn_expired: bool,
    /// Fail validation if expired overrides exist
    pub fail_on_expired: bool,
    /// Show verbose override information
    pub verbose_overrides: bool,
}

impl OverrideOptions {
    /// Create override options with all features enabled
    #[must_use]
    pub const fn enabled() -> Self {
        Self {
            use_config_overrides: true,
            use_cargo_metadata_overrides: true,
            warn_expired: true,
            fail_on_expired: false,
            verbose_overrides: false,
        }
    }
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
    /// CLI-provided overrides (highest priority)
    pub cli_overrides: Vec<ValidationOverride>,
    /// Override behavior options
    pub override_options: OverrideOptions,
    /// Packages to ignore entirely (supports wildcards)
    pub ignore_packages: Vec<String>,
    /// Features to ignore globally (supports wildcards)
    pub ignore_features: Vec<String>,
    /// Parent package validation configuration
    pub parent_config: ParentValidationConfig,
}

/// Runtime configuration for parent package validation
#[derive(Debug, Clone)]
pub struct ParentValidationConfig {
    /// Packages to validate as parent packages (from CLI)
    pub cli_packages: Vec<String>,
    /// CLI-specified depth override
    pub cli_depth: Option<u8>,
    /// CLI-specified skip features (added to defaults)
    pub cli_skip_features: Vec<String>,
    /// CLI-specified prefix overrides
    pub cli_prefix_overrides: Vec<PrefixOverride>,
    /// Whether to load parent config from clippier.toml files
    pub use_config: bool,
}

impl Default for ValidatorConfig {
    fn default() -> Self {
        Self {
            features: None,
            skip_features: None,
            workspace_only: true,
            output_format: OutputType::Raw,
            strict_optional_propagation: false,
            cli_overrides: Vec::new(),
            override_options: OverrideOptions::enabled(),
            ignore_packages: Vec::new(),
            ignore_features: Vec::new(),
            parent_config: ParentValidationConfig::default(),
        }
    }
}

impl Default for ParentValidationConfig {
    fn default() -> Self {
        Self {
            cli_packages: Vec::new(),
            cli_depth: None,
            cli_skip_features: Vec::new(),
            cli_prefix_overrides: Vec::new(),
            use_config: true,
        }
    }
}

impl ValidatorConfig {
    /// Create a config with defaults suitable for testing
    ///
    /// Similar to `Default::default()`, but with overrides disabled for predictable test behavior.
    #[must_use]
    pub fn test_default() -> Self {
        Self {
            features: None,
            skip_features: None,
            workspace_only: true,
            output_format: OutputType::Raw,
            strict_optional_propagation: false,
            cli_overrides: Vec::new(),
            override_options: OverrideOptions::default(),
            ignore_packages: Vec::new(),
            ignore_features: Vec::new(),
            parent_config: ParentValidationConfig {
                use_config: false,
                ..ParentValidationConfig::default()
            },
        }
    }
}

/// Main validator struct
pub struct FeatureValidator {
    workspace_packages: BTreeSet<String>,
    package_cargo_values: BTreeMap<String, Value>,
    package_paths: BTreeMap<String, String>,
    workspace_root: PathBuf,
    config: ValidatorConfig,
}

/// Statistics for tracking applied overrides
#[derive(Debug, Default)]
struct OverrideStats {
    total_applied: usize,
    by_source: BTreeMap<String, usize>,
    by_type: BTreeMap<String, usize>,
    expired: usize,
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
        let (workspace_packages, package_paths, package_cargo_values) =
            load_workspace_data(&workspace_root)?;

        Ok(Self {
            workspace_packages,
            package_cargo_values,
            package_paths,
            workspace_root,
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
        let mut overridden_errors = Vec::new();
        let mut valid_count = 0;

        // Collect all overrides from all sources
        let all_overrides = self.collect_all_overrides();

        // Track override statistics
        let mut override_stats = OverrideStats::default();

        // Check for expired overrides
        for override_rule in &all_overrides {
            if let Some(ref expires) = override_rule.expires
                && Self::is_expired(expires)
            {
                override_stats.expired += 1;
                if self.config.override_options.warn_expired {
                    warnings.push(PackageValidationWarning {
                        package: override_rule
                            .package
                            .as_ref()
                            .map_or_else(|| "*".to_string(), Clone::clone),
                        message: format!(
                            "Override expired: {}:{} (expired {})",
                            override_rule.feature, override_rule.dependency, expires
                        ),
                    });
                }
            }
        }

        let packages_to_check: Vec<(&String, &Value)> = if self.config.workspace_only {
            self.package_cargo_values
                .iter()
                .filter(|(name, _)| self.workspace_packages.contains(*name))
                .collect()
        } else {
            self.package_cargo_values.iter().collect()
        };

        for (package_name, cargo_value) in packages_to_check {
            // Check if package should be ignored
            if self.should_ignore_package(package_name) {
                valid_count += 1;
                continue;
            }

            match self.validate_package_with_overrides(
                package_name,
                cargo_value,
                &all_overrides,
                &mut override_stats,
            ) {
                Ok((maybe_error, package_overridden)) => {
                    if let Some(error) = maybe_error {
                        errors.push(error);
                    } else {
                        valid_count += 1;
                    }
                    overridden_errors.extend(package_overridden);
                }
                Err(e) => warnings.push(PackageValidationWarning {
                    package: package_name.clone(),
                    message: format!("Failed to validate: {e}"),
                }),
            }
        }

        let override_summary = if !all_overrides.is_empty() || !overridden_errors.is_empty() {
            Some(OverrideSummary {
                total_applied: override_stats.total_applied,
                by_source: override_stats.by_source,
                by_type: override_stats.by_type,
                expired: override_stats.expired,
            })
        } else {
            None
        };

        // Parent package validation
        let parent_results = self.validate_parent_packages(&mut warnings);

        Ok(ValidationResult {
            total_packages: valid_count + errors.len(),
            valid_packages: valid_count,
            errors,
            warnings,
            overridden_errors,
            override_summary,
            parent_results,
        })
    }

    /// Validate a single package with override support
    fn validate_package_with_overrides(
        &self,
        package_name: &str,
        cargo_value: &Value,
        overrides: &[ValidationOverride],
        stats: &mut OverrideStats,
    ) -> Result<(Option<PackageValidationError>, Vec<OverriddenError>)> {
        let features_to_check = self.get_features_to_check(package_name, cargo_value);

        if features_to_check.is_empty() {
            return Ok((None, Vec::new()));
        }

        let mut feature_errors = Vec::new();
        let mut overridden_errors = Vec::new();

        for feature in features_to_check {
            // Check if feature should be ignored globally
            if self.should_ignore_feature(&feature) {
                continue;
            }

            let (missing, incorrect) =
                self.validate_feature(package_name, &feature, cargo_value)?;

            // Filter missing propagations by overrides
            let (filtered_missing, overridden_missing) = Self::filter_missing_with_overrides(
                package_name,
                &feature,
                missing,
                overrides,
                stats,
            );

            // Filter incorrect propagations by overrides
            let (filtered_incorrect, overridden_incorrect) = Self::filter_incorrect_with_overrides(
                package_name,
                &feature,
                incorrect,
                overrides,
                stats,
            );

            overridden_errors.extend(overridden_missing);
            overridden_errors.extend(overridden_incorrect);

            if !filtered_missing.is_empty() || !filtered_incorrect.is_empty() {
                feature_errors.push(FeatureError {
                    feature: feature.clone(),
                    missing_propagations: filtered_missing,
                    incorrect_propagations: filtered_incorrect,
                });
            }
        }

        let error = if feature_errors.is_empty() {
            None
        } else {
            Some(PackageValidationError {
                package: package_name.to_string(),
                errors: feature_errors,
            })
        };

        Ok((error, overridden_errors))
    }

    /// Get features to check for a package
    fn get_features_to_check(&self, _package_name: &str, cargo_value: &Value) -> Vec<String> {
        let Some(features_table) = cargo_value.get("features").and_then(|f| f.as_table()) else {
            return Vec::new();
        };

        // Build skip list using centralized resolver
        let skip_features_vec = resolve_skip_features(&self.config.skip_features);

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

    /// Collect all overrides from all sources with precedence
    fn collect_all_overrides(&self) -> Vec<ValidationOverride> {
        let mut overrides = Vec::new();

        // 1. Load from workspace clippier.toml (LOWEST priority - base defaults)
        if self.config.override_options.use_config_overrides
            && let Ok(workspace_overrides) = self.load_workspace_clippier_overrides()
        {
            overrides.extend(workspace_overrides);
        }

        // 2. Load from package-level Cargo.toml metadata (overrides workspace)
        if self.config.override_options.use_cargo_metadata_overrides {
            for (package_name, cargo_value) in &self.package_cargo_values {
                let metadata_overrides =
                    Self::load_cargo_metadata_overrides(package_name, cargo_value);
                overrides.extend(metadata_overrides);
            }
        }

        // 3. Load from package-level clippier.toml (overrides Cargo.toml)
        if self.config.override_options.use_config_overrides {
            for package_name in &self.workspace_packages {
                if let Ok(package_overrides) = self.load_package_clippier_overrides(package_name) {
                    overrides.extend(package_overrides);
                }
            }
        }

        // 4. Load from CLI args (HIGHEST priority - overrides everything)
        overrides.extend(self.config.cli_overrides.clone());

        // Sort by source priority (CLI=0 > PackageClippier=1 > CargoToml=2 > WorkspaceClippier=3)
        overrides.sort_by_key(|o| o.source);

        overrides
    }

    /// Load overrides from workspace-level clippier.toml
    fn load_workspace_clippier_overrides(&self) -> Result<Vec<ValidationOverride>> {
        let config_path = self.workspace_root.join("clippier.toml");
        if !config_path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&config_path)?;
        let value: Value = toml::from_str(&content)?;

        let mut overrides = Vec::new();
        if let Some(feature_validation) = value.get("feature-validation")
            && let Some(config) = feature_validation
                .get("override")
                .and_then(|v| v.as_array())
        {
            for entry in config {
                if let Ok(override_entry) = entry.clone().try_into::<OverrideConfigEntry>() {
                    // Expand array dependencies into individual overrides
                    for dependency in override_entry.dependency.to_vec() {
                        overrides.push(ValidationOverride {
                            feature: override_entry.feature.clone(),
                            dependency,
                            package: None,
                            override_type: override_entry.override_type,
                            reason: Some(override_entry.reason.clone()),
                            expires: override_entry.expires.clone(),
                            source: OverrideSource::WorkspaceClippierToml,
                        });
                    }
                }
            }
        }

        Ok(overrides)
    }

    /// Load overrides from package-level clippier.toml
    fn load_package_clippier_overrides(
        &self,
        package_name: &str,
    ) -> Result<Vec<ValidationOverride>> {
        let package_path = self
            .package_paths
            .get(package_name)
            .ok_or_else(|| anyhow!("Package path not found for {package_name}"))?;

        let config_path = self.workspace_root.join(package_path).join("clippier.toml");
        if !config_path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&config_path)?;
        let value: Value = toml::from_str(&content)?;

        let mut overrides = Vec::new();
        if let Some(feature_validation) = value.get("feature-validation")
            && let Some(config) = feature_validation
                .get("override")
                .and_then(|v| v.as_array())
        {
            for entry in config {
                if let Ok(override_entry) = entry.clone().try_into::<OverrideConfigEntry>() {
                    // Expand array dependencies into individual overrides
                    for dependency in override_entry.dependency.to_vec() {
                        overrides.push(ValidationOverride {
                            feature: override_entry.feature.clone(),
                            dependency,
                            package: Some(package_name.to_string()),
                            override_type: override_entry.override_type,
                            reason: Some(override_entry.reason.clone()),
                            expires: override_entry.expires.clone(),
                            source: OverrideSource::PackageClippierToml,
                        });
                    }
                }
            }
        }
        Ok(overrides)
    }

    /// Load overrides from Cargo.toml metadata
    fn load_cargo_metadata_overrides(
        package_name: &str,
        cargo_value: &Value,
    ) -> Vec<ValidationOverride> {
        let mut overrides = Vec::new();

        if let Some(metadata) = cargo_value
            .get("package")
            .and_then(|p| p.get("metadata"))
            .and_then(|m| m.get("clippier"))
            .and_then(|c| c.get("feature-validation"))
            && let Some(config) = metadata.get("override").and_then(|v| v.as_array())
        {
            for entry in config {
                if let Ok(override_entry) = entry.clone().try_into::<OverrideConfigEntry>() {
                    // Expand array dependencies into individual overrides
                    for dependency in override_entry.dependency.to_vec() {
                        overrides.push(ValidationOverride {
                            feature: override_entry.feature.clone(),
                            dependency,
                            package: Some(package_name.to_string()),
                            override_type: override_entry.override_type,
                            reason: Some(override_entry.reason.clone()),
                            expires: override_entry.expires.clone(),
                            source: OverrideSource::CargoTomlMetadata,
                        });
                    }
                }
            }
        }

        overrides
    }

    /// Check if an override matches a specific validation case
    fn matches_override(
        override_rule: &ValidationOverride,
        package: &str,
        feature: &str,
        dependency: &str,
    ) -> bool {
        // Check package filter (if specified)
        if let Some(ref pkg_pattern) = override_rule.package
            && !matches_pattern(package, pkg_pattern)
        {
            return false;
        }

        // Check feature and dependency patterns
        matches_pattern(feature, &override_rule.feature)
            && matches_pattern(dependency, &override_rule.dependency)
    }

    /// Find the applicable override for a missing propagation (if any)
    fn find_override_for_missing<'a>(
        package: &str,
        feature: &str,
        dependency: &str,
        overrides: &'a [ValidationOverride],
    ) -> Option<&'a ValidationOverride> {
        // Find first matching override (already sorted by priority)
        overrides.iter().find(|override_rule| {
            (override_rule.override_type == OverrideType::AllowMissing
                || override_rule.override_type == OverrideType::Suppress)
                && Self::matches_override(override_rule, package, feature, dependency)
        })
    }

    /// Find the applicable override for an incorrect propagation (if any)
    fn find_override_for_incorrect<'a>(
        package: &str,
        feature: &str,
        entry: &str,
        overrides: &'a [ValidationOverride],
    ) -> Option<&'a ValidationOverride> {
        // Extract dependency from entry (e.g., "dep/feature" -> "dep")
        let dependency = entry
            .split('/')
            .next()
            .unwrap_or(entry)
            .trim_end_matches('?');

        // Find first matching override (already sorted by priority)
        overrides.iter().find(|override_rule| {
            (override_rule.override_type == OverrideType::AllowIncorrect
                || override_rule.override_type == OverrideType::Suppress)
                && Self::matches_override(override_rule, package, feature, dependency)
        })
    }

    /// Check if a package should be ignored entirely
    fn should_ignore_package(&self, package: &str) -> bool {
        self.config
            .ignore_packages
            .iter()
            .any(|pattern| matches_pattern(package, pattern))
    }

    /// Check if a feature should be ignored globally
    fn should_ignore_feature(&self, feature: &str) -> bool {
        self.config
            .ignore_features
            .iter()
            .any(|pattern| matches_pattern(feature, pattern))
    }

    /// Filter missing propagations by overrides
    fn filter_missing_with_overrides(
        package: &str,
        feature: &str,
        missing: Vec<MissingPropagation>,
        overrides: &[ValidationOverride],
        stats: &mut OverrideStats,
    ) -> (Vec<MissingPropagation>, Vec<OverriddenError>) {
        let mut filtered = Vec::new();
        let mut overridden = Vec::new();

        for prop in missing {
            if let Some(override_rule) =
                Self::find_override_for_missing(package, feature, &prop.dependency, overrides)
            {
                // This error is overridden
                Self::record_override_stat(override_rule, stats);
                overridden.push(OverriddenError {
                    package: package.to_string(),
                    feature: feature.to_string(),
                    dependency: prop.dependency.clone(),
                    expected: prop.expected.clone(),
                    original_reason: prop.reason.clone(),
                    override_info: OverrideInfo {
                        override_type: override_rule.override_type,
                        reason: override_rule.reason.clone(),
                        source: override_rule.source,
                        expires: override_rule.expires.clone(),
                    },
                });
            } else {
                // Keep this error
                filtered.push(prop);
            }
        }

        (filtered, overridden)
    }

    /// Filter incorrect propagations by overrides
    fn filter_incorrect_with_overrides(
        package: &str,
        feature: &str,
        incorrect: Vec<IncorrectPropagation>,
        overrides: &[ValidationOverride],
        stats: &mut OverrideStats,
    ) -> (Vec<IncorrectPropagation>, Vec<OverriddenError>) {
        let mut filtered = Vec::new();
        let mut overridden = Vec::new();

        for prop in incorrect {
            if let Some(override_rule) =
                Self::find_override_for_incorrect(package, feature, &prop.entry, overrides)
            {
                // This error is overridden
                Self::record_override_stat(override_rule, stats);
                let dependency = prop
                    .entry
                    .split('/')
                    .next()
                    .unwrap_or(&prop.entry)
                    .trim_end_matches('?')
                    .to_string();
                overridden.push(OverriddenError {
                    package: package.to_string(),
                    feature: feature.to_string(),
                    dependency,
                    expected: prop.entry.clone(),
                    original_reason: prop.reason.clone(),
                    override_info: OverrideInfo {
                        override_type: override_rule.override_type,
                        reason: override_rule.reason.clone(),
                        source: override_rule.source,
                        expires: override_rule.expires.clone(),
                    },
                });
            } else {
                // Keep this error
                filtered.push(prop);
            }
        }

        (filtered, overridden)
    }

    /// Record statistics for an applied override
    fn record_override_stat(override_rule: &ValidationOverride, stats: &mut OverrideStats) {
        stats.total_applied += 1;

        let source_key = match override_rule.source {
            OverrideSource::Cli => "cli",
            OverrideSource::PackageClippierToml => "package-clippier-toml",
            OverrideSource::CargoTomlMetadata => "cargo-toml-metadata",
            OverrideSource::WorkspaceClippierToml => "workspace-clippier-toml",
        };
        *stats.by_source.entry(source_key.to_string()).or_insert(0) += 1;

        let type_key = match override_rule.override_type {
            OverrideType::AllowMissing => "allow-missing",
            OverrideType::AllowIncorrect => "allow-incorrect",
            OverrideType::Suppress => "suppress",
        };
        *stats.by_type.entry(type_key.to_string()).or_insert(0) += 1;
    }

    /// Check if an expiration date has passed
    fn is_expired(expires: &str) -> bool {
        // Try to parse as RFC 3339 date
        if let Ok(expiry_date) = chrono::DateTime::parse_from_rfc3339(expires) {
            return chrono::Utc::now() > expiry_date;
        }

        // Try to parse as simple date (YYYY-MM-DD)
        if let Ok(naive_date) = chrono::NaiveDate::parse_from_str(expires, "%Y-%m-%d") {
            let expiry_datetime = naive_date
                .and_hms_opt(23, 59, 59)
                .unwrap()
                .and_local_timezone(chrono::Utc)
                .unwrap();
            return chrono::Utc::now() > expiry_datetime;
        }

        // If we can't parse, assume not expired
        false
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

    // ==================== Parent Package Validation ====================

    /// Validate parent packages and return results
    fn validate_parent_packages(
        &self,
        warnings: &mut Vec<PackageValidationWarning>,
    ) -> Vec<ParentValidationResult> {
        let parent_configs = self.collect_parent_configs(warnings);

        if parent_configs.is_empty() {
            return Vec::new();
        }

        let mut results = Vec::new();

        for (package_name, config) in parent_configs {
            if let Some(cargo_value) = self.package_cargo_values.get(&package_name) {
                let result = self.validate_single_parent_package(
                    &package_name,
                    cargo_value,
                    &config,
                    warnings,
                );
                results.push(result);
            } else {
                warnings.push(PackageValidationWarning {
                    package: package_name.clone(),
                    message: format!("Parent package '{package_name}' not found in workspace"),
                });
            }
        }

        results
    }

    /// Collect parent package configurations from all sources
    fn collect_parent_configs(
        &self,
        warnings: &mut Vec<PackageValidationWarning>,
    ) -> BTreeMap<String, ResolvedParentConfig> {
        let mut configs: BTreeMap<String, ResolvedParentConfig> = BTreeMap::new();

        // 1. Load from workspace-level clippier.toml (lowest priority)
        if self.config.parent_config.use_config
            && let Ok(workspace_configs) = self.load_workspace_parent_configs()
        {
            for (pkg_name, pkg_config) in workspace_configs {
                configs.insert(pkg_name, pkg_config);
            }
        }

        // 2. Load from package-level clippier.toml (overrides workspace)
        if self.config.parent_config.use_config {
            for package_name in &self.workspace_packages {
                if let Ok(Some(pkg_config)) = self.load_package_parent_config(package_name) {
                    configs.insert(package_name.clone(), pkg_config);
                }
            }
        }

        // 3. Apply CLI overrides (highest priority)
        for pkg_name in &self.config.parent_config.cli_packages {
            let existing = configs.get(pkg_name);
            let resolved = ResolvedParentConfig {
                depth: self
                    .config
                    .parent_config
                    .cli_depth
                    .or_else(|| existing.and_then(|e| e.depth)),
                skip_features: if self.config.parent_config.cli_skip_features.is_empty() {
                    // No CLI skip features - use existing config or None (defaults)
                    existing.and_then(|e| e.skip_features.clone())
                } else {
                    // CLI skip features provided - merge with existing (resolved to defaults if None)
                    let mut merged = existing
                        .and_then(|e| e.skip_features.clone())
                        .unwrap_or_else(|| {
                            DEFAULT_SKIP_FEATURES
                                .iter()
                                .map(|s| (*s).to_string())
                                .collect()
                        });
                    merged.extend(self.config.parent_config.cli_skip_features.clone());
                    Some(merged)
                },
                prefix_overrides: {
                    let mut merged =
                        existing.map_or_else(BTreeMap::new, |e| e.prefix_overrides.clone());
                    for po in &self.config.parent_config.cli_prefix_overrides {
                        merged.insert(po.dependency.clone(), po.prefix.clone());
                    }
                    merged
                },
            };
            configs.insert(pkg_name.clone(), resolved);
        }

        // Validate that all configured packages exist
        for pkg_name in configs.keys() {
            if !self.workspace_packages.contains(pkg_name) {
                warnings.push(PackageValidationWarning {
                    package: pkg_name.clone(),
                    message: format!("Parent package '{pkg_name}' not found in workspace"),
                });
            }
        }

        configs
    }

    /// Load parent package configs from workspace-level clippier.toml
    fn load_workspace_parent_configs(&self) -> Result<BTreeMap<String, ResolvedParentConfig>> {
        let config_path = self.workspace_root.join("clippier.toml");
        if !config_path.exists() {
            return Ok(BTreeMap::new());
        }

        let content = fs::read_to_string(&config_path)?;
        let value: Value = toml::from_str(&content)?;

        let mut configs = BTreeMap::new();

        // Parse parent-packages array
        if let Some(feature_validation) = value.get("feature-validation") {
            // Global prefix overrides
            let global_prefixes: BTreeMap<String, String> = feature_validation
                .get("parent-prefix")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|entry| {
                            let dep = entry.get("dependency")?.as_str()?;
                            let prefix = entry.get("prefix")?.as_str()?;
                            Some((dep.to_string(), prefix.to_string()))
                        })
                        .collect()
                })
                .unwrap_or_default();

            // Parent packages
            if let Some(parent_packages) = feature_validation
                .get("parent-packages")
                .and_then(|v| v.as_array())
            {
                for entry in parent_packages {
                    if let Some(pkg_name) = entry.get("package").and_then(|v| v.as_str()) {
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let depth = entry
                            .get("depth")
                            .and_then(toml::Value::as_integer)
                            .map(|d| d as u8);

                        let skip_features = entry
                            .get("skip-features")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            });

                        configs.insert(
                            pkg_name.to_string(),
                            ResolvedParentConfig {
                                depth,
                                skip_features,
                                prefix_overrides: global_prefixes.clone(),
                            },
                        );
                    }
                }
            }
        }

        Ok(configs)
    }

    /// Load parent config from package-level clippier.toml
    fn load_package_parent_config(
        &self,
        package_name: &str,
    ) -> Result<Option<ResolvedParentConfig>> {
        let package_path = self
            .package_paths
            .get(package_name)
            .ok_or_else(|| anyhow!("Package path not found for {package_name}"))?;

        let config_path = self.workspace_root.join(package_path).join("clippier.toml");
        if !config_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&config_path)?;
        let value: Value = toml::from_str(&content)?;

        if let Some(feature_validation) = value.get("feature-validation")
            && let Some(parent) = feature_validation.get("parent")
        {
            let enabled = parent
                .get("enabled")
                .and_then(toml::Value::as_bool)
                .unwrap_or(false);

            if !enabled {
                return Ok(None);
            }

            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let depth = parent
                .get("depth")
                .and_then(toml::Value::as_integer)
                .map(|d| d as u8);

            let skip_features = parent
                .get("skip-features")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                });

            let prefix_overrides = parent
                .get("prefix")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|entry| {
                            let dep = entry.get("dependency")?.as_str()?;
                            let prefix = entry.get("prefix")?.as_str()?;
                            Some((dep.to_string(), prefix.to_string()))
                        })
                        .collect()
                })
                .unwrap_or_default();

            return Ok(Some(ResolvedParentConfig {
                depth,
                skip_features,
                prefix_overrides,
            }));
        }

        Ok(None)
    }

    /// Validate a single parent package
    fn validate_single_parent_package(
        &self,
        parent_name: &str,
        parent_cargo: &Value,
        config: &ResolvedParentConfig,
        _warnings: &mut Vec<PackageValidationWarning>,
    ) -> ParentValidationResult {
        let mut missing_exposures = Vec::new();
        let mut features_checked = 0;
        let mut features_exposed = 0;

        // Get parent package's features
        let parent_features = get_all_feature_names(parent_cargo);

        // Build skip features list using centralized resolver
        let skip_features = resolve_skip_features(&config.skip_features);

        // Get workspace dependencies of parent package
        let deps = extract_all_dependencies(parent_cargo, false);
        let workspace_deps: Vec<(String, bool)> = deps
            .into_iter()
            .filter(|(name, _)| self.workspace_packages.contains(name))
            .collect();

        // Track visited packages to prevent cycles
        let mut visited = BTreeSet::new();
        visited.insert(parent_name.to_string());

        // Validate each workspace dependency
        for (dep_name, is_optional) in &workspace_deps {
            if let Some(dep_cargo) = self.package_cargo_values.get(dep_name) {
                self.validate_parent_dependency(
                    parent_name,
                    &parent_features,
                    dep_name,
                    dep_cargo,
                    *is_optional,
                    config,
                    &skip_features,
                    1,
                    &mut vec![parent_name.to_string(), dep_name.clone()],
                    &mut visited,
                    &mut missing_exposures,
                    &mut features_checked,
                    &mut features_exposed,
                );
            }
        }

        ParentValidationResult {
            package: parent_name.to_string(),
            missing_exposures,
            features_checked,
            features_exposed,
        }
    }

    /// Validate that parent exposes all features from a dependency
    #[allow(clippy::too_many_arguments)]
    fn validate_parent_dependency(
        &self,
        parent_name: &str,
        parent_features: &BTreeSet<String>,
        dep_name: &str,
        dep_cargo: &Value,
        is_optional: bool,
        config: &ResolvedParentConfig,
        skip_features: &[String],
        current_depth: u8,
        chain: &mut Vec<String>,
        visited: &mut BTreeSet<String>,
        missing_exposures: &mut Vec<MissingFeatureExposure>,
        features_checked: &mut usize,
        features_exposed: &mut usize,
    ) {
        // Get prefix for this dependency
        let prefix = config
            .prefix_overrides
            .get(dep_name)
            .cloned()
            .unwrap_or_else(|| infer_prefix(parent_name, dep_name));

        // Get all features of the dependency
        let dep_features = get_all_feature_names(dep_cargo);

        for dep_feature in &dep_features {
            // Skip features that match skip patterns
            if should_skip_feature(dep_feature, skip_features) {
                continue;
            }

            *features_checked += 1;

            // Expected feature name in parent: "{prefix}-{feature}"
            let expected_parent_feature = format!("{prefix}-{dep_feature}");

            // Check if parent has this feature (with prefix or exact match)
            if parent_features.contains(&expected_parent_feature)
                || parent_features.contains(dep_feature)
            {
                *features_exposed += 1;
            } else {
                let expected_propagation = if is_optional {
                    format!("{dep_name}?/{dep_feature}")
                } else {
                    format!("{dep_name}/{dep_feature}")
                };

                missing_exposures.push(MissingFeatureExposure {
                    parent_package: parent_name.to_string(),
                    dependency: dep_name.to_string(),
                    dependency_feature: dep_feature.clone(),
                    expected_parent_feature,
                    expected_propagation,
                    depth: current_depth,
                    chain: chain.clone(),
                });
            }
        }

        // Recurse into nested dependencies if depth allows
        let should_recurse = config.depth.is_none_or(|max| current_depth < max);

        if should_recurse {
            let nested_deps = extract_all_dependencies(dep_cargo, false);
            let nested_workspace_deps: Vec<(String, bool)> = nested_deps
                .into_iter()
                .filter(|(name, _)| {
                    self.workspace_packages.contains(name) && !visited.contains(name)
                })
                .collect();

            for (nested_dep_name, nested_is_optional) in nested_workspace_deps {
                if visited.insert(nested_dep_name.clone())
                    && let Some(nested_cargo) = self.package_cargo_values.get(&nested_dep_name)
                {
                    chain.push(nested_dep_name.clone());
                    self.validate_parent_dependency(
                        parent_name,
                        parent_features,
                        &nested_dep_name,
                        nested_cargo,
                        nested_is_optional,
                        config,
                        skip_features,
                        current_depth + 1,
                        chain,
                        visited,
                        missing_exposures,
                        features_checked,
                        features_exposed,
                    );
                    chain.pop();
                }
            }
        }
    }
}

/// Resolved parent package configuration (after merging all sources)
#[derive(Debug, Clone, Default)]
struct ResolvedParentConfig {
    /// Maximum depth (None = unlimited)
    depth: Option<u8>,
    /// Features to skip (None = use defaults, Some(empty) = skip nothing)
    skip_features: Option<Vec<String>>,
    /// Prefix overrides (dependency -> prefix)
    prefix_overrides: BTreeMap<String, String>,
}

/// Infer prefix from dependency name relative to parent package
///
/// Examples:
/// - `parent="switchy"`, `dep="switchy_database"` -> `"database"`
/// - `parent="hyperchad"`, `dep="hyperchad_renderer_html"` -> `"renderer-html"`
/// - `parent="moosicbox"`, `dep="moosicbox_audio_decoder"` -> `"audio-decoder"`
fn infer_prefix(parent: &str, dep: &str) -> String {
    let parent_prefix = format!("{parent}_");
    if dep.starts_with(&parent_prefix) {
        dep[parent_prefix.len()..].replace('_', "-")
    } else {
        dep.replace('_', "-")
    }
}

/// Get all feature names from a Cargo.toml value
fn get_all_feature_names(cargo_value: &Value) -> BTreeSet<String> {
    cargo_value
        .get("features")
        .and_then(|f| f.as_table())
        .map(|t| t.keys().cloned().collect())
        .unwrap_or_default()
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
#[allow(clippy::too_many_lines)]
pub fn print_human_output(result: &ValidationResult) {
    println!("🔍 Feature Propagation Validation Results");
    println!("=========================================");
    println!("Total packages checked: {}", result.total_packages);
    println!("Valid packages: {}", result.valid_packages);

    // Print override summary if present
    if let Some(ref summary) = result.override_summary {
        println!("\n📋 Override Summary:");
        println!("  Applied: {} overrides", summary.total_applied);
        if !summary.by_source.is_empty() {
            for (source, count) in &summary.by_source {
                println!("    - {source}: {count}");
            }
        }
        if summary.expired > 0 {
            println!("  ⚠️  Expired: {} overrides", summary.expired);
        }
    }

    if !result.warnings.is_empty() {
        println!("\n⚠️  Warnings:");
        for warning in &result.warnings {
            println!("  - {}: {}", warning.package, warning.message);
        }
    }

    // Print overridden errors if present
    if !result.overridden_errors.is_empty() {
        println!(
            "\n🔕 Overridden Errors ({}):",
            result.overridden_errors.len()
        );
        for overridden in &result.overridden_errors {
            println!(
                "  📦 {}:{}:{}",
                overridden.package, overridden.feature, overridden.dependency
            );
            if let Some(ref reason) = overridden.override_info.reason {
                println!("    Reason: {reason}");
            }
            println!("    Source: {:?}", overridden.override_info.source);
            if let Some(ref expires) = overridden.override_info.expires {
                println!("    Expires: {expires}");
            }
        }
    }

    if result.errors.is_empty() {
        let override_msg = if result.overridden_errors.is_empty() {
            String::new()
        } else {
            format!(" (with {} overrides)", result.overridden_errors.len())
        };
        println!("\n✅ All packages correctly propagate features{override_msg}!");
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

    // Print parent package validation results
    if !result.parent_results.is_empty() {
        println!("\n🔍 Parent Package Validation Results");
        println!("=====================================");

        for parent_result in &result.parent_results {
            let status = if parent_result.missing_exposures.is_empty() {
                "✅"
            } else {
                "❌"
            };
            println!(
                "\n{status} Parent Package: {} ({}/{} features exposed)",
                parent_result.package,
                parent_result.features_exposed,
                parent_result.features_checked
            );

            if !parent_result.missing_exposures.is_empty() {
                // Group by dependency for cleaner output
                let mut by_dep: BTreeMap<&str, Vec<&MissingFeatureExposure>> = BTreeMap::new();
                for exposure in &parent_result.missing_exposures {
                    by_dep
                        .entry(&exposure.dependency)
                        .or_default()
                        .push(exposure);
                }

                for (dep_name, exposures) in by_dep {
                    println!("  📦 {dep_name}:");
                    for exposure in exposures {
                        println!(
                            "    - {} → expected \"{}\" with [\"{}\"]",
                            exposure.dependency_feature,
                            exposure.expected_parent_feature,
                            exposure.expected_propagation
                        );
                        if exposure.depth > 1 {
                            println!("      (chain: {})", exposure.chain.join(" → "));
                        }
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

    // Parent package validation errors
    for parent_result in &result.parent_results {
        for exposure in &parent_result.missing_exposures {
            println!(
                "::error file=packages/{}/Cargo.toml::Missing feature exposure '{}' for dependency '{}' feature '{}'",
                parent_result.package,
                exposure.expected_parent_feature,
                exposure.dependency,
                exposure.dependency_feature
            );
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
            ..ValidatorConfig::test_default()
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
            ..ValidatorConfig::test_default()
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
            ..ValidatorConfig::test_default()
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
            ..ValidatorConfig::test_default()
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
            ..ValidatorConfig::test_default()
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
            ..ValidatorConfig::test_default()
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
            ..ValidatorConfig::test_default()
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
            ..ValidatorConfig::test_default()
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
            overridden_errors: vec![],
            override_summary: None,
            parent_results: vec![],
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
            ..ValidatorConfig::test_default()
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
            ..ValidatorConfig::test_default()
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
            ..ValidatorConfig::test_default()
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
            ..ValidatorConfig::test_default()
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
            ..ValidatorConfig::test_default()
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
            ..ValidatorConfig::test_default()
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
            ..ValidatorConfig::test_default()
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
            ..ValidatorConfig::test_default()
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
            ..ValidatorConfig::test_default()
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

    #[test]
    fn test_string_or_array_to_vec_single() {
        let single = StringOrArray::Single("test".to_string());
        assert_eq!(single.to_vec(), vec!["test".to_string()]);
    }

    #[test]
    fn test_string_or_array_to_vec_multiple() {
        let multiple = StringOrArray::Multiple(vec![
            "dep1".to_string(),
            "dep2".to_string(),
            "dep3".to_string(),
        ]);
        assert_eq!(
            multiple.to_vec(),
            vec!["dep1".to_string(), "dep2".to_string(), "dep3".to_string()]
        );
    }

    #[test]
    fn test_string_or_array_to_vec_empty() {
        let empty = StringOrArray::Multiple(vec![]);
        assert_eq!(empty.to_vec(), Vec::<String>::new());
    }

    #[test]
    fn test_override_config_entry_single_dependency() {
        let toml_str = r#"
feature = "test-feature"
dependency = "single_dep"
type = "allow-missing"
reason = "Test reason"
"#;
        let parsed: OverrideConfigEntry = toml::from_str(toml_str).unwrap();

        assert_eq!(parsed.feature, "test-feature");
        assert_eq!(parsed.dependency.to_vec(), vec!["single_dep".to_string()]);
        assert!(matches!(parsed.override_type, OverrideType::AllowMissing));
        assert_eq!(parsed.reason, "Test reason");
        assert!(parsed.expires.is_none());
    }

    #[test]
    fn test_override_config_entry_array_dependencies() {
        let toml_str = r#"
feature = "test-feature"
dependencies = ["dep1", "dep2", "dep3"]
type = "allow-missing"
reason = "Test reason for multiple deps"
"#;
        let parsed: OverrideConfigEntry = toml::from_str(toml_str).unwrap();

        assert_eq!(parsed.feature, "test-feature");
        assert_eq!(
            parsed.dependency.to_vec(),
            vec!["dep1".to_string(), "dep2".to_string(), "dep3".to_string()]
        );
        assert!(matches!(parsed.override_type, OverrideType::AllowMissing));
        assert_eq!(parsed.reason, "Test reason for multiple deps");
    }

    #[test]
    fn test_override_config_entry_alias_support() {
        // Test that both 'dependency' and 'dependencies' work
        let toml_single = r#"
feature = "feat"
dependency = "dep"
type = "allow-missing"
reason = "reason"
"#;
        let parsed_single: OverrideConfigEntry = toml::from_str(toml_single).unwrap();
        assert_eq!(parsed_single.dependency.to_vec(), vec!["dep".to_string()]);

        let toml_plural = r#"
feature = "feat"
dependencies = ["dep"]
type = "allow-missing"
reason = "reason"
"#;
        let parsed_plural: OverrideConfigEntry = toml::from_str(toml_plural).unwrap();
        assert_eq!(parsed_plural.dependency.to_vec(), vec!["dep".to_string()]);
    }

    #[test]
    fn test_override_config_entry_with_expiration() {
        let toml_str = r#"
feature = "test-feature"
dependency = "dep"
type = "allow-incorrect"
reason = "Temporary override"
expires = "2025-12-31T23:59:59Z"
"#;
        let parsed: OverrideConfigEntry = toml::from_str(toml_str).unwrap();

        assert_eq!(parsed.feature, "test-feature");
        assert!(matches!(parsed.override_type, OverrideType::AllowIncorrect));
        assert_eq!(parsed.expires, Some("2025-12-31T23:59:59Z".to_string()));
    }

    #[test]
    fn test_override_config_entry_suppress_type() {
        let toml_str = r#"
feature = "*"
dependencies = ["dep1", "dep2"]
type = "suppress"
reason = "Suppress all validation"
"#;
        let parsed: OverrideConfigEntry = toml::from_str(toml_str).unwrap();

        assert_eq!(parsed.feature, "*");
        assert_eq!(parsed.dependency.to_vec().len(), 2);
        assert!(matches!(parsed.override_type, OverrideType::Suppress));
    }

    #[test]
    fn test_override_config_entry_wildcards_in_array() {
        let toml_str = r#"
feature = "profiling-*"
dependencies = ["dep_*", "other_dep"]
type = "allow-missing"
reason = "Wildcard pattern test"
"#;
        let parsed: OverrideConfigEntry = toml::from_str(toml_str).unwrap();

        assert_eq!(parsed.feature, "profiling-*");
        let deps = parsed.dependency.to_vec();
        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0], "dep_*");
        assert_eq!(deps[1], "other_dep");
    }

    // ==================== Parent Package Validation Tests ====================

    #[test]
    fn test_infer_prefix_with_matching_parent() {
        // When dependency starts with parent_, strip the prefix
        assert_eq!(infer_prefix("switchy", "switchy_database"), "database");
        assert_eq!(infer_prefix("switchy", "switchy_async"), "async");
        assert_eq!(
            infer_prefix("hyperchad", "hyperchad_renderer_html"),
            "renderer-html"
        );
        assert_eq!(
            infer_prefix("moosicbox", "moosicbox_audio_decoder"),
            "audio-decoder"
        );
    }

    #[test]
    fn test_infer_prefix_without_matching_parent() {
        // When dependency doesn't start with parent_, use full name with underscores replaced
        assert_eq!(infer_prefix("switchy", "other_package"), "other-package");
        assert_eq!(infer_prefix("hyperchad", "some_lib"), "some-lib");
    }

    #[test]
    fn test_get_all_feature_names() {
        let cargo_toml = r#"[package]
name = "test_pkg"
version = "0.1.0"

[features]
default = []
api = []
serde = []
test-feature = []
"#;
        let value: Value = toml::from_str(cargo_toml).unwrap();
        let features = get_all_feature_names(&value);

        assert_eq!(features.len(), 4);
        assert!(features.contains("default"));
        assert!(features.contains("api"));
        assert!(features.contains("serde"));
        assert!(features.contains("test-feature"));
    }

    #[test]
    fn test_get_all_feature_names_empty() {
        let cargo_toml = r#"[package]
name = "test_pkg"
version = "0.1.0"
"#;
        let value: Value = toml::from_str(cargo_toml).unwrap();
        let features = get_all_feature_names(&value);

        assert!(features.is_empty());
    }

    /// Helper to create a test workspace for parent validation
    fn create_parent_test_workspace() -> TempDir {
        let temp_dir = tempfile::tempdir().unwrap();
        let root_path = temp_dir.path();

        // Create workspace Cargo.toml
        let workspace_cargo = r#"[workspace]
members = ["parent", "child_a", "child_b"]
"#;
        fs::write(root_path.join("Cargo.toml"), workspace_cargo).unwrap();

        // Create parent package that should expose child features
        fs::create_dir(root_path.join("parent")).unwrap();
        let parent_cargo = r#"[package]
name = "parent"
version = "0.1.0"

[dependencies]
parent_child_a = { path = "../child_a", optional = true }
parent_child_b = { path = "../child_b", optional = true }

[features]
default = []
child-a = ["dep:parent_child_a"]
child-a-api = ["child-a", "parent_child_a?/api"]
child-b = ["dep:parent_child_b"]
# Missing: child-a-serde, child-b-api, child-b-serde
"#;
        fs::write(root_path.join("parent/Cargo.toml"), parent_cargo).unwrap();

        // Create child_a with features
        fs::create_dir(root_path.join("child_a")).unwrap();
        let child_a_cargo = r#"[package]
name = "parent_child_a"
version = "0.1.0"

[features]
default = []
api = []
serde = []
"#;
        fs::write(root_path.join("child_a/Cargo.toml"), child_a_cargo).unwrap();

        // Create child_b with features
        fs::create_dir(root_path.join("child_b")).unwrap();
        let child_b_cargo = r#"[package]
name = "parent_child_b"
version = "0.1.0"

[features]
default = []
api = []
serde = []
"#;
        fs::write(root_path.join("child_b/Cargo.toml"), child_b_cargo).unwrap();

        temp_dir
    }

    #[test]
    fn test_parent_validation_detects_missing_features() {
        let temp_workspace = create_parent_test_workspace();
        let config = ValidatorConfig {
            features: None,
            skip_features: None,
            workspace_only: true,
            output_format: OutputType::Raw,
            parent_config: ParentValidationConfig {
                cli_packages: vec!["parent".to_string()],
                cli_depth: Some(1),
                cli_skip_features: vec![],
                cli_prefix_overrides: vec![],
                use_config: false,
            },
            ..ValidatorConfig::test_default()
        };

        let validator =
            FeatureValidator::new(Some(temp_workspace.path().to_path_buf()), config).unwrap();
        let result = validator.validate().unwrap();

        // Should have parent results
        assert_eq!(result.parent_results.len(), 1);

        let parent_result = &result.parent_results[0];
        assert_eq!(parent_result.package, "parent");

        // Should have missing exposures
        assert!(!parent_result.missing_exposures.is_empty());

        // Should be missing child-a-serde
        let missing_serde_a = parent_result
            .missing_exposures
            .iter()
            .find(|e| e.dependency == "parent_child_a" && e.dependency_feature == "serde");
        assert!(missing_serde_a.is_some());

        // Should be missing child-b-api and child-b-serde
        let missing_api_b = parent_result
            .missing_exposures
            .iter()
            .find(|e| e.dependency == "parent_child_b" && e.dependency_feature == "api");
        assert!(missing_api_b.is_some());

        let missing_serde_b = parent_result
            .missing_exposures
            .iter()
            .find(|e| e.dependency == "parent_child_b" && e.dependency_feature == "serde");
        assert!(missing_serde_b.is_some());
    }

    #[test]
    fn test_parent_validation_respects_skip_features() {
        let temp_workspace = create_parent_test_workspace();
        let config = ValidatorConfig {
            features: None,
            skip_features: None,
            workspace_only: true,
            output_format: OutputType::Raw,
            parent_config: ParentValidationConfig {
                cli_packages: vec!["parent".to_string()],
                cli_depth: Some(1),
                cli_skip_features: vec!["serde".to_string()], // Skip serde features
                cli_prefix_overrides: vec![],
                use_config: false,
            },
            ..ValidatorConfig::test_default()
        };

        let validator =
            FeatureValidator::new(Some(temp_workspace.path().to_path_buf()), config).unwrap();
        let result = validator.validate().unwrap();

        let parent_result = &result.parent_results[0];

        // Should NOT report serde as missing (it's skipped)
        let missing_serde = parent_result
            .missing_exposures
            .iter()
            .find(|e| e.dependency_feature == "serde");
        assert!(missing_serde.is_none());

        // Should still report api as missing for child_b
        let missing_api_b = parent_result
            .missing_exposures
            .iter()
            .find(|e| e.dependency == "parent_child_b" && e.dependency_feature == "api");
        assert!(missing_api_b.is_some());
    }
}
