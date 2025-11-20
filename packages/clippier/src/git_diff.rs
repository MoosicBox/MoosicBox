//! Git diff analysis for detecting external dependency changes.
//!
//! This module provides functionality for analyzing git repository diffs to detect changes
//! in external dependencies via `Cargo.lock` modifications. It maps those changes to affected
//! workspace packages, enabling comprehensive impact analysis for CI/CD pipelines.
//!
//! # Features
//!
//! * Parse `Cargo.lock` diff to detect dependency version changes
//! * Map external dependencies to workspace packages that use them
//! * Identify packages affected by external dependency updates
//! * Support for git-based change detection between commits
//!
//! # Example
//!
//! ```rust,ignore
//! use clippier::git_diff::get_affected_packages_from_git;
//!
//! let affected = get_affected_packages_from_git(
//!     "/path/to/repo",
//!     "origin/main",
//!     "HEAD",
//!     &workspace_packages,
//!     &package_cargo_values,
//! )?;
//! ```

use git2::Repository;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

// Local definitions to avoid import issues
fn parse_dependency_name(dep_spec: &str) -> String {
    // Parse dependency name (remove version constraints, features, etc.)
    dep_spec
        .split_whitespace()
        .next()
        .unwrap_or(dep_spec)
        .to_string()
}
/// A package entry in a Cargo.lock file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CargoLockPackage {
    /// Package name
    pub name: String,
    /// Package version
    pub version: String,
    /// Package source (registry, git, etc.)
    pub source: Option<String>,
    /// Package dependencies
    pub dependencies: Option<Vec<String>>,
}

/// Representation of a Cargo.lock file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CargoLock {
    /// Cargo.lock format version
    pub version: u32,
    /// List of packages in the lockfile
    pub package: Vec<CargoLockPackage>,
}

/// Parse Cargo.lock changes into a `Vec` of package names
#[must_use]
pub fn parse_cargo_lock_changes(changes: &[(char, String)]) -> Vec<String> {
    let mut changed_packages = std::collections::BTreeSet::new();
    let mut current_package = None;
    let mut has_version_change = false;
    let mut is_new_package = false;

    for (op, line) in changes {
        let line = line.trim();

        if line.starts_with("name = \"") {
            if let Some(name_start) = line.find('"')
                && let Some(name_end) = line.rfind('"')
                && name_end > name_start
            {
                current_package = Some(line[name_start + 1..name_end].to_string());
                has_version_change = false;
                is_new_package = *op == '+'; // New package if the name line is added
            }
        } else if line.starts_with("version = \"") && (*op == '-' || *op == '+') {
            has_version_change = true;
        } else if line.starts_with("[[package]]") {
            // Include package if it has version changes OR if it's a newly added package
            if let Some(package) = &current_package
                && (has_version_change || is_new_package)
            {
                changed_packages.insert(package.clone());
            }
            current_package = None;
            has_version_change = false;
            is_new_package = false;
        } else if line.is_empty() {
            // For empty lines, we process the current package and reset, but only if we haven't seen a new [[package]] marker
            // This handles the case where a package section ends with an empty line rather than a new [[package]]
            if let Some(package) = &current_package
                && (has_version_change || is_new_package)
            {
                changed_packages.insert(package.clone());
            }
            current_package = None;
            has_version_change = false;
            is_new_package = false;
        }
        // Ignore checksum-only changes - these don't indicate meaningful dependency changes
        // that would require rebuilding dependent packages
    }

    // Handle the last package in the diff
    if let Some(package) = current_package
        && (has_version_change || is_new_package)
    {
        changed_packages.insert(package);
    }

    let mut result: Vec<String> = changed_packages.into_iter().collect();
    result.sort();
    result
}

/// Parse Cargo.lock content into a `CargoLock` struct
///
/// # Errors
///
/// * If the Cargo.lock content is not valid TOML
/// * If the version is not a valid `u32`
pub fn parse_cargo_lock(content: &str) -> Result<CargoLock, Box<dyn std::error::Error>> {
    let toml_value: toml::Value = toml::from_str(content)?;

    #[allow(clippy::cast_sign_loss)]
    let version = u32::try_from(
        toml_value
            .get("version")
            .and_then(toml::Value::as_integer)
            .unwrap_or(3),
    )?;

    let packages = toml_value
        .get("package")
        .and_then(|p| p.as_array())
        .map(|packages| {
            packages
                .iter()
                .filter_map(|pkg| {
                    let name = pkg.get("name")?.as_str()?.to_string();
                    let version = pkg.get("version")?.as_str()?.to_string();
                    let source = pkg
                        .get("source")
                        .and_then(|s| s.as_str())
                        .map(ToString::to_string);
                    let dependencies = pkg
                        .get("dependencies")
                        .and_then(|deps| deps.as_array())
                        .map(|deps| {
                            deps.iter()
                                .filter_map(|d| d.as_str().map(ToString::to_string))
                                .collect()
                        });

                    Some(CargoLockPackage {
                        name,
                        version,
                        source,
                        dependencies,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(CargoLock {
        version,
        package: packages,
    })
}

/// Extract list of changed files between two git commits
///
/// # Errors
///
/// * If the repository is not found
/// * If the base commit is not found
/// * If the head commit is not found
pub fn get_changed_files_from_git(
    workspace_root: &Path,
    base_commit: &str,
    head_commit: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let repo = Repository::open(workspace_root)?;
    let base_oid = repo.revparse_single(base_commit)?.id();
    let head_oid = repo.revparse_single(head_commit)?.id();

    let base_commit = repo.find_commit(base_oid)?;
    let head_commit = repo.find_commit(head_oid)?;

    let base_tree = base_commit.tree()?;
    let head_tree = head_commit.tree()?;

    let diff = repo.diff_tree_to_tree(Some(&base_tree), Some(&head_tree), None)?;

    let mut changed_files = Vec::new();

    diff.foreach(
        &mut |delta, _progress| {
            if let Some(new_file) = delta.new_file().path() {
                if let Some(path_str) = new_file.to_str() {
                    changed_files.push(path_str.to_string());
                }
            } else if let Some(old_file) = delta.old_file().path()
                && let Some(path_str) = old_file.to_str()
            {
                changed_files.push(path_str.to_string());
            }
            true
        },
        None,
        None,
        None,
    )?;

    changed_files.sort();
    changed_files.dedup();

    log::debug!(
        "Found {} changed files from git: {changed_files:?}",
        changed_files.len()
    );

    Ok(changed_files)
}

/// Extract changed external dependencies from git diff
///
/// # Errors
///
/// * If the repository is not found
/// * If the base commit is not found
/// * If the head commit is not found
pub fn extract_changed_dependencies_from_git(
    workspace_root: &Path,
    base_commit: &str,
    head_commit: &str,
    _changed_files: &[String],
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let repo = Repository::open(workspace_root)?;
    let base_oid = repo.revparse_single(base_commit)?.id();
    let head_oid = repo.revparse_single(head_commit)?.id();

    let base_commit = repo.find_commit(base_oid)?;
    let head_commit = repo.find_commit(head_oid)?;

    let base_tree = base_commit.tree()?;
    let head_tree = head_commit.tree()?;

    let mut diff_opts = git2::DiffOptions::new();
    diff_opts.pathspec("Cargo.lock"); // Only look at Cargo.lock changes

    let diff = repo.diff_tree_to_tree(Some(&base_tree), Some(&head_tree), Some(&mut diff_opts))?;

    let mut cargo_lock_changes = Vec::new();

    // Extract changes from Cargo.lock only
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        let content = std::str::from_utf8(line.content()).unwrap_or("");
        cargo_lock_changes.push((line.origin(), content.to_string()));
        true
    })?;

    log::debug!(
        "Found {} lines in Cargo.lock diff",
        cargo_lock_changes.len()
    );

    // Parse the changes to find affected external dependencies
    let directly_changed_deps = parse_cargo_lock_changes(&cargo_lock_changes);

    log::debug!("Directly changed dependencies (before filtering): {directly_changed_deps:?}");

    // Get the current and previous Cargo.lock
    let current_cargo_lock_content = std::fs::read_to_string(workspace_root.join("Cargo.lock"))?;
    let current_cargo_lock = parse_cargo_lock(&current_cargo_lock_content)?;

    let previous_cargo_lock = get_cargo_lock_from_commit(&repo, base_oid)?;

    // Use the enhanced function to analyze transitive dependencies
    let all_affected = previous_cargo_lock.map_or_else(
        || find_transitively_affected_external_deps(&current_cargo_lock, &directly_changed_deps),
        |previous_cargo_lock| {
            find_transitively_affected_external_deps_with_previous(
                &current_cargo_lock,
                Some(&previous_cargo_lock),
                &directly_changed_deps,
            )
        },
    );

    // Filter out workspace packages - we only want actual external dependencies
    // First, get the list of workspace package names
    let workspace_cargo_path = workspace_root.join("Cargo.toml");
    let workspace_source = std::fs::read_to_string(&workspace_cargo_path)?;
    let workspace_value: toml::Value = toml::from_str(&workspace_source)?;

    let mut workspace_package_names = std::collections::BTreeSet::new();

    if let Some(workspace_members) = workspace_value
        .get("workspace")
        .and_then(|x| x.get("members"))
        .and_then(|x| x.as_array())
        .and_then(|x| x.iter().map(|x| x.as_str()).collect::<Option<Vec<_>>>())
    {
        for member_path in workspace_members {
            let full_path = workspace_root.join(member_path);
            let cargo_path = full_path.join("Cargo.toml");

            if cargo_path.exists()
                && let Ok(source) = std::fs::read_to_string(&cargo_path)
                && let Ok(value) = toml::from_str::<toml::Value>(&source)
                && let Some(package_name) = value
                    .get("package")
                    .and_then(|x| x.get("name"))
                    .and_then(|x| x.as_str())
            {
                workspace_package_names.insert(package_name.to_string());
            }
        }
    }

    // Filter out workspace packages from the affected dependencies
    let result: Vec<String> = all_affected
        .into_iter()
        .filter(|dep| !workspace_package_names.contains(dep))
        .collect();

    log::debug!("External dependencies after filtering out workspace packages: {result:?}");
    log::debug!("Total affected external dependencies: {}", result.len());
    Ok(result)
}

/// Get Cargo.lock content from a specific commit
///
/// # Errors
///
/// * If the commit is not found
/// * If the commit tree is not found
/// * If the Cargo.lock file is not found in the commit
/// * If the Cargo.lock file cannot be parsed
pub fn get_cargo_lock_from_commit(
    repo: &Repository,
    commit_oid: git2::Oid,
) -> Result<Option<CargoLock>, Box<dyn std::error::Error>> {
    let commit = repo.find_commit(commit_oid)?;
    let tree = commit.tree()?;

    if let Some(entry) = tree.get_name("Cargo.lock") {
        let blob = repo.find_blob(entry.id())?;
        let content = std::str::from_utf8(blob.content())?;
        match parse_cargo_lock(content) {
            Ok(cargo_lock) => Ok(Some(cargo_lock)),
            Err(e) => {
                log::warn!("Failed to parse Cargo.lock from commit {commit_oid}: {e}");
                Ok(None)
            }
        }
    } else {
        log::trace!("Cargo.lock not found in commit {commit_oid}");
        Ok(None)
    }
}

/// Find all external dependencies that are transitively affected by the changed dependencies,
/// but distinguish between new and changed dependencies
#[must_use]
pub fn find_transitively_affected_external_deps_with_previous(
    current_cargo_lock: &CargoLock,
    previous_cargo_lock: Option<&CargoLock>,
    directly_changed_deps: &[String],
) -> Vec<String> {
    use std::collections::VecDeque;

    const MAX_DEPTH: usize = 5; // Reasonable depth for transitive analysis

    // If we don't have the previous Cargo.lock, fall back to the old behavior
    let Some(previous_cargo_lock) = previous_cargo_lock else {
        return find_transitively_affected_external_deps(current_cargo_lock, directly_changed_deps);
    };

    log::trace!("Finding transitively affected external dependencies with previous context");
    log::trace!("Directly changed dependencies: {directly_changed_deps:?}");

    // Build a set of packages that existed in the previous Cargo.lock
    let previous_packages: BTreeSet<String> = previous_cargo_lock
        .package
        .iter()
        .map(|pkg| pkg.name.clone())
        .collect();

    // Separate newly added dependencies from existing ones that changed
    let (new_deps, changed_deps): (Vec<_>, Vec<_>) =
        directly_changed_deps.iter().partition(|&dep| {
            // A dependency is "new" if it didn't exist in the previous Cargo.lock
            !previous_packages.contains(dep)
        });

    log::trace!("New dependencies (direct analysis only): {new_deps:?}");
    log::trace!("Changed dependencies (transitive analysis): {changed_deps:?}");

    // Build a reverse dependency map: dependency -> packages that depend on it
    let mut reverse_dep_map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for package in &current_cargo_lock.package {
        if let Some(dependencies) = &package.dependencies {
            for dep in dependencies {
                // Parse dependency name (remove version constraints, features, etc.)
                let dep_name = dep.split_whitespace().next().unwrap_or(dep).to_string();
                reverse_dep_map
                    .entry(dep_name)
                    .or_default()
                    .push(package.name.clone());
            }
        }
    }

    log::trace!(
        "Built reverse dependency map with {} entries",
        reverse_dep_map.len()
    );

    let mut affected_packages = BTreeSet::new();

    // For new dependencies, include them but don't do transitive analysis
    // (newly added dependencies shouldn't cause broad rebuilds)
    for dep in &new_deps {
        affected_packages.insert((*dep).clone());
    }

    // For changed dependencies, include them and do full transitive analysis
    let mut queue = VecDeque::new();
    let mut depth_map = BTreeMap::new();

    // Start with existing dependencies that changed
    for dep in &changed_deps {
        affected_packages.insert((*dep).clone());
        queue.push_back((*dep).clone());
        depth_map.insert((*dep).clone(), 0);
    }

    // Process the queue to find all transitive dependents with depth limit
    while let Some(current_dep) = queue.pop_front() {
        let current_depth = depth_map[&current_dep];

        if current_depth >= MAX_DEPTH {
            continue;
        }

        if let Some(dependents) = reverse_dep_map.get(&current_dep) {
            for dependent in dependents {
                if !affected_packages.contains(dependent) {
                    log::trace!(
                        "External package '{dependent}' is transitively affected by '{current_dep}' at depth {}",
                        current_depth + 1
                    );
                    affected_packages.insert(dependent.clone());

                    // Continue BFS if we haven't reached max depth
                    if current_depth + 1 < MAX_DEPTH {
                        queue.push_back(dependent.clone());
                        depth_map.insert(dependent.clone(), current_depth + 1);
                    }
                }
            }
        }
    }

    let result: Vec<String> = affected_packages.into_iter().collect();

    log::trace!(
        "Found {} total affected external dependencies ({} new, {} changed with transitive analysis)",
        result.len(),
        new_deps.len(),
        changed_deps.len()
    );

    result
}

/// Find all external dependencies that are transitively affected by the changed dependencies
#[must_use]
pub fn find_transitively_affected_external_deps(
    cargo_lock: &CargoLock,
    directly_changed_deps: &[String],
) -> Vec<String> {
    find_transitively_affected_external_deps_with_depth(cargo_lock, 10, directly_changed_deps)
}

fn find_transitively_affected_external_deps_with_depth(
    cargo_lock: &CargoLock,
    max_depth: usize,
    directly_changed_deps: &[String],
) -> Vec<String> {
    let mut all_affected = std::collections::BTreeSet::new();
    let mut visited = std::collections::BTreeSet::new();

    // Add directly changed dependencies
    for dep in directly_changed_deps {
        all_affected.insert(dep.clone());
    }

    // Build dependency map for faster lookup
    let mut dep_map: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    for package in &cargo_lock.package {
        if let Some(deps) = &package.dependencies {
            for dep_str in deps {
                let dep_name = parse_dependency_name(dep_str);
                dep_map
                    .entry(dep_name)
                    .or_default()
                    .insert(package.name.clone());
            }
        }
    }

    // For each directly changed dependency, find what depends on it (with depth limit)
    for changed_dep in directly_changed_deps {
        find_recursive_dependents(
            &dep_map,
            changed_dep,
            &mut all_affected,
            &mut visited,
            0,
            max_depth,
        );
    }

    let mut result: Vec<String> = all_affected.into_iter().collect();
    result.sort();
    result
}

fn find_recursive_dependents(
    dep_map: &std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    current_dep: &str,
    all_affected: &mut std::collections::BTreeSet<String>,
    visited: &mut std::collections::BTreeSet<String>,
    current_depth: usize,
    max_depth: usize,
) {
    if current_depth >= max_depth || visited.contains(current_dep) {
        return;
    }

    visited.insert(current_dep.to_string());

    if let Some(dependents) = dep_map.get(current_dep) {
        for dependent in dependents {
            all_affected.insert(dependent.clone());
            find_recursive_dependents(
                dep_map,
                dependent,
                all_affected,
                visited,
                current_depth + 1,
                max_depth,
            );
        }
    }

    visited.remove(current_dep);
}

/// Build a map of external dependencies to workspace packages that use them
///
/// # Errors
///
/// * If the workspace Cargo.toml file cannot be read
/// * If the workspace Cargo.toml file cannot be parsed
/// * If the workspace Cargo.toml file does not contain a workspace section
/// * If the workspace Cargo.toml file does not contain a dependencies section
pub fn build_external_dependency_map(
    workspace_root: &std::path::Path,
    workspace_members: &[String],
) -> Result<BTreeMap<String, Vec<String>>, Box<dyn std::error::Error>> {
    let mut external_dep_map: BTreeMap<String, Vec<String>> = BTreeMap::new();

    // First, parse workspace-level Cargo.toml to get workspace dependencies
    let workspace_cargo_path = workspace_root.join("Cargo.toml");
    let workspace_source = std::fs::read_to_string(&workspace_cargo_path)?;
    let workspace_value: toml::Value = toml::from_str(&workspace_source)?;

    // Get workspace dependencies (these are external packages that can be referenced with workspace = true)
    let mut workspace_external_deps = BTreeSet::new();
    if let Some(workspace_deps) = workspace_value
        .get("workspace")
        .and_then(|w| w.get("dependencies"))
        .and_then(|d| d.as_table())
    {
        for dep_name in workspace_deps.keys() {
            workspace_external_deps.insert(dep_name.clone());
        }
    }

    log::trace!(
        "Found {} workspace-level external dependencies: {:?}",
        workspace_external_deps.len(),
        workspace_external_deps
    );

    // Process each workspace member
    for member_path in workspace_members {
        let full_path = workspace_root.join(member_path);
        let cargo_path = full_path.join("Cargo.toml");

        if !cargo_path.exists() {
            continue;
        }

        let source = std::fs::read_to_string(&cargo_path)?;
        let value: toml::Value = toml::from_str(&source)?;

        // Get package name
        let package_name = value
            .get("package")
            .and_then(|x| x.get("name"))
            .and_then(|x| x.as_str())
            .unwrap_or("unknown");

        // Check all dependency sections
        for dep_section in ["dependencies", "dev-dependencies", "build-dependencies"] {
            if let Some(dependencies) = value.get(dep_section).and_then(|x| x.as_table()) {
                for (dep_name, dep_value) in dependencies {
                    let is_external = match dep_value {
                        toml::Value::String(_) => {
                            // Direct external dependency
                            true
                        }
                        toml::Value::Table(table) => {
                            // Check if it's a workspace dependency referencing an external package
                            if table.get("workspace") == Some(&toml::Value::Boolean(true)) {
                                workspace_external_deps.contains(dep_name)
                            } else {
                                // Check if it's not a path dependency (path dependencies are internal)
                                !table.contains_key("path")
                            }
                        }
                        _ => false,
                    };

                    if is_external {
                        external_dep_map
                            .entry(dep_name.clone())
                            .or_default()
                            .push(package_name.to_string());
                    }
                }
            }
        }
    }

    // Remove duplicates
    for packages in external_dep_map.values_mut() {
        packages.sort();
        packages.dedup();
    }

    log::trace!(
        "Built external dependency map with {} entries",
        external_dep_map.len()
    );
    for (dep, packages) in &external_dep_map {
        log::trace!("  {dep} -> {packages:?}");
    }

    Ok(external_dep_map)
}

/// Find packages affected by external dependency changes
#[must_use]
pub fn find_packages_affected_by_external_deps(
    external_dep_map: &BTreeMap<String, Vec<String>>,
    changed_external_deps: &[String],
) -> Vec<String> {
    let mut affected_packages = BTreeSet::new();

    for dep in changed_external_deps {
        if let Some(packages) = external_dep_map.get(dep) {
            for package in packages {
                affected_packages.insert(package.clone());
            }
        }
    }

    let result: Vec<String> = affected_packages.into_iter().collect();

    log::trace!("External dependencies {changed_external_deps:?} affect packages: {result:?}");

    result
}

/// Find packages affected by external dependency changes with specific mapping
/// Returns a map of package name -> list of external dependencies that affect it
#[must_use]
pub fn find_packages_affected_by_external_deps_with_mapping(
    external_dep_map: &BTreeMap<String, Vec<String>>,
    changed_external_deps: &[String],
) -> BTreeMap<String, Vec<String>> {
    let mut package_to_deps: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for dep in changed_external_deps {
        if let Some(packages) = external_dep_map.get(dep) {
            for package in packages {
                package_to_deps
                    .entry(package.clone())
                    .or_default()
                    .push(dep.clone());
            }
        }
    }

    // Sort dependencies for each package for consistent output
    for deps in package_to_deps.values_mut() {
        deps.sort();
        deps.dedup();
    }

    log::trace!("Specific external dependency mapping: {package_to_deps:?}");

    package_to_deps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cargo_lock_changes_version_change() {
        let changes = vec![
            (' ', "[[package]]".to_string()),
            ('+', "name = \"serde\"".to_string()),
            ('-', "version = \"1.0.0\"".to_string()),
            ('+', "version = \"1.0.1\"".to_string()),
        ];

        let result = parse_cargo_lock_changes(&changes);
        assert_eq!(result, vec!["serde"]);
    }

    #[test]
    fn test_parse_cargo_lock_changes_new_package() {
        let changes = vec![
            ('+', "[[package]]".to_string()),
            ('+', "name = \"new_dep\"".to_string()),
            ('+', "version = \"1.0.0\"".to_string()),
        ];

        let result = parse_cargo_lock_changes(&changes);
        assert_eq!(result, vec!["new_dep"]);
    }

    #[test]
    fn test_parse_cargo_lock_changes_checksum_only() {
        let changes = vec![
            (' ', "[[package]]".to_string()),
            (' ', "name = \"unchanged\"".to_string()),
            (' ', "version = \"1.0.0\"".to_string()),
            ('-', "checksum = \"abc123\"".to_string()),
            ('+', "checksum = \"def456\"".to_string()),
        ];

        let result = parse_cargo_lock_changes(&changes);
        assert!(result.is_empty(), "Checksum-only changes should be ignored");
    }

    #[test]
    fn test_parse_cargo_lock_changes_multiple_packages() {
        let changes = vec![
            (' ', "[[package]]".to_string()),
            ('+', "name = \"dep1\"".to_string()),
            ('-', "version = \"1.0.0\"".to_string()),
            ('+', "version = \"1.0.1\"".to_string()),
            (' ', String::new()),
            (' ', "[[package]]".to_string()),
            ('+', "name = \"dep2\"".to_string()),
            ('+', "version = \"2.0.0\"".to_string()),
        ];

        let result = parse_cargo_lock_changes(&changes);
        assert_eq!(result, vec!["dep1", "dep2"]);
    }

    #[test]
    fn test_parse_cargo_lock_changes_empty_lines() {
        let changes = vec![
            (' ', "[[package]]".to_string()),
            ('+', "name = \"test_dep\"".to_string()),
            ('+', "version = \"1.0.0\"".to_string()),
            (' ', String::new()),
            (' ', String::new()),
        ];

        let result = parse_cargo_lock_changes(&changes);
        assert_eq!(result, vec!["test_dep"]);
    }

    #[test]
    fn test_parse_cargo_lock_changes_new_package_detection() {
        let changes = vec![
            ('+', "[[package]]".to_string()),
            ('+', "name = \"new_package\"".to_string()),
            ('+', "version = \"1.0.0\"".to_string()),
        ];

        let result = parse_cargo_lock_changes(&changes);
        // Should detect as a new package since [[package]] line is marked with '+'
        assert_eq!(result, vec!["new_package"]);
    }

    #[test]
    fn test_parse_cargo_lock_empty_input() {
        let changes: Vec<(char, String)> = vec![];
        let result = parse_cargo_lock_changes(&changes);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_cargo_lock_basic() {
        let content = r#"
version = 3

[[package]]
name = "serde"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "toml"
version = "0.5.0"
dependencies = [
    "serde",
]
"#;

        let result = parse_cargo_lock(content).unwrap();
        assert_eq!(result.version, 3);
        assert_eq!(result.package.len(), 2);

        let serde_pkg = result.package.iter().find(|p| p.name == "serde").unwrap();
        assert_eq!(serde_pkg.version, "1.0.0");
        assert!(serde_pkg.source.is_some());

        let toml_pkg = result.package.iter().find(|p| p.name == "toml").unwrap();
        assert_eq!(toml_pkg.version, "0.5.0");
        assert_eq!(toml_pkg.dependencies.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_parse_cargo_lock_empty() {
        let content = r"
version = 3
";

        let result = parse_cargo_lock(content).unwrap();
        assert_eq!(result.version, 3);
        assert!(result.package.is_empty());
    }

    #[test]
    fn test_parse_cargo_lock_missing_version() {
        // Test with missing version field - should default to 3
        let content = r"
[[package]]
name = 'test'
version = '1.0.0'
";

        let result = parse_cargo_lock(content);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().version, 3);
    }

    #[test]
    fn test_find_transitively_affected_external_deps_direct_only() {
        let cargo_lock = CargoLock {
            version: 3,
            package: vec![
                CargoLockPackage {
                    name: "dep1".to_string(),
                    version: "1.0.0".to_string(),
                    source: None,
                    dependencies: None,
                },
                CargoLockPackage {
                    name: "dep2".to_string(),
                    version: "1.0.0".to_string(),
                    source: None,
                    dependencies: Some(vec!["dep1".to_string()]),
                },
            ],
        };

        let changed = vec!["dep1".to_string()];
        let result = find_transitively_affected_external_deps(&cargo_lock, &changed);

        assert!(result.contains(&"dep1".to_string()));
        assert!(result.contains(&"dep2".to_string()));
    }

    #[test]
    fn test_find_transitively_affected_external_deps_no_dependents() {
        let cargo_lock = CargoLock {
            version: 3,
            package: vec![CargoLockPackage {
                name: "standalone".to_string(),
                version: "1.0.0".to_string(),
                source: None,
                dependencies: None,
            }],
        };

        let changed = vec!["standalone".to_string()];
        let result = find_transitively_affected_external_deps(&cargo_lock, &changed);

        assert_eq!(result, vec!["standalone"]);
    }

    #[test]
    fn test_find_transitively_affected_external_deps_chain() {
        let cargo_lock = CargoLock {
            version: 3,
            package: vec![
                CargoLockPackage {
                    name: "dep1".to_string(),
                    version: "1.0.0".to_string(),
                    source: None,
                    dependencies: None,
                },
                CargoLockPackage {
                    name: "dep2".to_string(),
                    version: "1.0.0".to_string(),
                    source: None,
                    dependencies: Some(vec!["dep1 1.0.0".to_string()]),
                },
                CargoLockPackage {
                    name: "dep3".to_string(),
                    version: "1.0.0".to_string(),
                    source: None,
                    dependencies: Some(vec!["dep2 1.0.0".to_string()]),
                },
            ],
        };

        let changed = vec!["dep1".to_string()];
        let result = find_transitively_affected_external_deps(&cargo_lock, &changed);

        assert!(result.contains(&"dep1".to_string()));
        assert!(result.contains(&"dep2".to_string()));
        assert!(result.contains(&"dep3".to_string()));
    }

    #[test]
    fn test_find_transitively_affected_with_previous_new_deps() {
        let previous = CargoLock {
            version: 3,
            package: vec![CargoLockPackage {
                name: "existing".to_string(),
                version: "1.0.0".to_string(),
                source: None,
                dependencies: None,
            }],
        };

        let current = CargoLock {
            version: 3,
            package: vec![
                CargoLockPackage {
                    name: "existing".to_string(),
                    version: "1.0.0".to_string(),
                    source: None,
                    dependencies: None,
                },
                CargoLockPackage {
                    name: "new_dep".to_string(),
                    version: "1.0.0".to_string(),
                    source: None,
                    dependencies: None,
                },
            ],
        };

        let changed = vec!["new_dep".to_string()];
        let result = find_transitively_affected_external_deps_with_previous(
            &current,
            Some(&previous),
            &changed,
        );

        assert_eq!(result, vec!["new_dep"]);
    }

    #[test]
    fn test_find_transitively_affected_with_previous_changed_deps() {
        let previous = CargoLock {
            version: 3,
            package: vec![CargoLockPackage {
                name: "dep1".to_string(),
                version: "1.0.0".to_string(),
                source: None,
                dependencies: None,
            }],
        };

        let current = CargoLock {
            version: 3,
            package: vec![
                CargoLockPackage {
                    name: "dep1".to_string(),
                    version: "2.0.0".to_string(),
                    source: None,
                    dependencies: None,
                },
                CargoLockPackage {
                    name: "dep2".to_string(),
                    version: "1.0.0".to_string(),
                    source: None,
                    dependencies: Some(vec!["dep1 2.0.0".to_string()]),
                },
            ],
        };

        let changed = vec!["dep1".to_string()];
        let result = find_transitively_affected_external_deps_with_previous(
            &current,
            Some(&previous),
            &changed,
        );

        assert!(result.contains(&"dep1".to_string()));
        assert!(result.contains(&"dep2".to_string()));
    }

    #[test]
    fn test_find_packages_affected_by_external_deps_basic() {
        let mut dep_map = BTreeMap::new();
        dep_map.insert(
            "serde".to_string(),
            vec!["pkg1".to_string(), "pkg2".to_string()],
        );
        dep_map.insert("tokio".to_string(), vec!["pkg1".to_string()]);

        let changed = vec!["serde".to_string()];
        let result = find_packages_affected_by_external_deps(&dep_map, &changed);

        assert_eq!(result.len(), 2);
        assert!(result.contains(&"pkg1".to_string()));
        assert!(result.contains(&"pkg2".to_string()));
    }

    #[test]
    fn test_find_packages_affected_by_external_deps_multiple_changes() {
        let mut dep_map = BTreeMap::new();
        dep_map.insert("serde".to_string(), vec!["pkg1".to_string()]);
        dep_map.insert("tokio".to_string(), vec!["pkg2".to_string()]);

        let changed = vec!["serde".to_string(), "tokio".to_string()];
        let result = find_packages_affected_by_external_deps(&dep_map, &changed);

        assert_eq!(result.len(), 2);
        assert!(result.contains(&"pkg1".to_string()));
        assert!(result.contains(&"pkg2".to_string()));
    }

    #[test]
    fn test_find_packages_affected_by_external_deps_no_match() {
        let mut dep_map = BTreeMap::new();
        dep_map.insert("serde".to_string(), vec!["pkg1".to_string()]);

        let changed = vec!["unknown_dep".to_string()];
        let result = find_packages_affected_by_external_deps(&dep_map, &changed);

        assert!(result.is_empty());
    }

    #[test]
    fn test_find_packages_affected_with_mapping() {
        let mut dep_map = BTreeMap::new();
        dep_map.insert(
            "serde".to_string(),
            vec!["pkg1".to_string(), "pkg2".to_string()],
        );
        dep_map.insert(
            "tokio".to_string(),
            vec!["pkg1".to_string(), "pkg3".to_string()],
        );

        let changed = vec!["serde".to_string(), "tokio".to_string()];
        let result = find_packages_affected_by_external_deps_with_mapping(&dep_map, &changed);

        assert_eq!(result.len(), 3);

        let pkg1_deps = result.get("pkg1").unwrap();
        assert_eq!(pkg1_deps.len(), 2);
        assert!(pkg1_deps.contains(&"serde".to_string()));
        assert!(pkg1_deps.contains(&"tokio".to_string()));

        let pkg2_deps = result.get("pkg2").unwrap();
        assert_eq!(pkg2_deps, &vec!["serde".to_string()]);

        let pkg3_deps = result.get("pkg3").unwrap();
        assert_eq!(pkg3_deps, &vec!["tokio".to_string()]);
    }

    #[test]
    fn test_find_packages_affected_with_mapping_dedup() {
        let mut dep_map = BTreeMap::new();
        dep_map.insert("serde".to_string(), vec!["pkg1".to_string()]);

        // Same dependency listed twice
        let changed = vec!["serde".to_string(), "serde".to_string()];
        let result = find_packages_affected_by_external_deps_with_mapping(&dep_map, &changed);

        let pkg1_deps = result.get("pkg1").unwrap();
        assert_eq!(pkg1_deps.len(), 1);
    }

    #[test]
    fn test_parse_dependency_name_simple() {
        assert_eq!(parse_dependency_name("serde"), "serde");
    }

    #[test]
    fn test_parse_dependency_name_with_version() {
        assert_eq!(parse_dependency_name("serde 1.0.0"), "serde");
    }

    #[test]
    fn test_parse_dependency_name_with_features() {
        assert_eq!(
            parse_dependency_name("serde 1.0.0 (registry+https://...)"),
            "serde"
        );
    }

    #[test]
    fn test_parse_dependency_name_empty() {
        assert_eq!(parse_dependency_name(""), String::new());
    }

    #[test]
    fn test_cargo_lock_package_serialization() {
        let package = CargoLockPackage {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            source: Some("registry".to_string()),
            dependencies: Some(vec!["dep1".to_string()]),
        };

        let json = serde_json::to_string(&package).unwrap();
        let deserialized: CargoLockPackage = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, "test");
        assert_eq!(deserialized.version, "1.0.0");
        assert_eq!(deserialized.source, Some("registry".to_string()));
    }
}
