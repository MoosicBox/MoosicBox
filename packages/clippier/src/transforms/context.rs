//! Transform context providing workspace metadata and analysis capabilities.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use toml::Value;

use crate::WorkspaceContext;

/// Context available to transform scripts
#[derive(Clone)]
pub struct TransformContext {
    packages: BTreeMap<String, PackageInfo>,
}

/// Package metadata
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    pub path: PathBuf,
    #[serde(skip, default = "default_cargo_toml")]
    pub cargo_toml: Value,
    pub features: BTreeMap<String, Vec<String>>,
    pub dependencies: Vec<DependencyInfo>,
}

fn default_cargo_toml() -> Value {
    Value::Table(toml::map::Map::new())
}

/// Dependency information
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DependencyInfo {
    pub name: String,
    pub optional: bool,
    pub workspace_member: bool,
    pub features: Vec<String>,
}

impl TransformContext {
    /// Create a new transform context by analyzing the workspace
    ///
    /// # Errors
    ///
    /// * Workspace root not found
    /// * Failed to read package metadata
    /// * Invalid Cargo.toml files
    pub fn new(workspace_root: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let workspace = WorkspaceContext::new(workspace_root)?;

        // Load all package metadata
        let mut packages = BTreeMap::new();
        workspace.ensure_fully_loaded();

        // Get all workspace members
        let members = get_workspace_members(&workspace, workspace_root)?;

        for (name, path) in members {
            let cargo_path = path.join("Cargo.toml");
            if !cargo_path.exists() {
                continue;
            }

            let cargo_toml: Value = toml::from_str(&std::fs::read_to_string(&cargo_path)?)?;

            let features = extract_features(&cargo_toml);
            let dependencies = extract_dependencies(&cargo_toml, &workspace, &path);

            packages.insert(
                name.clone(),
                PackageInfo {
                    name,
                    path,
                    cargo_toml,
                    features,
                    dependencies,
                },
            );
        }

        Ok(Self { packages })
    }

    /// Get package metadata by name
    #[must_use]
    pub fn get_package(&self, name: &str) -> Option<&PackageInfo> {
        self.packages.get(name)
    }

    /// Check if a name is a workspace member
    #[must_use]
    pub fn is_workspace_member(&self, name: &str) -> bool {
        self.packages.contains_key(name)
    }

    /// Get all package names
    #[must_use]
    pub fn get_all_packages(&self) -> Vec<String> {
        self.packages.keys().cloned().collect()
    }

    /// Check if a package depends on another package
    #[must_use]
    pub fn package_depends_on(&self, package: &str, dependency: &str) -> bool {
        self.packages
            .get(package)
            .is_some_and(|pkg| pkg.dependencies.iter().any(|dep| dep.name == dependency))
    }

    /// Check if a feature exists in a package
    #[must_use]
    pub fn feature_exists(&self, package: &str, feature: &str) -> bool {
        self.packages
            .get(package)
            .is_some_and(|pkg| pkg.features.contains_key(feature))
    }
}

impl PackageInfo {
    /// Check if this package depends on another package
    #[must_use]
    pub fn depends_on(&self, dep_name: &str) -> bool {
        self.dependencies.iter().any(|dep| dep.name == dep_name)
    }

    /// Check if a feature exists
    #[must_use]
    pub fn has_feature(&self, feature: &str) -> bool {
        self.features.contains_key(feature)
    }

    /// Get feature definition
    #[must_use]
    pub fn feature_definition(&self, feature: &str) -> Option<&Vec<String>> {
        self.features.get(feature)
    }

    /// Get dependencies activated by a feature
    #[must_use]
    pub fn feature_activates_dependencies(&self, feature: &str) -> Vec<DependencyInfo> {
        let Some(feature_def) = self.features.get(feature) else {
            return vec![];
        };

        let mut activated_deps = vec![];

        for entry in feature_def {
            if entry.contains('/') {
                let parts: Vec<_> = entry.split('/').collect();
                let dep_name = parts[0].trim_end_matches('?');
                let dep_feature = parts[1];

                // Find the dependency
                if let Some(dep) = self.dependencies.iter().find(|d| d.name == dep_name) {
                    let mut dep_info = dep.clone();
                    dep_info.features = vec![dep_feature.to_string()];
                    activated_deps.push(dep_info);
                }
            }
        }

        activated_deps
    }

    /// Check if a feature is skipped on a specific OS (from clippier.toml)
    #[must_use]
    pub fn skips_feature_on_os(&self, feature: &str, os: &str) -> bool {
        let clippier_path = self.path.join("clippier.toml");
        let Ok(content) = std::fs::read_to_string(clippier_path) else {
            return false;
        };

        let Ok(conf) = toml::from_str::<Value>(&content) else {
            return false;
        };

        // Check if any OS config has skip-features containing this feature
        if let Some(configs) = conf.get("config").and_then(|c| c.as_array()) {
            for config in configs {
                if let Some(config_os) = config.get("os").and_then(|o| o.as_str())
                    && (config_os == os || os.contains(config_os))
                    && let Some(skip_features) = config.get("skip-features")
                    && let Some(arr) = skip_features.as_array()
                {
                    for skip_feature in arr {
                        if skip_feature.as_str() == Some(feature) {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    /// Get all features
    #[must_use]
    pub fn get_all_features(&self) -> Vec<String> {
        self.features.keys().cloned().collect()
    }
}

/// Extract features from Cargo.toml
fn extract_features(cargo_toml: &Value) -> BTreeMap<String, Vec<String>> {
    let Some(features_table) = cargo_toml.get("features").and_then(|f| f.as_table()) else {
        return BTreeMap::new();
    };

    let mut features = BTreeMap::new();

    for (name, value) in features_table {
        if let Some(arr) = value.as_array() {
            let feature_list: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(ToString::to_string))
                .collect();
            features.insert(name.clone(), feature_list);
        }
    }

    features
}

/// Extract dependencies from Cargo.toml
fn extract_dependencies(
    cargo_toml: &Value,
    workspace: &WorkspaceContext,
    _package_path: &Path,
) -> Vec<DependencyInfo> {
    let mut dependencies = vec![];

    let sections = ["dependencies", "dev-dependencies", "build-dependencies"];

    for section in &sections {
        let Some(deps_table) = cargo_toml.get(section).and_then(|d| d.as_table()) else {
            continue;
        };

        for (name, value) in deps_table {
            let optional = value
                .as_table()
                .and_then(|table| table.get("optional"))
                .and_then(Value::as_bool)
                .unwrap_or(false);

            let features = value
                .as_table()
                .and_then(|table| table.get("features"))
                .and_then(Value::as_array)
                .map_or_else(Vec::new, |feat_array| {
                    feat_array
                        .iter()
                        .filter_map(|v| v.as_str().map(ToString::to_string))
                        .collect()
                });

            let workspace_member = workspace.is_member_by_name(name);

            dependencies.push(DependencyInfo {
                name: name.clone(),
                optional,
                workspace_member,
                features,
            });
        }
    }

    dependencies
}

/// Get all workspace members
fn get_workspace_members(
    _workspace: &WorkspaceContext,
    workspace_root: &Path,
) -> Result<BTreeMap<String, PathBuf>, Box<dyn std::error::Error>> {
    let mut members = BTreeMap::new();

    let workspace_cargo = workspace_root.join("Cargo.toml");
    let content = std::fs::read_to_string(workspace_cargo)?;
    let workspace_toml: Value = toml::from_str(&content)?;

    if let Some(member_patterns) = workspace_toml
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
    {
        for pattern in member_patterns {
            if let Some(pattern_str) = pattern.as_str() {
                // Simple glob expansion - handle */foo pattern
                if pattern_str.contains('*') {
                    let parts: Vec<_> = pattern_str.split('/').collect();
                    if parts[0] == "*" || parts[0] == "**" {
                        // Scan directories
                        if let Ok(entries) = std::fs::read_dir(workspace_root) {
                            for entry in entries.flatten() {
                                if entry.file_type().is_ok_and(|ft| ft.is_dir()) {
                                    let member_path = entry.path();
                                    let cargo_path = member_path.join("Cargo.toml");
                                    if cargo_path.exists()
                                        && let Some(name) =
                                            WorkspaceContext::read_package_name(&member_path)
                                    {
                                        members.insert(name, member_path);
                                    }
                                }
                            }
                        }
                    }
                } else {
                    let member_path = workspace_root.join(pattern_str);
                    if member_path.exists()
                        && let Some(name) = WorkspaceContext::read_package_name(&member_path)
                    {
                        members.insert(name, member_path);
                    }
                }
            }
        }
    }

    Ok(members)
}
