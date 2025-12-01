//! Rust workspace analysis and automation tool for managing multi-package projects.
//!
//! Clippier provides utilities for analyzing Rust workspaces and automating development tasks
//! with a focus on CI/CD pipeline generation, dependency analysis, and feature management.
//!
//! # Core Features
//!
//! * **CI/CD Pipeline Generation** - Generate feature matrices for comprehensive testing
//! * **Dependency Analysis** - Analyze workspace dependencies and relationships
//! * **Feature Management** - Generate and validate feature combinations
//! * **Feature Propagation Validation** - Ensure features propagate correctly across workspace dependencies
//! * **Change Impact Analysis** - Determine which packages are affected by file changes
//! * **Docker Integration** - Generate optimized Dockerfiles for workspace packages
//! * **External Dependency Tracking** - Detect changes in external dependencies via git diff
//!
//! # Basic Usage
//!
//! The primary entry point is the command-line tool, but the library also exports core
//! functionality for programmatic use:
//!
//! ```rust
//! use clippier::{FeatureValidator, ValidatorConfig, OutputType};
//!
//! type BoxError = Box<dyn std::error::Error + Send + Sync>;
//! # fn example() -> Result<(), BoxError> {
//! // Validate feature propagation across workspace
//! let config = ValidatorConfig {
//!     features: Some(vec!["fail-on-warnings".to_string()]),
//!     output_format: OutputType::Json,
//!     ..Default::default()
//! };
//!
//! let validator = FeatureValidator::new(None, config)?;
//! let result = validator.validate()?;
//!
//! println!("Validated {} packages", result.total_packages);
//! # Ok(())
//! # }
//! ```
//!
//! # Optional Features
//!
//! * `git-diff` (default) - Enhanced change analysis using git diff to detect external dependency changes
//! * `test-utils` - Utilities for testing (automatically enabled when running tests)
//! * `fail-on-warnings` - Fail build on compiler warnings

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

/// Package filtering based on Cargo.toml properties.
///
/// This module provides functionality to filter workspace packages based on
/// criteria applied to their Cargo.toml properties using a flexible filter syntax.
/// Supports logical operators (AND, OR, NOT), various comparison operators,
/// and Unicode support.
pub mod package_filter;

/// Tool detection and execution infrastructure for linting and formatting.
///
/// This module provides the common infrastructure for detecting installed tools,
/// configuring them, and running them with unified output aggregation.
/// It acts as an orchestrator, delegating to native tools while providing
/// a consistent interface.
///
/// This is an internal module enabled by either `check` or `format` features.
#[cfg(feature = "_tools")]
pub mod tools;

#[cfg(feature = "_transforms")]
pub mod transforms;

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::Path,
};

use clap::ValueEnum;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use toml::Value;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Git diff analysis for detecting external dependency changes.
///
/// This module provides functionality for analyzing git diffs to detect changes in
/// external dependencies via `Cargo.lock` modifications, mapping those changes to
/// affected workspace packages.
///
/// Requires the `git-diff` feature (enabled by default).
#[cfg(feature = "git-diff")]
pub mod git_diff;

/// Feature propagation validation for workspace dependencies.
///
/// This module validates that features are correctly propagated across workspace
/// dependencies to ensure consistent builds and prevent feature-related compilation failures.
///
/// # Example
///
/// ```rust
/// use clippier::{FeatureValidator, ValidatorConfig, OutputType};
///
/// type BoxError = Box<dyn std::error::Error + Send + Sync>;
/// # fn example() -> Result<(), BoxError> {
/// let config = ValidatorConfig {
///     features: Some(vec!["fail-on-warnings".to_string()]),
///     output_format: OutputType::Json,
///     ..Default::default()
/// };
///
/// let validator = FeatureValidator::new(None, config)?;
/// let result = validator.validate()?;
/// # Ok(())
/// # }
/// ```
pub mod feature_validator;

/// Testing utilities for workspace analysis.
///
/// This module provides test helpers and utilities for creating test workspaces
/// and validating workspace analysis functionality.
///
/// Requires the `test-utils` feature or test configuration.
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;

#[cfg(any(test, feature = "test-utils"))]
pub use test_utils::*;

pub use feature_validator::{
    FeatureValidator, ValidationResult, ValidatorConfig, print_github_output, print_human_output,
};

/// Output format for CLI commands
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[clap(rename_all = "kebab_case")]
pub enum OutputType {
    /// JSON formatted output
    Json,
    /// Raw text output
    Raw,
}

/// Information about a package affected by changes
#[derive(Debug, Serialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AffectedPackageInfo {
    /// Name of the affected package
    pub name: String,
    /// Optional reasoning explaining why the package is affected
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Vec<String>>,
}

/// Representation of a Cargo.lock file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CargoLock {
    /// Cargo.lock format version
    pub version: u32,
    /// List of packages in the lockfile
    pub package: Vec<CargoLockPackage>,
}

/// A package entry in a Cargo.lock file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CargoLockPackage {
    /// Package name
    pub name: String,
    /// Package version
    pub version: String,
    /// Package source (registry, git, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Package dependencies
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<Vec<String>>,
}

/// A build or dependency step in CI configuration
#[derive(Debug, Clone, Deserialize)]
pub struct Step {
    /// Command to execute
    command: Option<String>,
    /// Rust toolchain to use (e.g., "stable", "nightly")
    toolchain: Option<String>,
    /// Features required for this step
    features: Option<Vec<String>>,
}

/// Environment variable configuration that can be filtered by features
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ClippierEnv {
    /// Simple string value
    Value(String),
    /// Value with optional feature filtering
    FilteredValue {
        /// The environment variable value
        value: String,
        /// Features that activate this environment variable
        features: Option<Vec<String>>,
    },
}

/// Helper type that accepts either a single item or a vector of items
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum VecOrItem<T> {
    /// A single value
    Value(T),
    /// Multiple values
    Values(Vec<T>),
}

impl<T> From<VecOrItem<T>> for Vec<T> {
    fn from(value: VecOrItem<T>) -> Self {
        match value {
            VecOrItem::Value(x) => vec![x],
            VecOrItem::Values(x) => x,
        }
    }
}

impl<T> Default for VecOrItem<T> {
    fn default() -> Self {
        Self::Values(vec![])
    }
}

/// Configuration for a single platform/OS in clippier.toml
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ClippierConfiguration {
    /// CI steps to run for this configuration
    pub ci_steps: Option<VecOrItem<Step>>,
    /// Cargo commands to run
    pub cargo: Option<VecOrItem<String>>,
    /// Environment variables for this configuration
    pub env: Option<BTreeMap<String, ClippierEnv>>,
    /// System dependencies required
    pub dependencies: Option<Vec<Step>>,
    /// Operating system this configuration applies to
    pub os: String,
    /// Features to skip for this configuration
    pub skip_features: Option<Vec<String>>,
    /// Features required for this configuration
    pub required_features: Option<Vec<String>>,
    /// Optional name for this configuration
    pub name: Option<String>,
    /// Whether to use nightly toolchain
    pub nightly: Option<bool>,
    /// Whether git submodules are needed
    pub git_submodules: Option<bool>,
}

/// Configuration for parallelization settings
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ParallelizationConfig {
    /// Number of chunks to split features into
    pub chunked: u16,
}

/// Root configuration structure for clippier.toml files
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ClippierConf {
    /// Global CI steps
    pub ci_steps: Option<VecOrItem<Step>>,
    /// Global cargo commands
    pub cargo: Option<VecOrItem<String>>,
    /// Platform-specific configurations (optional to allow feature-validation-only configs)
    pub config: Option<Vec<ClippierConfiguration>>,
    /// Global environment variables
    pub env: Option<BTreeMap<String, ClippierEnv>>,
    /// Parallelization configuration
    pub parallelization: Option<ParallelizationConfig>,
    /// Whether to use nightly toolchain globally
    pub nightly: Option<bool>,
    /// Whether git submodules are needed globally
    pub git_submodules: Option<bool>,
    /// Tool configuration for check/format commands
    #[cfg(feature = "_tools")]
    pub tools: Option<tools::ToolsConfig>,
}

/// Workspace-level configuration (root clippier.toml)
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct WorkspaceClippierConf {
    /// Default environment variables
    pub env: Option<BTreeMap<String, ClippierEnv>>,
    /// Default CI steps
    pub ci_steps: Option<VecOrItem<Step>>,
    /// Default cargo commands
    pub cargo: Option<VecOrItem<String>>,
    /// Default nightly setting
    pub nightly: Option<bool>,
    /// Default git submodules setting
    pub git_submodules: Option<bool>,
    /// Default dependencies
    pub dependencies: Option<Vec<Step>>,
}

/// List of features that may be chunked for parallel processing
#[derive(Debug, Clone)]
pub enum FeaturesList {
    /// Features split into chunks for parallel processing
    Chunked(Vec<Vec<String>>),
    /// Features as a single list without chunking
    NotChunked(Vec<String>),
}

/// Configuration that propagates from workspace dependencies
#[derive(Debug, Clone, Default)]
pub struct PropagatedConfig {
    /// Whether git submodules are needed (propagated from dependencies)
    pub git_submodules: Option<bool>,
    /// System dependencies collected from workspace dependencies
    pub dependencies: Vec<Step>,
    /// CI steps collected from workspace dependencies
    pub ci_steps: Vec<Step>,
    /// Environment variables collected from workspace dependencies
    pub env: BTreeMap<String, ClippierEnv>,
}

/// Splits a slice into approximately `n` equal-sized chunks
pub fn split<T>(slice: &[T], n: usize) -> impl Iterator<Item = &[T]> {
    if slice.is_empty() || n == 0 {
        return SplitIter::empty();
    }

    let chunk_size = slice.len().div_ceil(n);
    SplitIter::new(slice, chunk_size)
}

/// Iterator that yields chunks from a slice
pub struct SplitIter<'a, T> {
    slice: &'a [T],
    chunk_size: usize,
}

impl<'a, T> SplitIter<'a, T> {
    const fn new(slice: &'a [T], chunk_size: usize) -> Self {
        Self { slice, chunk_size }
    }

    const fn empty() -> Self {
        Self {
            slice: &[],
            chunk_size: 0,
        }
    }
}

impl<'a, T> Iterator for SplitIter<'a, T> {
    type Item = &'a [T];

    fn next(&mut self) -> Option<Self::Item> {
        if self.slice.is_empty() {
            return None;
        }

        let chunk_size = self.chunk_size.min(self.slice.len());
        let (chunk, rest) = self.slice.split_at(chunk_size);
        self.slice = rest;
        Some(chunk)
    }
}

/// Processes a list of features with optional chunking, spreading, and randomization
#[must_use]
pub fn process_features(
    features: Vec<String>,
    chunked: Option<u16>,
    spread: bool,
    randomize: bool,
    seed: Option<u64>,
) -> FeaturesList {
    let mut features = features;

    // Randomize features if requested
    if randomize {
        use switchy_random::rand::rand::seq::SliceRandom;

        let actual_seed = seed.unwrap_or_else(|| {
            // Generate a random seed
            let generated_seed = switchy_random::rng().next_u64();
            eprintln!("Generated seed: {generated_seed}");
            generated_seed
        });

        // Use the seed (provided or generated) for deterministic randomization
        let mut rng = switchy_random::Rng::from_seed(actual_seed);
        features.shuffle(&mut rng);
    }

    if let Some(max_features_per_chunk) = chunked {
        let chunk_size = max_features_per_chunk as usize;

        if spread && features.len() > chunk_size {
            // When spread is true, we want to distribute features more evenly
            // while still respecting the chunk_size limit
            let num_chunks = features.len().div_ceil(chunk_size);
            let mut result = vec![Vec::new(); num_chunks];

            // Distribute features ensuring no chunk exceeds chunk_size
            for (i, feature) in features.into_iter().enumerate() {
                let chunk_index = i % num_chunks;
                // Only add if the chunk hasn't reached its limit
                if result[chunk_index].len() < chunk_size {
                    result[chunk_index].push(feature);
                } else {
                    // Find the first chunk that has space
                    if let Some(available_chunk) =
                        result.iter_mut().find(|chunk| chunk.len() < chunk_size)
                    {
                        available_chunk.push(feature);
                    } else {
                        // This shouldn't happen if our math is correct, but as a fallback
                        // create a new chunk
                        result.push(vec![feature]);
                    }
                }
            }
            FeaturesList::Chunked(result.into_iter().filter(|v| !v.is_empty()).collect())
        } else {
            // Regular chunking - up to max_features_per_chunk features per package
            let chunked_features: Vec<Vec<String>> = features
                .chunks(chunk_size)
                .map(<[std::string::String]>::to_vec)
                .collect();
            FeaturesList::Chunked(chunked_features)
        }
    } else {
        FeaturesList::NotChunked(features)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DependencyKind {
    WorkspaceReference,
    WorkspaceMember,
    External,
}

#[derive(Debug, Clone)]
struct DependencyInfo<'a> {
    name: &'a str,
    kind: DependencyKind,
    is_optional: bool,
}

/// Context for workspace member resolution and caching
pub struct WorkspaceContext {
    root: std::path::PathBuf,
    member_patterns: Vec<String>,
    member_cache: RefCell<BTreeMap<String, std::path::PathBuf>>,
    path_cache: RefCell<BTreeSet<std::path::PathBuf>>,
    fully_loaded: RefCell<bool>,
    #[allow(clippy::option_option)]
    workspace_config: RefCell<Option<Option<WorkspaceClippierConf>>>,
}

impl WorkspaceContext {
    fn new(workspace_root: &Path) -> Result<Self, BoxError> {
        let workspace_cargo = workspace_root.join("Cargo.toml");
        let content = switchy_fs::sync::read_to_string(&workspace_cargo)?;
        let root_toml: Value = toml::from_str(&content)?;

        let mut member_patterns = Vec::new();

        if let Some(Value::Table(workspace)) = root_toml.get("workspace")
            && let Some(Value::Array(member_list)) = workspace.get("members")
        {
            for member in member_list {
                if let Value::String(member_pattern) = member {
                    member_patterns.push(member_pattern.clone());
                }
            }
        }

        Ok(Self {
            root: workspace_root.to_path_buf(),
            member_patterns,
            member_cache: RefCell::new(BTreeMap::new()),
            path_cache: RefCell::new(BTreeSet::new()),
            fully_loaded: RefCell::new(false),
            workspace_config: RefCell::new(None),
        })
    }

    fn is_member_by_path(&self, path: &Path) -> bool {
        let Ok(canonical) = switchy_fs::sync::canonicalize(path) else {
            return false;
        };

        if self.path_cache.borrow().contains(&canonical) {
            return true;
        }

        for pattern in &self.member_patterns {
            let member_path = self.root.join(pattern);
            if let Ok(member_canonical) = switchy_fs::sync::canonicalize(&member_path)
                && member_canonical == canonical
            {
                self.path_cache.borrow_mut().insert(canonical.clone());

                if let Some(name) = Self::read_package_name(&canonical) {
                    self.member_cache.borrow_mut().insert(name, canonical);
                }

                return true;
            }
        }

        false
    }

    fn ensure_fully_loaded(&self) {
        if *self.fully_loaded.borrow() {
            return;
        }

        log::trace!(
            "🔄 Loading all {} workspace members",
            self.member_patterns.len()
        );
        let start = std::time::Instant::now();

        for pattern in &self.member_patterns {
            let member_path = self.root.join(pattern);
            if switchy_fs::exists(&member_path)
                && let Ok(canonical) = switchy_fs::sync::canonicalize(&member_path)
                && !self.path_cache.borrow().contains(&canonical)
                && let Some(actual_name) = Self::read_package_name(&canonical)
            {
                self.member_cache
                    .borrow_mut()
                    .insert(actual_name, canonical.clone());
                self.path_cache.borrow_mut().insert(canonical);
            }
        }

        *self.fully_loaded.borrow_mut() = true;
        log::trace!(
            "✅ Loaded {} members in {:?}",
            self.member_cache.borrow().len(),
            start.elapsed()
        );
    }

    fn is_member_by_name(&self, name: &str) -> bool {
        self.ensure_fully_loaded();
        self.member_cache.borrow().contains_key(name)
    }

    fn find_member(&self, name: &str) -> Option<std::path::PathBuf> {
        if let Some(path) = self.member_cache.borrow().get(name) {
            return Some(path.clone());
        }

        if self.is_member_by_name(name) {
            self.member_cache.borrow().get(name).cloned()
        } else {
            None
        }
    }

    fn read_package_name(package_path: &Path) -> Option<String> {
        let cargo_toml_path = package_path.join("Cargo.toml");
        let content = switchy_fs::sync::read_to_string(cargo_toml_path).ok()?;
        let toml: Value = toml::from_str(&content).ok()?;
        toml.get("package")?.get("name")?.as_str().map(String::from)
    }

    /// Get workspace-level configuration, loading it if necessary
    fn workspace_config(&self) -> Result<Option<WorkspaceClippierConf>, BoxError> {
        // Check if we've already tried to load the config
        if let Some(cached) = self.workspace_config.borrow().as_ref() {
            return Ok(cached.clone());
        }

        // Try to load workspace config
        let workspace_conf_path = self.root.join("clippier.toml");

        let result = if switchy_fs::exists(&workspace_conf_path) {
            log::trace!(
                "Loading workspace config from: {}",
                workspace_conf_path.display()
            );
            let content = switchy_fs::sync::read_to_string(&workspace_conf_path)?;
            let conf: WorkspaceClippierConf = toml::from_str(&content)?;
            Some(conf)
        } else {
            log::trace!(
                "No workspace-level clippier.toml found at: {}",
                workspace_conf_path.display()
            );
            None
        };

        // Cache the result (None means we tried and found nothing)
        *self.workspace_config.borrow_mut() = Some(result.clone());

        Ok(result)
    }
}

fn classify_dependency(
    dep_value: &Value,
    context: &WorkspaceContext,
    package_path: &Path,
) -> DependencyKind {
    if let Value::Table(table) = dep_value {
        if table.get("workspace") == Some(&Value::Boolean(true)) {
            return DependencyKind::WorkspaceReference;
        }

        if let Some(Value::String(path_str)) = table.get("path") {
            let dep_path = package_path.join(path_str);
            if context.is_member_by_path(&dep_path) {
                return DependencyKind::WorkspaceMember;
            }
        }
    }

    DependencyKind::External
}

fn iterate_dependencies<'a>(
    cargo_toml: &'a Value,
    context: &'a WorkspaceContext,
    package_path: &'a Path,
) -> impl Iterator<Item = DependencyInfo<'a>> + 'a {
    const SECTIONS: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];

    SECTIONS.into_iter().flat_map(move |section_name| {
        cargo_toml
            .get(section_name)
            .and_then(Value::as_table)
            .into_iter()
            .flat_map(move |deps_table| {
                deps_table.iter().map(move |(dep_name, dep_value)| {
                    let kind = classify_dependency(dep_value, context, package_path);
                    let is_optional = if let Value::Table(table) = dep_value {
                        table.get("optional") == Some(&Value::Boolean(true))
                    } else {
                        false
                    };

                    DependencyInfo {
                        name: dep_name,
                        kind,
                        is_optional,
                    }
                })
            })
    })
}

fn extract_dependencies<F>(
    cargo_toml: &Value,
    context: &WorkspaceContext,
    package_path: &Path,
    filter: F,
) -> Vec<String>
where
    F: Fn(&DependencyInfo) -> bool,
{
    let mut deps: Vec<String> = iterate_dependencies(cargo_toml, context, package_path)
        .filter(filter)
        .map(|dep| dep.name.to_string())
        .collect();

    deps.sort();
    deps.dedup();
    deps
}

fn extract_workspace_deps_simple(
    cargo_toml: &Value,
    context: &WorkspaceContext,
    package_path: &Path,
) -> Vec<String> {
    extract_dependencies(cargo_toml, context, package_path, |dep| match dep.kind {
        DependencyKind::WorkspaceMember => true,
        DependencyKind::WorkspaceReference => context.is_member_by_name(dep.name),
        DependencyKind::External => false,
    })
}

fn merge_steps(mut base: Vec<Step>, overlay: Vec<Step>) -> Vec<Step> {
    for step in overlay {
        if !base.iter().any(|s| {
            s.command == step.command
                && s.toolchain == step.toolchain
                && s.features == step.features
        }) {
            base.push(step);
        }
    }
    base
}

fn merge_env_maps(
    mut base: BTreeMap<String, ClippierEnv>,
    overlay: BTreeMap<String, ClippierEnv>,
) -> BTreeMap<String, ClippierEnv> {
    for (key, value) in overlay {
        base.insert(key, value);
    }
    base
}

fn merge_propagated_configs(base: PropagatedConfig, overlay: PropagatedConfig) -> PropagatedConfig {
    PropagatedConfig {
        git_submodules: match (base.git_submodules, overlay.git_submodules) {
            (Some(true), _) | (_, Some(true)) => Some(true),
            (Some(false), None | Some(false)) | (None, Some(false)) => Some(false),
            (None, None) => None,
        },
        dependencies: merge_steps(base.dependencies, overlay.dependencies),
        ci_steps: merge_steps(base.ci_steps, overlay.ci_steps),
        env: merge_env_maps(base.env, overlay.env),
    }
}

#[allow(clippy::too_many_lines, clippy::similar_names)]
fn collect_propagated_config(
    context: &WorkspaceContext,
    package_name: &str,
    os_filter: Option<&str>,
    visited: &mut BTreeSet<String>,
    cache: &mut BTreeMap<String, PropagatedConfig>,
) -> Result<PropagatedConfig, BoxError> {
    if let Some(cached) = cache.get(package_name) {
        return Ok(cached.clone());
    }

    if visited.contains(package_name) {
        log::trace!("🔁 Circular dependency detected for {package_name}, returning empty config");
        return Ok(PropagatedConfig::default());
    }

    visited.insert(package_name.to_string());

    log::trace!("📦 Collecting propagated config for package: {package_name}");

    let package_path = context
        .find_member(package_name)
        .ok_or_else(|| format!("Package {package_name} not found in workspace"))?;
    let cargo_toml_path = package_path.join("Cargo.toml");

    if !switchy_fs::exists(&cargo_toml_path) {
        log::trace!("⚠️ No Cargo.toml found for {package_name}, skipping");
        visited.remove(package_name);
        return Ok(PropagatedConfig::default());
    }

    let cargo_toml_content = switchy_fs::sync::read_to_string(&cargo_toml_path)?;
    let cargo_toml: Value = toml::from_str(&cargo_toml_content)?;

    let clippier_toml_path = package_path.join("clippier.toml");
    let own_config = if switchy_fs::exists(&clippier_toml_path) {
        let content = switchy_fs::sync::read_to_string(&clippier_toml_path)?;
        let conf: ClippierConf = toml::from_str(&content)?;

        let mut prop = PropagatedConfig {
            git_submodules: conf.git_submodules,
            dependencies: Vec::new(),
            ci_steps: Vec::new(),
            env: conf.env.unwrap_or_default(),
        };

        if let Some(configs) = &conf.config {
            for config in configs {
                let os_matches = os_filter.is_none() || os_filter == Some(&config.os);

                if os_matches {
                    if let Some(deps) = &config.dependencies {
                        prop.dependencies.extend(deps.clone());
                    }
                    if let Some(steps) = &config.ci_steps {
                        match steps {
                            VecOrItem::Value(step) => prop.ci_steps.push(step.clone()),
                            VecOrItem::Values(steps) => prop.ci_steps.extend(steps.clone()),
                        }
                    }
                }

                if config.git_submodules.is_some() {
                    prop.git_submodules = prop.git_submodules.or(config.git_submodules);
                }
            }
        }

        if let Some(steps) = &conf.ci_steps {
            match steps {
                VecOrItem::Value(step) => prop.ci_steps.push(step.clone()),
                VecOrItem::Values(steps) => prop.ci_steps.extend(steps.clone()),
            }
        }

        prop
    } else {
        PropagatedConfig::default()
    };

    let workspace_deps = extract_workspace_deps_simple(&cargo_toml, context, &package_path);

    let mut merged = PropagatedConfig::default();
    for dep in workspace_deps {
        log::trace!("  ↳ Processing dependency: {dep}");
        match collect_propagated_config(context, &dep, os_filter, visited, cache) {
            Ok(dep_config) => {
                merged = merge_propagated_configs(merged, dep_config);
            }
            Err(e) => {
                log::warn!("Failed to collect config for dependency {dep}: {e}");
            }
        }
    }

    merged = merge_propagated_configs(merged, own_config);

    cache.insert(package_name.to_string(), merged.clone());

    visited.remove(package_name);

    log::trace!(
        "✅ Collected config for {package_name}: git_submodules={:?}, deps={}, ci_steps={}, env={}",
        merged.git_submodules,
        merged.dependencies.len(),
        merged.ci_steps.len(),
        merged.env.len()
    );

    Ok(merged)
}

fn find_workspace_root_from_package(package_path: &Path) -> Result<std::path::PathBuf, BoxError> {
    let mut current = package_path.to_path_buf();

    while let Some(parent) = current.parent() {
        let cargo_toml = parent.join("Cargo.toml");
        if switchy_fs::exists(&cargo_toml) {
            let content = switchy_fs::sync::read_to_string(&cargo_toml)?;
            let toml_value: Value = toml::from_str(&content)?;

            if toml_value.get("workspace").is_some() {
                return Ok(parent.to_path_buf());
            }
        }
        current = parent.to_path_buf();
    }

    Err("Workspace root not found".into())
}

/// Checks if a feature should be skipped based on patterns.
///
/// Supports:
/// * Exact matches: `"default"` matches only `"default"`
/// * Wildcards: `"*-default"` matches `"bob-default"`, `"sally-default"`, etc.
/// * Single char wildcards: `"v?"` matches `"v1"`, `"v2"` but not `"v10"`
/// * Negation: `"!enable-bob"` keeps `"enable-bob"` even if other patterns match
///
/// Patterns are evaluated in order, with the last matching pattern determining the result.
///
/// # Examples
///
/// ```
/// # use clippier::should_skip_feature;
/// // Skip all features ending with -default
/// assert!(should_skip_feature("bob-default", &["*-default".to_string()]));
/// assert!(!should_skip_feature("enable-bob", &["*-default".to_string()]));
///
/// // Skip everything except enable-bob
/// assert!(should_skip_feature("feature1", &["*".to_string(), "!enable-bob".to_string()]));
/// assert!(!should_skip_feature("enable-bob", &["*".to_string(), "!enable-bob".to_string()]));
/// ```
#[must_use]
pub fn should_skip_feature(feature: &str, patterns: &[String]) -> bool {
    let mut should_skip = false;

    for pattern in patterns {
        // Check for negation prefix (!)
        let (is_negation, pattern_str) = pattern
            .strip_prefix('!')
            .map_or((false, pattern.as_str()), |p| (true, p));

        let matches = if pattern_str.contains('*') || pattern_str.contains('?') {
            // Use globset for wildcard patterns
            globset::Glob::new(pattern_str)
                .ok()
                .is_some_and(|g| g.compile_matcher().is_match(feature))
        } else {
            // Exact match for non-wildcard patterns
            feature == pattern_str
        };

        if matches {
            should_skip = !is_negation;
        }
    }

    should_skip
}

/// Checks if an item matches a pattern (supports wildcards and exact matches).
///
/// This is a simpler version of `should_skip_feature` without negation support,
/// used for inclusion patterns.
///
/// # Examples
///
/// ```
/// # use clippier::matches_pattern;
/// assert!(matches_pattern("bob-default", "*-default"));
/// assert!(matches_pattern("v1", "v?"));
/// assert!(!matches_pattern("v10", "v?"));
/// assert!(matches_pattern("exact", "exact"));
/// ```
#[must_use]
pub fn matches_pattern(item: &str, pattern: &str) -> bool {
    if pattern.contains('*') || pattern.contains('?') {
        // Use globset for wildcard patterns
        globset::Glob::new(pattern)
            .ok()
            .is_some_and(|g| g.compile_matcher().is_match(item))
    } else {
        // Exact match for non-wildcard patterns
        item == pattern
    }
}

/// Expands wildcard patterns in a list to match against available items.
///
/// Supports:
/// * Exact matches: Returns the pattern as-is
/// * Wildcards: Expands to all matching items from `available_items`
/// * Negation: `!pattern` removes matching items (processed after additions)
///
/// Patterns are evaluated in order. Negations remove items from the result set.
///
/// # Examples
///
/// ```
/// # use clippier::expand_pattern_list;
/// let available = vec!["default".to_string(), "bob-default".to_string(), "sally-default".to_string(), "production".to_string()];
/// let patterns = vec!["*-default".to_string(), "production".to_string()];
/// let expanded = expand_pattern_list(&patterns, &available);
/// assert!(expanded.contains(&"bob-default".to_string()));
/// assert!(expanded.contains(&"sally-default".to_string()));
/// assert!(expanded.contains(&"production".to_string()));
/// assert!(!expanded.contains(&"default".to_string()));
///
/// // With negation
/// let patterns = vec!["*".to_string(), "!bob-default".to_string()];
/// let expanded = expand_pattern_list(&patterns, &available);
/// assert!(!expanded.contains(&"bob-default".to_string()));
/// assert!(expanded.contains(&"default".to_string()));
/// ```
#[must_use]
pub fn expand_pattern_list(patterns: &[String], available_items: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    let mut to_remove = Vec::new();

    for pattern in patterns {
        // Check for negation prefix (!)
        let (is_negation, pattern_str) = pattern
            .strip_prefix('!')
            .map_or((false, pattern.as_str()), |p| (true, p));

        if is_negation {
            // Negation - mark items to remove
            if pattern_str.contains('*') || pattern_str.contains('?') {
                // Wildcard negation - collect matching items to remove
                for item in available_items {
                    if matches_pattern(item, pattern_str) && !to_remove.contains(item) {
                        to_remove.push(item.clone());
                    }
                }
            } else {
                // Exact negation
                let pattern_string = pattern_str.to_string();
                if !to_remove.contains(&pattern_string) {
                    to_remove.push(pattern_string);
                }
            }
        } else {
            // Regular pattern - add items
            if pattern_str.contains('*') || pattern_str.contains('?') {
                // Wildcard - expand to all matching items
                for item in available_items {
                    if matches_pattern(item, pattern_str) && !result.contains(item) {
                        result.push(item.clone());
                    }
                }
            } else {
                // Exact match - add as-is (even if it doesn't exist in available_items)
                // This preserves the original behavior for exact matches
                let pattern_string = pattern_str.to_string();
                if !result.contains(&pattern_string) {
                    result.push(pattern_string);
                }
            }
        }
    }

    // Remove negated items
    result.retain(|item| !to_remove.contains(item));

    result
}

/// Expands wildcard patterns in a feature list against available features from Cargo.toml.
///
/// This is used to expand patterns like `enable-*` into concrete feature names.
///
/// # Examples
///
/// ```
/// # use clippier::expand_features_from_cargo_toml;
/// # use toml::Value;
/// let cargo_toml_str = r#"
/// [features]
/// default = []
/// enable-bob = []
/// enable-sally = []
/// production = []
/// "#;
/// let cargo_toml: Value = toml::from_str(cargo_toml_str).unwrap();
/// let patterns = vec!["enable-*".to_string(), "production".to_string()];
/// let expanded = expand_features_from_cargo_toml(&cargo_toml, &patterns);
/// assert!(expanded.contains(&"enable-bob".to_string()));
/// assert!(expanded.contains(&"enable-sally".to_string()));
/// assert!(expanded.contains(&"production".to_string()));
/// ```
#[must_use]
pub fn expand_features_from_cargo_toml(cargo_toml: &Value, patterns: &[String]) -> Vec<String> {
    let Some(Value::Table(features_table)) = cargo_toml.get("features") else {
        return vec![];
    };

    // Get all available features from Cargo.toml
    let all_features: Vec<String> = features_table
        .keys()
        .filter(|k| !k.starts_with('_'))
        .cloned()
        .collect();

    expand_pattern_list(patterns, &all_features)
}

/// Fetches and filters features from a Cargo.toml file
#[must_use]
pub fn fetch_features(
    cargo_toml: &Value,
    offset: Option<u16>,
    max: Option<u16>,
    specific_features: Option<&[String]>,
    skip_features: Option<&[String]>,
    _required_features: Option<&[String]>,
) -> Vec<String> {
    let Some(Value::Table(features_table)) = cargo_toml.get("features") else {
        return vec![];
    };

    // Get all available features from Cargo.toml
    let all_features: Vec<String> = features_table
        .keys()
        .filter(|k| !k.starts_with('_'))
        .cloned()
        .collect();

    let mut features: Vec<String> = specific_features.map_or_else(
        || all_features.clone(),
        |patterns| {
            // Expand wildcard patterns in specific_features
            expand_pattern_list(patterns, &all_features)
        },
    );

    if let Some(skip) = skip_features {
        features.retain(|f| !should_skip_feature(f, skip));
    }

    let offset = offset.unwrap_or(0) as usize;
    if offset < features.len() {
        features = features[offset..].to_vec();
    } else {
        features = vec![];
    }

    if let Some(max_count) = max {
        features.truncate(max_count as usize);
    }

    features
}

/// Checks if a dependency uses workspace inheritance
#[must_use]
pub fn is_workspace_dependency(dep_value: &Value) -> bool {
    match dep_value {
        Value::Table(table) => table.get("workspace") == Some(&Value::Boolean(true)),
        _ => false,
    }
}

/// Checks if a dependency uses workspace inheritance with features
#[must_use]
pub fn is_workspace_dependency_with_features(dep_value: &Value) -> bool {
    if !is_workspace_dependency(dep_value) {
        return false;
    }

    match dep_value {
        Value::Table(table) => {
            // Optional dependencies should return false unless activated by features
            if table.get("optional") == Some(&Value::Boolean(true)) {
                return false;
            }
            true
        }
        _ => false,
    }
}

/// Gets the default-features setting for a dependency
#[must_use]
pub fn get_dependency_default_features(dep_value: &Value) -> Option<bool> {
    match dep_value {
        Value::Table(table) => {
            // Check both dash and underscore variants
            table.get("default-features").map_or_else(
                || {
                    table.get("default_features").and_then(
                        |default_features| match default_features {
                            Value::Boolean(b) => Some(*b),
                            _ => None,
                        },
                    )
                },
                |default_features| match default_features {
                    Value::Boolean(b) => Some(*b),
                    _ => None,
                },
            )
        }
        _ => None,
    }
}

/// Determines if a path string is a git URL
#[must_use]
pub fn is_git_url(path: &str) -> bool {
    path.starts_with("http://")
        || path.starts_with("https://")
        || path.starts_with("git@")
        || path.starts_with("ssh://git@")
        || path.starts_with("git://")
}

/// Gets the binary name for a package, using override if provided or reading from Cargo.toml
#[must_use]
pub fn get_binary_name(
    workspace_root: &Path,
    target_package: &str,
    target_package_path: &str,
    bin_override: Option<&str>,
) -> String {
    // If a binary name override is provided, use it directly
    if let Some(bin_name) = bin_override {
        return bin_name.to_string();
    }

    // Try to read the target package's Cargo.toml to get the correct binary name
    let cargo_path = workspace_root.join(target_package_path).join("Cargo.toml");

    if let Ok(source) = switchy_fs::sync::read_to_string(&cargo_path)
        && let Ok(value) = toml::from_str::<Value>(&source)
    {
        // Check for explicit binary definitions
        if let Some(bins) = value.get("bin").and_then(|b| b.as_array()) {
            // Use the first binary definition if it has a name
            if let Some(bin) = bins.first()
                && let Some(bin_name) = bin.get("name").and_then(|n| n.as_str())
            {
                return bin_name.to_string();
            }
        }

        // Check for a single binary definition (not array)
        if let Some(bin) = value.get("bin").and_then(|b| b.as_table())
            && let Some(bin_name) = bin.get("name").and_then(|n| n.as_str())
        {
            return bin_name.to_string();
        }
    }

    // Fallback: use package name with underscores converted to dashes
    target_package.replace('-', "_")
}

/// Processes a Cargo.toml file and returns a list of packages with their features
///
/// # Errors
///
/// * If the Cargo.toml file is not found or cannot be read
/// * If the Cargo.toml file has an invalid format
/// * If the Cargo.toml file has a syntax error
///
/// # Panics
///
/// * If the `path` argument cannot be converted to a string
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub async fn process_configs(
    path: &Path,
    offset: Option<u16>,
    max: Option<u16>,
    chunked: Option<u16>,
    spread: bool,
    randomize: bool,
    seed: Option<u64>,
    specific_features: Option<&[String]>,
    skip_features_override: Option<&[String]>,
    required_features_override: Option<&[String]>,
) -> Result<Vec<serde_json::Map<String, serde_json::Value>>, BoxError> {
    log::debug!("Loading file '{}'", path.display());
    let cargo_path = path.join("Cargo.toml");
    let source = switchy_fs::unsync::read_to_string(cargo_path).await?;
    let value: Value = toml::from_str(&source)?;

    let conf_path = path.join("clippier.toml");
    let conf = if switchy_fs::unsync::is_file(&conf_path).await {
        let source = switchy_fs::unsync::read_to_string(conf_path).await?;
        let value: ClippierConf = toml::from_str(&source)?;
        Some(value)
    } else {
        None
    };

    log::debug!("{} conf={conf:?}", path.display());

    let default_config = vec![ClippierConfiguration {
        os: "ubuntu".to_string(),
        dependencies: None,
        env: None,
        cargo: None,
        name: None,
        ci_steps: None,
        skip_features: None,
        required_features: None,
        nightly: None,
        git_submodules: None,
    }];

    let configs = conf
        .as_ref()
        .and_then(|x| x.config.clone())
        .unwrap_or(default_config);

    let mut packages = vec![];

    let workspace_root =
        find_workspace_root_from_package(path).unwrap_or_else(|_| path.to_path_buf());
    let workspace_context = WorkspaceContext::new(&workspace_root)?;

    if let Some(name) = value
        .get("package")
        .and_then(|x| x.get("name"))
        .and_then(|x| x.as_str())
        .map(str::to_string)
    {
        for config in configs {
            // Combine skip_features from command line and config file
            let combined_skip_features =
                match (skip_features_override, config.skip_features.as_deref()) {
                    (Some(override_features), Some(config_features)) => {
                        // Combine both lists and remove duplicates
                        let mut combined = override_features.to_vec();
                        for feature in config_features {
                            if !combined.contains(feature) {
                                combined.push(feature.clone());
                            }
                        }
                        Some(combined)
                    }
                    (Some(override_features), None) => Some(override_features.to_vec()),
                    (None, Some(config_features)) => Some(config_features.to_vec()),
                    (None, None) => None,
                };

            let features = fetch_features(
                &value,
                offset,
                max,
                specific_features,
                combined_skip_features.as_deref(),
                required_features_override.or(config.required_features.as_deref()),
            );
            let features = process_features(
                features,
                conf.as_ref()
                    .and_then(|x| x.parallelization.as_ref().map(|x| x.chunked))
                    .or(chunked),
                spread,
                randomize,
                seed,
            );

            // Expand wildcards in required_features
            let expanded_required_features = required_features_override
                .or(config.required_features.as_deref())
                .map(|patterns| expand_features_from_cargo_toml(&value, patterns));

            match &features {
                FeaturesList::Chunked(x) => {
                    for features in x {
                        packages.push(create_map(
                            &workspace_context,
                            &name,
                            conf.as_ref(),
                            &config,
                            path.to_str().unwrap(),
                            &name,
                            expanded_required_features.as_deref(),
                            features,
                        )?);
                    }
                }
                FeaturesList::NotChunked(x) => {
                    packages.push(create_map(
                        &workspace_context,
                        &name,
                        conf.as_ref(),
                        &config,
                        path.to_str().unwrap(),
                        &name,
                        expanded_required_features.as_deref(),
                        x,
                    )?);
                }
            }
        }
    }

    Ok(packages)
}

/// Handle chunking and `max_parallel` re-chunking by combining packages
///
/// Applies `max_parallel` re-chunking by combining packages when the total count
/// exceeds `max_parallel`. Respects the original chunking constraint - only
/// combines packages when necessary to meet the `max_parallel` limit, never
/// creates packages with fewer features than the original chunking.
///
/// # Errors
///
/// * If JSON serialization fails
pub fn apply_max_parallel_rechunking(
    packages: Vec<serde_json::Map<String, serde_json::Value>>,
    max_parallel: usize,
    chunked: Option<u16>,
) -> Result<Vec<serde_json::Map<String, serde_json::Value>>, BoxError> {
    if packages.len() <= max_parallel {
        // Already within limit, no need to re-chunk
        return Ok(packages);
    }

    let mut result = Vec::new();

    // Distribute packages evenly across exactly max_parallel slots
    let total_packages = packages.len();
    let base_packages_per_slot = total_packages / max_parallel;
    let extra_packages = total_packages % max_parallel;

    let mut package_index = 0;

    for slot_index in 0..max_parallel {
        if package_index >= total_packages {
            break;
        }

        // Some slots get one extra package to distribute remainder evenly
        let packages_for_this_slot = if slot_index < extra_packages {
            base_packages_per_slot + 1
        } else {
            base_packages_per_slot
        };

        if packages_for_this_slot == 0 {
            break;
        }

        let end_index = (package_index + packages_for_this_slot).min(total_packages);
        let chunk_packages = &packages[package_index..end_index];

        if chunk_packages.len() == 1 {
            // Single package, no need to combine
            result.push(chunk_packages[0].clone());
        } else {
            // Combine multiple packages while respecting chunking constraints
            let mut combined_features = Vec::new();
            let template_package = chunk_packages[0].clone();

            for package in chunk_packages {
                if let Some(features) = package.get("features").and_then(|f| f.as_array()) {
                    for feature in features {
                        if let Some(feature_str) = feature.as_str() {
                            combined_features.push(feature_str.to_string());
                        }
                    }
                }
            }

            // Remove duplicates while preserving order
            combined_features.sort();
            combined_features.dedup();

            // Respect chunking constraint if specified
            if let Some(chunk_limit) = chunked {
                let chunk_limit = chunk_limit as usize;
                if combined_features.len() > chunk_limit {
                    combined_features.truncate(chunk_limit);
                }
            }

            // Create the combined package
            let mut combined_package = template_package;
            combined_package.insert(
                "features".to_string(),
                serde_json::to_value(combined_features)?,
            );

            result.push(combined_package);
        }

        package_index = end_index;
    }

    // Sort results by package name for consistent, predictable output
    result.sort_by(|a, b| {
        let name_a = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let name_b = b.get("name").and_then(|v| v.as_str()).unwrap_or("");
        name_a.cmp(name_b)
    });

    Ok(result)
}

/// Creates a JSON map from a configuration
///
/// # Errors
///
/// * If the configuration is invalid
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub fn create_map(
    context: &WorkspaceContext,
    package_name: &str,
    conf: Option<&ClippierConf>,
    config: &ClippierConfiguration,
    file: &str,
    name: &str,
    required_features: Option<&[String]>,
    features: &[String],
) -> Result<serde_json::Map<String, serde_json::Value>, BoxError> {
    let mut visited = BTreeSet::new();
    let mut cache = BTreeMap::new();
    let propagated = collect_propagated_config(
        context,
        package_name,
        Some(&config.os),
        &mut visited,
        &mut cache,
    )
    .unwrap_or_default();

    // Get workspace config for defaults
    let workspace_conf = context.workspace_config().ok().flatten();

    let mut map = serde_json::Map::new();
    map.insert("os".to_string(), serde_json::to_value(&config.os)?);
    map.insert("path".to_string(), serde_json::to_value(file)?);
    map.insert(
        "name".to_string(),
        serde_json::to_value(config.name.as_deref().unwrap_or(name))?,
    );
    map.insert("features".to_string(), features.into());
    map.insert("requiredFeatures".to_string(), required_features.into());
    map.insert(
        "nightly".to_string(),
        config
            .nightly
            .or_else(|| conf.as_ref().and_then(|x| x.nightly))
            .or_else(|| workspace_conf.as_ref().and_then(|x| x.nightly))
            .unwrap_or_default()
            .into(),
    );

    let mut all_dependencies = propagated.dependencies.clone();
    // Merge workspace defaults
    if let Some(workspace_deps) = workspace_conf
        .as_ref()
        .and_then(|x| x.dependencies.as_ref())
    {
        all_dependencies = merge_steps(all_dependencies, workspace_deps.clone());
    }
    // Then config-specific dependencies
    if let Some(dependencies) = &config.dependencies {
        all_dependencies = merge_steps(all_dependencies, dependencies.clone());
    }

    if !all_dependencies.is_empty() {
        let dependencies = &all_dependencies;
        let matches = dependencies
            .iter()
            .filter(|x| {
                x.features.as_ref().is_none_or(|f| {
                    f.iter()
                        .any(|required| features.iter().any(|x| x == required))
                })
            })
            .collect::<Vec<_>>();

        if !matches.is_empty() {
            let dependencies = matches
                .iter()
                .filter_map(|x| x.command.as_ref())
                .map(String::as_str)
                .collect::<Vec<_>>();

            if !dependencies.is_empty() {
                map.insert(
                    "dependencies".to_string(),
                    serde_json::to_value(dependencies.join("\n"))?,
                );
            }

            let toolchains = matches
                .iter()
                .filter_map(|x| x.toolchain.as_ref())
                .map(String::as_str)
                .collect::<Vec<_>>();

            if !toolchains.is_empty() {
                map.insert(
                    "toolchains".to_string(),
                    serde_json::to_value(toolchains.join("\n"))?,
                );
            }
        }
    }

    let mut env = propagated.env.clone();
    // Merge workspace defaults first
    if let Some(workspace_env) = workspace_conf.as_ref().and_then(|x| x.env.as_ref()) {
        env.extend(workspace_env.clone());
    }
    // Then package-level config
    if let Some(conf_env) = conf.and_then(|x| x.env.as_ref()) {
        env.extend(conf_env.clone());
    }
    // Finally config-specific overrides
    env.extend(config.env.clone().unwrap_or_default());

    let matches = env
        .iter()
        .filter(|(_k, v)| match v {
            ClippierEnv::Value(..) => true,
            ClippierEnv::FilteredValue { features: f, .. } => f.as_ref().is_none_or(|f| {
                f.iter()
                    .any(|required| features.iter().any(|x| x == required))
            }),
        })
        .map(|(k, v)| {
            (
                k,
                match v {
                    ClippierEnv::Value(value) | ClippierEnv::FilteredValue { value, .. } => value,
                },
            )
        })
        .collect::<Vec<_>>();

    if !matches.is_empty() {
        map.insert(
            "env".to_string(),
            serde_json::to_value(
                matches
                    .iter()
                    .map(|(k, v)| serde_json::to_value(v).map(|v| format!("{k}={v}")))
                    .collect::<Result<Vec<_>, _>>()?
                    .join("\n"),
            )?,
        );
    }

    let mut cargo: Vec<_> = workspace_conf
        .as_ref()
        .and_then(|x| x.cargo.as_ref())
        .cloned()
        .unwrap_or_default()
        .into();
    let conf_cargo: Vec<_> = conf
        .and_then(|x| x.cargo.as_ref())
        .cloned()
        .unwrap_or_default()
        .into();
    cargo.extend(conf_cargo);
    let config_cargo: Vec<_> = config.cargo.clone().unwrap_or_default().into();
    cargo.extend(config_cargo);

    if !cargo.is_empty() {
        map.insert("cargo".to_string(), serde_json::to_value(cargo.join(" "))?);
    }

    let mut ci_steps: Vec<_> = propagated.ci_steps.clone();
    // Merge workspace defaults
    if let Some(workspace_ci_steps) = workspace_conf.as_ref().and_then(|x| x.ci_steps.as_ref()) {
        let workspace_ci_steps_vec: Vec<_> = workspace_ci_steps.clone().into();
        ci_steps = merge_steps(ci_steps, workspace_ci_steps_vec);
    }
    // Then package-level config
    if let Some(conf_ci_steps) = conf.and_then(|x| x.ci_steps.as_ref()) {
        let conf_ci_steps_vec: Vec<_> = conf_ci_steps.clone().into();
        ci_steps = merge_steps(ci_steps, conf_ci_steps_vec);
    }
    // Finally config-specific overrides
    let config_ci_steps: Vec<_> = config.ci_steps.clone().unwrap_or_default().into();
    ci_steps = merge_steps(ci_steps, config_ci_steps);

    let matches = ci_steps
        .iter()
        .filter(|x| {
            x.features.as_ref().is_none_or(|f| {
                f.iter()
                    .any(|required| features.iter().any(|x| x == required))
            })
        })
        .collect::<Vec<_>>();

    if !matches.is_empty() {
        let commands = matches
            .iter()
            .filter_map(|x| x.command.as_ref())
            .map(String::as_str)
            .collect::<Vec<_>>();

        if !commands.is_empty() {
            map.insert(
                "ciSteps".to_string(),
                serde_json::to_value(commands.join("\n"))?,
            );
        }

        let toolchains = matches
            .iter()
            .filter_map(|x| x.toolchain.as_ref())
            .map(String::as_str)
            .collect::<Vec<_>>();

        if !toolchains.is_empty() {
            map.insert(
                "ciToolchains".to_string(),
                serde_json::to_value(toolchains.join("\n"))?,
            );
        }
    }

    if let Some(git_submodules) = propagated
        .git_submodules
        .or(config.git_submodules)
        .or_else(|| conf.and_then(|x| x.git_submodules))
        .or_else(|| workspace_conf.as_ref().and_then(|x| x.git_submodules))
    {
        map.insert(
            "gitSubmodules".to_string(),
            serde_json::to_value(git_submodules)?,
        );
    }

    Ok(map)
}

/// Finds workspace dependencies for a target package
///
/// # Errors
///
/// * If the workspace root directory is not found or cannot be read
/// * If the target package is not found in the workspace
/// * If the target package has an invalid format
/// * If the target package has a syntax error
/// * If the target package has a dependency that is not a workspace dependency
/// * If the target package has a dependency that is not activated by features
/// * If the target package has a dev dependency that is not activated by features
/// * If the target package has a build dependency that is not activated by features
///
/// # Panics
///
/// * Should be infallible
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
pub fn find_workspace_dependencies(
    workspace_root: &Path,
    target_package: &str,
    enabled_features: Option<&[String]>,
    all_potential_deps: bool,
) -> Result<Vec<(String, String)>, BoxError> {
    log::trace!("🔍 Finding workspace dependencies for package: {target_package}");
    if let Some(features) = enabled_features {
        log::trace!("📋 Enabled features: {features:?}");
    } else {
        log::trace!("📋 Using default features");
    }

    let workspace_context = WorkspaceContext::new(workspace_root)?;

    // First, load the workspace and get all members
    let workspace_cargo_path = workspace_root.join("Cargo.toml");
    log::trace!(
        "📂 Loading workspace from: {}",
        workspace_cargo_path.display()
    );
    let workspace_source = switchy_fs::sync::read_to_string(&workspace_cargo_path)?;
    let workspace_value: Value = toml::from_str(&workspace_source)?;

    let workspace_members = workspace_value
        .get("workspace")
        .and_then(|x| x.get("members"))
        .and_then(|x| x.as_array())
        .and_then(|x| x.iter().map(|x| x.as_str()).collect::<Option<Vec<_>>>())
        .ok_or("No workspace members found")?;

    log::trace!("🏢 Found {} workspace members", workspace_members.len());

    // Create a map of package name -> package path for all workspace members
    let mut package_paths = BTreeMap::new();
    let mut package_dependencies: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut package_cargo_values: BTreeMap<String, Value> = BTreeMap::new();

    for member_path in workspace_members {
        let full_path = workspace_root.join(member_path);
        let cargo_path = full_path.join("Cargo.toml");

        if !switchy_fs::exists(&cargo_path) {
            log::trace!("⚠️  Skipping {member_path}: Cargo.toml not found");
            continue;
        }

        log::trace!("📄 Processing package: {member_path}");
        let source = switchy_fs::sync::read_to_string(&cargo_path)?;
        let value: Value = toml::from_str(&source)?;

        // Get package name
        if let Some(package_name) = value
            .get("package")
            .and_then(|x| x.get("name"))
            .and_then(|x| x.as_str())
        {
            log::trace!("📦 Package name: {package_name} -> {member_path}");
            package_paths.insert(package_name.to_string(), member_path.to_string());
            package_cargo_values.insert(package_name.to_string(), value.clone());

            // Extract dependencies that are workspace members - we'll resolve them later
            let deps =
                extract_workspace_dependencies(&value, &workspace_context, all_potential_deps);
            log::trace!("📊 Direct dependencies for {package_name}: {deps:?}");
            package_dependencies.insert(package_name.to_string(), deps);
        }
    }

    if !package_paths.contains_key(target_package) {
        return Err(format!("Package '{target_package}' not found in workspace").into());
    }

    log::trace!(
        "🚀 Starting recursive dependency resolution from target package: {target_package}"
    );

    // Perform recursive dependency resolution to find all transitive dependencies
    let mut resolved_dependencies = BTreeSet::new();
    let mut processing_queue = VecDeque::new();
    let mut visited = BTreeSet::new();

    processing_queue.push_back((
        target_package.to_string(),
        enabled_features.map(<[String]>::to_vec),
    ));

    while let Some((current_package, current_features)) = processing_queue.pop_front() {
        if visited.contains(&current_package) {
            continue;
        }
        visited.insert(current_package.clone());

        // Add current package to result if it's not the target package
        if current_package != target_package
            && let Some(package_path) = package_paths.get(&current_package)
        {
            resolved_dependencies.insert((current_package.clone(), package_path.clone()));
        }

        // Get dependencies for current package
        if let Some(direct_deps) = package_dependencies.get(&current_package) {
            // For each direct dependency, check if it's activated and add to queue
            for dep_name in direct_deps {
                if !visited.contains(dep_name) && package_paths.contains_key(dep_name) {
                    // Check if this dependency is activated by current features
                    let is_activated = if all_potential_deps {
                        true // Include all in potential mode
                    } else {
                        is_dependency_activated(
                            &package_cargo_values,
                            &current_package,
                            dep_name,
                            current_features.as_deref(),
                        )
                    };

                    if is_activated {
                        log::trace!("  ✅ Adding activated dependency: {dep_name}");
                        // For the dependency, we need to determine what features to enable
                        // For now, use default features (empty feature set)
                        processing_queue.push_back((dep_name.clone(), None));
                    } else {
                        log::trace!("  ⏸️  Skipping dependency (not activated): {dep_name}");
                    }
                }
            }
        }
    }

    let mut result_paths: Vec<(String, String)> = resolved_dependencies.into_iter().collect();
    result_paths.sort_by(|a, b| a.0.cmp(&b.0));
    log::trace!("🏁 Final workspace dependencies: {result_paths:?}");

    Ok(result_paths)
}

/// Extracts all workspace dependencies from a Cargo.toml value
fn extract_workspace_dependencies(
    cargo_value: &Value,
    context: &WorkspaceContext,
    all_potential_deps: bool,
) -> Vec<String> {
    let filter = |dep: &DependencyInfo| -> bool {
        if dep.kind != DependencyKind::WorkspaceReference {
            return false;
        }

        if all_potential_deps {
            return true;
        }

        if dep.is_optional {
            is_optional_dependency_activated(cargo_value, dep.name, None)
        } else {
            true
        }
    };

    extract_dependencies(cargo_value, context, &context.root, filter)
}

/// Checks if a dependency is activated by the given features
fn is_dependency_activated(
    package_cargo_values: &BTreeMap<String, Value>,
    package_name: &str,
    dep_name: &str,
    enabled_features: Option<&[String]>,
) -> bool {
    let Some(cargo_value) = package_cargo_values.get(package_name) else {
        return false;
    };

    // Check if dependency is in regular dependencies and not optional
    if let Some(dependencies) = cargo_value.get("dependencies").and_then(|d| d.as_table())
        && let Some(dep_value) = dependencies.get(dep_name)
        && is_workspace_dependency(dep_value)
    {
        // Check if it's optional
        if let Value::Table(table) = dep_value
            && table.get("optional") == Some(&Value::Boolean(true))
        {
            // Optional dependency - check if activated by features
            return is_optional_dependency_activated(cargo_value, dep_name, enabled_features);
        }
        // Non-optional workspace dependency is always activated
        return true;
    }

    // Check dev and build dependencies
    for section_name in ["dev-dependencies", "build-dependencies"] {
        if let Some(section) = cargo_value.get(section_name).and_then(|d| d.as_table())
            && let Some(dep_value) = section.get(dep_name)
            && is_workspace_dependency(dep_value)
        {
            return true; // Dev and build deps are typically always enabled
        }
    }

    false
}

/// Checks if an optional dependency is activated by features
fn is_optional_dependency_activated(
    cargo_value: &Value,
    dep_name: &str,
    enabled_features: Option<&[String]>,
) -> bool {
    let Some(features_table) = cargo_value.get("features").and_then(|f| f.as_table()) else {
        return false;
    };

    // Get the features to check - use default features if none specified
    let Some(features_to_check) = enabled_features else {
        // Get default features
        if let Some(default_features) = features_table.get("default").and_then(|f| f.as_array()) {
            let default_feature_names: Vec<String> = default_features
                .iter()
                .filter_map(|v| v.as_str())
                .map(ToString::to_string)
                .collect();
            return check_features_activate_dependency(
                features_table,
                &default_feature_names,
                dep_name,
            );
        }

        return false;
    };

    check_features_activate_dependency(features_table, features_to_check, dep_name)
}

/// Helper function to check if any of the given features activate a dependency
fn check_features_activate_dependency(
    features_table: &toml::map::Map<String, Value>,
    features_to_check: &[String],
    dep_name: &str,
) -> bool {
    // Check if any enabled feature activates this dependency
    for feature_name in features_to_check {
        if let Some(feature_list) = features_table.get(feature_name).and_then(|f| f.as_array()) {
            for feature_item in feature_list {
                if let Some(feature_str) = feature_item.as_str() {
                    // Check for "dep:package_name" syntax
                    if feature_str == format!("dep:{dep_name}") {
                        return true;
                    }
                    // Check for "package_name/feature" syntax
                    if feature_str.starts_with(&format!("{dep_name}/")) {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Extracts feature-activated dependencies from a Cargo.toml file
#[must_use]
pub fn get_feature_dependencies(
    cargo_toml: &Value,
    enabled_features: &BTreeSet<String>,
) -> Vec<String> {
    let mut feature_deps = Vec::new();

    if let Some(features_table) = cargo_toml.get("features").and_then(|f| f.as_table()) {
        for feature_name in enabled_features {
            if let Some(feature_list) = features_table.get(feature_name).and_then(|f| f.as_array())
            {
                for feature_item in feature_list {
                    if let Some(feature_str) = feature_item.as_str() {
                        // Check if this is a dependency feature (contains a slash)
                        if feature_str.contains('/') {
                            let parts: Vec<&str> = feature_str.split('/').collect();
                            if parts.len() == 2 {
                                let dep_name = parts[0];
                                // Only add if it's not already in the list
                                if !feature_deps.contains(&dep_name.to_string()) {
                                    feature_deps.push(dep_name.to_string());
                                }
                            }
                        } else if feature_str.starts_with("dep:") {
                            // Handle "dep:package_name" syntax
                            let dep_name = feature_str.strip_prefix("dep:").unwrap_or(feature_str);
                            if !feature_deps.contains(&dep_name.to_string()) {
                                feature_deps.push(dep_name.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    feature_deps
}

/// Generates a Dockerfile for a target package
///
/// # Errors
///
/// * If fails to find the workspace dependencies
/// * If fails to generate the dockerfile content
/// * If fails to write the dockerfile to the specified path
#[allow(clippy::too_many_arguments)]
pub async fn generate_dockerfile(
    workspace_root: &Path,
    target_package: &str,
    enabled_features: Option<&[String]>,
    no_default_features: bool,
    dockerfile_path: &Path,
    base_image: &str,
    final_image: &str,
    args: &[String],
    build_args: Option<&str>,
    generate_dockerignore: bool,
    custom_env_vars: &[String],
    build_env_vars: &[String],
    bin: Option<&str>,
) -> Result<(), BoxError> {
    // Get all potential dependencies for the target package (needed for Docker build compatibility)
    // Docker builds require all possible dependencies to ensure proper layer caching
    let mut dependencies =
        find_workspace_dependencies(workspace_root, target_package, enabled_features, true)?;

    // Add the target package itself to the dependencies list if not already present
    let default_target_path = format!(
        "packages/{}",
        target_package
            .strip_prefix("moosicbox_")
            .unwrap_or(target_package)
    );
    if !dependencies.iter().any(|(name, _)| name == target_package) {
        dependencies.push((target_package.to_string(), default_target_path.clone()));
    }

    // Get target package path
    let target_package_path = dependencies
        .iter()
        .find(|(name, _)| name == target_package)
        .map_or_else(|| default_target_path.as_str(), |(_, path)| path.as_str());

    // Create the Dockerfile content
    let dockerfile_content = generate_dockerfile_content(
        &dependencies,
        target_package,
        enabled_features,
        no_default_features,
        base_image,
        final_image,
        args,
        build_args,
        workspace_root,
        target_package_path,
        custom_env_vars,
        build_env_vars,
        bin,
    )
    .await?;

    // Write the Dockerfile
    switchy_fs::sync::write(dockerfile_path, dockerfile_content)?;

    if generate_dockerignore {
        let dockerignore_content =
            generate_dockerignore_content(&dependencies, target_package, enabled_features)?;
        let dockerignore_path = dockerfile_path.with_extension("dockerignore");
        switchy_fs::sync::write(dockerignore_path, dockerignore_content)?;
    }

    Ok(())
}

/// Generates a Dockerfile for a target package from a git URL
///
/// # Errors
///
/// * If fails to generate the dockerfile content
/// * If fails to write the dockerfile to the specified path
#[allow(clippy::too_many_arguments)]
pub fn generate_dockerfile_from_git(
    git_url: &str,
    git_ref: &str,
    target_package: &str,
    enabled_features: Option<&[String]>,
    no_default_features: bool,
    dockerfile_path: &Path,
    base_image: &str,
    final_image: &str,
    args: &[String],
    build_args: Option<&str>,
    generate_dockerignore: bool,
    custom_env_vars: &[String],
    build_env_vars: &[String],
    bin: Option<&str>,
) -> Result<(), BoxError> {
    // Create the Dockerfile content
    let dockerfile_content = generate_dockerfile_content_from_git(
        git_url,
        git_ref,
        target_package,
        enabled_features,
        no_default_features,
        base_image,
        final_image,
        args,
        build_args,
        custom_env_vars,
        build_env_vars,
        bin,
    )?;

    // Write the Dockerfile
    switchy_fs::sync::write(dockerfile_path, dockerfile_content)?;

    if generate_dockerignore {
        // For git mode, create a minimal dockerignore
        let dockerignore_content = generate_dockerignore_content_for_git()?;
        let dockerignore_path = dockerfile_path.with_extension("dockerignore");
        switchy_fs::sync::write(dockerignore_path, dockerignore_content)?;
    }

    Ok(())
}

/// Generates Dockerfile content for a package built from a git repository.
///
/// Creates a multi-stage Dockerfile that clones the repository, builds the specified
/// package with the given features, and packages it into a minimal final image.
///
/// # Arguments
///
/// * `git_url` - URL of the git repository to clone
/// * `git_ref` - Git reference (branch, tag, or commit) to checkout
/// * `target_package` - Name of the workspace package to build
/// * `enabled_features` - Optional list of features to enable
/// * `no_default_features` - Whether to disable default features
/// * `base_image` - Docker base image for the build stage
/// * `final_image` - Docker image for the final runtime stage
/// * `args` - Additional arguments to pass to the binary
/// * `custom_env_vars` - Environment variables for runtime
/// * `build_env_vars` - Environment variables for build stage
/// * `bin` - Optional binary name (if different from package name)
///
/// # Errors
///
/// * `std::fmt::Error` - If writing to the string fails
#[allow(
    clippy::too_many_lines,
    clippy::too_many_arguments,
    clippy::cognitive_complexity
)]
pub fn generate_dockerfile_content_from_git(
    git_url: &str,
    git_ref: &str,
    target_package: &str,
    enabled_features: Option<&[String]>,
    no_default_features: bool,
    base_image: &str,
    final_image: &str,
    args: &[String],
    _build_args: Option<&str>,
    custom_env_vars: &[String],
    build_env_vars: &[String],
    bin: Option<&str>,
) -> Result<String, BoxError> {
    use std::fmt::Write as _;

    let mut content = String::new();

    // Builder stage
    writeln!(content, "# Builder")?;
    writeln!(content, "FROM {base_image} AS builder")?;
    writeln!(content, "WORKDIR /app\n")?;

    // System dependencies
    writeln!(content, "# Install system dependencies")?;
    writeln!(content, "RUN apt-get update && \\")?;
    writeln!(
        content,
        "    apt-get install -y git build-essential cmake pkg-config && \\"
    )?;
    writeln!(content, "    rm -rf /var/lib/apt/lists/*\n")?;

    // Set build-time environment variables
    if !build_env_vars.is_empty() {
        writeln!(content, "# Set build-time environment variables")?;
        for env_var in build_env_vars {
            if let Some((key, value)) = env_var.split_once('=') {
                writeln!(content, "ENV {key}={value}")?;
            }
        }
        content.push('\n');
    }

    // Hardcoded git clone
    writeln!(content, "# Clone specific repository and ref")?;
    writeln!(
        content,
        "RUN git clone --depth 1 --branch {git_ref} {git_url} . || \\"
    )?;
    writeln!(
        content,
        "    (git clone --filter=blob:none --no-checkout {git_url} . && \\"
    )?;
    writeln!(content, "     git checkout {git_ref})\n")?;

    // Remove git directory
    writeln!(content, "# Remove .git directory to save space")?;
    writeln!(content, "RUN rm -rf .git\n")?;

    // Build dependencies first
    writeln!(content, "# Build dependencies first (better caching)")?;
    writeln!(content, "RUN cargo fetch\n")?;

    // Build the package
    writeln!(content, "# Build the specific package")?;
    let mut build_cmd = format!("RUN cargo build --release --package {target_package}");

    if no_default_features {
        build_cmd.push_str(" --no-default-features");
    }

    if let Some(features) = enabled_features
        && !features.is_empty()
    {
        use std::fmt::Write as _;
        write!(build_cmd, " --features=\"{}\"", features.join(","))?;
    }

    writeln!(content, "{build_cmd}\n")?;

    // Runtime stage
    writeln!(content, "# Runtime")?;
    writeln!(content, "FROM {final_image}")?;
    writeln!(content, "WORKDIR /")?;
    writeln!(
        content,
        "RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*\n"
    )?;

    // Get binary name - use override if provided, otherwise use package name conversion
    let binary_name = bin.map_or_else(|| target_package.replace('-', "_"), ToString::to_string);
    writeln!(content, "# Copy the built binary")?;
    writeln!(
        content,
        "COPY --from=builder /app/target/release/{binary_name} /\n"
    )?;

    // Environment variables
    writeln!(content, "# Set runtime environment")?;
    writeln!(
        content,
        "ENV RUST_LOG=info,moosicbox=debug,moosicbox_middleware::api_logger=trace"
    )?;

    // Custom environment variables
    for env_var in custom_env_vars {
        if let Some((key, value)) = env_var.split_once('=') {
            writeln!(content, "ENV {key}={value}")?;
        }
    }

    // Final command
    if args.is_empty() {
        writeln!(content, "\n# Run the binary")?;
        writeln!(content, "CMD [\"./{binary_name}\"]")?;
    } else {
        let args_json = args
            .iter()
            .map(|arg| format!("\"{arg}\""))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(content, "\n# Run the binary with args")?;
        writeln!(content, "CMD [\"./{binary_name}\", {args_json}]")?;
    }

    Ok(content)
}

/// Generates `.dockerignore` file content for git-based builds.
///
/// Creates a minimal dockerignore file optimized for git-based Docker builds,
/// excluding only essential directories (.git, target) and build artifacts.
///
/// # Errors
///
/// * `std::fmt::Error` - If writing to the string fails
pub fn generate_dockerignore_content_for_git() -> Result<String, BoxError> {
    use std::fmt::Write as _;

    let mut content = String::new();

    // For git mode, we don't need to exclude much since everything is cloned fresh
    writeln!(content, "# Git mode dockerignore - minimal exclusions")?;
    writeln!(content, ".git")?;
    writeln!(content, "target/")?;
    writeln!(content, "*.dockerfile")?;
    writeln!(content, "*.dockerignore")?;

    Ok(content)
}

/// Generates Dockerfile content for a workspace package.
///
/// Creates an optimized multi-stage Dockerfile that builds only the specified package
/// and its workspace dependencies, packaging the result into a minimal final image.
///
/// # Arguments
///
/// * `dependencies` - List of (`package_name`, `package_path`) tuples for workspace dependencies
/// * `target_package` - Name of the workspace package to build
/// * `enabled_features` - Optional list of features to enable
/// * `no_default_features` - Whether to disable default features
/// * `base_image` - Docker base image for the build stage
/// * `final_image` - Docker image for the final runtime stage
/// * `args` - Additional arguments to pass to the binary
/// * `build_args` - Optional cargo build arguments
/// * `custom_env_vars` - Environment variables for runtime
/// * `build_env_vars` - Environment variables for build stage
/// * `bin` - Optional binary name (if different from package name)
///
/// # Errors
///
/// * `std::fmt::Error` - If writing to the string fails
#[allow(
    clippy::too_many_lines,
    clippy::too_many_arguments,
    clippy::cognitive_complexity
)]
pub async fn generate_dockerfile_content(
    dependencies: &[(String, String)],
    target_package: &str,
    enabled_features: Option<&[String]>,
    no_default_features: bool,
    base_image: &str,
    final_image: &str,
    args: &[String],
    build_args: Option<&str>,
    workspace_root: &Path,
    target_package_path: &str,
    custom_env_vars: &[String],
    build_env_vars: &[String],
    bin: Option<&str>,
) -> Result<String, BoxError> {
    use std::fmt::Write as _;

    let mut content = String::new();

    // Collect environment variables for the target package early
    let env_vars = collect_environment_variables(
        workspace_root,
        target_package,
        target_package_path,
        enabled_features,
        "ubuntu",
    )
    .await?;

    // Builder stage
    writeln!(
        content,
        "# Builder\nFROM {base_image} AS builder\nWORKDIR /app\n"
    )?;

    // APT configuration for faster downloads (early in build for caching)
    writeln!(content, "# APT configuration for faster downloads")?;
    content.push_str(
        "RUN echo 'Acquire::http::Timeout \"10\";' >>/etc/apt/apt.conf.d/httpproxy && \\\n",
    );
    writeln!(
        content,
        "  echo 'Acquire::ftp::Timeout \"10\";' >>/etc/apt/apt.conf.d/httpproxy\n"
    )?;

    // Collect and install system dependencies early for better caching
    let system_deps =
        collect_system_dependencies(workspace_root, dependencies, enabled_features, "ubuntu")
            .await?;

    // Always ensure essential build tools are available
    writeln!(
        content,
        "# Install system dependencies (early for better Docker layer caching)"
    )?;
    writeln!(content, "RUN apt-get update && \\")?;

    // Parse and consolidate apt-get install commands
    let mut install_packages = BTreeSet::new();
    let mut custom_commands = Vec::new();

    // Add essential build dependencies that are always needed
    install_packages.insert("build-essential".to_string());
    install_packages.insert("cmake".to_string());
    install_packages.insert("pkg-config".to_string());

    for dep in &system_deps {
        if dep.contains("apt-get install") {
            // Extract package names from apt-get install commands
            if let Some(packages_part) = dep.split("apt-get install").nth(1) {
                for package in packages_part.split_whitespace() {
                    if !package.is_empty() && !package.starts_with('-') {
                        install_packages.insert(package.to_string());
                    }
                }
            }
        } else if !dep.contains("apt-get update") {
            // Keep other custom commands
            custom_commands.push(dep);
        }
    }

    // Install all packages in one command
    if !install_packages.is_empty() {
        use std::fmt::Write as _;
        let mut packages: Vec<String> = install_packages.into_iter().collect();
        packages.sort();
        write!(content, "    apt-get -y install {}", packages.join(" "))?;
        if custom_commands.is_empty() {
            content.push_str("\n\n");
        } else {
            content.push_str(" && \\\n");
        }
    }

    // Add custom commands
    for (i, cmd) in custom_commands.iter().enumerate() {
        if cmd.starts_with("sudo ") {
            // Remove sudo since we're already running as root in Docker
            let cmd_without_sudo = cmd.strip_prefix("sudo ").unwrap_or(cmd);
            write!(content, "    {cmd_without_sudo}")?;
        } else {
            write!(content, "    {cmd}")?;
        }

        if i < custom_commands.len() - 1 {
            content.push_str(" && \\\n");
        } else {
            content.push_str("\n\n");
        }
    }

    // Set build-time environment variables
    if !build_env_vars.is_empty() {
        writeln!(content, "# Set build-time environment variables")?;
        for env_var in build_env_vars {
            if let Some((key, value)) = env_var.split_once('=') {
                writeln!(content, "ENV {key}={value}")?;
            }
        }
        content.push('\n');
    }

    // Copy workspace manifest files
    writeln!(
        content,
        "COPY Cargo.toml Cargo.toml\nCOPY Cargo.lock Cargo.lock\n"
    )?;

    // Generate workspace members list - create a simple list of quoted package names
    let members_list = dependencies
        .iter()
        .map(|(_, path)| format!("\"{path}\""))
        .collect::<Vec<_>>()
        .join(", ");

    // Modify Cargo.toml to include only needed packages using multi-line sed
    writeln!(
        content,
        "RUN sed -e '/^members = \\[/,/^\\]/c\\members = [{members_list}]' Cargo.toml > Cargo2.toml && mv Cargo2.toml Cargo.toml\n"
    )?;

    // Copy packages folder (dockerignore will filter out irrelevant packages)
    writeln!(content, "# Copy packages folder for Cargo.toml files")?;
    writeln!(content, "COPY packages/ packages/")?;

    // Remove source files, keeping only Cargo.toml and build.rs files for caching
    writeln!(
        content,
        "RUN find packages/ -name '*.rs' ! -name 'build.rs' -delete"
    )?;

    content.push('\n');

    // Copy real source code for all packages (needed for dependency build)
    writeln!(content, "# Copy real source code for building dependencies")?;
    writeln!(content, "COPY packages/ packages/")?;

    // Create stub for target package only to prevent it from being built during dependency phase
    writeln!(
        content,
        "# Create stub for target package to prevent premature build"
    )?;
    let target_cargo_path = workspace_root.join(target_package_path).join("Cargo.toml");
    if switchy_fs::exists(&target_cargo_path) {
        let target_source = switchy_fs::sync::read_to_string(&target_cargo_path)?;
        let target_value: Value = toml::from_str(&target_source)?;

        // Check if this package has a binary target
        let has_binary = target_value.get("bin").is_some()
            || switchy_fs::exists(workspace_root.join(target_package_path).join("src/main.rs"));

        if has_binary {
            writeln!(
                content,
                "RUN echo 'fn main() {{}}' > {target_package_path}/src/main.rs"
            )?;
        } else {
            // Create lib.rs stub for target package if it's a library
            writeln!(content, "RUN echo '' > {target_package_path}/src/lib.rs")?;
        }
    }

    content.push('\n');

    // Build feature flags
    let mut feature_flags = Vec::new();

    if no_default_features {
        feature_flags.push("--no-default-features".to_string());
    }

    if let Some(features) = enabled_features
        && !features.is_empty()
    {
        feature_flags.push(format!("--features={}", features.join(",")));
    }

    let features_flag = feature_flags.join(" ");

    // Build only dependencies first (not the target package)
    // This allows Docker to cache the dependency compilation layer
    writeln!(content, "# Build dependencies first (not target package)")?;

    // Build all workspace packages except the target package
    // This handles interdependencies between workspace packages correctly
    writeln!(
        content,
        "RUN cargo build --release --workspace --exclude {target_package}"
    )?;

    // Copy target package source code for final build
    writeln!(content, "\n# Copy target package source code")?;
    writeln!(
        content,
        "COPY {target_package_path}/ {target_package_path}/"
    )?;

    // Add environment variables for build-time access (like std::env! macro)
    if !env_vars.is_empty() {
        writeln!(
            content,
            "\n# Accept build args and set as env vars for build process"
        )?;
        for (key, _) in &env_vars {
            writeln!(content, "ARG {key}")?;
            writeln!(content, "ENV {key}=${{{key}}}")?;
        }
    }

    // Final build with actual source code
    writeln!(content, "\n# Final build with actual source")?;
    if features_flag.is_empty() {
        writeln!(
            content,
            "RUN cargo build --release --package {target_package}"
        )?;
    } else {
        writeln!(
            content,
            "RUN cargo build --release --package {target_package} {features_flag}"
        )?;
    }

    // Runtime stage
    writeln!(content, "\n# Runtime")?;
    writeln!(content, "FROM {final_image}")?;
    writeln!(content, "WORKDIR /")?;

    // Install runtime dependencies if needed
    let runtime_packages = vec!["ca-certificates"];
    let runtime_packages_vec: Vec<&str> = runtime_packages.into_iter().collect();
    writeln!(
        content,
        "RUN apt-get update && apt-get install -y {}",
        runtime_packages_vec.join(" ")
    )?;

    // Copy binary from builder
    let binary_name = get_binary_name(workspace_root, target_package, target_package_path, bin);
    writeln!(
        content,
        "COPY --from=builder /app/target/release/{binary_name} /"
    )?;

    // Runtime environment
    if let Some(args) = build_args {
        for arg in args.split(',') {
            let arg = arg.trim();
            writeln!(content, "ARG {arg}\nENV {arg}=${{{arg}}}")?;
        }
    }

    // Add package-specific environment variables at runtime
    if !env_vars.is_empty() {
        for (key, _) in &env_vars {
            writeln!(content, "ARG {key}")?;
            writeln!(content, "ENV {key}=${{{key}}}")?;
        }
    }

    writeln!(
        content,
        "ENV RUST_LOG=info,moosicbox=debug,moosicbox_middleware::api_logger=trace"
    )?;

    // Add custom environment variables if provided
    for env_var in custom_env_vars {
        if let Some((key, value)) = env_var.split_once('=') {
            writeln!(content, "ENV {key}={value}")?;
        }
    }

    // Final command
    if args.is_empty() {
        writeln!(content, "CMD [\"./{binary_name}\"]")?;
    } else {
        let args_json = args
            .iter()
            .map(|arg| format!("\"{arg}\""))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(content, "CMD [\"./{binary_name}\", {args_json}]")?;
    }

    Ok(content)
}

/// Generates the content of a .dockerignore file for a target package
///
/// # Errors
///
/// * If IO error occurs
pub fn generate_dockerignore_content(
    dependencies: &[(String, String)],
    _target_package: &str,
    _enabled_features: Option<&[String]>,
) -> Result<String, BoxError> {
    use std::fmt::Write as _;

    let mut content = String::new();

    // Exclude all packages first
    writeln!(content, "/packages/*\n")?;

    // Include only required packages
    for (_, path) in dependencies {
        writeln!(content, "!/{path}")?;
    }

    content.push('\n');

    Ok(content)
}

/// Checks if a file should be ignored based on glob patterns
///
/// Handles negation patterns (!) similar to GitHub Actions path filters.
/// Patterns are evaluated in order, with later patterns overriding earlier ones.
///
/// # Errors
///
/// * If glob pattern compilation fails
fn should_ignore_file(file_path: &str, ignore_patterns: &[String]) -> Result<bool, BoxError> {
    if ignore_patterns.is_empty() {
        return Ok(false);
    }

    let mut ignored = false;

    for pattern in ignore_patterns {
        let (is_negation, pattern_str) = pattern
            .strip_prefix('!')
            .map_or((false, pattern.as_str()), |p| (true, p));

        let glob = globset::Glob::new(pattern_str)?;
        if glob.compile_matcher().is_match(file_path) {
            ignored = !is_negation;
        }
    }

    Ok(ignored)
}

/// Finds packages that are affected by changed files
///
/// # Errors
///
/// * If IO error occurs
/// * If no workspace members are found
/// * If ignore pattern compilation fails
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
pub fn find_affected_packages(
    workspace_root: &Path,
    changed_files: &[String],
    ignore_patterns: &[String],
) -> Result<Vec<String>, BoxError> {
    log::trace!("🔍 Finding affected packages for changed files: {changed_files:?}");

    // First, load the workspace and get all members
    let workspace_cargo_path = workspace_root.join("Cargo.toml");
    let workspace_source = switchy_fs::sync::read_to_string(&workspace_cargo_path)?;
    let workspace_value: Value = toml::from_str(&workspace_source)?;

    let workspace_members = workspace_value
        .get("workspace")
        .and_then(|x| x.get("members"))
        .and_then(|x| x.as_array())
        .and_then(|x| x.iter().map(|x| x.as_str()).collect::<Option<Vec<_>>>())
        .ok_or("No workspace members found")?;

    log::trace!("🏢 Found {} workspace members", workspace_members.len());

    // Create a map of package name -> package path and package_path -> package name
    let mut package_path_to_name = BTreeMap::new();
    let mut package_dependencies: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for member_path in workspace_members {
        let full_path = workspace_root.join(member_path);
        let cargo_path = full_path.join("Cargo.toml");

        if !switchy_fs::exists(&cargo_path) {
            log::trace!("⚠️  Skipping {member_path}: Cargo.toml not found");
            continue;
        }

        log::trace!("📄 Processing package: {member_path}");
        let source = switchy_fs::sync::read_to_string(&cargo_path)?;
        let value: Value = toml::from_str(&source)?;

        // Get package name
        if let Some(package_name) = value
            .get("package")
            .and_then(|x| x.get("name"))
            .and_then(|x| x.as_str())
        {
            log::trace!("📦 Package name: {package_name} -> {member_path}");
            package_path_to_name.insert(member_path.to_string(), package_name.to_string());

            // Extract dependencies that are workspace members
            let mut deps = Vec::new();

            // Check regular dependencies
            if let Some(dependencies) = value.get("dependencies").and_then(|x| x.as_table()) {
                for (dep_name, dep_value) in dependencies {
                    if is_workspace_dependency(dep_value) {
                        deps.push(dep_name.clone());
                    }
                }
            }

            // Check dev dependencies
            if let Some(dev_dependencies) = value.get("dev-dependencies").and_then(|x| x.as_table())
            {
                for (dep_name, dep_value) in dev_dependencies {
                    if is_workspace_dependency(dep_value) && !deps.contains(dep_name) {
                        deps.push(dep_name.clone());
                    }
                }
            }

            // Check build dependencies
            if let Some(build_dependencies) =
                value.get("build-dependencies").and_then(|x| x.as_table())
            {
                for (dep_name, dep_value) in build_dependencies {
                    if is_workspace_dependency(dep_value) && !deps.contains(dep_name) {
                        deps.push(dep_name.clone());
                    }
                }
            }

            log::trace!("📊 Dependencies for {package_name}: {deps:?}");
            package_dependencies.insert(package_name.to_string(), deps);
        }
    }

    // Find packages directly affected by changed files
    let mut directly_affected_packages = BTreeSet::new();

    for changed_file in changed_files {
        if should_ignore_file(changed_file, ignore_patterns)? {
            log::trace!("🚫 Ignoring file (matched pattern): {changed_file}");
            continue;
        }

        let changed_path = std::path::PathBuf::from(changed_file);

        // Find the most specific (longest) matching package path for this changed file
        // This prevents nested packages from incorrectly affecting their parent packages
        let mut best_match: Option<(&String, &String)> = None;
        let mut best_match_length = 0;

        for (package_path, package_name) in &package_path_to_name {
            let package_path_buf = std::path::PathBuf::from(package_path);

            // Check if the changed file is within this package's directory
            if changed_path.starts_with(&package_path_buf) {
                let path_length = package_path.len();
                // Only update if this is a longer (more specific) match
                if path_length > best_match_length {
                    best_match = Some((package_path, package_name));
                    best_match_length = path_length;
                }
            }
        }

        // Only add the most specific match to avoid nested package false positives
        if let Some((package_path, package_name)) = best_match {
            log::trace!(
                "📝 File {changed_file} affects package {package_name} (path: {package_path})"
            );
            directly_affected_packages.insert(package_name.clone());
        }
    }

    log::trace!("🎯 Directly affected packages: {directly_affected_packages:?}");

    // Now find all packages that depend on the directly affected packages (transitive dependencies)
    let mut all_affected_packages = directly_affected_packages.clone();
    let mut queue = VecDeque::new();

    // Add all directly affected packages to the queue
    for package in &directly_affected_packages {
        queue.push_back(package.clone());
    }

    // Build reverse dependency map (package -> packages that depend on it)
    let mut reverse_deps: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (package, deps) in &package_dependencies {
        for dep in deps {
            reverse_deps
                .entry(dep.clone())
                .or_default()
                .push(package.clone());
        }
    }

    // Process the queue to find all transitive dependents
    while let Some(current_package) = queue.pop_front() {
        if let Some(dependents) = reverse_deps.get(&current_package) {
            for dependent in dependents {
                if !all_affected_packages.contains(dependent) {
                    log::trace!(
                        "🔄 Package {dependent} depends on affected package {current_package}"
                    );
                    all_affected_packages.insert(dependent.clone());
                    queue.push_back(dependent.clone());
                }
            }
        }
    }

    let mut result: Vec<String> = all_affected_packages.into_iter().collect();
    result.sort();

    log::trace!("🏁 Final affected packages: {result:?}");

    Ok(result)
}

/// Finds packages that are affected by changed files with reasoning
///
/// # Errors
///
/// * If IO error occurs
/// * If no workspace members are found
/// * If ignore pattern compilation fails
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
pub fn find_affected_packages_with_reasoning(
    workspace_root: &Path,
    changed_files: &[String],
    ignore_patterns: &[String],
) -> Result<Vec<AffectedPackageInfo>, BoxError> {
    log::trace!("🔍 Finding affected packages with reasoning for changed files: {changed_files:?}");

    // First, load the workspace and get all members
    let workspace_cargo_path = workspace_root.join("Cargo.toml");
    let workspace_source = switchy_fs::sync::read_to_string(&workspace_cargo_path)?;
    let workspace_value: Value = toml::from_str(&workspace_source)?;

    let workspace_members = workspace_value
        .get("workspace")
        .and_then(|x| x.get("members"))
        .and_then(|x| x.as_array())
        .and_then(|x| x.iter().map(|x| x.as_str()).collect::<Option<Vec<_>>>())
        .ok_or("No workspace members found")?;

    log::trace!("🏢 Found {} workspace members", workspace_members.len());

    // Create a map of package name -> package path and package_path -> package name
    let mut package_path_to_name = BTreeMap::new();
    let mut package_dependencies: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for member_path in workspace_members {
        let full_path = workspace_root.join(member_path);
        let cargo_path = full_path.join("Cargo.toml");

        if !switchy_fs::exists(&cargo_path) {
            log::trace!("⚠️  Skipping {member_path}: Cargo.toml not found");
            continue;
        }

        log::trace!("📄 Processing package: {member_path}");
        let source = switchy_fs::sync::read_to_string(&cargo_path)?;
        let value: Value = toml::from_str(&source)?;

        // Get package name
        if let Some(package_name) = value
            .get("package")
            .and_then(|x| x.get("name"))
            .and_then(|x| x.as_str())
        {
            log::trace!("📦 Package name: {package_name} -> {member_path}");
            package_path_to_name.insert(member_path.to_string(), package_name.to_string());

            // Extract dependencies that are workspace members
            let mut deps = Vec::new();

            // Check regular dependencies
            if let Some(dependencies) = value.get("dependencies").and_then(|x| x.as_table()) {
                for (dep_name, dep_value) in dependencies {
                    if is_workspace_dependency(dep_value) {
                        deps.push(dep_name.clone());
                    }
                }
            }

            // Check dev dependencies
            if let Some(dev_dependencies) = value.get("dev-dependencies").and_then(|x| x.as_table())
            {
                for (dep_name, dep_value) in dev_dependencies {
                    if is_workspace_dependency(dep_value) && !deps.contains(dep_name) {
                        deps.push(dep_name.clone());
                    }
                }
            }

            // Check build dependencies
            if let Some(build_dependencies) =
                value.get("build-dependencies").and_then(|x| x.as_table())
            {
                for (dep_name, dep_value) in build_dependencies {
                    if is_workspace_dependency(dep_value) && !deps.contains(dep_name) {
                        deps.push(dep_name.clone());
                    }
                }
            }

            log::trace!("📊 Dependencies for {package_name}: {deps:?}");
            package_dependencies.insert(package_name.to_string(), deps);
        }
    }

    // Find packages directly affected by changed files
    let mut directly_affected_packages = BTreeMap::new(); // package name -> list of changed files
    let mut reasoning_map: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for changed_file in changed_files {
        if should_ignore_file(changed_file, ignore_patterns)? {
            log::trace!("🚫 Ignoring file (matched pattern): {changed_file}");
            continue;
        }

        let changed_path = std::path::PathBuf::from(changed_file);

        // Find the most specific (longest) matching package path for this changed file
        // This prevents nested packages from incorrectly affecting their parent packages
        let mut best_match: Option<(&String, &String)> = None;
        let mut best_match_length = 0;

        for (package_path, package_name) in &package_path_to_name {
            let package_path_buf = std::path::PathBuf::from(package_path);

            // Check if the changed file is within this package's directory
            if changed_path.starts_with(&package_path_buf) {
                let path_length = package_path.len();
                // Only update if this is a longer (more specific) match
                if path_length > best_match_length {
                    best_match = Some((package_path, package_name));
                    best_match_length = path_length;
                }
            }
        }

        // Only add the most specific match to avoid nested package false positives
        if let Some((package_path, package_name)) = best_match {
            log::trace!(
                "📝 File {changed_file} affects package {package_name} (path: {package_path})"
            );
            directly_affected_packages
                .entry(package_name.clone())
                .or_insert_with(Vec::new)
                .push(changed_file.clone());

            reasoning_map
                .entry(package_name.clone())
                .or_default()
                .push(format!("Contains changed file: {changed_file}"));
        }
    }

    log::trace!("🎯 Directly affected packages: {directly_affected_packages:?}");

    // Now find all packages that depend on the directly affected packages (transitive dependencies)
    let mut all_affected_packages = directly_affected_packages
        .keys()
        .cloned()
        .collect::<BTreeSet<String>>();
    let mut queue = VecDeque::new();

    // Add all directly affected packages to the queue
    for package in directly_affected_packages.keys() {
        queue.push_back(package.clone());
    }

    // Build reverse dependency map (package -> packages that depend on it)
    let mut reverse_deps: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (package, deps) in &package_dependencies {
        for dep in deps {
            reverse_deps
                .entry(dep.clone())
                .or_default()
                .push(package.clone());
        }
    }

    // Process the queue to find all transitive dependents
    while let Some(current_package) = queue.pop_front() {
        if let Some(dependents) = reverse_deps.get(&current_package) {
            for dependent in dependents {
                if !all_affected_packages.contains(dependent) {
                    log::trace!(
                        "🔄 Package {dependent} depends on affected package {current_package}"
                    );
                    all_affected_packages.insert(dependent.clone());
                    queue.push_back(dependent.clone());

                    reasoning_map
                        .entry(dependent.clone())
                        .or_default()
                        .push(format!("Depends on affected package: {current_package}"));
                }
            }
        }
    }

    let mut result: Vec<AffectedPackageInfo> = all_affected_packages
        .into_iter()
        .map(|name| {
            let reasoning = reasoning_map.get(&name).cloned();
            AffectedPackageInfo { name, reasoning }
        })
        .collect();

    result.sort_by(|a, b| a.name.cmp(&b.name));

    log::trace!("🏁 Final affected packages with reasoning: {result:?}");

    Ok(result)
}

/// Collects environment variables for a target package
///
/// # Errors
///
/// * If fails to process configs
pub async fn collect_environment_variables(
    workspace_root: &Path,
    _target_package: &str,
    target_package_path: &str,
    enabled_features: Option<&[String]>,
    target_os: &str,
) -> Result<Vec<(String, String)>, BoxError> {
    let path = workspace_root.join(target_package_path);

    // Skip if no clippier.toml exists for this package
    let clippier_path = path.join("clippier.toml");
    if !switchy_fs::exists(&clippier_path) {
        return Ok(Vec::new());
    }

    // Convert features to comma-separated string for the dependencies command
    let features_str = enabled_features.map(|f| f.join(",")).unwrap_or_default();
    let specific_features = if features_str.is_empty() {
        None
    } else {
        Some(
            features_str
                .split(',')
                .map(str::to_string)
                .collect::<Vec<_>>(),
        )
    };

    let packages = process_configs(
        &path,
        None,
        None,
        None,
        false,
        false, // randomize = false for system dependencies collection
        None,  // seed = None for system dependencies collection
        specific_features.as_deref(),
        None,
        None,
    )
    .await?;

    let mut env_vars = Vec::new();

    // Extract environment variables
    for package in packages {
        if let Some(os) = package.get("os").and_then(|v| v.as_str())
            && os == target_os
            && let Some(env_str) = package.get("env").and_then(|v| v.as_str())
        {
            for line in env_str.lines() {
                if let Some((key, value)) = line.split_once('=') {
                    env_vars.push((key.to_string(), value.to_string()));
                }
            }
        }
    }

    Ok(env_vars)
}

/// Collects system dependencies for a target package
///
/// # Errors
///
/// * If fails to process configs
pub async fn collect_system_dependencies(
    workspace_root: &Path,
    dependencies: &[(String, String)],
    enabled_features: Option<&[String]>,
    target_os: &str,
) -> Result<Vec<String>, BoxError> {
    let mut all_deps = BTreeSet::new();

    // Convert features to comma-separated string for the dependencies command
    let features_str = enabled_features.map(|f| f.join(",")).unwrap_or_default();

    for (_, package_path) in dependencies {
        let path = workspace_root.join(package_path);

        // Skip if no clippier.toml exists for this package
        let clippier_path = path.join("clippier.toml");
        if !switchy_fs::exists(&clippier_path) {
            continue;
        }

        // Use the existing process_configs function to get dependencies
        let specific_features = if features_str.is_empty() {
            None
        } else {
            Some(
                features_str
                    .split(',')
                    .map(str::to_string)
                    .collect::<Vec<_>>(),
            )
        };

        let packages = process_configs(
            &path,
            None,
            None,
            None,
            false,
            false, // randomize = false for dependencies collection
            None,  // seed = None for dependencies collection
            specific_features.as_deref(),
            None,
            None,
        )
        .await?;

        // Extract system dependencies
        for package in packages {
            if let Some(os) = package.get("os").and_then(|v| v.as_str())
                && os == target_os
                && let Some(deps) = package.get("dependencies").and_then(|v| v.as_str())
            {
                for dep in deps.lines() {
                    if !dep.trim().is_empty() {
                        all_deps.insert(dep.trim().to_string());
                    }
                }
            }
        }
    }

    // Convert to sorted vector for consistent output
    let mut result: Vec<String> = all_deps.into_iter().collect();
    result.sort();
    Ok(result)
}

/// Parses a dependency line to extract the package name
#[must_use]
pub fn parse_dependency_name(dependency_line: &str) -> String {
    // Simple implementation that extracts the first word (package name)
    dependency_line
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string()
}

/// Information about a workspace package
#[derive(Debug, Serialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PackageInfo {
    /// Package name
    pub name: String,
    /// Relative path to the package
    pub path: String,
}

/// Result structure for workspace dependencies command
#[derive(Debug, Serialize)]
pub struct WorkspaceDepsResult {
    /// List of workspace packages
    pub packages: Vec<PackageInfo>,
}

/// Result structure for affected packages command
#[derive(Debug, Serialize)]
pub struct AffectedPackagesResult {
    /// List of affected packages
    pub affected_packages: Vec<AffectedPackageInfo>,
}

/// Result structure for single package analysis
#[derive(Debug, Serialize)]
pub struct SinglePackageResult {
    /// Package name being analyzed
    pub package: String,
    /// Whether this package is affected
    pub affected: bool,
    /// Reasoning for why the package is affected
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Vec<String>>,
    /// All packages affected by the changes
    pub all_affected: Vec<AffectedPackageInfo>,
}

// Business logic functions for CLI commands

/// Handles the dependencies command
///
/// # Errors
///
/// * If fails to process configs or output results
pub async fn handle_dependencies_command(
    file: &str,
    os: Option<&str>,
    features: Option<&str>,
    output: OutputType,
) -> Result<String, BoxError> {
    use std::str::FromStr;

    let path = std::path::PathBuf::from_str(file)?;
    let specific_features = features.map(|f| f.split(',').map(str::to_string).collect::<Vec<_>>());

    let packages = process_workspace_configs(
        &path,
        None,
        None,
        None,
        false,
        false, // randomize = false for dependencies command
        None,  // seed = None for dependencies command
        specific_features.as_deref(),
        None,
        None,
    )
    .await?;

    let dependencies: Vec<String> = packages
        .iter()
        .filter(|package| {
            package
                .get("os")
                .and_then(|v| v.as_str())
                .is_some_and(|package_os| os.is_none() || os == Some(package_os))
        })
        .filter_map(|package| {
            package
                .get("dependencies")
                .and_then(|v| v.as_str())
                .map(ToString::to_string)
        })
        .unique()
        .collect();

    match output {
        OutputType::Json => Ok(serde_json::to_string(&dependencies)?),
        OutputType::Raw => Ok(dependencies.join("\n")),
    }
}

/// Handles the environment command
///
/// # Errors
///
/// * If fails to process configs or output results
pub async fn handle_environment_command(
    file: &str,
    os: Option<&str>,
    features: Option<&str>,
    output: OutputType,
) -> Result<String, BoxError> {
    use std::str::FromStr;

    let path = std::path::PathBuf::from_str(file)?;
    let specific_features = features.map(|f| f.split(',').map(str::to_string).collect::<Vec<_>>());

    let packages = process_workspace_configs(
        &path,
        None,
        None,
        None,
        false,
        false, // randomize = false for environment command
        None,  // seed = None for environment command
        specific_features.as_deref(),
        None,
        None,
    )
    .await?;

    let environment_vars = packages
        .iter()
        .filter(|package| {
            package
                .get("os")
                .and_then(|v| v.as_str())
                .is_some_and(|package_os| os.is_none() || os == Some(package_os))
        })
        .filter_map(|package| {
            package
                .get("env")
                .and_then(|v| v.as_str())
                .map(ToString::to_string)
        })
        .unique()
        .collect::<Vec<_>>();

    match output {
        OutputType::Json => Ok(serde_json::to_string(&environment_vars)?),
        OutputType::Raw => Ok(environment_vars.join("\n")),
    }
}

/// Handles the CI steps command
///
/// # Errors
///
/// * If fails to process configs or output results
pub async fn handle_ci_steps_command(
    file: &str,
    os: Option<&str>,
    features: Option<&str>,
    output: OutputType,
) -> Result<String, BoxError> {
    use std::str::FromStr;

    let path = std::path::PathBuf::from_str(file)?;
    let specific_features = features.map(|f| f.split(',').map(str::to_string).collect::<Vec<_>>());

    let packages = process_workspace_configs(
        &path,
        None,
        None,
        None,
        false,
        false, // randomize = false for ci steps command
        None,  // seed = None for ci steps command
        specific_features.as_deref(),
        None,
        None,
    )
    .await?;

    let ci_steps = packages
        .iter()
        .filter(|package| {
            package
                .get("os")
                .and_then(|v| v.as_str())
                .is_some_and(|package_os| os.is_none() || os == Some(package_os))
        })
        .filter_map(|package| {
            package
                .get("ciSteps")
                .and_then(|v| v.as_str())
                .map(ToString::to_string)
        })
        .unique()
        .collect::<Vec<_>>();

    match output {
        OutputType::Json => Ok(serde_json::to_string(&ci_steps)?),
        OutputType::Raw => Ok(ci_steps.join("\n")),
    }
}

/// Generates a feature matrix for workspace packages.
///
/// Analyzes the workspace to determine all valid feature combinations for each package,
/// optionally filtering by affected packages and applying feature constraints. Returns
/// the matrix in JSON or raw format for use in CI/CD pipelines.
///
/// # Arguments
///
/// * `file` - Path to the workspace root or Cargo.toml
/// * `only_affected` - Only include packages affected by changes
/// * `change_ref` - Git reference for change detection (e.g., "origin/main")
/// * `head_ref` - Git reference for the current state (e.g., "HEAD")
/// * `skip_filters` - Package filters to skip (excluded packages)
/// * `include_filters` - Package filters to include (only these packages)
/// * `enable` - Features to enable for all packages
/// * `disable` - Features to disable for all packages
/// * `features` - Specific features to generate combinations for
/// * `max_parallel` - Maximum parallel jobs (for rechunking)
/// * `randomize` - Randomize the order of feature combinations
/// * `seed` - Seed for randomization
/// * `output` - Output format (JSON or raw)
///
/// # Errors
///
/// * `std::io::Error` - If file operations fail
/// * `serde_json::Error` - If JSON serialization fails
/// * `anyhow::Error` - If workspace processing or filtering fails
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::cognitive_complexity
)]
#[allow(clippy::fn_params_excessive_bools, clippy::future_not_send)]
pub async fn handle_features_command(
    file: &str,
    os: Option<&str>,
    offset: Option<u16>,
    max: Option<u16>,
    max_parallel: Option<u16>,
    chunked: Option<u16>,
    spread: bool,
    randomize: bool,
    seed: Option<u64>,
    features: Option<&str>,
    skip_features: Option<&str>,
    required_features: Option<&str>,
    packages: Option<&[String]>,
    changed_files: Option<&[String]>,
    #[cfg(feature = "git-diff")] git_base: Option<&str>,
    #[cfg(feature = "git-diff")] git_head: Option<&str>,
    include_reasoning: bool,
    ignore_patterns: Option<&[String]>,
    skip_if: &[String],
    include_if: &[String],
    #[cfg(feature = "_transforms")] transform_scripts: &[std::path::PathBuf],
    #[cfg(feature = "_transforms")] transform_trace: bool,
    output: OutputType,
) -> Result<String, BoxError> {
    use std::str::FromStr;

    let path = std::path::PathBuf::from_str(file)?;
    let specific_features = features.map(|f| f.split(',').map(str::to_string).collect::<Vec<_>>());
    let skip_features_list =
        skip_features.map(|f| f.split(',').map(str::to_string).collect::<Vec<_>>());
    let required_features_list =
        required_features.map(|f| f.split(',').map(str::to_string).collect::<Vec<_>>());

    // If specific packages are requested, filter to only those packages
    if let Some(selected_packages) = packages
        && !selected_packages.is_empty()
    {
        log::debug!("Filtering to specific packages: {selected_packages:?}");

        // Get workspace members
        let workspace_cargo_path = path.join("Cargo.toml");
        let workspace_source = switchy_fs::unsync::read_to_string(&workspace_cargo_path).await?;
        let workspace_value: Value = toml::from_str(&workspace_source)?;

        let workspace_members = workspace_value
            .get("workspace")
            .and_then(|x| x.get("members"))
            .and_then(|x| x.as_array())
            .and_then(|x| x.iter().map(|x| x.as_str()).collect::<Option<Vec<_>>>())
            .unwrap_or_default();

        // Map package names to paths
        let mut package_name_to_path = BTreeMap::new();
        for member_path in workspace_members {
            let full_path = path.join(member_path);
            let cargo_path = full_path.join("Cargo.toml");

            if switchy_fs::unsync::exists(&cargo_path).await {
                let source = switchy_fs::unsync::read_to_string(&cargo_path).await?;
                let value: Value = toml::from_str(&source)?;

                if let Some(package_name) = value
                    .get("package")
                    .and_then(|x| x.get("name"))
                    .and_then(|x| x.as_str())
                {
                    package_name_to_path.insert(package_name.to_string(), member_path.to_string());
                }
            }
        }

        // Get all available package names for wildcard expansion
        let all_package_names: Vec<String> = package_name_to_path.keys().cloned().collect();

        // Expand wildcard patterns in selected_packages
        let expanded_packages = expand_pattern_list(selected_packages, &all_package_names);
        log::debug!("Expanded packages: {expanded_packages:?}");

        // Apply package filters if any
        let filtered_packages = if !skip_if.is_empty() || !include_if.is_empty() {
            package_filter::apply_filters(
                &expanded_packages,
                &package_name_to_path,
                &path,
                skip_if,
                include_if,
            )?
        } else {
            expanded_packages
        };

        // Process only filtered packages
        let mut all_filtered_packages = Vec::new();
        for selected_pkg in &filtered_packages {
            if let Some(package_path) = package_name_to_path.get(selected_pkg) {
                let package_dir = path.join(package_path);
                let packages = process_configs(
                    &package_dir,
                    offset,
                    max,
                    chunked,
                    spread,
                    randomize,
                    seed,
                    specific_features.as_deref(),
                    skip_features_list.as_deref(),
                    required_features_list.as_deref(),
                )
                .await?;

                all_filtered_packages.extend(packages);
            } else {
                log::warn!("Package '{selected_pkg}' not found in workspace");
            }
        }

        // Filter by OS if specified
        if let Some(target_os) = os {
            all_filtered_packages.retain(|package| {
                package
                    .get("os")
                    .and_then(|v| v.as_str())
                    .is_some_and(|pkg_os| pkg_os == target_os)
            });
        }

        // Apply max_parallel re-chunking if specified
        if let Some(max_parallel_limit) = max_parallel {
            all_filtered_packages = apply_max_parallel_rechunking(
                all_filtered_packages,
                max_parallel_limit as usize,
                chunked,
            )?;
        }

        let result = match output {
            OutputType::Json => serde_json::to_string(&all_filtered_packages)?,
            OutputType::Raw => {
                let mut results = Vec::new();
                for package in all_filtered_packages {
                    if let Some(features) = package.get("features") {
                        results.push(features.to_string());
                    }
                }
                results.join("\n")
            }
        };
        return Ok(result);
    }

    // Determine if we should use filtering logic based on changed files
    let use_filtering = changed_files.is_some() || {
        #[cfg(feature = "git-diff")]
        {
            git_base.is_some() && git_head.is_some()
        }
        #[cfg(not(feature = "git-diff"))]
        {
            false
        }
    };

    if use_filtering {
        #[cfg(feature = "git-diff")]
        use crate::git_diff::{
            build_external_dependency_map, extract_changed_dependencies_from_git,
            find_packages_affected_by_external_deps_with_mapping, get_changed_files_from_git,
        };

        // Handle changed files filtering (from manual specification or git)
        let mut all_changed_files = Vec::new();

        // Add manually specified changed files
        if let Some(changed_files) = changed_files {
            all_changed_files.extend(changed_files.to_vec());
        }

        // Add changed files from git if git parameters are provided
        #[cfg(feature = "git-diff")]
        if let (Some(base), Some(head)) = (git_base, git_head) {
            let git_changed_files = get_changed_files_from_git(&path, base, head)?;
            log::debug!("Git changed files: {git_changed_files:?}");
            all_changed_files.extend(git_changed_files);
        }

        // Remove duplicates and sort
        all_changed_files.sort();
        all_changed_files.dedup();

        // Additionally, when git parameters are provided, analyze external dependency changes
        #[allow(unused_mut)]
        let mut external_affected_packages = Vec::<String>::new();
        #[allow(unused_mut)]
        let mut external_dependency_mapping = BTreeMap::<String, Vec<String>>::new();

        #[cfg(feature = "git-diff")]
        if let (Some(base), Some(head)) = (git_base, git_head) {
            log::debug!("Analyzing external dependency changes from Cargo.lock");

            // Extract changed external dependencies from Cargo.lock
            if let Ok(changed_external_deps) =
                extract_changed_dependencies_from_git(&path, base, head, &all_changed_files)
            {
                log::debug!("Changed external dependencies: {changed_external_deps:?}");

                if !changed_external_deps.is_empty() {
                    // Get workspace members for building external dependency map
                    let workspace_cargo_path = path.join("Cargo.toml");
                    let workspace_source =
                        switchy_fs::unsync::read_to_string(&workspace_cargo_path).await?;
                    let workspace_value: Value = toml::from_str(&workspace_source)?;

                    if let Some(workspace_members) = workspace_value
                        .get("workspace")
                        .and_then(|x| x.get("members"))
                        .and_then(|x| x.as_array())
                        .and_then(|x| x.iter().map(|x| x.as_str()).collect::<Option<Vec<_>>>())
                    {
                        // Convert to Vec<String> for build_external_dependency_map
                        let workspace_members_owned: Vec<String> = workspace_members
                            .into_iter()
                            .map(ToString::to_string)
                            .collect();

                        // Build external dependency map
                        if let Ok(external_dep_map) =
                            build_external_dependency_map(&path, &workspace_members_owned)
                        {
                            // Find packages affected by external dependency changes with specific mapping
                            let external_affected_mapping =
                                find_packages_affected_by_external_deps_with_mapping(
                                    &external_dep_map,
                                    &changed_external_deps,
                                );
                            external_affected_packages =
                                external_affected_mapping.keys().cloned().collect();
                            external_dependency_mapping = external_affected_mapping;
                            log::debug!(
                                "Packages affected by external dependency changes: {external_affected_packages:?}"
                            );
                            log::debug!(
                                "External dependency mapping: {external_dependency_mapping:?}"
                            );
                        }
                    }
                }
            }
        }
        log::debug!("All changed files: {all_changed_files:?}");

        // If no files were found, return empty result
        if all_changed_files.is_empty() {
            return match output {
                OutputType::Json => Ok("[]".to_string()),
                OutputType::Raw => Ok(String::new()),
            };
        }

        // First find affected packages from file changes
        let ignore_patterns_vec = ignore_patterns.unwrap_or(&[]).to_vec();
        let (mut affected_packages, affected_with_reasoning) = if include_reasoning {
            let with_reasoning = find_affected_packages_with_reasoning(
                &path,
                &all_changed_files,
                &ignore_patterns_vec,
            )?;
            let packages: Vec<String> = with_reasoning.iter().map(|pkg| pkg.name.clone()).collect();
            (packages, Some(with_reasoning))
        } else {
            (
                find_affected_packages(&path, &all_changed_files, &ignore_patterns_vec)?,
                None,
            )
        };

        // Add packages affected by external dependency changes and update reasoning if needed
        let mut updated_reasoning = affected_with_reasoning;
        for external_pkg in external_affected_packages {
            if !affected_packages.contains(&external_pkg) {
                affected_packages.push(external_pkg.clone());
                log::debug!("Added package affected by external dependencies: {external_pkg}");

                // If reasoning is enabled, add reasoning entry for external dependency affected package
                if include_reasoning && let Some(ref mut reasoning_data) = updated_reasoning {
                    // Get specific external dependencies that affected this package
                    let specific_deps = external_dependency_mapping.get(&external_pkg).map_or_else(
                        || vec!["Affected by external dependency changes".to_string()],
                        |deps| {
                            deps.iter()
                                .map(|dep| format!("Affected by external dependency: {dep}"))
                                .collect()
                        },
                    );

                    reasoning_data.push(AffectedPackageInfo {
                        name: external_pkg,
                        reasoning: Some(specific_deps),
                    });
                }
            }
        }

        // Sort for consistent output
        affected_packages.sort();

        // Update the reasoning data reference
        let affected_with_reasoning = updated_reasoning;

        let workspace_cargo_path = path.join("Cargo.toml");
        let workspace_source = switchy_fs::unsync::read_to_string(&workspace_cargo_path).await?;
        let workspace_value: Value = toml::from_str(&workspace_source)?;

        let workspace_members = workspace_value
            .get("workspace")
            .and_then(|x| x.get("members"))
            .and_then(|x| x.as_array())
            .and_then(|x| x.iter().map(|x| x.as_str()).collect::<Option<Vec<_>>>())
            .unwrap_or_default();

        let mut package_name_to_path = BTreeMap::new();
        for member_path in workspace_members {
            let full_path = path.join(member_path);
            let cargo_path = full_path.join("Cargo.toml");

            if switchy_fs::unsync::exists(&cargo_path).await {
                let source = switchy_fs::unsync::read_to_string(&cargo_path).await?;
                let value: Value = toml::from_str(&source)?;

                if let Some(package_name) = value
                    .get("package")
                    .and_then(|x| x.get("name"))
                    .and_then(|x| x.as_str())
                {
                    package_name_to_path.insert(package_name.to_string(), member_path.to_string());
                }
            }
        }

        // Process configs for each affected package
        // When filtering by changed files, ignore chunking/spreading to return complete feature sets
        let mut all_filtered_packages = Vec::new();
        for affected_package in affected_packages {
            if let Some(package_path) = package_name_to_path.get(&affected_package) {
                let package_dir = path.join(package_path);
                let mut packages = process_configs(
                    &package_dir,
                    offset,
                    max,
                    chunked,   // Respect chunking when filtering by changed files
                    spread,    // Respect spreading when filtering by changed files
                    randomize, // Respect randomization when filtering by changed files
                    seed,      // Respect seed when filtering by changed files
                    specific_features.as_deref(),
                    skip_features_list.as_deref(),
                    required_features_list.as_deref(),
                )
                .await?;

                // Add reasoning to packages if include_reasoning is true
                if let Some(ref reasoning_data) = affected_with_reasoning
                    && let Some(pkg_reasoning) = reasoning_data
                        .iter()
                        .find(|pkg| pkg.name == affected_package)
                    && let Some(reasoning) = &pkg_reasoning.reasoning
                {
                    for package in &mut packages {
                        package.insert("reasoning".to_string(), serde_json::to_value(reasoning)?);
                    }
                }

                all_filtered_packages.extend(packages);
            }
        }

        // Filter by OS if specified
        if let Some(target_os) = os {
            all_filtered_packages.retain(|package| {
                package
                    .get("os")
                    .and_then(|v| v.as_str())
                    .is_some_and(|pkg_os| pkg_os == target_os)
            });
        }

        // Apply max_parallel re-chunking if specified (redistribute instead of truncate)
        if let Some(max_parallel_limit) = max_parallel {
            all_filtered_packages = apply_max_parallel_rechunking(
                all_filtered_packages,
                max_parallel_limit as usize,
                chunked,
            )?;
        }

        let result = match output {
            OutputType::Json => serde_json::to_string(&all_filtered_packages)?,
            OutputType::Raw => {
                let mut results = Vec::new();
                for package in all_filtered_packages {
                    if let Some(features) = package.get("features") {
                        results.push(features.to_string());
                    }
                }
                results.join("\n")
            }
        };
        return Ok(result);
    }

    // Use max_parallel as chunked if chunked is not provided
    let effective_chunked = chunked.or(max_parallel);

    let mut packages = process_workspace_configs(
        &path,
        offset,
        max,
        effective_chunked,
        spread,
        randomize,
        seed,
        specific_features.as_deref(),
        skip_features_list.as_deref(),
        required_features_list.as_deref(),
    )
    .await?;

    // Filter by OS if specified
    if let Some(target_os) = os {
        packages.retain(|package| {
            package
                .get("os")
                .and_then(|v| v.as_str())
                .is_some_and(|pkg_os| pkg_os == target_os)
        });
    }

    // Apply max_parallel re-chunking if specified (redistribute instead of truncate)
    if let Some(max_parallel_limit) = max_parallel {
        packages = apply_max_parallel_rechunking(packages, max_parallel_limit as usize, chunked)?;
    }

    // Apply Lua transforms if specified
    #[cfg(feature = "_transforms")]
    if !transform_scripts.is_empty() {
        log::info!("Applying {} transform script(s)", transform_scripts.len());

        let engine = if transform_trace {
            crate::transforms::TransformEngine::with_trace(&path, true)?
        } else {
            crate::transforms::TransformEngine::new(&path)?
        };

        for script_path in transform_scripts {
            log::info!("Applying transform: {}", script_path.display());
            let script = switchy_fs::unsync::read_to_string(script_path).await?;
            engine.apply_transform(&mut packages, &script)?;
        }

        log::info!(
            "Transforms applied successfully. Matrix size: {}",
            packages.len()
        );
    }

    let result = match output {
        OutputType::Json => serde_json::to_string(&packages)?,
        OutputType::Raw => {
            let mut results = Vec::new();
            for package in packages {
                if let Some(features) = package.get("features") {
                    results.push(features.to_string());
                }
            }
            results.join("\n")
        }
    };

    Ok(result)
}

/// Handles the workspace deps command
///
/// # Errors
///
/// * If fails to find workspace dependencies
pub fn handle_workspace_deps_command(
    workspace_root: &Path,
    package: &str,
    features: Option<&[String]>,
    format: &str,
    all_potential_deps: bool,
) -> Result<String, BoxError> {
    let deps = find_workspace_dependencies(workspace_root, package, features, all_potential_deps)?;

    let result = if format == "json" {
        let result = WorkspaceDepsResult {
            packages: deps
                .into_iter()
                .map(|(name, path)| PackageInfo { name, path })
                .collect(),
        };
        serde_json::to_string(&result)?
    } else {
        let mut results = Vec::new();
        for (name, path) in deps {
            results.push(format!("{name}: {path}"));
        }
        results.join("\n")
    };

    Ok(result)
}

/// Handles the generate dockerfile command
///
/// # Errors
///
/// * If fails to generate dockerfile
#[allow(clippy::too_many_arguments)]
pub async fn handle_generate_dockerfile_command(
    workspace_root: &Path,
    package: &str,
    git_ref: &str,
    features: Option<&[String]>,
    no_default_features: bool,
    output: &Path,
    base_image: &str,
    final_image: &str,
    args: &[String],
    build_args: Option<&str>,
    generate_dockerignore: bool,
    env: &[String],
    build_env: &[String],
    bin: Option<&str>,
) -> Result<String, BoxError> {
    let workspace_root_str = workspace_root.to_string_lossy();

    if is_git_url(&workspace_root_str) {
        // Git mode - generate dockerfile that clones from git
        generate_dockerfile_from_git(
            &workspace_root_str,
            git_ref,
            package,
            features,
            no_default_features,
            output,
            base_image,
            final_image,
            args,
            build_args,
            generate_dockerignore,
            env,
            build_env,
            bin,
        )?;
    } else {
        // Local mode - existing logic unchanged
        generate_dockerfile(
            workspace_root,
            package,
            features,
            no_default_features,
            output,
            base_image,
            final_image,
            args,
            build_args,
            generate_dockerignore,
            env,
            build_env,
            bin,
        )
        .await?;
    }

    Ok(format!("Generated Dockerfile at: {}", output.display()))
}

/// Handles the affected packages command
///
/// # Errors
///
/// * If fails to find affected packages
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub async fn handle_affected_packages_command(
    workspace_root: &Path,
    changed_files: &[String],
    target_package: Option<&str>,
    #[cfg(feature = "git-diff")] git_base: Option<&str>,
    #[cfg(feature = "git-diff")] git_head: Option<&str>,
    include_reasoning: bool,
    ignore_patterns: Option<&[String]>,
    output: OutputType,
) -> Result<String, BoxError> {
    #[cfg(feature = "git-diff")]
    use crate::git_diff::{
        build_external_dependency_map, extract_changed_dependencies_from_git,
        find_packages_affected_by_external_deps_with_mapping, get_changed_files_from_git,
    };

    // Combine manual changed files with git-extracted files
    let mut all_changed_files = changed_files.to_vec();
    #[allow(unused_mut)]
    let mut external_dependency_mapping = BTreeMap::<String, Vec<String>>::new();

    // Add changed files from git if git parameters are provided
    #[cfg(feature = "git-diff")]
    if let (Some(base), Some(head)) = (git_base, git_head) {
        let git_changed_files = get_changed_files_from_git(workspace_root, base, head)?;
        log::debug!("Git changed files: {git_changed_files:?}");
        all_changed_files.extend(git_changed_files);

        // Analyze external dependency changes from Cargo.lock
        log::debug!("Analyzing external dependency changes from Cargo.lock");
        if let Ok(changed_external_deps) =
            extract_changed_dependencies_from_git(workspace_root, base, head, &all_changed_files)
        {
            log::debug!("Changed external dependencies: {changed_external_deps:?}");

            if !changed_external_deps.is_empty() {
                // Get workspace members for building external dependency map
                let workspace_cargo_path = workspace_root.join("Cargo.toml");
                let workspace_source =
                    switchy_fs::unsync::read_to_string(&workspace_cargo_path).await?;
                let workspace_value: Value = toml::from_str(&workspace_source)?;

                if let Some(workspace_members) = workspace_value
                    .get("workspace")
                    .and_then(|x| x.get("members"))
                    .and_then(|x| x.as_array())
                    .and_then(|x| x.iter().map(|x| x.as_str()).collect::<Option<Vec<_>>>())
                {
                    let workspace_members_owned: Vec<String> = workspace_members
                        .into_iter()
                        .map(ToString::to_string)
                        .collect();

                    // Build external dependency map and find affected packages
                    if let Ok(external_dep_map) =
                        build_external_dependency_map(workspace_root, &workspace_members_owned)
                    {
                        external_dependency_mapping =
                            find_packages_affected_by_external_deps_with_mapping(
                                &external_dep_map,
                                &changed_external_deps,
                            );
                        log::debug!("External dependency mapping: {external_dependency_mapping:?}");
                    }
                }
            }
        }
    }

    // Remove duplicates and sort
    all_changed_files.sort();
    all_changed_files.dedup();

    // Find affected packages from file changes
    let ignore_patterns_vec = ignore_patterns.unwrap_or(&[]).to_vec();
    let mut affected = if include_reasoning {
        find_affected_packages_with_reasoning(
            workspace_root,
            &all_changed_files,
            &ignore_patterns_vec,
        )?
    } else {
        find_affected_packages(workspace_root, &all_changed_files, &ignore_patterns_vec)?
            .into_iter()
            .map(|name| AffectedPackageInfo {
                name,
                reasoning: None,
            })
            .collect()
    };

    // Add packages affected by external dependency changes
    for (external_pkg, external_deps) in &external_dependency_mapping {
        if !affected.iter().any(|pkg| &pkg.name == external_pkg) {
            let reasoning = if include_reasoning {
                Some(
                    external_deps
                        .iter()
                        .map(|dep| format!("Affected by external dependency: {dep}"))
                        .collect(),
                )
            } else {
                None
            };

            affected.push(AffectedPackageInfo {
                name: external_pkg.clone(),
                reasoning,
            });
            log::debug!("Added package affected by external dependencies: {external_pkg}");
        }
    }

    // Sort for consistent output
    affected.sort_by(|a, b| a.name.cmp(&b.name));

    let result = if let Some(target) = target_package {
        let is_affected = affected.iter().any(|p| p.name == target);
        let reasoning = affected
            .iter()
            .find(|p| p.name == target)
            .and_then(|p| p.reasoning.clone());

        let result = SinglePackageResult {
            package: target.to_string(),
            affected: is_affected,
            reasoning,
            all_affected: affected,
        };

        match output {
            OutputType::Json => serde_json::to_string(&result)?,
            OutputType::Raw => if is_affected { "true" } else { "false" }.to_string(),
        }
    } else {
        let result = AffectedPackagesResult {
            affected_packages: affected,
        };

        match output {
            OutputType::Json => serde_json::to_string(&result)?,
            OutputType::Raw => {
                let mut results = Vec::new();
                for package in result.affected_packages {
                    results.push(package.name);
                }
                results.join("\n")
            }
        }
    };

    Ok(result)
}

/// Processes workspace configurations and returns a list of packages with their dependencies
///
/// # Errors
///
/// * If the workspace root directory is not found or cannot be read
/// * If workspace member directories cannot be processed
/// * If any workspace member has invalid configuration
#[allow(clippy::too_many_arguments)]
pub async fn process_workspace_configs(
    workspace_path: &Path,
    offset: Option<u16>,
    max: Option<u16>,
    chunked: Option<u16>,
    spread: bool,
    randomize: bool,
    seed: Option<u64>,
    specific_features: Option<&[String]>,
    skip_features_override: Option<&[String]>,
    required_features_override: Option<&[String]>,
) -> Result<Vec<serde_json::Map<String, serde_json::Value>>, BoxError> {
    log::debug!(
        "Processing workspace configs from '{}'",
        workspace_path.display()
    );

    // First, check if this is a workspace root
    let workspace_cargo_path = workspace_path.join("Cargo.toml");
    let workspace_source = switchy_fs::unsync::read_to_string(&workspace_cargo_path).await?;
    let workspace_value: Value = toml::from_str(&workspace_source)?;

    let workspace_members = workspace_value
        .get("workspace")
        .and_then(|x| x.get("members"))
        .and_then(|x| x.as_array())
        .and_then(|x| x.iter().map(|x| x.as_str()).collect::<Option<Vec<_>>>());

    match workspace_members {
        None => {
            process_configs(
                workspace_path,
                offset,
                max,
                chunked,
                spread,
                randomize,
                seed,
                specific_features,
                skip_features_override,
                required_features_override,
            )
            .await
        }
        Some(members) => {
            // This is a workspace root, process all members
            let mut all_packages = Vec::new();

            for member_path in members {
                let full_path = workspace_path.join(member_path);

                // Check if this member has a Cargo.toml file (basic validation)
                let cargo_path = full_path.join("Cargo.toml");
                if !switchy_fs::unsync::exists(&cargo_path).await {
                    log::trace!("Skipping workspace member {member_path} (no Cargo.toml)");
                    continue;
                }

                log::debug!("Processing workspace member: {member_path}");

                // Process this member's configs (with default config if no clippier.toml)
                match process_configs(
                    &full_path,
                    offset,
                    max,
                    chunked,
                    spread,
                    randomize,
                    seed,
                    specific_features,
                    skip_features_override,
                    required_features_override,
                )
                .await
                {
                    Ok(mut packages) => {
                        all_packages.append(&mut packages);
                    }
                    Err(e) => {
                        log::warn!("Failed to process workspace member {member_path}: {e}");
                        // Continue processing other members
                    }
                }
            }

            Ok(all_packages)
        }
    }
}

/// Handles the validate feature propagation command
///
/// # Errors
///
/// * If validation fails
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub fn handle_validate_feature_propagation_command(
    features: Option<Vec<String>>,
    skip_features: Option<Vec<String>>,
    path: Option<std::path::PathBuf>,
    workspace_only: bool,
    output: OutputType,
    strict_optional_propagation: bool,
    allow_missing: &[String],
    allow_incorrect: &[String],
    ignore_packages: &[String],
    ignore_features: &[String],
    use_config_overrides: bool,
    use_cargo_metadata_overrides: bool,
    warn_expired: bool,
    fail_on_expired: bool,
    verbose_overrides: bool,
    parent_packages: Option<Vec<String>>,
    parent_depth: Option<u8>,
    parent_skip_features: Option<Vec<String>>,
    parent_prefix: &[String],
    no_parent_config: bool,
) -> Result<ValidationResult, BoxError> {
    use crate::feature_validator::{
        OverrideOptions, OverrideSource, OverrideType, ParentValidationConfig, PrefixOverride,
        ValidationOverride,
    };

    // Parse CLI overrides for allow-missing
    let mut cli_overrides = Vec::new();
    for entry in allow_missing {
        let parts: Vec<&str> = entry.split(':').collect();
        let (package, feature, dependency) = match parts.len() {
            2 => (None, parts[0], parts[1]),
            3 => (Some(parts[0].to_string()), parts[1], parts[2]),
            _ => {
                return Err(format!("Invalid --allow-missing format: {entry}. Expected '[package:]feature:dependency'").into());
            }
        };
        cli_overrides.push(ValidationOverride {
            feature: feature.to_string(),
            dependency: dependency.to_string(),
            package,
            override_type: OverrideType::AllowMissing,
            reason: Some("CLI override".to_string()),
            expires: None,
            source: OverrideSource::Cli,
        });
    }

    // Parse CLI overrides for allow-incorrect
    for entry in allow_incorrect {
        let parts: Vec<&str> = entry.split(':').collect();
        let (package, feature, dependency) = match parts.len() {
            2 => (None, parts[0], parts[1]),
            3 => (Some(parts[0].to_string()), parts[1], parts[2]),
            _ => {
                return Err(format!(
                    "Invalid --allow-incorrect format: {entry}. Expected '[package:]feature:entry'"
                )
                .into());
            }
        };
        cli_overrides.push(ValidationOverride {
            feature: feature.to_string(),
            dependency: dependency.to_string(),
            package,
            override_type: OverrideType::AllowIncorrect,
            reason: Some("CLI override".to_string()),
            expires: None,
            source: OverrideSource::Cli,
        });
    }

    // Parse CLI prefix overrides for parent packages
    let mut cli_prefix_overrides = Vec::new();
    for entry in parent_prefix {
        let parts: Vec<&str> = entry.split(':').collect();
        if parts.len() != 2 {
            return Err(format!(
                "Invalid --parent-prefix format: {entry}. Expected 'dependency:prefix'"
            )
            .into());
        }
        cli_prefix_overrides.push(PrefixOverride {
            dependency: parts[0].to_string(),
            prefix: parts[1].to_string(),
        });
    }

    let config = ValidatorConfig {
        features,
        skip_features,
        workspace_only,
        output_format: output,
        strict_optional_propagation,
        cli_overrides,
        override_options: OverrideOptions {
            use_config_overrides,
            use_cargo_metadata_overrides,
            warn_expired,
            fail_on_expired,
            verbose_overrides,
        },
        ignore_packages: ignore_packages.to_vec(),
        ignore_features: ignore_features.to_vec(),
        parent_config: ParentValidationConfig {
            cli_packages: parent_packages.unwrap_or_default(),
            cli_depth: parent_depth,
            cli_skip_features: parent_skip_features.unwrap_or_default(),
            cli_prefix_overrides,
            use_config: !no_parent_config,
        },
    };

    let validator = FeatureValidator::new(path, config)?;
    Ok(validator.validate()?)
}

/// # Errors
///
/// * If the workspace path is invalid or cannot be read
/// * If the workspace Cargo.toml file cannot be read or parsed
/// * If any package Cargo.toml file cannot be read or parsed
/// * If package analysis fails when determining affected packages
/// * If JSON serialization fails
/// * If the git-diff feature is required but not enabled
/// * If git diff analysis fails when `git_base` and `git_head` are provided
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn handle_packages_command(
    file: &str,
    os: Option<&str>,
    packages: Option<&[String]>,
    changed_files: Option<&[String]>,
    #[cfg(feature = "git-diff")] git_base: Option<&str>,
    #[cfg(feature = "git-diff")] git_head: Option<&str>,
    #[cfg(feature = "git-diff")] include_reasoning: bool,
    max_parallel: Option<u16>,
    #[cfg(feature = "git-diff")] ignore_patterns: Option<&[String]>,
    skip_if: &[String],
    include_if: &[String],
    output: OutputType,
) -> Result<String, BoxError> {
    use std::str::FromStr;

    let path = std::path::PathBuf::from_str(file)?;

    let workspace_cargo_path = path.join("Cargo.toml");
    let workspace_source = switchy_fs::sync::read_to_string(&workspace_cargo_path)?;
    let workspace_value: Value = toml::from_str(&workspace_source)?;

    let workspace_members = workspace_value
        .get("workspace")
        .and_then(|x| x.get("members"))
        .and_then(|x| x.as_array())
        .and_then(|x| x.iter().map(|x| x.as_str()).collect::<Option<Vec<_>>>())
        .unwrap_or_default();

    let mut package_name_to_path = BTreeMap::new();

    for member_path in &workspace_members {
        let full_path = path.join(member_path);
        let cargo_path = full_path.join("Cargo.toml");

        if switchy_fs::exists(&cargo_path) {
            let source = switchy_fs::sync::read_to_string(&cargo_path)?;
            let value: Value = toml::from_str(&source)?;

            if let Some(package_name) = value
                .get("package")
                .and_then(|x| x.get("name"))
                .and_then(|x| x.as_str())
            {
                package_name_to_path.insert(package_name.to_string(), (*member_path).to_string());
            }
        }
    }

    // Get all available package names for wildcard expansion
    let all_package_names: Vec<String> = package_name_to_path.keys().cloned().collect();

    // Apply package filters if any
    let filtered_packages = if !skip_if.is_empty() || !include_if.is_empty() {
        package_filter::apply_filters(
            &all_package_names,
            &package_name_to_path,
            &path,
            skip_if,
            include_if,
        )?
    } else {
        all_package_names.clone()
    };

    let selected_packages: Vec<String> = if let Some(pkg_list) = packages
        && !pkg_list.is_empty()
    {
        // Expand wildcard patterns in package list
        let expanded_packages = expand_pattern_list(pkg_list, &all_package_names);
        log::debug!("Expanded packages: {expanded_packages:?}");

        // Intersect user-specified packages with filtered packages
        expanded_packages
            .iter()
            .filter(|p| filtered_packages.contains(p))
            .cloned()
            .collect()
    } else {
        filtered_packages
    };

    // Determine if we should use filtering logic based on changed files or git diff
    let use_filtering = changed_files.is_some() || {
        #[cfg(feature = "git-diff")]
        {
            git_base.is_some() && git_head.is_some()
        }
        #[cfg(not(feature = "git-diff"))]
        {
            false
        }
    };

    let affected_packages: Vec<String> = if use_filtering {
        #[cfg(feature = "git-diff")]
        use crate::git_diff::{
            build_external_dependency_map, extract_changed_dependencies_from_git,
            find_packages_affected_by_external_deps_with_mapping, get_changed_files_from_git,
        };

        // Collect all changed files (from manual specification and/or git)
        let mut all_changed_files = Vec::new();

        // Add manually specified changed files
        if let Some(files) = changed_files {
            all_changed_files.extend(files.to_vec());
        }

        // Add changed files from git if git parameters are provided
        #[cfg(feature = "git-diff")]
        if let (Some(base), Some(head)) = (git_base, git_head) {
            let git_changed_files = get_changed_files_from_git(&path, base, head)?;
            log::debug!("Git changed files: {git_changed_files:?}");
            all_changed_files.extend(git_changed_files);
        }

        // Remove duplicates and sort
        all_changed_files.sort();
        all_changed_files.dedup();

        // Analyze external dependency changes when git parameters are provided
        #[allow(unused_mut)]
        let mut external_affected_packages = Vec::<String>::new();

        #[cfg(feature = "git-diff")]
        if let (Some(base), Some(head)) = (git_base, git_head) {
            log::debug!("Analyzing external dependency changes from Cargo.lock");

            // Extract changed external dependencies from Cargo.lock
            if let Ok(changed_external_deps) =
                extract_changed_dependencies_from_git(&path, base, head, &all_changed_files)
            {
                log::debug!("Changed external dependencies: {changed_external_deps:?}");

                if !changed_external_deps.is_empty() {
                    // Convert workspace members to Vec<String> for build_external_dependency_map
                    let workspace_members_owned: Vec<String> =
                        workspace_members.iter().map(|s| (*s).to_string()).collect();

                    // Build external dependency map
                    if let Ok(external_dep_map) =
                        build_external_dependency_map(&path, &workspace_members_owned)
                    {
                        // Find packages affected by external dependency changes
                        let external_affected_mapping =
                            find_packages_affected_by_external_deps_with_mapping(
                                &external_dep_map,
                                &changed_external_deps,
                            );
                        external_affected_packages =
                            external_affected_mapping.keys().cloned().collect();
                        log::debug!(
                            "Packages affected by external dependency changes: {external_affected_packages:?}"
                        );
                    }
                }
            }
        }

        // Find packages affected by file changes
        let mut file_affected_packages = if all_changed_files.is_empty() {
            Vec::new()
        } else {
            #[cfg(feature = "git-diff")]
            {
                let ignore_patterns_vec = ignore_patterns.unwrap_or(&[]).to_vec();
                if include_reasoning {
                    let with_reasoning = find_affected_packages_with_reasoning(
                        &path,
                        &all_changed_files,
                        &ignore_patterns_vec,
                    )?;
                    with_reasoning.iter().map(|pkg| pkg.name.clone()).collect()
                } else {
                    find_affected_packages(&path, &all_changed_files, &ignore_patterns_vec)?
                }
            }
            #[cfg(not(feature = "git-diff"))]
            {
                return Err("Git diff analysis requires the git-diff feature".into());
            }
        };

        // Combine both sources of affected packages
        file_affected_packages.extend(external_affected_packages);
        file_affected_packages.sort();
        file_affected_packages.dedup();

        file_affected_packages
    } else {
        selected_packages.clone()
    };

    let mut package_list = Vec::new();

    for package_name in selected_packages {
        if affected_packages.contains(&package_name)
            && let Some(package_path) = package_name_to_path.get(&package_name)
        {
            let mut entry = serde_json::Map::new();
            entry.insert(
                "name".to_string(),
                serde_json::Value::String(package_name.clone()),
            );
            entry.insert(
                "path".to_string(),
                serde_json::Value::String(package_path.clone()),
            );

            let os_value = format!("{}{}", os.unwrap_or("ubuntu"), "-latest");
            entry.insert("os".to_string(), serde_json::Value::String(os_value));

            package_list.push(entry);
        }
    }

    if let Some(limit) = max_parallel {
        package_list.truncate(limit as usize);
    }

    let result = match output {
        OutputType::Json => serde_json::to_string(&package_list)?,
        OutputType::Raw => package_list
            .iter()
            .filter_map(|p| p.get("name").and_then(|n| n.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
    };

    Ok(result)
}

/// Aggregated toolchain information for workspace-level CI setup
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceToolchains {
    /// System dependencies to install (commands to run)
    pub dependencies: Vec<String>,
    /// Toolchain identifiers (e.g., "cargo-machete", "taplo", "node")
    pub toolchains: Vec<String>,
    /// CI steps commands to run
    pub ci_steps: Vec<String>,
    /// Environment variables
    pub env: BTreeMap<String, String>,
    /// Package names that require nightly toolchain
    pub nightly_packages: Vec<String>,
    /// Whether git submodules are needed
    pub git_submodules: bool,
}

/// Aggregates all toolchains and dependencies from workspace packages for CI setup.
///
/// Scans all packages in the workspace and collects their toolchains, dependencies,
/// CI steps, and environment variables for the specified OS. This enables workspace-level
/// CI setup without needing to specify a specific package.
///
/// # Arguments
///
/// * `workspace_root` - Path to the workspace root
/// * `os` - Target operating system (e.g., "ubuntu", "windows", "macos")
/// * `output` - Output format (JSON or Raw)
///
/// # Returns
///
/// JSON or raw text containing aggregated toolchain information
///
/// # Errors
///
/// * If workspace cannot be read
/// * If any clippier.toml has invalid format
/// * If JSON serialization fails
#[allow(clippy::too_many_lines)]
pub fn handle_workspace_toolchains_command(
    workspace_root: &Path,
    os: &str,
    output: OutputType,
) -> Result<String, BoxError> {
    let mut all_dependencies: BTreeSet<String> = BTreeSet::new();
    let mut all_toolchains: BTreeSet<String> = BTreeSet::new();
    let mut all_ci_steps: BTreeSet<String> = BTreeSet::new();
    let mut all_env: BTreeMap<String, String> = BTreeMap::new();
    let mut nightly_packages: BTreeSet<String> = BTreeSet::new();
    let mut needs_git_submodules = false;

    // First check for workspace-level clippier.toml
    let workspace_clippier_path = workspace_root.join("clippier.toml");
    if switchy_fs::exists(&workspace_clippier_path) {
        let content = switchy_fs::sync::read_to_string(&workspace_clippier_path)?;
        if let Ok(conf) = toml::from_str::<WorkspaceClippierConf>(&content) {
            log::debug!("Found workspace-level clippier.toml");

            // Process workspace-level dependencies
            if let Some(deps) = &conf.dependencies {
                for dep in deps {
                    if let Some(cmd) = &dep.command {
                        all_dependencies.insert(cmd.clone());
                    }
                    if let Some(toolchain) = &dep.toolchain {
                        all_toolchains.insert(toolchain.clone());
                    }
                }
            }

            // Process workspace-level ci_steps
            if let Some(ci_steps) = &conf.ci_steps {
                let steps: Vec<Step> = ci_steps.clone().into();
                for step in steps {
                    if let Some(cmd) = &step.command {
                        all_ci_steps.insert(cmd.clone());
                    }
                    if let Some(toolchain) = &step.toolchain {
                        all_toolchains.insert(toolchain.clone());
                    }
                }
            }

            // Process workspace-level env
            if let Some(env) = &conf.env {
                for (key, value) in env {
                    let resolved_value = match value {
                        ClippierEnv::Value(v) => v.clone(),
                        ClippierEnv::FilteredValue { value, .. } => value.clone(),
                    };
                    all_env.insert(key.clone(), resolved_value);
                }
            }

            // Note: workspace-level nightly is ignored since it doesn't have a package name
            // nightly_packages only tracks specific packages that need nightly
            if conf.git_submodules == Some(true) {
                needs_git_submodules = true;
            }
        }
    }

    // Scan all packages in the workspace
    let packages_dir = workspace_root.join("packages");
    if switchy_fs::exists(&packages_dir) {
        for entry in switchy_fs::sync::walk_dir_sorted(&packages_dir)? {
            let entry_path = entry.path();
            let clippier_path = entry_path.join("clippier.toml");
            if !switchy_fs::exists(&clippier_path) {
                continue;
            }

            log::debug!("Processing clippier.toml at: {}", clippier_path.display());

            // Try to get package name from Cargo.toml in the same directory
            let cargo_toml_path = entry_path.join("Cargo.toml");
            let package_name = if switchy_fs::exists(&cargo_toml_path) {
                switchy_fs::sync::read_to_string(&cargo_toml_path)
                    .ok()
                    .and_then(|content| toml::from_str::<Value>(&content).ok())
                    .and_then(|value| {
                        value
                            .get("package")
                            .and_then(|p| p.get("name"))
                            .and_then(|n| n.as_str())
                            .map(ToString::to_string)
                    })
            } else {
                None
            };

            // Fallback to directory name if Cargo.toml doesn't have package name
            let package_name = package_name.unwrap_or_else(|| {
                entry_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string()
            });

            let content = switchy_fs::sync::read_to_string(&clippier_path)?;
            let conf: ClippierConf = match toml::from_str(&content) {
                Ok(c) => c,
                Err(e) => {
                    log::warn!("Failed to parse {}: {}", clippier_path.display(), e);
                    continue;
                }
            };

            // Process global settings
            if conf.nightly == Some(true) {
                nightly_packages.insert(package_name.clone());
            }
            if conf.git_submodules == Some(true) {
                needs_git_submodules = true;
            }

            // Process global ci_steps
            if let Some(ci_steps) = &conf.ci_steps {
                let steps: Vec<Step> = ci_steps.clone().into();
                for step in steps {
                    if let Some(cmd) = &step.command {
                        all_ci_steps.insert(cmd.clone());
                    }
                    if let Some(toolchain) = &step.toolchain {
                        all_toolchains.insert(toolchain.clone());
                    }
                }
            }

            // Process global env
            if let Some(env) = &conf.env {
                for (key, value) in env {
                    let resolved_value = match value {
                        ClippierEnv::Value(v) => v.clone(),
                        ClippierEnv::FilteredValue { value, .. } => value.clone(),
                    };
                    all_env.insert(key.clone(), resolved_value);
                }
            }

            // Process OS-specific configs
            if let Some(configs) = &conf.config {
                for config in configs {
                    if config.os != os {
                        continue;
                    }

                    // Process config-specific settings
                    if config.nightly == Some(true) {
                        nightly_packages.insert(package_name.clone());
                    }
                    if config.git_submodules == Some(true) {
                        needs_git_submodules = true;
                    }

                    // Process dependencies
                    if let Some(deps) = &config.dependencies {
                        for dep in deps {
                            // Include all dependencies for workspace-level setup
                            // (we don't filter by features since we want complete setup)
                            if let Some(cmd) = &dep.command {
                                all_dependencies.insert(cmd.clone());
                            }
                            if let Some(toolchain) = &dep.toolchain {
                                all_toolchains.insert(toolchain.clone());
                            }
                        }
                    }

                    // Process CI steps
                    if let Some(ci_steps) = &config.ci_steps {
                        let steps: Vec<Step> = ci_steps.clone().into();
                        for step in steps {
                            if let Some(cmd) = &step.command {
                                all_ci_steps.insert(cmd.clone());
                            }
                            if let Some(toolchain) = &step.toolchain {
                                all_toolchains.insert(toolchain.clone());
                            }
                        }
                    }

                    // Process env
                    if let Some(env) = &config.env {
                        for (key, value) in env {
                            let resolved_value = match value {
                                ClippierEnv::Value(v) => v.clone(),
                                ClippierEnv::FilteredValue { value, .. } => value.clone(),
                            };
                            all_env.insert(key.clone(), resolved_value);
                        }
                    }
                }
            }
        }
    }

    let result = WorkspaceToolchains {
        dependencies: all_dependencies.into_iter().collect(),
        toolchains: all_toolchains.into_iter().collect(),
        ci_steps: all_ci_steps.into_iter().collect(),
        env: all_env,
        nightly_packages: nightly_packages.into_iter().collect(),
        git_submodules: needs_git_submodules,
    };

    match output {
        OutputType::Json => Ok(serde_json::to_string(&result)?),
        OutputType::Raw => {
            use std::fmt::Write as _;

            let mut output = String::new();
            output.push_str("Dependencies:\n");
            for dep in &result.dependencies {
                writeln!(output, "  {dep}")?;
            }
            output.push_str("\nToolchains:\n");
            for toolchain in &result.toolchains {
                writeln!(output, "  {toolchain}")?;
            }
            output.push_str("\nCI Steps:\n");
            for step in &result.ci_steps {
                writeln!(output, "  {step}")?;
            }
            if !result.env.is_empty() {
                output.push_str("\nEnvironment:\n");
                for (key, value) in &result.env {
                    writeln!(output, "  {key}={value}")?;
                }
            }
            if !result.nightly_packages.is_empty() {
                output.push_str("\nNightly Packages:\n");
                for pkg in &result.nightly_packages {
                    writeln!(output, "  {pkg}")?;
                }
            }
            writeln!(output, "\nGit Submodules: {}", result.git_submodules)?;
            Ok(output)
        }
    }
}

/// Handles the check command (run linters and format checkers)
///
/// Requires the `check` feature.
///
/// # Errors
///
/// * If tool detection fails
/// * If a required tool is not found
/// * If tool execution fails
#[cfg(feature = "check")]
pub fn handle_check_command(
    working_dir: Option<&Path>,
    tool_names: Option<&[String]>,
    list_tools: bool,
    config: tools::ToolsConfig,
    output: OutputType,
) -> Result<String, BoxError> {
    use tools::{ToolRegistry, ToolRunner};

    let registry = ToolRegistry::new(config)?;

    if list_tools {
        let tool_info = registry.list_tools();
        return match output {
            OutputType::Json => Ok(serde_json::to_string_pretty(
                &tool_info
                    .iter()
                    .map(|t| {
                        serde_json::json!({
                            "name": t.name,
                            "display_name": t.display_name,
                            "available": t.available,
                            "required": t.required,
                            "skipped": t.skipped,
                            "path": t.path,
                        })
                    })
                    .collect::<Vec<_>>(),
            )?),
            OutputType::Raw => {
                use std::fmt::Write;
                let mut output = String::new();
                for tool in tool_info {
                    let status = if tool.skipped {
                        "SKIPPED"
                    } else if tool.available {
                        "AVAILABLE"
                    } else if tool.required {
                        "REQUIRED (missing)"
                    } else {
                        "not found"
                    };
                    let _ = writeln!(output, "{}: {}", tool.display_name, status);
                }
                Ok(output)
            }
        };
    }

    let runner = working_dir.map_or_else(
        || ToolRunner::new(&registry),
        |dir| ToolRunner::new(&registry).with_working_dir(dir),
    );

    let results = if let Some(names) = tool_names {
        let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
        runner.run_specific(&name_refs, &[], true)?
    } else {
        // Run both linters and format checkers
        let linter_results = runner.run_linters(&[])?;
        let format_check_results = runner.run_format_check(&[])?;

        // Combine results
        let mut combined_results = linter_results.results;
        combined_results.extend(format_check_results.results);

        let success_count = combined_results.iter().filter(|r| r.success).count();
        let failure_count = combined_results.len() - success_count;

        tools::AggregatedResults {
            results: combined_results,
            total_duration: linter_results.total_duration + format_check_results.total_duration,
            success_count,
            failure_count,
        }
    };

    match output {
        OutputType::Json => Ok(tools::results_to_json(&results)?),
        OutputType::Raw => {
            tools::print_summary(&results);
            Ok(String::new())
        }
    }
}

/// Handles the fmt command (run formatters)
///
/// Requires the `format` feature.
///
/// # Errors
///
/// * If tool detection fails
/// * If a required tool is not found
/// * If tool execution fails
#[cfg(feature = "format")]
pub fn handle_fmt_command(
    working_dir: Option<&Path>,
    tool_names: Option<&[String]>,
    check_only: bool,
    list_tools: bool,
    config: tools::ToolsConfig,
    output: OutputType,
) -> Result<String, BoxError> {
    use tools::{ToolRegistry, ToolRunner};

    let registry = ToolRegistry::new(config)?;

    if list_tools {
        let tool_info: Vec<_> = registry
            .list_tools()
            .into_iter()
            .filter(|t| t.capabilities.contains(&tools::ToolCapability::Format))
            .collect();

        return match output {
            OutputType::Json => Ok(serde_json::to_string_pretty(
                &tool_info
                    .iter()
                    .map(|t| {
                        serde_json::json!({
                            "name": t.name,
                            "display_name": t.display_name,
                            "available": t.available,
                            "required": t.required,
                            "skipped": t.skipped,
                            "path": t.path,
                        })
                    })
                    .collect::<Vec<_>>(),
            )?),
            OutputType::Raw => {
                use std::fmt::Write;
                let mut output = String::new();
                for tool in tool_info {
                    let status = if tool.skipped {
                        "SKIPPED"
                    } else if tool.available {
                        "AVAILABLE"
                    } else if tool.required {
                        "REQUIRED (missing)"
                    } else {
                        "not found"
                    };
                    let _ = writeln!(output, "{}: {}", tool.display_name, status);
                }
                Ok(output)
            }
        };
    }

    let runner = working_dir.map_or_else(
        || ToolRunner::new(&registry),
        |dir| ToolRunner::new(&registry).with_working_dir(dir),
    );

    let results = if let Some(names) = tool_names {
        let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
        runner.run_specific(&name_refs, &[], check_only)?
    } else if check_only {
        runner.run_format_check(&[])?
    } else {
        runner.run_formatters(&[])?
    };

    match output {
        OutputType::Json => Ok(tools::results_to_json(&results)?),
        OutputType::Raw => {
            tools::print_summary(&results);
            Ok(String::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[switchy_async::test]
    async fn test_skip_features_combination() {
        // Create a temporary directory for testing
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();

        // Create a simple Cargo.toml with features
        let cargo_toml = r#"
[package]
name = "test-package"
version = "0.1.0"

[features]
default = []
feature1 = []
feature2 = []
simd = []
fail-on-warnings = []
"#;
        switchy_fs::sync::write(temp_path.join("Cargo.toml"), cargo_toml).unwrap();

        // Create a clippier.toml with skip_features
        let clippier_toml = r#"
[[config]]
os = "ubuntu"
skip-features = ["simd"]
"#;
        switchy_fs::sync::write(temp_path.join("clippier.toml"), clippier_toml).unwrap();

        // Test the combination: command line skip_features + config skip_features
        let result = process_configs(
            temp_path,
            None,
            None,
            None,
            false,
            false,
            None,
            None,
            Some(&["fail-on-warnings".to_string()]), // Command line skip_features
            None,
        )
        .await
        .unwrap();

        // Verify that both "simd" and "fail-on-warnings" are skipped
        assert!(!result.is_empty());
        let features = result[0].get("features").unwrap().as_array().unwrap();
        let feature_names: Vec<String> = features
            .iter()
            .map(|f| f.as_str().unwrap().to_string())
            .collect();

        // Should not contain "simd" (from config) or "fail-on-warnings" (from command line)
        assert!(!feature_names.contains(&"simd".to_string()));
        assert!(!feature_names.contains(&"fail-on-warnings".to_string()));

        // Should contain other features
        assert!(feature_names.contains(&"feature1".to_string()));
        assert!(feature_names.contains(&"feature2".to_string()));
    }

    #[switchy_async::test]
    async fn test_wildcard_skip_features_suffix() {
        // Test wildcard pattern matching with *-default
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();

        let cargo_toml = r#"
[package]
name = "test-package"
version = "0.1.0"

[features]
default = []
bob-default = []
sally-default = []
audio-default = []
feature1 = []
feature2 = []
enable-bob = []
"#;
        switchy_fs::sync::write(temp_path.join("Cargo.toml"), cargo_toml).unwrap();

        let clippier_toml = r#"
[[config]]
os = "ubuntu"
skip-features = ["*-default"]
"#;
        switchy_fs::sync::write(temp_path.join("clippier.toml"), clippier_toml).unwrap();

        let result = process_configs(
            temp_path, None, None, None, false, false, None, None, None, None,
        )
        .await
        .unwrap();

        assert!(!result.is_empty());
        let features = result[0].get("features").unwrap().as_array().unwrap();
        let feature_names: Vec<String> = features
            .iter()
            .map(|f| f.as_str().unwrap().to_string())
            .collect();

        // Should skip all features ending with -default
        assert!(!feature_names.contains(&"bob-default".to_string()));
        assert!(!feature_names.contains(&"sally-default".to_string()));
        assert!(!feature_names.contains(&"audio-default".to_string()));

        // Should keep other features
        assert!(feature_names.contains(&"default".to_string()));
        assert!(feature_names.contains(&"feature1".to_string()));
        assert!(feature_names.contains(&"feature2".to_string()));
        assert!(feature_names.contains(&"enable-bob".to_string()));
    }

    #[switchy_async::test]
    async fn test_wildcard_skip_features_prefix() {
        // Test wildcard pattern matching with test-*
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();

        let cargo_toml = r#"
[package]
name = "test-package"
version = "0.1.0"

[features]
default = []
test-utils = []
test-integration = []
test-e2e = []
feature1 = []
production = []
"#;
        switchy_fs::sync::write(temp_path.join("Cargo.toml"), cargo_toml).unwrap();

        let clippier_toml = r#"
[[config]]
os = "ubuntu"
skip-features = ["test-*"]
"#;
        switchy_fs::sync::write(temp_path.join("clippier.toml"), clippier_toml).unwrap();

        let result = process_configs(
            temp_path, None, None, None, false, false, None, None, None, None,
        )
        .await
        .unwrap();

        assert!(!result.is_empty());
        let features = result[0].get("features").unwrap().as_array().unwrap();
        let feature_names: Vec<String> = features
            .iter()
            .map(|f| f.as_str().unwrap().to_string())
            .collect();

        // Should skip all features starting with test-
        assert!(!feature_names.contains(&"test-utils".to_string()));
        assert!(!feature_names.contains(&"test-integration".to_string()));
        assert!(!feature_names.contains(&"test-e2e".to_string()));

        // Should keep other features
        assert!(feature_names.contains(&"default".to_string()));
        assert!(feature_names.contains(&"feature1".to_string()));
        assert!(feature_names.contains(&"production".to_string()));
    }

    #[switchy_async::test]
    async fn test_wildcard_skip_features_single_char() {
        // Test single character wildcard with ?
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();

        let cargo_toml = r#"
[package]
name = "test-package"
version = "0.1.0"

[features]
default = []
v1 = []
v2 = []
v3 = []
v10 = []
version = []
"#;
        switchy_fs::sync::write(temp_path.join("Cargo.toml"), cargo_toml).unwrap();

        let clippier_toml = r#"
[[config]]
os = "ubuntu"
skip-features = ["v?"]
"#;
        switchy_fs::sync::write(temp_path.join("clippier.toml"), clippier_toml).unwrap();

        let result = process_configs(
            temp_path, None, None, None, false, false, None, None, None, None,
        )
        .await
        .unwrap();

        assert!(!result.is_empty());
        let features = result[0].get("features").unwrap().as_array().unwrap();
        let feature_names: Vec<String> = features
            .iter()
            .map(|f| f.as_str().unwrap().to_string())
            .collect();

        // Should skip v1, v2, v3 (exactly 2 chars starting with v)
        assert!(!feature_names.contains(&"v1".to_string()));
        assert!(!feature_names.contains(&"v2".to_string()));
        assert!(!feature_names.contains(&"v3".to_string()));

        // Should keep v10 (3 chars) and version (7 chars)
        assert!(feature_names.contains(&"v10".to_string()));
        assert!(feature_names.contains(&"version".to_string()));
        assert!(feature_names.contains(&"default".to_string()));
    }

    #[switchy_async::test]
    async fn test_negation_skip_all_except_one() {
        // Test negation pattern: skip all except enable-bob
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();

        let cargo_toml = r#"
[package]
name = "test-package"
version = "0.1.0"

[features]
default = []
feature1 = []
feature2 = []
enable-bob = []
enable-sally = []
"#;
        switchy_fs::sync::write(temp_path.join("Cargo.toml"), cargo_toml).unwrap();

        let clippier_toml = r#"
[[config]]
os = "ubuntu"
skip-features = ["*", "!enable-bob"]
"#;
        switchy_fs::sync::write(temp_path.join("clippier.toml"), clippier_toml).unwrap();

        let result = process_configs(
            temp_path, None, None, None, false, false, None, None, None, None,
        )
        .await
        .unwrap();

        assert!(!result.is_empty());
        let features = result[0].get("features").unwrap().as_array().unwrap();
        let feature_names: Vec<String> = features
            .iter()
            .map(|f| f.as_str().unwrap().to_string())
            .collect();

        // Should only keep enable-bob
        assert!(feature_names.contains(&"enable-bob".to_string()));

        // Should skip everything else
        assert!(!feature_names.contains(&"default".to_string()));
        assert!(!feature_names.contains(&"feature1".to_string()));
        assert!(!feature_names.contains(&"feature2".to_string()));
        assert!(!feature_names.contains(&"enable-sally".to_string()));
    }

    #[switchy_async::test]
    async fn test_negation_skip_all_except_pattern() {
        // Test negation with wildcard: skip all except enable-*
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();

        let cargo_toml = r#"
[package]
name = "test-package"
version = "0.1.0"

[features]
default = []
feature1 = []
enable-bob = []
enable-sally = []
enable-feature = []
disable-test = []
"#;
        switchy_fs::sync::write(temp_path.join("Cargo.toml"), cargo_toml).unwrap();

        let clippier_toml = r#"
[[config]]
os = "ubuntu"
skip-features = ["*", "!enable-*"]
"#;
        switchy_fs::sync::write(temp_path.join("clippier.toml"), clippier_toml).unwrap();

        let result = process_configs(
            temp_path, None, None, None, false, false, None, None, None, None,
        )
        .await
        .unwrap();

        assert!(!result.is_empty());
        let features = result[0].get("features").unwrap().as_array().unwrap();
        let feature_names: Vec<String> = features
            .iter()
            .map(|f| f.as_str().unwrap().to_string())
            .collect();

        // Should keep all enable-* features
        assert!(feature_names.contains(&"enable-bob".to_string()));
        assert!(feature_names.contains(&"enable-sally".to_string()));
        assert!(feature_names.contains(&"enable-feature".to_string()));

        // Should skip everything else
        assert!(!feature_names.contains(&"default".to_string()));
        assert!(!feature_names.contains(&"feature1".to_string()));
        assert!(!feature_names.contains(&"disable-test".to_string()));
    }

    #[switchy_async::test]
    async fn test_complex_combined_patterns() {
        // Test complex combination: skip *-default and test-*, but keep test-utils
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();

        let cargo_toml = r#"
[package]
name = "test-package"
version = "0.1.0"

[features]
default = []
bob-default = []
sally-default = []
test-integration = []
test-e2e = []
test-utils = []
feature1 = []
production = []
"#;
        switchy_fs::sync::write(temp_path.join("Cargo.toml"), cargo_toml).unwrap();

        let clippier_toml = r#"
[[config]]
os = "ubuntu"
skip-features = ["*-default", "test-*", "!test-utils"]
"#;
        switchy_fs::sync::write(temp_path.join("clippier.toml"), clippier_toml).unwrap();

        let result = process_configs(
            temp_path, None, None, None, false, false, None, None, None, None,
        )
        .await
        .unwrap();

        assert!(!result.is_empty());
        let features = result[0].get("features").unwrap().as_array().unwrap();
        let feature_names: Vec<String> = features
            .iter()
            .map(|f| f.as_str().unwrap().to_string())
            .collect();

        // Should skip all *-default features
        assert!(!feature_names.contains(&"bob-default".to_string()));
        assert!(!feature_names.contains(&"sally-default".to_string()));

        // Should skip test-* features except test-utils
        assert!(!feature_names.contains(&"test-integration".to_string()));
        assert!(!feature_names.contains(&"test-e2e".to_string()));
        assert!(feature_names.contains(&"test-utils".to_string())); // Kept by negation

        // Should keep other features
        assert!(feature_names.contains(&"default".to_string()));
        assert!(feature_names.contains(&"feature1".to_string()));
        assert!(feature_names.contains(&"production".to_string()));
    }

    #[switchy_async::test]
    async fn test_command_line_wildcard_override() {
        // Test combining command line wildcards with config file patterns
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();

        let cargo_toml = r#"
[package]
name = "test-package"
version = "0.1.0"

[features]
default = []
bob-default = []
test-utils = []
test-e2e = []
feature1 = []
simd = []
"#;
        switchy_fs::sync::write(temp_path.join("Cargo.toml"), cargo_toml).unwrap();

        let clippier_toml = r#"
[[config]]
os = "ubuntu"
skip-features = ["*-default"]
"#;
        switchy_fs::sync::write(temp_path.join("clippier.toml"), clippier_toml).unwrap();

        let result = process_configs(
            temp_path,
            None,
            None,
            None,
            false,
            false,
            None,
            None,
            Some(&["test-*".to_string()]), // Command line wildcard
            None,
        )
        .await
        .unwrap();

        assert!(!result.is_empty());
        let features = result[0].get("features").unwrap().as_array().unwrap();
        let feature_names: Vec<String> = features
            .iter()
            .map(|f| f.as_str().unwrap().to_string())
            .collect();

        // Should skip *-default (from config) and test-* (from command line)
        assert!(!feature_names.contains(&"bob-default".to_string()));
        assert!(!feature_names.contains(&"test-utils".to_string()));
        assert!(!feature_names.contains(&"test-e2e".to_string()));

        // Should keep other features
        assert!(feature_names.contains(&"default".to_string()));
        assert!(feature_names.contains(&"feature1".to_string()));
        assert!(feature_names.contains(&"simd".to_string()));
    }

    #[switchy_async::test]
    async fn test_changed_files_deduplication() {
        let mut files = vec![
            "packages/api/src/lib.rs".to_string(),
            "packages/core/src/lib.rs".to_string(),
            "packages/api/src/lib.rs".to_string(),
            "packages/models/Cargo.toml".to_string(),
        ];

        files.sort();
        files.dedup();

        assert_eq!(files.len(), 3);
        assert_eq!(
            files,
            vec![
                "packages/api/src/lib.rs",
                "packages/core/src/lib.rs",
                "packages/models/Cargo.toml",
            ]
        );
    }

    #[switchy_async::test]
    async fn test_package_list_merging() {
        let mut file_affected = vec!["api".to_string(), "core".to_string()];
        let external_affected = vec!["models".to_string(), "api".to_string()];

        file_affected.extend(external_affected);
        file_affected.sort();
        file_affected.dedup();

        assert_eq!(file_affected.len(), 3);
        assert_eq!(file_affected, vec!["api", "core", "models"]);
    }

    #[switchy_async::test]
    async fn test_package_filtering_intersection() {
        use std::collections::HashSet;

        let selected: HashSet<String> = ["api", "web", "cli", "core"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();

        let affected: HashSet<String> = ["api", "core", "models"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();

        let mut result: Vec<String> = selected
            .iter()
            .filter(|pkg| affected.contains(*pkg))
            .cloned()
            .collect();
        result.sort();

        assert_eq!(result.len(), 2);
        assert!(result.contains(&"api".to_string()));
        assert!(result.contains(&"core".to_string()));
        assert!(!result.contains(&"web".to_string()));
        assert!(!result.contains(&"models".to_string()));
    }

    #[switchy_async::test]
    async fn test_empty_changed_files_deduplication() {
        let mut files: Vec<String> = vec![];
        files.sort();
        files.dedup();
        assert_eq!(files.len(), 0);
    }

    #[switchy_async::test]
    async fn test_all_duplicate_files() {
        let mut files = vec![
            "packages/api/src/lib.rs".to_string(),
            "packages/api/src/lib.rs".to_string(),
            "packages/api/src/lib.rs".to_string(),
        ];

        files.sort();
        files.dedup();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0], "packages/api/src/lib.rs");
    }

    #[switchy_async::test]
    async fn test_package_filtering_no_overlap() {
        use std::collections::HashSet;

        let selected: HashSet<String> = ["api", "web"].iter().map(|s| (*s).to_string()).collect();

        let affected: HashSet<String> = ["models", "core"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();

        let count = selected
            .iter()
            .filter(|pkg| affected.contains(*pkg))
            .count();

        assert_eq!(count, 0);
    }

    #[switchy_async::test]
    async fn test_package_filtering_all_selected_affected() {
        use std::collections::HashSet;

        let selected: HashSet<String> = ["api", "web"].iter().map(|s| (*s).to_string()).collect();

        let affected: HashSet<String> = ["api", "web", "core", "models"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();

        let mut result: Vec<String> = selected
            .iter()
            .filter(|pkg| affected.contains(*pkg))
            .cloned()
            .collect();
        result.sort();

        assert_eq!(result.len(), 2);
        assert!(result.contains(&"api".to_string()));
        assert!(result.contains(&"web".to_string()));
    }

    #[switchy_async::test]
    async fn test_features_wildcard_expansion() {
        // Test that --features supports wildcard expansion
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();

        let cargo_toml = r#"
[package]
name = "test-package"
version = "0.1.0"

[features]
default = []
enable-bob = []
enable-sally = []
enable-feature = []
disable-test = []
production = []
"#;
        switchy_fs::sync::write(temp_path.join("Cargo.toml"), cargo_toml).unwrap();

        let clippier_toml = r#"
[[config]]
os = "ubuntu"
"#;
        switchy_fs::sync::write(temp_path.join("clippier.toml"), clippier_toml).unwrap();

        // Test with wildcard pattern in specific_features
        let result = process_configs(
            temp_path,
            None,
            None,
            None,
            false,
            false,
            None,
            Some(&["enable-*".to_string()]), // specific_features with wildcard
            None,
            None,
        )
        .await
        .unwrap();

        assert!(!result.is_empty());
        let features = result[0].get("features").unwrap().as_array().unwrap();
        let feature_names: Vec<String> = features
            .iter()
            .map(|f| f.as_str().unwrap().to_string())
            .collect();

        // Should only include enable-* features
        assert!(feature_names.contains(&"enable-bob".to_string()));
        assert!(feature_names.contains(&"enable-sally".to_string()));
        assert!(feature_names.contains(&"enable-feature".to_string()));

        // Should not include other features
        assert!(!feature_names.contains(&"default".to_string()));
        assert!(!feature_names.contains(&"disable-test".to_string()));
        assert!(!feature_names.contains(&"production".to_string()));
    }

    #[switchy_async::test]
    async fn test_features_multiple_wildcard_patterns() {
        // Test that --features supports multiple wildcard patterns
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();

        let cargo_toml = r#"
[package]
name = "test-package"
version = "0.1.0"

[features]
default = []
enable-bob = []
enable-sally = []
test-utils = []
test-integration = []
production = []
development = []
"#;
        switchy_fs::sync::write(temp_path.join("Cargo.toml"), cargo_toml).unwrap();

        let clippier_toml = r#"
[[config]]
os = "ubuntu"
"#;
        switchy_fs::sync::write(temp_path.join("clippier.toml"), clippier_toml).unwrap();

        // Test with multiple wildcard patterns
        let result = process_configs(
            temp_path,
            None,
            None,
            None,
            false,
            false,
            None,
            Some(&["enable-*".to_string(), "test-*".to_string()]),
            None,
            None,
        )
        .await
        .unwrap();

        assert!(!result.is_empty());
        let features = result[0].get("features").unwrap().as_array().unwrap();
        let feature_names: Vec<String> = features
            .iter()
            .map(|f| f.as_str().unwrap().to_string())
            .collect();

        // Should include enable-* and test-* features
        assert!(feature_names.contains(&"enable-bob".to_string()));
        assert!(feature_names.contains(&"enable-sally".to_string()));
        assert!(feature_names.contains(&"test-utils".to_string()));
        assert!(feature_names.contains(&"test-integration".to_string()));

        // Should not include other features
        assert!(!feature_names.contains(&"default".to_string()));
        assert!(!feature_names.contains(&"production".to_string()));
        assert!(!feature_names.contains(&"development".to_string()));
    }

    #[switchy_async::test]
    async fn test_features_mixed_exact_and_wildcard() {
        // Test mixing exact feature names with wildcard patterns
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();

        let cargo_toml = r#"
[package]
name = "test-package"
version = "0.1.0"

[features]
default = []
enable-bob = []
enable-sally = []
production = []
development = []
"#;
        switchy_fs::sync::write(temp_path.join("Cargo.toml"), cargo_toml).unwrap();

        let clippier_toml = r#"
[[config]]
os = "ubuntu"
"#;
        switchy_fs::sync::write(temp_path.join("clippier.toml"), clippier_toml).unwrap();

        // Test with mix of exact and wildcard
        let result = process_configs(
            temp_path,
            None,
            None,
            None,
            false,
            false,
            None,
            Some(&["enable-*".to_string(), "production".to_string()]),
            None,
            None,
        )
        .await
        .unwrap();

        assert!(!result.is_empty());
        let features = result[0].get("features").unwrap().as_array().unwrap();
        let feature_names: Vec<String> = features
            .iter()
            .map(|f| f.as_str().unwrap().to_string())
            .collect();

        // Should include enable-* features and production
        assert!(feature_names.contains(&"enable-bob".to_string()));
        assert!(feature_names.contains(&"enable-sally".to_string()));
        assert!(feature_names.contains(&"production".to_string()));

        // Should not include other features
        assert!(!feature_names.contains(&"default".to_string()));
        assert!(!feature_names.contains(&"development".to_string()));
    }

    #[switchy_async::test]
    async fn test_matches_pattern_helper() {
        // Test the matches_pattern helper function
        assert!(matches_pattern("bob-default", "*-default"));
        assert!(matches_pattern("sally-default", "*-default"));
        assert!(!matches_pattern("default", "*-default"));

        assert!(matches_pattern("test-utils", "test-*"));
        assert!(matches_pattern("test-integration", "test-*"));
        assert!(!matches_pattern("utils", "test-*"));

        assert!(matches_pattern("v1", "v?"));
        assert!(matches_pattern("v2", "v?"));
        assert!(!matches_pattern("v10", "v?"));

        assert!(matches_pattern("exact", "exact"));
        assert!(!matches_pattern("exact", "exac"));
    }

    #[switchy_async::test]
    async fn test_expand_pattern_list_helper() {
        // Test the expand_pattern_list helper function
        let available = vec![
            "default".to_string(),
            "bob-default".to_string(),
            "sally-default".to_string(),
            "enable-bob".to_string(),
            "production".to_string(),
        ];

        // Test wildcard expansion
        let patterns = vec!["*-default".to_string()];
        let expanded = expand_pattern_list(&patterns, &available);
        assert_eq!(expanded.len(), 2);
        assert!(expanded.contains(&"bob-default".to_string()));
        assert!(expanded.contains(&"sally-default".to_string()));

        // Test mixed exact and wildcard
        let patterns = vec!["*-default".to_string(), "production".to_string()];
        let expanded = expand_pattern_list(&patterns, &available);
        assert_eq!(expanded.len(), 3);
        assert!(expanded.contains(&"bob-default".to_string()));
        assert!(expanded.contains(&"sally-default".to_string()));
        assert!(expanded.contains(&"production".to_string()));

        // Test exact match for non-existent item (should still be included)
        let patterns = vec!["nonexistent".to_string()];
        let expanded = expand_pattern_list(&patterns, &available);
        assert_eq!(expanded.len(), 1);
        assert!(expanded.contains(&"nonexistent".to_string()));
    }

    #[switchy_async::test]
    async fn test_required_features_wildcard_expansion() {
        // Test that --required-features wildcards are expanded in the output
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();

        let cargo_toml = r#"
[package]
name = "test-package"
version = "0.1.0"

[features]
default = []
enable-bob = []
enable-sally = []
enable-feature = []
production = []
development = []
"#;
        switchy_fs::sync::write(temp_path.join("Cargo.toml"), cargo_toml).unwrap();

        let clippier_toml = r#"
[[config]]
os = "ubuntu"
"#;
        switchy_fs::sync::write(temp_path.join("clippier.toml"), clippier_toml).unwrap();

        // Test with wildcard pattern in required_features
        let result = process_configs(
            temp_path,
            None,
            None,
            None,
            false,
            false,
            None,
            None,                                                      // specific_features
            None,                                                      // skip_features
            Some(&["enable-*".to_string(), "production".to_string()]), // required_features
        )
        .await
        .unwrap();

        assert!(!result.is_empty());
        let required_features = result[0]
            .get("requiredFeatures")
            .unwrap()
            .as_array()
            .unwrap();
        let required_feature_names: Vec<String> = required_features
            .iter()
            .map(|f| f.as_str().unwrap().to_string())
            .collect();

        // Should have expanded enable-* to concrete feature names
        assert!(required_feature_names.contains(&"enable-bob".to_string()));
        assert!(required_feature_names.contains(&"enable-sally".to_string()));
        assert!(required_feature_names.contains(&"enable-feature".to_string()));
        assert!(required_feature_names.contains(&"production".to_string()));

        // Should NOT contain the wildcard pattern itself
        assert!(!required_feature_names.contains(&"enable-*".to_string()));

        // Should not contain features that don't match
        assert!(!required_feature_names.contains(&"default".to_string()));
        assert!(!required_feature_names.contains(&"development".to_string()));
    }

    #[switchy_async::test]
    async fn test_required_features_from_config_file() {
        // Test that required-features in config file also get expanded
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();

        let cargo_toml = r#"
[package]
name = "test-package"
version = "0.1.0"

[features]
default = []
test-utils = []
test-integration = []
test-e2e = []
production = []
"#;
        switchy_fs::sync::write(temp_path.join("Cargo.toml"), cargo_toml).unwrap();

        let clippier_toml = r#"
[[config]]
os = "ubuntu"
required-features = ["test-*"]
"#;
        switchy_fs::sync::write(temp_path.join("clippier.toml"), clippier_toml).unwrap();

        let result = process_configs(
            temp_path, None, None, None, false, false, None, None, None,
            None, // No command line override
        )
        .await
        .unwrap();

        assert!(!result.is_empty());
        let required_features = result[0]
            .get("requiredFeatures")
            .unwrap()
            .as_array()
            .unwrap();
        let required_feature_names: Vec<String> = required_features
            .iter()
            .map(|f| f.as_str().unwrap().to_string())
            .collect();

        // Should have expanded test-* from config file
        assert!(required_feature_names.contains(&"test-utils".to_string()));
        assert!(required_feature_names.contains(&"test-integration".to_string()));
        assert!(required_feature_names.contains(&"test-e2e".to_string()));

        // Should NOT contain the wildcard pattern
        assert!(!required_feature_names.contains(&"test-*".to_string()));

        // Should not contain non-matching features
        assert!(!required_feature_names.contains(&"default".to_string()));
        assert!(!required_feature_names.contains(&"production".to_string()));
    }

    #[switchy_async::test]
    async fn test_expand_features_from_cargo_toml_helper() {
        // Test the expand_features_from_cargo_toml helper function
        let cargo_toml_str = r"
[features]
default = []
enable-bob = []
enable-sally = []
production = []
";
        let cargo_toml: Value = toml::from_str(cargo_toml_str).unwrap();

        // Test wildcard expansion
        let patterns = vec!["enable-*".to_string()];
        let expanded = expand_features_from_cargo_toml(&cargo_toml, &patterns);
        assert_eq!(expanded.len(), 2);
        assert!(expanded.contains(&"enable-bob".to_string()));
        assert!(expanded.contains(&"enable-sally".to_string()));

        // Test mixed patterns
        let patterns = vec!["enable-*".to_string(), "production".to_string()];
        let expanded = expand_features_from_cargo_toml(&cargo_toml, &patterns);
        assert_eq!(expanded.len(), 3);
        assert!(expanded.contains(&"enable-bob".to_string()));
        assert!(expanded.contains(&"enable-sally".to_string()));
        assert!(expanded.contains(&"production".to_string()));
    }

    #[switchy_async::test]
    async fn test_features_negation_all_except_one() {
        // Test --features with negation: include all except one specific feature
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();

        let cargo_toml = r#"
[package]
name = "test-package"
version = "0.1.0"

[features]
default = []
enable-bob = []
enable-sally = []
enable-experimental = []
production = []
"#;
        switchy_fs::sync::write(temp_path.join("Cargo.toml"), cargo_toml).unwrap();

        let clippier_toml = r#"
[[config]]
os = "ubuntu"
"#;
        switchy_fs::sync::write(temp_path.join("clippier.toml"), clippier_toml).unwrap();

        // Test: --features "*,!enable-experimental"
        let result = process_configs(
            temp_path,
            None,
            None,
            None,
            false,
            false,
            None,
            Some(&["*".to_string(), "!enable-experimental".to_string()]),
            None,
            None,
        )
        .await
        .unwrap();

        assert!(!result.is_empty());
        let features = result[0].get("features").unwrap().as_array().unwrap();
        let feature_names: Vec<String> = features
            .iter()
            .map(|f| f.as_str().unwrap().to_string())
            .collect();

        // Should include all features except enable-experimental
        assert!(feature_names.contains(&"default".to_string()));
        assert!(feature_names.contains(&"enable-bob".to_string()));
        assert!(feature_names.contains(&"enable-sally".to_string()));
        assert!(feature_names.contains(&"production".to_string()));

        // Should NOT include enable-experimental
        assert!(!feature_names.contains(&"enable-experimental".to_string()));
    }

    #[switchy_async::test]
    async fn test_features_negation_wildcard() {
        // Test --features with wildcard negation: include all except test-*
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();

        let cargo_toml = r#"
[package]
name = "test-package"
version = "0.1.0"

[features]
default = []
production = []
test-utils = []
test-integration = []
test-e2e = []
enable-bob = []
"#;
        switchy_fs::sync::write(temp_path.join("Cargo.toml"), cargo_toml).unwrap();

        let clippier_toml = r#"
[[config]]
os = "ubuntu"
"#;
        switchy_fs::sync::write(temp_path.join("clippier.toml"), clippier_toml).unwrap();

        // Test: --features "*,!test-*"
        let result = process_configs(
            temp_path,
            None,
            None,
            None,
            false,
            false,
            None,
            Some(&["*".to_string(), "!test-*".to_string()]),
            None,
            None,
        )
        .await
        .unwrap();

        assert!(!result.is_empty());
        let features = result[0].get("features").unwrap().as_array().unwrap();
        let feature_names: Vec<String> = features
            .iter()
            .map(|f| f.as_str().unwrap().to_string())
            .collect();

        // Should include non-test features
        assert!(feature_names.contains(&"default".to_string()));
        assert!(feature_names.contains(&"production".to_string()));
        assert!(feature_names.contains(&"enable-bob".to_string()));

        // Should NOT include any test-* features
        assert!(!feature_names.contains(&"test-utils".to_string()));
        assert!(!feature_names.contains(&"test-integration".to_string()));
        assert!(!feature_names.contains(&"test-e2e".to_string()));
    }

    #[switchy_async::test]
    async fn test_features_negation_complex() {
        // Test complex negation: enable-* except enable-experimental
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();

        let cargo_toml = r#"
[package]
name = "test-package"
version = "0.1.0"

[features]
default = []
enable-bob = []
enable-sally = []
enable-experimental = []
production = []
test-utils = []
"#;
        switchy_fs::sync::write(temp_path.join("Cargo.toml"), cargo_toml).unwrap();

        let clippier_toml = r#"
[[config]]
os = "ubuntu"
"#;
        switchy_fs::sync::write(temp_path.join("clippier.toml"), clippier_toml).unwrap();

        // Test: --features "enable-*,!enable-experimental,production"
        let result = process_configs(
            temp_path,
            None,
            None,
            None,
            false,
            false,
            None,
            Some(&[
                "enable-*".to_string(),
                "!enable-experimental".to_string(),
                "production".to_string(),
            ]),
            None,
            None,
        )
        .await
        .unwrap();

        assert!(!result.is_empty());
        let features = result[0].get("features").unwrap().as_array().unwrap();
        let feature_names: Vec<String> = features
            .iter()
            .map(|f| f.as_str().unwrap().to_string())
            .collect();

        // Should include enable-bob, enable-sally, and production
        assert!(feature_names.contains(&"enable-bob".to_string()));
        assert!(feature_names.contains(&"enable-sally".to_string()));
        assert!(feature_names.contains(&"production".to_string()));

        // Should NOT include enable-experimental, default, or test-utils
        assert!(!feature_names.contains(&"enable-experimental".to_string()));
        assert!(!feature_names.contains(&"default".to_string()));
        assert!(!feature_names.contains(&"test-utils".to_string()));
    }

    #[switchy_async::test]
    async fn test_required_features_negation() {
        // Test that --required-features also supports negation
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();

        let cargo_toml = r#"
[package]
name = "test-package"
version = "0.1.0"

[features]
default = []
enable-bob = []
enable-sally = []
enable-experimental = []
production = []
"#;
        switchy_fs::sync::write(temp_path.join("Cargo.toml"), cargo_toml).unwrap();

        let clippier_toml = r#"
[[config]]
os = "ubuntu"
"#;
        switchy_fs::sync::write(temp_path.join("clippier.toml"), clippier_toml).unwrap();

        // Test: --required-features "enable-*,!enable-experimental"
        let result = process_configs(
            temp_path,
            None,
            None,
            None,
            false,
            false,
            None,
            None,
            None,
            Some(&["enable-*".to_string(), "!enable-experimental".to_string()]),
        )
        .await
        .unwrap();

        assert!(!result.is_empty());
        let required_features = result[0]
            .get("requiredFeatures")
            .unwrap()
            .as_array()
            .unwrap();
        let required_feature_names: Vec<String> = required_features
            .iter()
            .map(|f| f.as_str().unwrap().to_string())
            .collect();

        // Should include enable-bob and enable-sally
        assert!(required_feature_names.contains(&"enable-bob".to_string()));
        assert!(required_feature_names.contains(&"enable-sally".to_string()));

        // Should NOT include enable-experimental
        assert!(!required_feature_names.contains(&"enable-experimental".to_string()));
    }

    #[switchy_async::test]
    async fn test_expand_pattern_list_with_negation() {
        // Test the expand_pattern_list helper with negation
        let available = vec![
            "default".to_string(),
            "enable-bob".to_string(),
            "enable-sally".to_string(),
            "enable-experimental".to_string(),
            "production".to_string(),
        ];

        // Test: all except one
        let patterns = vec!["*".to_string(), "!enable-experimental".to_string()];
        let expanded = expand_pattern_list(&patterns, &available);
        assert_eq!(expanded.len(), 4);
        assert!(expanded.contains(&"default".to_string()));
        assert!(expanded.contains(&"enable-bob".to_string()));
        assert!(expanded.contains(&"enable-sally".to_string()));
        assert!(expanded.contains(&"production".to_string()));
        assert!(!expanded.contains(&"enable-experimental".to_string()));

        // Test: wildcard with wildcard negation
        let patterns = vec!["enable-*".to_string(), "!enable-experimental".to_string()];
        let expanded = expand_pattern_list(&patterns, &available);
        assert_eq!(expanded.len(), 2);
        assert!(expanded.contains(&"enable-bob".to_string()));
        assert!(expanded.contains(&"enable-sally".to_string()));
        assert!(!expanded.contains(&"enable-experimental".to_string()));
    }

    // Tests for should_skip_feature glob pattern matching
    #[switchy_async::test]
    async fn test_should_skip_feature_exact_match() {
        assert!(should_skip_feature("default", &["default".to_string()]));
        assert!(!should_skip_feature("test-utils", &["default".to_string()]));
    }

    #[switchy_async::test]
    async fn test_should_skip_feature_wildcard_suffix() {
        assert!(should_skip_feature("test-utils", &["test-*".to_string()]));
        assert!(should_skip_feature("test-foo", &["test-*".to_string()]));
        assert!(!should_skip_feature("utils-test", &["test-*".to_string()]));
    }

    #[switchy_async::test]
    async fn test_should_skip_feature_wildcard_prefix() {
        assert!(should_skip_feature("mp3-codec", &["*-codec".to_string()]));
        assert!(should_skip_feature("flac-codec", &["*-codec".to_string()]));
        assert!(!should_skip_feature("codec-mp3", &["*-codec".to_string()]));
    }

    #[switchy_async::test]
    async fn test_should_skip_feature_wildcard_anywhere() {
        assert!(should_skip_feature(
            "test_foo_bar",
            &["*_foo_*".to_string()]
        ));
        assert!(should_skip_feature(
            "prefix_foo_suffix",
            &["*_foo_*".to_string()]
        ));
        assert!(!should_skip_feature(
            "test_bar_baz",
            &["*_foo_*".to_string()]
        ));
    }

    #[switchy_async::test]
    async fn test_should_skip_feature_question_mark_single_char() {
        assert!(should_skip_feature("test1", &["test?".to_string()]));
        assert!(should_skip_feature("testX", &["test?".to_string()]));
        assert!(!should_skip_feature("test12", &["test?".to_string()]));
        assert!(!should_skip_feature("test", &["test?".to_string()]));
    }

    #[switchy_async::test]
    async fn test_should_skip_feature_skip_all_with_asterisk() {
        assert!(should_skip_feature("anything", &["*".to_string()]));
        assert!(should_skip_feature("default", &["*".to_string()]));
        assert!(should_skip_feature("fail-on-warnings", &["*".to_string()]));
    }

    #[switchy_async::test]
    async fn test_should_skip_feature_negation_basic() {
        // Negation with ! prefix should NOT skip the matched item
        assert!(!should_skip_feature(
            "keep-this",
            &["!keep-this".to_string()]
        ));
        // Items that don't match the negation pattern are not affected (default is false)
        assert!(!should_skip_feature(
            "skip-this",
            &["!keep-this".to_string()]
        ));
    }

    #[switchy_async::test]
    async fn test_should_skip_feature_negation_overrides_wildcard() {
        // "*" skips all, but "!fail-on-warnings" keeps it
        let patterns = vec!["*".to_string(), "!fail-on-warnings".to_string()];

        assert!(!should_skip_feature("fail-on-warnings", &patterns));
        assert!(should_skip_feature("default", &patterns));
        assert!(should_skip_feature("test-utils", &patterns));
    }

    #[switchy_async::test]
    async fn test_should_skip_feature_negation_with_glob_pattern() {
        // Skip all test-* features except test-important
        let patterns = vec!["test-*".to_string(), "!test-important".to_string()];

        assert!(should_skip_feature("test-utils", &patterns));
        assert!(should_skip_feature("test-fixtures", &patterns));
        assert!(!should_skip_feature("test-important", &patterns));
        assert!(!should_skip_feature("production", &patterns));
    }

    #[switchy_async::test]
    async fn test_should_skip_feature_multiple_patterns_no_overlap() {
        let patterns = vec!["test-*".to_string(), "*-codec".to_string()];

        assert!(should_skip_feature("test-utils", &patterns));
        assert!(should_skip_feature("mp3-codec", &patterns));
        assert!(!should_skip_feature("fail-on-warnings", &patterns));
    }

    #[switchy_async::test]
    async fn test_should_skip_feature_order_matters_for_negation() {
        // Last pattern wins
        let patterns1 = vec!["*".to_string(), "!keep".to_string()];
        let patterns2 = vec!["!keep".to_string(), "*".to_string()];

        assert!(!should_skip_feature("keep", &patterns1)); // Kept by negation
        assert!(should_skip_feature("keep", &patterns2)); // Skipped by wildcard
    }

    #[switchy_async::test]
    async fn test_should_skip_feature_empty_pattern_list() {
        assert!(!should_skip_feature("anything", &[]));
    }

    #[switchy_async::test]
    async fn test_should_skip_feature_complex_real_world_scenario() {
        // Skip all codecs except opus, skip all test features except test-integration
        let patterns = vec![
            "*-codec".to_string(),
            "!opus-codec".to_string(),
            "test-*".to_string(),
            "!test-integration".to_string(),
        ];

        // Codec tests
        assert!(should_skip_feature("mp3-codec", &patterns));
        assert!(should_skip_feature("flac-codec", &patterns));
        assert!(!should_skip_feature("opus-codec", &patterns));

        // Test feature tests
        assert!(should_skip_feature("test-utils", &patterns));
        assert!(should_skip_feature("test-fixtures", &patterns));
        assert!(!should_skip_feature("test-integration", &patterns));

        // Other features
        assert!(!should_skip_feature("fail-on-warnings", &patterns));
    }

    #[switchy_async::test]
    async fn test_should_skip_feature_case_sensitivity() {
        // Patterns should be case-sensitive (Rust convention)
        assert!(should_skip_feature("Test", &["Test".to_string()]));
        assert!(!should_skip_feature("test", &["Test".to_string()]));
        assert!(should_skip_feature("test", &["test".to_string()]));
    }

    #[switchy_async::test]
    async fn test_should_skip_feature_multiple_negations() {
        let patterns = vec!["*".to_string(), "!keep1".to_string(), "!keep2".to_string()];

        assert!(!should_skip_feature("keep1", &patterns));
        assert!(!should_skip_feature("keep2", &patterns));
        assert!(should_skip_feature("skip-this", &patterns));
    }

    #[switchy_async::test]
    async fn test_should_skip_feature_overlapping_patterns() {
        // Multiple patterns that could match the same feature
        let patterns = vec!["test-*".to_string(), "*-utils".to_string()];

        assert!(should_skip_feature("test-utils", &patterns)); // Matches both
        assert!(should_skip_feature("test-fixtures", &patterns)); // Matches first
        assert!(should_skip_feature("string-utils", &patterns)); // Matches second
        assert!(!should_skip_feature("production", &patterns)); // Matches neither
    }

    #[switchy_async::test]
    async fn test_should_skip_feature_special_characters() {
        // Test patterns with hyphens, underscores, numbers
        assert!(should_skip_feature(
            "test-2024-feature",
            &["test-*".to_string()]
        ));
        assert!(should_skip_feature(
            "feature_v1_2_3",
            &["feature_v*".to_string()]
        ));
        assert!(should_skip_feature(
            "enable-foo-bar-baz",
            &["enable-*".to_string()]
        ));
    }

    #[switchy_async::test]
    async fn test_should_skip_feature_empty_string() {
        // Edge case: empty string feature name
        assert!(!should_skip_feature("", &["test-*".to_string()]));
        assert!(should_skip_feature("", &["*".to_string()]));
    }

    #[switchy_async::test]
    async fn test_should_skip_feature_negation_without_match() {
        // Negation pattern that doesn't match anything shouldn't affect results
        let patterns = vec!["test-*".to_string(), "!nonexistent".to_string()];

        assert!(should_skip_feature("test-utils", &patterns));
        assert!(!should_skip_feature("production", &patterns));
    }

    #[switchy_async::test]
    async fn test_should_skip_feature_complex_wildcards() {
        // Test multiple wildcards in one pattern
        assert!(should_skip_feature(
            "prefix-middle-suffix",
            &["prefix-*-suffix".to_string()]
        ));
        assert!(should_skip_feature("a-b-c-d-e", &["a-*-e".to_string()]));
        assert!(!should_skip_feature("a-b-c-d", &["a-*-e".to_string()]));
    }
}
