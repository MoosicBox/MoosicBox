//! Trait definitions for workspace abstractions.
//!
//! This module defines the core traits that allow clippier to work with different
//! workspace types (Cargo, Node.js) through a unified interface.
//!
//! All traits in this module are designed to be dyn-compatible, allowing for
//! runtime polymorphism via trait objects (`Box<dyn Workspace>`, etc.).

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use async_trait::async_trait;

use super::types::{ExternalDependency, WorkspaceDependency};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A package within a workspace.
///
/// This trait represents a single package (Cargo crate or Node.js package)
/// within a monorepo workspace.
pub trait Package: Send + Sync {
    /// Returns the package name.
    ///
    /// For Cargo, this is the crate name from `Cargo.toml`.
    /// For Node.js, this is the `name` field from `package.json`.
    fn name(&self) -> &str;

    /// Returns the package version, if specified.
    fn version(&self) -> Option<&str>;

    /// Returns the path to the package root directory.
    fn path(&self) -> &Path;

    /// Returns dependencies on other packages in this workspace.
    fn workspace_dependencies(&self) -> &[WorkspaceDependency];

    /// Returns dependencies on external packages (from registry/external sources).
    fn external_dependencies(&self) -> &[ExternalDependency];
}

/// A single entry in a lockfile.
///
/// Represents one package/dependency as recorded in the lockfile.
pub trait LockfileEntry: Send + Sync {
    /// Returns the package name.
    fn name(&self) -> &str;

    /// Returns the resolved version.
    fn version(&self) -> &str;

    /// Returns the names of direct dependencies.
    fn dependencies(&self) -> &[String];
}

/// A parsed lockfile.
///
/// This trait represents a lockfile that has been parsed into a queryable structure.
/// It is dyn-compatible, using boxed trait objects for entries.
pub trait Lockfile: Send + Sync {
    /// Returns all entries in the lockfile as boxed trait objects.
    fn entries(&self) -> Vec<Box<dyn LockfileEntry>>;

    /// Finds an entry by package name.
    fn find(&self, name: &str) -> Option<Box<dyn LockfileEntry>> {
        self.entries().into_iter().find(|e| e.name() == name)
    }

    /// Builds a reverse dependency map.
    ///
    /// Returns a map where each key is a package name and the value is a list
    /// of packages that depend on it.
    fn reverse_dependency_map(&self) -> BTreeMap<String, Vec<String>> {
        let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for entry in self.entries() {
            for dep in entry.dependencies() {
                let dep_name = parse_dependency_name(dep);
                map.entry(dep_name)
                    .or_default()
                    .push(entry.name().to_string());
            }
        }
        map
    }
}

/// Parser for lockfile diffs.
///
/// This trait parses the output of `git diff` on a lockfile to extract
/// which packages have changed.
pub trait LockfileDiffParser: Send + Sync {
    /// Parses diff lines to extract changed package names.
    ///
    /// # Arguments
    ///
    /// * `changes` - Tuples of (operation, line content) where operation is
    ///   `+` for additions, `-` for deletions, or ` ` for context.
    ///
    /// # Returns
    ///
    /// A list of package names that have changed (added, removed, or updated).
    fn parse_changes(&self, changes: &[(char, String)]) -> Vec<String>;
}

/// Workspace context - the main abstraction for workspace operations.
///
/// This trait provides async operations for discovering and analyzing
/// packages within a workspace (monorepo).
///
/// This trait is dyn-compatible, allowing for runtime polymorphism via
/// `Box<dyn Workspace>`. Detection and construction methods are not part
/// of this trait; they are inherent methods on concrete implementations.
#[async_trait]
pub trait Workspace: Send + Sync {
    /// Returns the workspace root path.
    fn root(&self) -> &Path;

    /// Returns the lockfile path relative to the workspace root.
    fn lockfile_path(&self) -> &'static str;

    /// Returns the workspace member patterns.
    ///
    /// These may be expanded glob patterns (e.g., `packages/*` expanded to
    /// `packages/foo`, `packages/bar`).
    fn member_patterns(&self) -> &[String];

    /// Checks if the given path is a workspace member.
    async fn is_member_by_path(&self, path: &Path) -> bool;

    /// Checks if the given package name is a workspace member.
    async fn is_member_by_name(&self, name: &str) -> bool;

    /// Finds a member's path by its name.
    async fn find_member(&self, name: &str) -> Option<PathBuf>;

    /// Returns all packages in the workspace as boxed trait objects.
    async fn packages(&self) -> Result<Vec<Box<dyn Package>>, BoxError>;

    /// Reads and parses the lockfile.
    async fn read_lockfile(&self) -> Result<Box<dyn Lockfile>, BoxError>;

    /// Returns a diff parser instance for this workspace's lockfile format.
    fn diff_parser(&self) -> Box<dyn LockfileDiffParser>;

    /// Returns all package names and their paths.
    ///
    /// This is a convenience method that iterates over packages and extracts
    /// their names and relative paths.
    async fn package_name_to_path(&self) -> Result<BTreeMap<String, String>, BoxError> {
        let mut result = BTreeMap::new();
        let root = self.root();
        for pkg in self.packages().await? {
            let rel_path = pkg
                .path()
                .strip_prefix(root)
                .unwrap_or_else(|_| pkg.path())
                .to_string_lossy()
                .to_string();
            result.insert(pkg.name().to_string(), rel_path);
        }
        Ok(result)
    }

    /// Returns all package names in the workspace.
    async fn package_names(&self) -> Result<Vec<String>, BoxError> {
        Ok(self
            .packages()
            .await?
            .iter()
            .map(|p| p.name().to_string())
            .collect())
    }

    /// Parses the lockfile diff and returns changed package names.
    fn parse_lockfile_diff(&self, changes: &[(char, String)]) -> Vec<String> {
        self.diff_parser().parse_changes(changes)
    }

    /// Reads and parses the lockfile, returning entries as tuples.
    ///
    /// Returns tuples of (name, version, dependencies) for each entry.
    async fn read_lockfile_entries(&self) -> Result<Vec<(String, String, Vec<String>)>, BoxError> {
        let lockfile = self.read_lockfile().await?;
        Ok(lockfile
            .entries()
            .iter()
            .map(|e| {
                (
                    e.name().to_string(),
                    e.version().to_string(),
                    e.dependencies().to_vec(),
                )
            })
            .collect())
    }
}

/// Parse a dependency specification to extract the package name.
///
/// Handles various formats:
/// - Simple name: `serde`
/// - With version: `serde 1.0.0`
/// - Cargo.lock format: `serde 1.0.0 (registry+...)`
/// - pnpm format: `/serde@1.0.0`
#[must_use]
pub fn parse_dependency_name(dep_spec: &str) -> String {
    let spec = dep_spec.trim();

    // Handle pnpm format: /name@version or /@scope/name@version
    if let Some(stripped) = spec.strip_prefix('/') {
        if let Some(at_pos) = stripped.rfind('@') {
            // Check if it's a scoped package (@scope/name@version)
            if stripped.starts_with('@') {
                // Find the second @ which separates name from version
                if let Some(first_slash) = stripped.find('/') {
                    let after_scope = &stripped[first_slash + 1..];
                    if let Some(version_at) = after_scope.find('@') {
                        return stripped[..first_slash + 1 + version_at].to_string();
                    }
                }
            }
            return stripped[..at_pos].to_string();
        }
        return stripped.to_string();
    }

    // Handle standard formats: name, name version, name version (source)
    spec.split_whitespace().next().unwrap_or(spec).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dependency_name_simple() {
        assert_eq!(parse_dependency_name("serde"), "serde");
    }

    #[test]
    fn test_parse_dependency_name_with_version() {
        assert_eq!(parse_dependency_name("serde 1.0.0"), "serde");
    }

    #[test]
    fn test_parse_dependency_name_cargo_lock_format() {
        assert_eq!(
            parse_dependency_name(
                "serde 1.0.0 (registry+https://github.com/rust-lang/crates.io-index)"
            ),
            "serde"
        );
    }

    #[test]
    fn test_parse_dependency_name_pnpm_format() {
        assert_eq!(parse_dependency_name("/lodash@4.17.21"), "lodash");
    }

    #[test]
    fn test_parse_dependency_name_pnpm_scoped() {
        assert_eq!(parse_dependency_name("/@babel/core@7.0.0"), "@babel/core");
    }
}
