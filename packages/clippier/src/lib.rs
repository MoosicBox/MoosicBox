#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::Path,
};

use clap::ValueEnum;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use toml::Value;

#[cfg(feature = "git-diff")]
pub mod git_diff;

#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;

#[cfg(any(test, feature = "test-utils"))]
pub use test_utils::*;

// Core types for tests and CLI
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[clap(rename_all = "kebab_case")]
pub enum OutputType {
    Json,
    Raw,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AffectedPackageInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Vec<String>>,
}

// Cargo.lock related types for test utilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CargoLock {
    pub version: u32,
    pub package: Vec<CargoLockPackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CargoLockPackage {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum DependencyFilteredByFeatures {
    Command {
        command: String,
        features: Option<Vec<String>>,
    },
    Toolchain {
        toolchain: String,
        features: Option<Vec<String>>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ClippierEnv {
    Value(String),
    FilteredValue {
        value: String,
        features: Option<Vec<String>>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum VecOrItem<T> {
    Value(T),
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ClippierConfiguration {
    pub ci_steps: Option<VecOrItem<DependencyFilteredByFeatures>>,
    pub cargo: Option<VecOrItem<String>>,
    pub env: Option<BTreeMap<String, ClippierEnv>>,
    pub dependencies: Option<Vec<DependencyFilteredByFeatures>>,
    pub os: String,
    pub skip_features: Option<Vec<String>>,
    pub required_features: Option<Vec<String>>,
    pub name: Option<String>,
    pub nightly: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ParallelizationConfig {
    pub chunked: u16,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ClippierConf {
    pub ci_steps: Option<VecOrItem<DependencyFilteredByFeatures>>,
    pub cargo: Option<VecOrItem<String>>,
    pub config: Vec<ClippierConfiguration>,
    pub env: Option<BTreeMap<String, ClippierEnv>>,
    pub parallelization: Option<ParallelizationConfig>,
    pub nightly: Option<bool>,
}

#[derive(Debug, Clone)]
pub enum FeaturesList {
    Chunked(Vec<Vec<String>>),
    NotChunked(Vec<String>),
}

// Utility functions - these are working implementations for tests
pub fn split<T>(slice: &[T], n: usize) -> impl Iterator<Item = &[T]> {
    if slice.is_empty() || n == 0 {
        return SplitIter::empty();
    }

    let chunk_size = slice.len().div_ceil(n);
    SplitIter::new(slice, chunk_size)
}

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

#[must_use]
pub fn process_features(features: Vec<String>, chunked: Option<u16>, spread: bool) -> FeaturesList {
    if let Some(max_features_per_chunk) = chunked {
        let chunk_size = max_features_per_chunk as usize;

        if spread && features.len() > chunk_size {
            // First, determine how many chunks we need based on chunk size
            let num_chunks = features.len().div_ceil(chunk_size);

            // Then distribute features evenly across those chunks
            let mut result = vec![Vec::new(); num_chunks];
            for (i, feature) in features.into_iter().enumerate() {
                result[i % num_chunks].push(feature);
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

    let mut features: Vec<String> = specific_features.map_or_else(
        || {
            features_table
                .keys()
                .filter(|k| !k.starts_with('_'))
                .cloned()
                .collect()
        },
        <[String]>::to_vec,
    );

    if let Some(skip) = skip_features {
        features.retain(|f| !skip.contains(f));
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

#[must_use]
pub fn is_workspace_dependency(dep_value: &Value) -> bool {
    match dep_value {
        Value::Table(table) => table.get("workspace") == Some(&Value::Boolean(true)),
        _ => false,
    }
}

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

#[must_use]
pub fn get_binary_name(
    workspace_root: &Path,
    target_package: &str,
    target_package_path: &str,
) -> String {
    // Try to read the target package's Cargo.toml to get the correct binary name
    let cargo_path = workspace_root.join(target_package_path).join("Cargo.toml");

    if let Ok(source) = std::fs::read_to_string(&cargo_path) {
        if let Ok(value) = toml::from_str::<Value>(&source) {
            // Check for explicit binary definitions
            if let Some(bins) = value.get("bin").and_then(|b| b.as_array()) {
                // Use the first binary definition if it has a name
                if let Some(bin) = bins.first() {
                    if let Some(bin_name) = bin.get("name").and_then(|n| n.as_str()) {
                        return bin_name.to_string();
                    }
                }
            }

            // Check for a single binary definition (not array)
            if let Some(bin) = value.get("bin").and_then(|b| b.as_table()) {
                if let Some(bin_name) = bin.get("name").and_then(|n| n.as_str()) {
                    return bin_name.to_string();
                }
            }
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
#[allow(clippy::too_many_arguments)]
pub fn process_configs(
    path: &Path,
    offset: Option<u16>,
    max: Option<u16>,
    chunked: Option<u16>,
    spread: bool,
    specific_features: Option<&[String]>,
    skip_features_override: Option<&[String]>,
    required_features_override: Option<&[String]>,
) -> Result<Vec<serde_json::Map<String, serde_json::Value>>, Box<dyn std::error::Error>> {
    log::debug!("Loading file '{}'", path.display());
    let cargo_path = path.join("Cargo.toml");
    let source = std::fs::read_to_string(cargo_path)?;
    let value: Value = toml::from_str(&source)?;

    let conf_path = path.join("clippier.toml");
    let conf = if conf_path.is_file() {
        let source = std::fs::read_to_string(conf_path)?;
        let value: ClippierConf = toml::from_str(&source)?;
        Some(value)
    } else {
        None
    };

    log::debug!("{} conf={conf:?}", path.display());

    let configs = conf.as_ref().map_or_else(
        || {
            vec![ClippierConfiguration {
                os: "ubuntu".to_string(),
                dependencies: None,
                env: None,
                cargo: None,
                name: None,
                ci_steps: None,
                skip_features: None,
                required_features: None,
                nightly: None,
            }]
        },
        |x| x.config.clone(),
    );

    let mut packages = vec![];

    if let Some(name) = value
        .get("package")
        .and_then(|x| x.get("name"))
        .and_then(|x| x.as_str())
        .map(str::to_string)
    {
        for config in configs {
            let features = fetch_features(
                &value,
                offset,
                max,
                specific_features,
                skip_features_override.or(config.skip_features.as_deref()),
                required_features_override.or(config.required_features.as_deref()),
            );
            let features = process_features(
                features,
                conf.as_ref()
                    .and_then(|x| x.parallelization.as_ref().map(|x| x.chunked))
                    .or(chunked),
                spread,
            );
            match &features {
                FeaturesList::Chunked(x) => {
                    for features in x {
                        packages.push(create_map(
                            conf.as_ref(),
                            &config,
                            path.to_str().unwrap(),
                            &name,
                            required_features_override.or(config.required_features.as_deref()),
                            features,
                        )?);
                    }
                }
                FeaturesList::NotChunked(x) => {
                    packages.push(create_map(
                        conf.as_ref(),
                        &config,
                        path.to_str().unwrap(),
                        &name,
                        required_features_override.or(config.required_features.as_deref()),
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
) -> Result<Vec<serde_json::Map<String, serde_json::Value>>, Box<dyn std::error::Error>> {
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
            // Combine multiple packages - this increases features per package as expected
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
#[allow(clippy::too_many_lines)]
pub fn create_map(
    conf: Option<&ClippierConf>,
    config: &ClippierConfiguration,
    file: &str,
    name: &str,
    required_features: Option<&[String]>,
    features: &[String],
) -> Result<serde_json::Map<String, serde_json::Value>, Box<dyn std::error::Error>> {
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
            .unwrap_or_default()
            .into(),
    );

    if let Some(dependencies) = &config.dependencies {
        let matches = dependencies
            .iter()
            .filter(|x| {
                let target_features = match x {
                    DependencyFilteredByFeatures::Command { features, .. }
                    | DependencyFilteredByFeatures::Toolchain { features, .. } => features,
                };

                target_features.as_ref().is_none_or(|f| {
                    f.iter()
                        .any(|required| features.iter().any(|x| x == required))
                })
            })
            .collect::<Vec<_>>();

        if !matches.is_empty() {
            let dependencies = matches
                .iter()
                .filter_map(|x| match x {
                    DependencyFilteredByFeatures::Command { command, .. } => Some(command),
                    DependencyFilteredByFeatures::Toolchain { .. } => None,
                })
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
                .filter_map(|x| match x {
                    DependencyFilteredByFeatures::Toolchain { toolchain, .. } => Some(toolchain),
                    DependencyFilteredByFeatures::Command { .. } => None,
                })
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

    let mut env = conf
        .and_then(|x| x.env.as_ref())
        .cloned()
        .unwrap_or_default();
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

    let mut cargo: Vec<_> = conf
        .and_then(|x| x.cargo.as_ref())
        .cloned()
        .unwrap_or_default()
        .into();
    let config_cargo: Vec<_> = config.cargo.clone().unwrap_or_default().into();
    cargo.extend(config_cargo);

    if !cargo.is_empty() {
        map.insert("cargo".to_string(), serde_json::to_value(cargo.join(" "))?);
    }

    let mut ci_steps: Vec<_> = conf
        .and_then(|x| x.ci_steps.as_ref())
        .cloned()
        .unwrap_or_default()
        .into();
    let config_ci_steps: Vec<_> = config.ci_steps.clone().unwrap_or_default().into();
    ci_steps.extend(config_ci_steps);

    let matches = ci_steps
        .iter()
        .filter(|x| {
            let target_features = match x {
                DependencyFilteredByFeatures::Command { features, .. }
                | DependencyFilteredByFeatures::Toolchain { features, .. } => features,
            };

            target_features.as_ref().is_none_or(|f| {
                f.iter()
                    .any(|required| features.iter().any(|x| x == required))
            })
        })
        .collect::<Vec<_>>();

    if !matches.is_empty() {
        let commands = matches
            .iter()
            .filter_map(|x| match x {
                DependencyFilteredByFeatures::Command { command, .. } => Some(command),
                DependencyFilteredByFeatures::Toolchain { .. } => None,
            })
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
            .filter_map(|x| match x {
                DependencyFilteredByFeatures::Toolchain { toolchain, .. } => Some(toolchain),
                DependencyFilteredByFeatures::Command { .. } => None,
            })
            .map(String::as_str)
            .collect::<Vec<_>>();

        if !toolchains.is_empty() {
            map.insert(
                "toolchains".to_string(),
                serde_json::to_value(toolchains.join("\n"))?,
            );
        }
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
) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    log::trace!("🔍 Finding workspace dependencies for package: {target_package}");
    if let Some(features) = enabled_features {
        log::trace!("📋 Enabled features: {features:?}");
    } else {
        log::trace!("📋 Using default features");
    }

    // First, load the workspace and get all members
    let workspace_cargo_path = workspace_root.join("Cargo.toml");
    log::trace!(
        "📂 Loading workspace from: {}",
        workspace_cargo_path.display()
    );
    let workspace_source = std::fs::read_to_string(&workspace_cargo_path)?;
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

        if !cargo_path.exists() {
            log::trace!("⚠️  Skipping {member_path}: Cargo.toml not found");
            continue;
        }

        log::trace!("📄 Processing package: {member_path}");
        let source = std::fs::read_to_string(&cargo_path)?;
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
            let deps = extract_workspace_dependencies(&value, all_potential_deps);
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
        if current_package != target_package {
            if let Some(package_path) = package_paths.get(&current_package) {
                resolved_dependencies.insert((current_package.clone(), package_path.clone()));
            }
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
fn extract_workspace_dependencies(cargo_value: &Value, all_potential_deps: bool) -> Vec<String> {
    let mut deps = Vec::new();

    // Helper function to extract deps from a dependency section
    let extract_from_section = |section: &Value| -> Vec<String> {
        let mut section_deps = Vec::new();
        if let Some(dependencies) = section.as_table() {
            for (dep_name, dep_value) in dependencies {
                if all_potential_deps {
                    if is_workspace_dependency(dep_value) {
                        section_deps.push(dep_name.clone());
                    }
                } else {
                    // For non-all-potential mode, include workspace deps that are either:
                    // 1. Non-optional, or
                    // 2. Optional but activated by default features
                    if is_workspace_dependency(dep_value) {
                        if let Value::Table(table) = dep_value {
                            if table.get("optional") == Some(&Value::Boolean(true)) {
                                // Optional dependency - check if activated by default features
                                if is_optional_dependency_activated(cargo_value, dep_name, None) {
                                    section_deps.push(dep_name.clone());
                                }
                            } else {
                                // Non-optional workspace dependency
                                section_deps.push(dep_name.clone());
                            }
                        } else {
                            // Simple workspace dependency (not a table)
                            section_deps.push(dep_name.clone());
                        }
                    }
                }
            }
        }
        section_deps
    };

    // Check regular dependencies
    if let Some(dependencies) = cargo_value.get("dependencies") {
        deps.extend(extract_from_section(dependencies));
    }

    // Check dev dependencies
    if let Some(dev_dependencies) = cargo_value.get("dev-dependencies") {
        deps.extend(extract_from_section(dev_dependencies));
    }

    // Check build dependencies
    if let Some(build_dependencies) = cargo_value.get("build-dependencies") {
        deps.extend(extract_from_section(build_dependencies));
    }

    // Remove duplicates
    deps.sort();
    deps.dedup();
    deps
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
    if let Some(dependencies) = cargo_value.get("dependencies").and_then(|d| d.as_table()) {
        if let Some(dep_value) = dependencies.get(dep_name) {
            if is_workspace_dependency(dep_value) {
                // Check if it's optional
                if let Value::Table(table) = dep_value {
                    if table.get("optional") == Some(&Value::Boolean(true)) {
                        // Optional dependency - check if activated by features
                        return is_optional_dependency_activated(
                            cargo_value,
                            dep_name,
                            enabled_features,
                        );
                    }
                }
                // Non-optional workspace dependency is always activated
                return true;
            }
        }
    }

    // Check dev and build dependencies
    for section_name in ["dev-dependencies", "build-dependencies"] {
        if let Some(section) = cargo_value.get(section_name).and_then(|d| d.as_table()) {
            if let Some(dep_value) = section.get(dep_name) {
                if is_workspace_dependency(dep_value) {
                    return true; // Dev and build deps are typically always enabled
                }
            }
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
pub fn generate_dockerfile(
    workspace_root: &Path,
    target_package: &str,
    enabled_features: Option<&[String]>,
    dockerfile_path: &Path,
    base_image: &str,
    final_image: &str,
    port: Option<u16>,
    build_args: Option<&str>,
    generate_dockerignore: bool,
) -> Result<(), Box<dyn std::error::Error>> {
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
        base_image,
        final_image,
        port,
        build_args,
        workspace_root,
        target_package_path,
    )?;

    // Write the Dockerfile
    std::fs::write(dockerfile_path, dockerfile_content)?;

    if generate_dockerignore {
        let dockerignore_content =
            generate_dockerignore_content(&dependencies, target_package, enabled_features)?;
        let dockerignore_path = dockerfile_path.with_extension("dockerignore");
        std::fs::write(dockerignore_path, dockerignore_content)?;
    }

    Ok(())
}

/// Generates the content of a Dockerfile for a target package
///
/// # Errors
///
/// * If IO error occurs
#[allow(
    clippy::too_many_lines,
    clippy::too_many_arguments,
    clippy::cognitive_complexity
)]
pub fn generate_dockerfile_content(
    dependencies: &[(String, String)],
    target_package: &str,
    enabled_features: Option<&[String]>,
    base_image: &str,
    final_image: &str,
    port: Option<u16>,
    build_args: Option<&str>,
    workspace_root: &Path,
    target_package_path: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    use std::fmt::Write as _;

    let mut content = String::new();

    // Collect environment variables for the target package early
    let env_vars = collect_environment_variables(
        workspace_root,
        target_package,
        target_package_path,
        enabled_features,
        "ubuntu",
    )?;

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
        collect_system_dependencies(workspace_root, dependencies, enabled_features, "ubuntu")?;

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
        let mut packages: Vec<String> = install_packages.into_iter().collect();
        packages.sort();
        writeln!(content, "    apt-get -y install {}", packages.join(" "))?;
        if custom_commands.is_empty() {
            content.push('\n');
        } else {
            writeln!(content, " && \\")?;
        }
    }

    // Add custom commands
    for (i, cmd) in custom_commands.iter().enumerate() {
        if cmd.starts_with("sudo ") {
            // Remove sudo since we're already running as root in Docker
            let cmd_without_sudo = cmd.strip_prefix("sudo ").unwrap_or(cmd);
            writeln!(content, "    {cmd_without_sudo}")?;
        } else {
            writeln!(content, "    {cmd}")?;
        }

        if i < custom_commands.len() - 1 {
            writeln!(content, " && \\")?;
        } else {
            content.push('\n');
        }
    }

    content.push('\n');

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
    if target_cargo_path.exists() {
        let target_source = std::fs::read_to_string(&target_cargo_path)?;
        let target_value: Value = toml::from_str(&target_source)?;

        // Check if this package has a binary target
        let has_binary = target_value.get("bin").is_some()
            || workspace_root
                .join(target_package_path)
                .join("src/main.rs")
                .exists();

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

    // Check for feature flags
    let features = enabled_features.unwrap_or(&[]);
    let features_flag = if features.is_empty() {
        String::new()
    } else {
        format!("--features={}", features.join(","))
    };

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
    let binary_name = get_binary_name(workspace_root, target_package, target_package_path);
    writeln!(
        content,
        "COPY --from=builder /app/target/release/{binary_name} /"
    )?;

    // Expose port if specified
    if let Some(port) = port {
        writeln!(content, "EXPOSE {port}")?;
    }

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
    writeln!(content, "ENV MAX_THREADS=64")?;
    writeln!(content, "ENV ACTIX_WORKERS=32")?;

    // Final command
    if let Some(port) = port {
        writeln!(content, "CMD [\"./{binary_name}\", \"{port}\"]")?;
    } else {
        writeln!(content, "CMD [\"./{binary_name}\"]")?;
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
) -> Result<String, Box<dyn std::error::Error>> {
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

/// Finds packages that are affected by changed files
///
/// # Errors
///
/// * If IO error occurs
/// * If no workspace members are found
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
pub fn find_affected_packages(
    workspace_root: &Path,
    changed_files: &[String],
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    log::trace!("🔍 Finding affected packages for changed files: {changed_files:?}");

    // First, load the workspace and get all members
    let workspace_cargo_path = workspace_root.join("Cargo.toml");
    let workspace_source = std::fs::read_to_string(&workspace_cargo_path)?;
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

        if !cargo_path.exists() {
            log::trace!("⚠️  Skipping {member_path}: Cargo.toml not found");
            continue;
        }

        log::trace!("📄 Processing package: {member_path}");
        let source = std::fs::read_to_string(&cargo_path)?;
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
        let changed_path = std::path::PathBuf::from(changed_file);

        // Check if the changed file belongs to a workspace package
        for (package_path, package_name) in &package_path_to_name {
            let package_path_buf = std::path::PathBuf::from(package_path);

            // Check if the changed file is within this package's directory
            if changed_path.starts_with(&package_path_buf) {
                log::trace!("📝 File {changed_file} affects package {package_name}");
                directly_affected_packages.insert(package_name.clone());
            }
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
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
pub fn find_affected_packages_with_reasoning(
    workspace_root: &Path,
    changed_files: &[String],
) -> Result<Vec<AffectedPackageInfo>, Box<dyn std::error::Error>> {
    log::trace!("🔍 Finding affected packages with reasoning for changed files: {changed_files:?}");

    // First, load the workspace and get all members
    let workspace_cargo_path = workspace_root.join("Cargo.toml");
    let workspace_source = std::fs::read_to_string(&workspace_cargo_path)?;
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

        if !cargo_path.exists() {
            log::trace!("⚠️  Skipping {member_path}: Cargo.toml not found");
            continue;
        }

        log::trace!("📄 Processing package: {member_path}");
        let source = std::fs::read_to_string(&cargo_path)?;
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
        let changed_path = std::path::PathBuf::from(changed_file);

        // Check if the changed file belongs to a workspace package
        for (package_path, package_name) in &package_path_to_name {
            let package_path_buf = std::path::PathBuf::from(package_path);

            // Check if the changed file is within this package's directory
            if changed_path.starts_with(&package_path_buf) {
                log::trace!("📝 File {changed_file} affects package {package_name}");
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
pub fn collect_environment_variables(
    workspace_root: &Path,
    _target_package: &str,
    target_package_path: &str,
    enabled_features: Option<&[String]>,
    target_os: &str,
) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let path = workspace_root.join(target_package_path);

    // Skip if no clippier.toml exists for this package
    let clippier_path = path.join("clippier.toml");
    if !clippier_path.exists() {
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
        specific_features.as_deref(),
        None,
        None,
    )?;

    let mut env_vars = Vec::new();

    // Extract environment variables
    for package in packages {
        if let Some(os) = package.get("os").and_then(|v| v.as_str()) {
            if os == target_os {
                if let Some(env_str) = package.get("env").and_then(|v| v.as_str()) {
                    for line in env_str.lines() {
                        if let Some((key, value)) = line.split_once('=') {
                            env_vars.push((key.to_string(), value.to_string()));
                        }
                    }
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
pub fn collect_system_dependencies(
    workspace_root: &Path,
    dependencies: &[(String, String)],
    enabled_features: Option<&[String]>,
    target_os: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut all_deps = BTreeSet::new();

    // Convert features to comma-separated string for the dependencies command
    let features_str = enabled_features.map(|f| f.join(",")).unwrap_or_default();

    for (_, package_path) in dependencies {
        let path = workspace_root.join(package_path);

        // Skip if no clippier.toml exists for this package
        let clippier_path = path.join("clippier.toml");
        if !clippier_path.exists() {
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
            specific_features.as_deref(),
            None,
            None,
        )?;

        // Extract system dependencies
        for package in packages {
            if let Some(os) = package.get("os").and_then(|v| v.as_str()) {
                if os == target_os {
                    if let Some(deps) = package.get("dependencies").and_then(|v| v.as_str()) {
                        for dep in deps.lines() {
                            if !dep.trim().is_empty() {
                                all_deps.insert(dep.trim().to_string());
                            }
                        }
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

// Additional functions needed by tests
#[must_use]
pub fn parse_dependency_name(dependency_line: &str) -> String {
    // Simple implementation that extracts the first word (package name)
    dependency_line
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string()
}

// Additional types needed for CLI commands (moved from main.rs)
#[derive(Debug, Serialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PackageInfo {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceDepsResult {
    pub packages: Vec<PackageInfo>,
}

#[derive(Debug, Serialize)]
pub struct AffectedPackagesResult {
    pub affected_packages: Vec<AffectedPackageInfo>,
}

#[derive(Debug, Serialize)]
pub struct SinglePackageResult {
    pub package: String,
    pub affected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Vec<String>>,
    pub all_affected: Vec<AffectedPackageInfo>,
}

// Business logic functions for CLI commands

/// Handles the dependencies command
///
/// # Errors
///
/// * If fails to process configs or output results
pub fn handle_dependencies_command(
    file: &str,
    os: Option<&str>,
    features: Option<&str>,
    output: OutputType,
) -> Result<String, Box<dyn std::error::Error>> {
    use std::str::FromStr;

    let path = std::path::PathBuf::from_str(file)?;
    let specific_features = features.map(|f| f.split(',').map(str::to_string).collect::<Vec<_>>());

    let packages = process_workspace_configs(
        &path,
        None,
        None,
        None,
        false,
        specific_features.as_deref(),
        None,
        None,
    )?;

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
pub fn handle_environment_command(
    file: &str,
    os: Option<&str>,
    features: Option<&str>,
    output: OutputType,
) -> Result<String, Box<dyn std::error::Error>> {
    use std::str::FromStr;

    let path = std::path::PathBuf::from_str(file)?;
    let specific_features = features.map(|f| f.split(',').map(str::to_string).collect::<Vec<_>>());

    let packages = process_workspace_configs(
        &path,
        None,
        None,
        None,
        false,
        specific_features.as_deref(),
        None,
        None,
    )?;

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
pub fn handle_ci_steps_command(
    file: &str,
    os: Option<&str>,
    features: Option<&str>,
    output: OutputType,
) -> Result<String, Box<dyn std::error::Error>> {
    use std::str::FromStr;

    let path = std::path::PathBuf::from_str(file)?;
    let specific_features = features.map(|f| f.split(',').map(str::to_string).collect::<Vec<_>>());

    let packages = process_workspace_configs(
        &path,
        None,
        None,
        None,
        false,
        specific_features.as_deref(),
        None,
        None,
    )?;

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

/// Handles the features command
///
/// # Errors
///
/// * If fails to process configs or find affected packages
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::cognitive_complexity
)]
pub fn handle_features_command(
    file: &str,
    os: Option<&str>,
    offset: Option<u16>,
    max: Option<u16>,
    max_parallel: Option<u16>,
    chunked: Option<u16>,
    spread: bool,
    features: Option<&str>,
    skip_features: Option<&str>,
    required_features: Option<&str>,
    changed_files: Option<&[String]>,
    #[cfg(feature = "git-diff")] git_base: Option<&str>,
    #[cfg(feature = "git-diff")] git_head: Option<&str>,
    include_reasoning: bool,
    output: OutputType,
) -> Result<String, Box<dyn std::error::Error>> {
    use std::str::FromStr;

    let path = std::path::PathBuf::from_str(file)?;
    let specific_features = features.map(|f| f.split(',').map(str::to_string).collect::<Vec<_>>());
    let skip_features_list =
        skip_features.map(|f| f.split(',').map(str::to_string).collect::<Vec<_>>());
    let required_features_list =
        required_features.map(|f| f.split(',').map(str::to_string).collect::<Vec<_>>());

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
                    let workspace_source = std::fs::read_to_string(&workspace_cargo_path)?;
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
        let (mut affected_packages, affected_with_reasoning) = if include_reasoning {
            let with_reasoning = find_affected_packages_with_reasoning(&path, &all_changed_files)?;
            let packages: Vec<String> = with_reasoning.iter().map(|pkg| pkg.name.clone()).collect();
            (packages, Some(with_reasoning))
        } else {
            (find_affected_packages(&path, &all_changed_files)?, None)
        };

        // Add packages affected by external dependency changes and update reasoning if needed
        let mut updated_reasoning = affected_with_reasoning;
        for external_pkg in external_affected_packages {
            if !affected_packages.contains(&external_pkg) {
                affected_packages.push(external_pkg.clone());
                log::debug!("Added package affected by external dependencies: {external_pkg}");

                // If reasoning is enabled, add reasoning entry for external dependency affected package
                if include_reasoning {
                    if let Some(ref mut reasoning_data) = updated_reasoning {
                        // Get specific external dependencies that affected this package
                        let specific_deps =
                            external_dependency_mapping.get(&external_pkg).map_or_else(
                                || vec!["Affected by external dependency changes".to_string()],
                                |deps| {
                                    deps.iter()
                                        .map(|dep| {
                                            format!("Affected by external dependency: {dep}")
                                        })
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
        }

        // Sort for consistent output
        affected_packages.sort();

        // Update the reasoning data reference
        let affected_with_reasoning = updated_reasoning;

        let workspace_cargo_path = path.join("Cargo.toml");
        let workspace_source = std::fs::read_to_string(&workspace_cargo_path)?;
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

            if cargo_path.exists() {
                let source = std::fs::read_to_string(&cargo_path)?;
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
                    None,  // Don't chunk when filtering by changed files
                    false, // Don't spread when filtering by changed files
                    specific_features.as_deref(),
                    skip_features_list.as_deref(),
                    required_features_list.as_deref(),
                )?;

                // Add reasoning to packages if include_reasoning is true
                if let Some(ref reasoning_data) = affected_with_reasoning {
                    if let Some(pkg_reasoning) = reasoning_data
                        .iter()
                        .find(|pkg| pkg.name == affected_package)
                    {
                        if let Some(reasoning) = &pkg_reasoning.reasoning {
                            for package in &mut packages {
                                package.insert(
                                    "reasoning".to_string(),
                                    serde_json::to_value(reasoning)?,
                                );
                            }
                        }
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
        specific_features.as_deref(),
        skip_features_list.as_deref(),
        required_features_list.as_deref(),
    )?;

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
        packages = apply_max_parallel_rechunking(packages, max_parallel_limit as usize)?;
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
) -> Result<String, Box<dyn std::error::Error>> {
    let deps = find_workspace_dependencies(workspace_root, package, features, all_potential_deps)?;

    let result = if format == "json" {
        let result = WorkspaceDepsResult {
            packages: deps
                .into_iter()
                .map(|(name, path)| PackageInfo { name, path })
                .collect(),
        };
        serde_json::to_string_pretty(&result)?
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
pub fn handle_generate_dockerfile_command(
    workspace_root: &Path,
    package: &str,
    features: Option<&[String]>,
    output: &Path,
    base_image: &str,
    final_image: &str,
    port: Option<u16>,
    build_args: Option<&str>,
    generate_dockerignore: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    generate_dockerfile(
        workspace_root,
        package,
        features,
        output,
        base_image,
        final_image,
        port,
        build_args,
        generate_dockerignore,
    )?;

    Ok(format!("Generated Dockerfile at: {}", output.display()))
}

/// Handles the affected packages command
///
/// # Errors
///
/// * If fails to find affected packages
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn handle_affected_packages_command(
    workspace_root: &Path,
    changed_files: &[String],
    target_package: Option<&str>,
    #[cfg(feature = "git-diff")] git_base: Option<&str>,
    #[cfg(feature = "git-diff")] git_head: Option<&str>,
    include_reasoning: bool,
    output: OutputType,
) -> Result<String, Box<dyn std::error::Error>> {
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
                let workspace_source = std::fs::read_to_string(&workspace_cargo_path)?;
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
    let mut affected = if include_reasoning {
        find_affected_packages_with_reasoning(workspace_root, &all_changed_files)?
    } else {
        find_affected_packages(workspace_root, &all_changed_files)?
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
            OutputType::Json => serde_json::to_string_pretty(&result)?,
            OutputType::Raw => if is_affected { "true" } else { "false" }.to_string(),
        }
    } else {
        let result = AffectedPackagesResult {
            affected_packages: affected,
        };

        match output {
            OutputType::Json => serde_json::to_string_pretty(&result)?,
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
pub fn process_workspace_configs(
    workspace_path: &Path,
    offset: Option<u16>,
    max: Option<u16>,
    chunked: Option<u16>,
    spread: bool,
    specific_features: Option<&[String]>,
    skip_features_override: Option<&[String]>,
    required_features_override: Option<&[String]>,
) -> Result<Vec<serde_json::Map<String, serde_json::Value>>, Box<dyn std::error::Error>> {
    log::debug!(
        "Processing workspace configs from '{}'",
        workspace_path.display()
    );

    // First, check if this is a workspace root
    let workspace_cargo_path = workspace_path.join("Cargo.toml");
    let workspace_source = std::fs::read_to_string(&workspace_cargo_path)?;
    let workspace_value: Value = toml::from_str(&workspace_source)?;

    let workspace_members = workspace_value
        .get("workspace")
        .and_then(|x| x.get("members"))
        .and_then(|x| x.as_array())
        .and_then(|x| x.iter().map(|x| x.as_str()).collect::<Option<Vec<_>>>());

    workspace_members.map_or_else(
        || {
            process_configs(
                workspace_path,
                offset,
                max,
                chunked,
                spread,
                specific_features,
                skip_features_override,
                required_features_override,
            )
        },
        |members| {
            // This is a workspace root, process all members
            let mut all_packages = Vec::new();

            for member_path in members {
                let full_path = workspace_path.join(member_path);

                // Check if this member has a Cargo.toml file (basic validation)
                let cargo_path = full_path.join("Cargo.toml");
                if !cargo_path.exists() {
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
                    specific_features,
                    skip_features_override,
                    required_features_override,
                ) {
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
        },
    )
}
