//! Node.js package implementation.

use std::path::{Path, PathBuf};

use crate::workspace::{
    traits::Package,
    types::{DependencyKind, ExternalDependency, WorkspaceDependency},
};

/// A Node.js package within a workspace.
#[derive(Debug, Clone)]
pub struct NodePackage {
    /// Package name from package.json
    pub(crate) name: String,

    /// Package version from package.json
    pub(crate) version: Option<String>,

    /// Path to the package directory
    pub(crate) path: PathBuf,

    /// Dependencies on other workspace packages
    pub(crate) workspace_deps: Vec<WorkspaceDependency>,

    /// Dependencies on external packages
    pub(crate) external_deps: Vec<ExternalDependency>,
}

impl NodePackage {
    /// Creates a new Node.js package.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        version: Option<String>,
        path: impl Into<PathBuf>,
        workspace_deps: Vec<WorkspaceDependency>,
        external_deps: Vec<ExternalDependency>,
    ) -> Self {
        Self {
            name: name.into(),
            version,
            path: path.into(),
            workspace_deps,
            external_deps,
        }
    }

    /// Creates a package with just name and path (dependencies loaded later).
    #[must_use]
    pub fn minimal(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            version: None,
            path: path.into(),
            workspace_deps: Vec::new(),
            external_deps: Vec::new(),
        }
    }
}

impl Package for NodePackage {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn workspace_dependencies(&self) -> &[WorkspaceDependency] {
        &self.workspace_deps
    }

    fn external_dependencies(&self) -> &[ExternalDependency] {
        &self.external_deps
    }
}

/// Parses dependencies from a package.json value.
///
/// # Arguments
///
/// * `package_json` - The parsed package.json as a JSON value
/// * `workspace_members` - Set of package names that are workspace members
///
/// # Returns
///
/// Tuple of (workspace dependencies, external dependencies)
#[must_use]
pub fn parse_dependencies(
    package_json: &serde_json::Value,
    workspace_members: &std::collections::BTreeSet<String>,
) -> (Vec<WorkspaceDependency>, Vec<ExternalDependency>) {
    let mut workspace_deps = Vec::new();
    let mut external_deps = Vec::new();

    // Parse each dependency section
    for (section, kind) in [
        ("dependencies", DependencyKind::Normal),
        ("devDependencies", DependencyKind::Dev),
        ("peerDependencies", DependencyKind::Peer),
        ("optionalDependencies", DependencyKind::Optional),
    ] {
        if let Some(deps) = package_json.get(section).and_then(|d| d.as_object()) {
            for (name, value) in deps {
                let is_workspace = is_workspace_dependency(value, workspace_members, name);
                let is_optional = kind == DependencyKind::Optional;

                if is_workspace {
                    workspace_deps.push(WorkspaceDependency::new(name.clone(), kind));
                } else {
                    external_deps.push(ExternalDependency::new(name.clone(), kind, is_optional));
                }
            }
        }
    }

    (workspace_deps, external_deps)
}

/// Checks if a dependency is a workspace dependency.
fn is_workspace_dependency(
    value: &serde_json::Value,
    workspace_members: &std::collections::BTreeSet<String>,
    name: &str,
) -> bool {
    // Check if package name is in workspace members
    if workspace_members.contains(name) {
        return true;
    }

    // Check for workspace: protocol
    if let Some(version_str) = value.as_str() {
        if version_str.starts_with("workspace:") {
            return true;
        }
        // Also check for file: protocol pointing to workspace
        if version_str.starts_with("file:") {
            return true;
        }
    }

    false
}

/// Reads the package name from a package.json file.
pub async fn read_package_name(package_json_path: &Path) -> Option<String> {
    let content = switchy_fs::unsync::read_to_string(package_json_path)
        .await
        .ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json.get("name")?.as_str().map(String::from)
}

/// Reads the package version from a package.json file.
pub async fn read_package_version(package_json_path: &Path) -> Option<String> {
    let content = switchy_fs::unsync::read_to_string(package_json_path)
        .await
        .ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json.get("version")?.as_str().map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn test_parse_dependencies_simple() {
        let json_str = r#"
            {
                "name": "test-pkg",
                "version": "1.0.0",
                "dependencies": {
                    "lodash": "^4.17.21",
                    "express": "^4.18.2"
                },
                "devDependencies": {
                    "jest": "^29.0.0"
                }
            }
        "#;

        let json: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let workspace_members = BTreeSet::new();

        let (workspace_deps, external_deps) = parse_dependencies(&json, &workspace_members);

        assert!(workspace_deps.is_empty());
        assert_eq!(external_deps.len(), 3);
        assert!(external_deps.iter().any(|d| d.name == "lodash"));
        assert!(external_deps.iter().any(|d| d.name == "express"));
        assert!(
            external_deps
                .iter()
                .any(|d| d.name == "jest" && d.kind == DependencyKind::Dev)
        );
    }

    #[test]
    fn test_parse_dependencies_workspace_protocol() {
        let json_str = r#"
            {
                "name": "test-pkg",
                "dependencies": {
                    "lodash": "^4.17.21",
                    "@myorg/shared": "workspace:*"
                }
            }
        "#;

        let json: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let workspace_members = BTreeSet::new();

        let (workspace_deps, external_deps) = parse_dependencies(&json, &workspace_members);

        assert_eq!(workspace_deps.len(), 1);
        assert_eq!(workspace_deps[0].name, "@myorg/shared");
        assert_eq!(external_deps.len(), 1);
        assert_eq!(external_deps[0].name, "lodash");
    }

    #[test]
    fn test_parse_dependencies_workspace_member() {
        let json_str = r#"
            {
                "name": "test-pkg",
                "dependencies": {
                    "lodash": "^4.17.21",
                    "@myorg/utils": "^1.0.0"
                }
            }
        "#;

        let json: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let mut workspace_members = BTreeSet::new();
        workspace_members.insert("@myorg/utils".to_string());

        let (workspace_deps, external_deps) = parse_dependencies(&json, &workspace_members);

        assert_eq!(workspace_deps.len(), 1);
        assert_eq!(workspace_deps[0].name, "@myorg/utils");
        assert_eq!(external_deps.len(), 1);
        assert_eq!(external_deps[0].name, "lodash");
    }
}
