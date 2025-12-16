//! Generic async analysis functions for workspace change detection.
//!
//! This module provides functions that work with any workspace type through
//! the `Workspace` trait, enabling shared logic for dependency analysis.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::{
    git::extract_file_diff_from_git,
    traits::{Lockfile, Workspace},
    types::AffectedPackageInfo,
};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Maximum depth for transitive dependency analysis.
const MAX_TRANSITIVE_DEPTH: usize = 5;

/// Finds packages affected by changed external dependencies.
///
/// This function maps changed external dependencies to workspace packages
/// that depend on them.
///
/// # Arguments
///
/// * `workspace` - The workspace to analyze
/// * `changed_deps` - List of external dependency names that have changed
///
/// # Returns
///
/// A map from affected package names to the list of changed dependencies
/// that affect them.
///
/// # Errors
///
/// Returns an error if loading workspace packages fails.
pub async fn find_affected_by_dependencies(
    workspace: &dyn Workspace,
    changed_deps: &[String],
) -> Result<BTreeMap<String, Vec<String>>, BoxError> {
    if changed_deps.is_empty() {
        return Ok(BTreeMap::new());
    }

    let packages = workspace.packages().await?;

    // Build: external dep name -> list of packages that use it
    let mut dep_to_packages: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for pkg in &packages {
        for dep in pkg.external_dependencies() {
            dep_to_packages
                .entry(&dep.name)
                .or_default()
                .push(pkg.name());
        }
    }

    // Find affected packages
    let mut affected: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for changed in changed_deps {
        if let Some(pkgs) = dep_to_packages.get(changed.as_str()) {
            for pkg in pkgs {
                affected
                    .entry((*pkg).to_string())
                    .or_default()
                    .push(changed.clone());
            }
        }
    }

    log::debug!(
        "Found {} packages affected by {} changed dependencies",
        affected.len(),
        changed_deps.len()
    );

    Ok(affected)
}

/// Finds transitively affected dependencies using BFS.
///
/// Starting from a set of changed dependencies, this finds all other
/// dependencies that transitively depend on them.
///
/// # Arguments
///
/// * `lockfile` - The parsed lockfile
/// * `changed_deps` - Initial set of changed dependency names
/// * `max_depth` - Maximum depth to traverse (use `MAX_TRANSITIVE_DEPTH` if unsure)
///
/// # Returns
///
/// A set of all transitively affected dependency names.
pub fn find_transitive_dependents(
    lockfile: &dyn Lockfile,
    changed_deps: &[String],
    max_depth: usize,
) -> BTreeSet<String> {
    let reverse_deps = lockfile.reverse_dependency_map();
    let mut affected = BTreeSet::new();
    let mut queue: VecDeque<(String, usize)> =
        changed_deps.iter().map(|d| (d.clone(), 0)).collect();

    while let Some((dep, depth)) = queue.pop_front() {
        if depth > max_depth || affected.contains(&dep) {
            continue;
        }
        affected.insert(dep.clone());

        if let Some(dependents) = reverse_deps.get(&dep) {
            for dependent in dependents {
                if !affected.contains(dependent) {
                    queue.push_back((dependent.clone(), depth + 1));
                }
            }
        }
    }

    affected
}

/// Gets lockfile changes from git and finds affected packages.
///
/// This is the main entry point for git-based affected package detection.
///
/// # Arguments
///
/// * `workspace` - The workspace to analyze
/// * `base_commit` - Base git commit reference
/// * `head_commit` - Head git commit reference
///
/// # Returns
///
/// A map from affected package names to the dependencies that affect them.
///
/// # Errors
///
/// Returns an error if:
/// * Extracting git diff fails
/// * Workspace package loading fails
#[cfg(feature = "git-diff")]
pub async fn get_affected_from_git(
    workspace: &dyn Workspace,
    base_commit: &str,
    head_commit: &str,
) -> Result<BTreeMap<String, Vec<String>>, BoxError> {
    let changes = extract_file_diff_from_git(
        workspace.root(),
        base_commit,
        head_commit,
        workspace.lockfile_path(),
    )
    .await?;

    if changes.is_empty() {
        log::debug!(
            "No changes to {} between {base_commit} and {head_commit}",
            workspace.lockfile_path()
        );
        return Ok(BTreeMap::new());
    }

    let parser = workspace.diff_parser();
    let changed_deps = parser.parse_changes(&changes);

    log::debug!(
        "Parsed {} changed dependencies from lockfile diff",
        changed_deps.len()
    );

    find_affected_by_dependencies(workspace, &changed_deps).await
}

/// Gets affected packages with full transitive analysis.
///
/// This function:
/// 1. Parses the lockfile diff to find directly changed dependencies
/// 2. Uses the lockfile to find transitively affected dependencies
/// 3. Maps all affected dependencies to workspace packages
///
/// # Arguments
///
/// * `workspace` - The workspace to analyze
/// * `base_commit` - Base git commit reference
/// * `head_commit` - Head git commit reference
///
/// # Returns
///
/// A list of affected packages with reasoning explaining why they're affected.
///
/// # Errors
///
/// Returns an error if:
/// * Extracting git diff fails
/// * Reading or parsing the lockfile fails
/// * Workspace package loading fails
#[cfg(feature = "git-diff")]
pub async fn get_affected_with_transitive_analysis(
    workspace: &dyn Workspace,
    base_commit: &str,
    head_commit: &str,
) -> Result<Vec<AffectedPackageInfo>, BoxError> {
    let changes = extract_file_diff_from_git(
        workspace.root(),
        base_commit,
        head_commit,
        workspace.lockfile_path(),
    )
    .await?;

    if changes.is_empty() {
        return Ok(Vec::new());
    }

    let parser = workspace.diff_parser();
    let directly_changed = parser.parse_changes(&changes);

    if directly_changed.is_empty() {
        return Ok(Vec::new());
    }

    // Read and parse lockfile for transitive analysis
    let lockfile = workspace.read_lockfile().await?;

    // Find transitively affected dependencies
    let all_affected_deps =
        find_transitive_dependents(lockfile.as_ref(), &directly_changed, MAX_TRANSITIVE_DEPTH);

    // Map to workspace packages
    let packages = workspace.packages().await?;
    let mut affected_packages: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for pkg in &packages {
        for dep in pkg.external_dependencies() {
            if all_affected_deps.contains(&dep.name) {
                affected_packages
                    .entry(pkg.name().to_string())
                    .or_default()
                    .insert(dep.name.clone());
            }
        }
    }

    // Convert to AffectedPackageInfo
    let result: Vec<AffectedPackageInfo> = affected_packages
        .into_iter()
        .map(|(name, deps)| {
            let reasoning: Vec<String> = deps
                .into_iter()
                .map(|dep| {
                    if directly_changed.contains(&dep) {
                        format!("Direct dependency '{dep}' changed")
                    } else {
                        format!("Transitive dependency '{dep}' affected")
                    }
                })
                .collect();

            AffectedPackageInfo::with_reasoning(name, reasoning)
        })
        .collect();

    Ok(result)
}

/// Builds a map of external dependencies to workspace packages.
///
/// # Arguments
///
/// * `workspace` - The workspace to analyze
///
/// # Returns
///
/// A map where keys are external dependency names and values are lists
/// of workspace package names that depend on them.
///
/// # Errors
///
/// Returns an error if loading workspace packages fails.
pub async fn build_dependency_map(
    workspace: &dyn Workspace,
) -> Result<BTreeMap<String, Vec<String>>, BoxError> {
    let packages = workspace.packages().await?;
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for pkg in packages {
        for dep in pkg.external_dependencies() {
            map.entry(dep.name.clone())
                .or_default()
                .push(pkg.name().to_string());
        }
    }

    Ok(map)
}

/// Finds which packages would be affected by changing specific dependencies.
///
/// This is useful for "what-if" analysis without actually having git changes.
///
/// # Arguments
///
/// * `workspace` - The workspace to analyze
/// * `dependency_names` - Names of dependencies to check
/// * `include_transitive` - Whether to include transitive dependents
///
/// # Returns
///
/// List of affected package names.
///
/// # Errors
///
/// Returns an error if:
/// * Reading or parsing the lockfile fails (when `include_transitive` is true)
/// * Workspace package loading fails
pub async fn find_packages_using_dependencies(
    workspace: &dyn Workspace,
    dependency_names: &[String],
    include_transitive: bool,
) -> Result<Vec<String>, BoxError> {
    let deps_to_check = if include_transitive {
        let lockfile = workspace.read_lockfile().await?;
        find_transitive_dependents(lockfile.as_ref(), dependency_names, MAX_TRANSITIVE_DEPTH)
            .into_iter()
            .collect()
    } else {
        dependency_names.to_vec()
    };

    let dep_map = build_dependency_map(workspace).await?;
    let mut affected: BTreeSet<String> = BTreeSet::new();

    for dep in &deps_to_check {
        if let Some(packages) = dep_map.get(dep) {
            affected.extend(packages.iter().cloned());
        }
    }

    Ok(affected.into_iter().collect())
}

// Stub for when git-diff is disabled
#[cfg(not(feature = "git-diff"))]
pub async fn get_affected_from_git(
    _workspace: &dyn Workspace,
    _base_commit: &str,
    _head_commit: &str,
) -> Result<BTreeMap<String, Vec<String>>, BoxError> {
    Err("git-diff feature is not enabled".into())
}

#[cfg(not(feature = "git-diff"))]
pub async fn get_affected_with_transitive_analysis(
    _workspace: &dyn Workspace,
    _base_commit: &str,
    _head_commit: &str,
) -> Result<Vec<AffectedPackageInfo>, BoxError> {
    Err("git-diff feature is not enabled".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock lockfile for testing transitive analysis
    struct MockLockfileEntry {
        name: String,
        version: String,
        deps: Vec<String>,
    }

    impl super::super::traits::LockfileEntry for MockLockfileEntry {
        fn name(&self) -> &str {
            &self.name
        }
        fn version(&self) -> &str {
            &self.version
        }
        fn dependencies(&self) -> &[String] {
            &self.deps
        }
    }

    struct MockLockfile {
        entries: Vec<MockLockfileEntry>,
    }

    impl super::super::traits::Lockfile for MockLockfile {
        fn entries(&self) -> Vec<Box<dyn super::super::traits::LockfileEntry>> {
            self.entries
                .iter()
                .map(|e| {
                    Box::new(MockLockfileEntry {
                        name: e.name.clone(),
                        version: e.version.clone(),
                        deps: e.deps.clone(),
                    }) as Box<dyn super::super::traits::LockfileEntry>
                })
                .collect()
        }
    }

    #[test]
    fn test_find_transitive_dependents() {
        // Create a dependency graph:
        // A -> B -> C -> D
        //      |
        //      v
        //      E
        let lockfile = MockLockfile {
            entries: vec![
                MockLockfileEntry {
                    name: "A".to_string(),
                    version: "1.0.0".to_string(),
                    deps: vec!["B".to_string()],
                },
                MockLockfileEntry {
                    name: "B".to_string(),
                    version: "1.0.0".to_string(),
                    deps: vec!["C".to_string(), "E".to_string()],
                },
                MockLockfileEntry {
                    name: "C".to_string(),
                    version: "1.0.0".to_string(),
                    deps: vec!["D".to_string()],
                },
                MockLockfileEntry {
                    name: "D".to_string(),
                    version: "1.0.0".to_string(),
                    deps: vec![],
                },
                MockLockfileEntry {
                    name: "E".to_string(),
                    version: "1.0.0".to_string(),
                    deps: vec![],
                },
            ],
        };

        // If D changes, C, B, and A should be affected
        let affected = find_transitive_dependents(&lockfile, &["D".to_string()], 5);
        assert!(affected.contains("D"));
        assert!(affected.contains("C"));
        assert!(affected.contains("B"));
        assert!(affected.contains("A"));
        assert!(!affected.contains("E")); // E doesn't depend on D

        // With max_depth=1, only direct dependents
        let affected = find_transitive_dependents(&lockfile, &["D".to_string()], 1);
        assert!(affected.contains("D"));
        assert!(affected.contains("C"));
        assert!(!affected.contains("B")); // B is 2 levels up
    }
}
