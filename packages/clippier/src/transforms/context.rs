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
                // Simple glob expansion - handle patterns like "packages/*"
                if pattern_str.contains('*') {
                    let parts: Vec<_> = pattern_str.split('/').collect();

                    // Handle patterns like "packages/*" or "*"
                    if parts.last() == Some(&"*") || parts.last() == Some(&"**") {
                        let base_path = if parts.len() > 1 {
                            workspace_root.join(parts[..parts.len() - 1].join("/"))
                        } else {
                            workspace_root.to_path_buf()
                        };

                        // Scan directories in the base path
                        if base_path.exists()
                            && let Ok(entries) = std::fs::read_dir(&base_path)
                        {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_extract_features_with_valid_features() {
        let cargo_toml = toml::from_str::<Value>(
            r#"
[features]
default = ["feature1"]
feature1 = ["dep1/feature1"]
feature2 = []
"#,
        )
        .unwrap();

        let features = extract_features(&cargo_toml);

        assert_eq!(features.len(), 3);
        assert_eq!(features.get("default"), Some(&vec!["feature1".to_string()]));
        assert_eq!(
            features.get("feature1"),
            Some(&vec!["dep1/feature1".to_string()])
        );
        assert_eq!(features.get("feature2"), Some(&vec![]));
    }

    #[test]
    fn test_extract_features_empty_cargo_toml() {
        let cargo_toml = toml::from_str::<Value>("[package]\nname = \"test\"").unwrap();
        let features = extract_features(&cargo_toml);
        assert!(features.is_empty());
    }

    #[test]
    fn test_extract_features_non_array_values() {
        let cargo_toml = toml::from_str::<Value>(
            r#"
[features]
valid = ["dep1"]
invalid = "string_value"
"#,
        )
        .unwrap();

        let features = extract_features(&cargo_toml);
        assert_eq!(features.len(), 1);
        assert!(features.contains_key("valid"));
        assert!(!features.contains_key("invalid"));
    }

    #[test]
    fn test_package_info_depends_on() {
        let pkg_info = PackageInfo {
            name: "test_pkg".to_string(),
            path: PathBuf::from("/test"),
            cargo_toml: Value::Table(toml::map::Map::new()),
            features: BTreeMap::new(),
            dependencies: vec![
                DependencyInfo {
                    name: "dep1".to_string(),
                    optional: false,
                    workspace_member: false,
                    features: vec![],
                },
                DependencyInfo {
                    name: "dep2".to_string(),
                    optional: true,
                    workspace_member: true,
                    features: vec!["feature1".to_string()],
                },
            ],
        };

        assert!(pkg_info.depends_on("dep1"));
        assert!(pkg_info.depends_on("dep2"));
        assert!(!pkg_info.depends_on("dep3"));
    }

    #[test]
    fn test_package_info_has_feature() {
        let mut features = BTreeMap::new();
        features.insert("feature1".to_string(), vec![]);
        features.insert("feature2".to_string(), vec!["dep1/feature2".to_string()]);

        let pkg_info = PackageInfo {
            name: "test_pkg".to_string(),
            path: PathBuf::from("/test"),
            cargo_toml: Value::Table(toml::map::Map::new()),
            features,
            dependencies: vec![],
        };

        assert!(pkg_info.has_feature("feature1"));
        assert!(pkg_info.has_feature("feature2"));
        assert!(!pkg_info.has_feature("feature3"));
    }

    #[test]
    fn test_package_info_feature_definition() {
        let mut features = BTreeMap::new();
        features.insert(
            "test_feature".to_string(),
            vec!["dep1/feature1".to_string(), "dep2/feature2".to_string()],
        );

        let pkg_info = PackageInfo {
            name: "test_pkg".to_string(),
            path: PathBuf::from("/test"),
            cargo_toml: Value::Table(toml::map::Map::new()),
            features,
            dependencies: vec![],
        };

        let def = pkg_info.feature_definition("test_feature");
        assert!(def.is_some());
        assert_eq!(def.unwrap().len(), 2);

        assert!(pkg_info.feature_definition("nonexistent").is_none());
    }

    #[test]
    fn test_feature_activates_dependencies_basic() {
        let mut features = BTreeMap::new();
        features.insert(
            "test_feature".to_string(),
            vec![
                "dep1/feature1".to_string(),
                "standalone_feature".to_string(),
            ],
        );

        let pkg_info = PackageInfo {
            name: "test_pkg".to_string(),
            path: PathBuf::from("/test"),
            cargo_toml: Value::Table(toml::map::Map::new()),
            features,
            dependencies: vec![DependencyInfo {
                name: "dep1".to_string(),
                optional: false,
                workspace_member: false,
                features: vec![],
            }],
        };

        let activated = pkg_info.feature_activates_dependencies("test_feature");
        assert_eq!(activated.len(), 1);
        assert_eq!(activated[0].name, "dep1");
        assert_eq!(activated[0].features, vec!["feature1".to_string()]);
    }

    #[test]
    fn test_feature_activates_dependencies_with_optional() {
        let mut features = BTreeMap::new();
        features.insert(
            "test_feature".to_string(),
            vec!["dep1?/feature1".to_string(), "dep2/feature2".to_string()],
        );

        let pkg_info = PackageInfo {
            name: "test_pkg".to_string(),
            path: PathBuf::from("/test"),
            cargo_toml: Value::Table(toml::map::Map::new()),
            features,
            dependencies: vec![
                DependencyInfo {
                    name: "dep1".to_string(),
                    optional: true,
                    workspace_member: false,
                    features: vec![],
                },
                DependencyInfo {
                    name: "dep2".to_string(),
                    optional: false,
                    workspace_member: true,
                    features: vec![],
                },
            ],
        };

        let activated = pkg_info.feature_activates_dependencies("test_feature");
        assert_eq!(activated.len(), 2);
        assert_eq!(activated[0].name, "dep1");
        assert_eq!(activated[0].features, vec!["feature1".to_string()]);
        assert_eq!(activated[1].name, "dep2");
        assert_eq!(activated[1].features, vec!["feature2".to_string()]);
    }

    #[test]
    fn test_feature_activates_dependencies_nonexistent_feature() {
        let pkg_info = PackageInfo {
            name: "test_pkg".to_string(),
            path: PathBuf::from("/test"),
            cargo_toml: Value::Table(toml::map::Map::new()),
            features: BTreeMap::new(),
            dependencies: vec![],
        };

        let activated = pkg_info.feature_activates_dependencies("nonexistent");
        assert!(activated.is_empty());
    }

    #[test]
    fn test_feature_activates_dependencies_no_slash() {
        let mut features = BTreeMap::new();
        features.insert(
            "test_feature".to_string(),
            vec!["standalone".to_string(), "another_standalone".to_string()],
        );

        let pkg_info = PackageInfo {
            name: "test_pkg".to_string(),
            path: PathBuf::from("/test"),
            cargo_toml: Value::Table(toml::map::Map::new()),
            features,
            dependencies: vec![],
        };

        let activated = pkg_info.feature_activates_dependencies("test_feature");
        assert!(activated.is_empty());
    }

    #[test]
    fn test_skips_feature_on_os_no_clippier_toml() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pkg_info = PackageInfo {
            name: "test_pkg".to_string(),
            path: temp_dir.path().to_path_buf(),
            cargo_toml: Value::Table(toml::map::Map::new()),
            features: BTreeMap::new(),
            dependencies: vec![],
        };

        assert!(!pkg_info.skips_feature_on_os("feature1", "linux"));
    }

    #[test]
    fn test_skips_feature_on_os_with_config() {
        let temp_dir = tempfile::tempdir().unwrap();
        let clippier_toml = r#"
[[config]]
os = "windows"
skip-features = ["windows_only_feature"]

[[config]]
os = "linux"
skip-features = ["linux_only_feature"]
"#;
        fs::write(temp_dir.path().join("clippier.toml"), clippier_toml).unwrap();

        let pkg_info = PackageInfo {
            name: "test_pkg".to_string(),
            path: temp_dir.path().to_path_buf(),
            cargo_toml: Value::Table(toml::map::Map::new()),
            features: BTreeMap::new(),
            dependencies: vec![],
        };

        assert!(pkg_info.skips_feature_on_os("windows_only_feature", "windows"));
        assert!(pkg_info.skips_feature_on_os("linux_only_feature", "linux"));
        assert!(!pkg_info.skips_feature_on_os("windows_only_feature", "linux"));
        assert!(!pkg_info.skips_feature_on_os("nonexistent_feature", "linux"));
    }

    #[test]
    fn test_skips_feature_on_os_malformed_toml() {
        let temp_dir = tempfile::tempdir().unwrap();
        fs::write(
            temp_dir.path().join("clippier.toml"),
            "invalid toml content [[[",
        )
        .unwrap();

        let pkg_info = PackageInfo {
            name: "test_pkg".to_string(),
            path: temp_dir.path().to_path_buf(),
            cargo_toml: Value::Table(toml::map::Map::new()),
            features: BTreeMap::new(),
            dependencies: vec![],
        };

        assert!(!pkg_info.skips_feature_on_os("feature1", "linux"));
    }

    #[test]
    fn test_get_all_features() {
        let mut features = BTreeMap::new();
        features.insert("feature1".to_string(), vec![]);
        features.insert("feature2".to_string(), vec!["dep1/feature2".to_string()]);
        features.insert("feature3".to_string(), vec![]);

        let pkg_info = PackageInfo {
            name: "test_pkg".to_string(),
            path: PathBuf::from("/test"),
            cargo_toml: Value::Table(toml::map::Map::new()),
            features,
            dependencies: vec![],
        };

        let all_features = pkg_info.get_all_features();
        assert_eq!(all_features.len(), 3);
        assert!(all_features.contains(&"feature1".to_string()));
        assert!(all_features.contains(&"feature2".to_string()));
        assert!(all_features.contains(&"feature3".to_string()));
    }

    #[test]
    fn test_extract_dependencies_all_sections() {
        let cargo_toml = toml::from_str::<Value>(
            r#"
[package]
name = "test_pkg"

[dependencies]
regular_dep = "1.0"
optional_dep = { version = "1.0", optional = true, features = ["feature1"] }

[dev-dependencies]
dev_dep = "2.0"

[build-dependencies]
build_dep = { version = "3.0", features = ["build_feature"] }
"#,
        )
        .unwrap();

        let temp_dir = tempfile::tempdir().unwrap();
        fs::write(
            temp_dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = []",
        )
        .unwrap();

        let workspace = WorkspaceContext::new(temp_dir.path()).unwrap();
        let deps = extract_dependencies(&cargo_toml, &workspace, temp_dir.path());

        assert_eq!(deps.len(), 4);

        let regular_dep = deps.iter().find(|d| d.name == "regular_dep").unwrap();
        assert!(!regular_dep.optional);
        assert!(regular_dep.features.is_empty());

        let optional_dep = deps.iter().find(|d| d.name == "optional_dep").unwrap();
        assert!(optional_dep.optional);
        assert_eq!(optional_dep.features, vec!["feature1".to_string()]);

        let dev_dep = deps.iter().find(|d| d.name == "dev_dep").unwrap();
        assert!(!dev_dep.optional);

        let build_dep = deps.iter().find(|d| d.name == "build_dep").unwrap();
        assert_eq!(build_dep.features, vec!["build_feature".to_string()]);
    }

    #[test]
    fn test_extract_dependencies_empty() {
        let cargo_toml = toml::from_str::<Value>("[package]\nname = \"test_pkg\"").unwrap();

        let temp_dir = tempfile::tempdir().unwrap();
        fs::write(
            temp_dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = []",
        )
        .unwrap();

        let workspace = WorkspaceContext::new(temp_dir.path()).unwrap();
        let deps = extract_dependencies(&cargo_toml, &workspace, temp_dir.path());

        assert!(deps.is_empty());
    }

    #[test]
    fn test_get_workspace_members_simple() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();

        // Create workspace
        fs::write(
            root.join("Cargo.toml"),
            r#"
[workspace]
members = ["pkg1", "pkg2"]
"#,
        )
        .unwrap();

        // Create pkg1
        let pkg1 = root.join("pkg1");
        fs::create_dir(&pkg1).unwrap();
        fs::write(pkg1.join("Cargo.toml"), "[package]\nname = \"pkg1\"").unwrap();

        // Create pkg2
        let pkg2 = root.join("pkg2");
        fs::create_dir(&pkg2).unwrap();
        fs::write(pkg2.join("Cargo.toml"), "[package]\nname = \"pkg2\"").unwrap();

        let workspace = WorkspaceContext::new(root).unwrap();
        let members = get_workspace_members(&workspace, root).unwrap();

        assert_eq!(members.len(), 2);
        assert!(members.contains_key("pkg1"));
        assert!(members.contains_key("pkg2"));
    }

    #[test]
    fn test_get_workspace_members_with_glob() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();

        // Create workspace with glob pattern
        fs::write(
            root.join("Cargo.toml"),
            r#"
[workspace]
members = ["packages/*"]
"#,
        )
        .unwrap();

        // Create packages directory
        let packages_dir = root.join("packages");
        fs::create_dir(&packages_dir).unwrap();

        // Create pkg1
        let pkg1 = packages_dir.join("pkg1");
        fs::create_dir(&pkg1).unwrap();
        fs::write(pkg1.join("Cargo.toml"), "[package]\nname = \"pkg1\"").unwrap();

        // Create pkg2
        let pkg2 = packages_dir.join("pkg2");
        fs::create_dir(&pkg2).unwrap();
        fs::write(pkg2.join("Cargo.toml"), "[package]\nname = \"pkg2\"").unwrap();

        let workspace = WorkspaceContext::new(root).unwrap();
        let members = get_workspace_members(&workspace, root).unwrap();

        assert_eq!(members.len(), 2);
        assert!(members.contains_key("pkg1"));
        assert!(members.contains_key("pkg2"));
    }

    #[test]
    fn test_get_workspace_members_missing_cargo_toml() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();

        fs::write(
            root.join("Cargo.toml"),
            r#"
[workspace]
members = ["pkg1"]
"#,
        )
        .unwrap();

        // Create directory without Cargo.toml
        fs::create_dir(root.join("pkg1")).unwrap();

        let workspace = WorkspaceContext::new(root).unwrap();
        let members = get_workspace_members(&workspace, root).unwrap();

        assert!(members.is_empty());
    }
}
