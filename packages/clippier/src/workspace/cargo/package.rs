//! Cargo package implementation.

use std::path::{Path, PathBuf};

use crate::workspace::{
    traits::Package,
    types::{DependencyKind, ExternalDependency, WorkspaceDependency},
};

/// A Cargo package (crate) within a workspace.
#[derive(Debug, Clone)]
pub struct CargoPackage {
    /// Package name from Cargo.toml
    pub(crate) name: String,

    /// Package version from Cargo.toml
    pub(crate) version: Option<String>,

    /// Path to the package directory
    pub(crate) path: PathBuf,

    /// Dependencies on other workspace packages
    pub(crate) workspace_deps: Vec<WorkspaceDependency>,

    /// Dependencies on external crates
    pub(crate) external_deps: Vec<ExternalDependency>,
}

impl CargoPackage {
    /// Creates a new Cargo package.
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

impl Package for CargoPackage {
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

/// Parses dependencies from a Cargo.toml value.
///
/// # Arguments
///
/// * `cargo_toml` - The parsed Cargo.toml as a TOML value
/// * `workspace_members` - Set of package names that are workspace members
///
/// # Returns
///
/// Tuple of (workspace dependencies, external dependencies)
#[must_use]
pub fn parse_dependencies(
    cargo_toml: &toml::Value,
    workspace_members: &std::collections::BTreeSet<String>,
) -> (Vec<WorkspaceDependency>, Vec<ExternalDependency>) {
    let mut workspace_deps = Vec::new();
    let mut external_deps = Vec::new();

    // Parse each dependency section
    for (section, kind) in [
        ("dependencies", DependencyKind::Normal),
        ("dev-dependencies", DependencyKind::Dev),
        ("build-dependencies", DependencyKind::Build),
    ] {
        if let Some(deps) = cargo_toml.get(section).and_then(|d| d.as_table()) {
            for (name, value) in deps {
                let (is_workspace, is_optional) = parse_dep_attributes(value);

                if is_workspace || workspace_members.contains(name) {
                    workspace_deps.push(WorkspaceDependency::new(name.clone(), kind));
                } else {
                    external_deps.push(ExternalDependency::new(name.clone(), kind, is_optional));
                }
            }
        }
    }

    // Parse target-specific dependencies
    if let Some(target) = cargo_toml.get("target").and_then(|t| t.as_table()) {
        for (_target_spec, target_deps) in target {
            for (section, kind) in [
                ("dependencies", DependencyKind::Normal),
                ("dev-dependencies", DependencyKind::Dev),
                ("build-dependencies", DependencyKind::Build),
            ] {
                if let Some(deps) = target_deps.get(section).and_then(|d| d.as_table()) {
                    for (name, value) in deps {
                        let (is_workspace, is_optional) = parse_dep_attributes(value);

                        if is_workspace || workspace_members.contains(name) {
                            // Avoid duplicates
                            if !workspace_deps.iter().any(|d| d.name == *name) {
                                workspace_deps.push(WorkspaceDependency::new(name.clone(), kind));
                            }
                        } else if !external_deps.iter().any(|d| d.name == *name) {
                            external_deps.push(ExternalDependency::new(
                                name.clone(),
                                kind,
                                is_optional,
                            ));
                        }
                    }
                }
            }
        }
    }

    (workspace_deps, external_deps)
}

/// Parses dependency attributes (workspace, optional) from a dependency value.
fn parse_dep_attributes(value: &toml::Value) -> (bool, bool) {
    match value {
        toml::Value::Table(table) => {
            let is_workspace = table
                .get("workspace")
                .and_then(toml::Value::as_bool)
                .unwrap_or(false);
            let is_optional = table
                .get("optional")
                .and_then(toml::Value::as_bool)
                .unwrap_or(false);
            // Also check for path dependencies that might be workspace members
            let has_path = table.get("path").is_some();
            (is_workspace || has_path, is_optional)
        }
        // Simple version string or other types mean external dependency
        _ => (false, false),
    }
}

/// Reads the package name from a Cargo.toml file.
pub async fn read_package_name(cargo_toml_path: &Path) -> Option<String> {
    let content = switchy_fs::unsync::read_to_string(cargo_toml_path)
        .await
        .ok()?;
    let toml: toml::Value = toml::from_str(&content).ok()?;
    toml.get("package")?.get("name")?.as_str().map(String::from)
}

/// Reads the package version from a Cargo.toml file.
pub async fn read_package_version(cargo_toml_path: &Path) -> Option<String> {
    let content = switchy_fs::unsync::read_to_string(cargo_toml_path)
        .await
        .ok()?;
    let toml: toml::Value = toml::from_str(&content).ok()?;
    toml.get("package")?
        .get("version")?
        .as_str()
        .map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn test_parse_dependencies_simple() {
        let toml_str = r#"
            [package]
            name = "test-pkg"
            version = "0.1.0"

            [dependencies]
            serde = "1.0"
            tokio = { version = "1.0", features = ["full"] }
            
            [dev-dependencies]
            insta = "1.0"
        "#;

        let toml: toml::Value = toml::from_str(toml_str).unwrap();
        let workspace_members = BTreeSet::new();

        let (workspace_deps, external_deps) = parse_dependencies(&toml, &workspace_members);

        assert!(workspace_deps.is_empty());
        assert_eq!(external_deps.len(), 3);
        assert!(external_deps.iter().any(|d| d.name == "serde"));
        assert!(external_deps.iter().any(|d| d.name == "tokio"));
        assert!(
            external_deps
                .iter()
                .any(|d| d.name == "insta" && d.kind == DependencyKind::Dev)
        );
    }

    #[test]
    fn test_parse_dependencies_workspace() {
        let toml_str = r#"
            [package]
            name = "test-pkg"
            version = "0.1.0"

            [dependencies]
            serde = "1.0"
            my-lib = { workspace = true }
        "#;

        let toml: toml::Value = toml::from_str(toml_str).unwrap();
        let workspace_members = BTreeSet::new();

        let (workspace_deps, external_deps) = parse_dependencies(&toml, &workspace_members);

        assert_eq!(workspace_deps.len(), 1);
        assert_eq!(workspace_deps[0].name, "my-lib");
        assert_eq!(external_deps.len(), 1);
        assert_eq!(external_deps[0].name, "serde");
    }

    #[test]
    fn test_parse_dependencies_optional() {
        let toml_str = r#"
            [package]
            name = "test-pkg"
            version = "0.1.0"

            [dependencies]
            serde = "1.0"
            optional-dep = { version = "1.0", optional = true }
        "#;

        let toml: toml::Value = toml::from_str(toml_str).unwrap();
        let workspace_members = BTreeSet::new();

        let (_, external_deps) = parse_dependencies(&toml, &workspace_members);

        assert_eq!(external_deps.len(), 2);
        let optional = external_deps
            .iter()
            .find(|d| d.name == "optional-dep")
            .unwrap();
        assert!(optional.is_optional);
    }
}
