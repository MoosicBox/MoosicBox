use git2::Repository;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CargoLockPackage {
    pub name: String,
    pub version: String,
    pub source: Option<String>,
    pub dependencies: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CargoLock {
    pub version: u32,
    pub package: Vec<CargoLockPackage>,
}

/// Extract changed dependencies from git diff between two commits
pub fn extract_changed_dependencies_from_git(
    repo_path: &std::path::Path,
    base_commit: &str,
    head_commit: &str,
    changed_files: &[String],
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    log::trace!("🔍 Extracting changed dependencies from git diff");
    log::trace!("Repository path: {}", repo_path.display());
    log::trace!("Base commit: {base_commit}");
    log::trace!("Head commit: {head_commit}");
    log::trace!("Changed files: {changed_files:?}");

    // Check if Cargo.lock is in the changed files
    if !changed_files.iter().any(|f| f == "Cargo.lock") {
        log::trace!("Cargo.lock not in changed files, returning empty result");
        return Ok(Vec::new());
    }

    let repo = Repository::open(repo_path).map_err(|e| {
        format!(
            "Failed to open repository at {}: {}",
            repo_path.display(),
            e
        )
    })?;

    // Parse commit references
    let base_oid = repo
        .revparse_single(base_commit)
        .map_err(|e| format!("Failed to parse base commit '{base_commit}': {e}"))?
        .id();
    let head_oid = repo
        .revparse_single(head_commit)
        .map_err(|e| format!("Failed to parse head commit '{head_commit}': {e}"))?
        .id();

    log::trace!("Base OID: {base_oid}");
    log::trace!("Head OID: {head_oid}");

    // Get the diff between the commits
    let base_commit = repo
        .find_commit(base_oid)
        .map_err(|e| format!("Failed to find base commit {base_oid}: {e}"))?;
    let head_commit = repo
        .find_commit(head_oid)
        .map_err(|e| format!("Failed to find head commit {head_oid}: {e}"))?;

    let base_tree = base_commit
        .tree()
        .map_err(|e| format!("Failed to get base tree: {e}"))?;
    let head_tree = head_commit
        .tree()
        .map_err(|e| format!("Failed to get head tree: {e}"))?;

    let diff = repo
        .diff_tree_to_tree(Some(&base_tree), Some(&head_tree), None)
        .map_err(|e| format!("Failed to create diff: {e}"))?;

    // Find Cargo.lock changes
    let mut cargo_lock_changes = Vec::new();
    let mut found_cargo_lock = false;

    // First, check if Cargo.lock is in the diff
    for delta in diff.deltas() {
        if let Some(path) = delta.new_file().path() {
            if path.to_string_lossy() == "Cargo.lock" {
                found_cargo_lock = true;
                log::trace!("Found Cargo.lock in diff");
                break;
            }
        }
    }

    if !found_cargo_lock {
        log::trace!("Cargo.lock not found in diff");
        return Ok(Vec::new());
    }

    // Now extract the changes using print callback
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        let content = std::str::from_utf8(line.content()).unwrap_or("");
        cargo_lock_changes.push((line.origin(), content.to_string()));
        true
    })?;

    if cargo_lock_changes.is_empty() {
        log::trace!("No Cargo.lock changes found in diff");
        return Ok(Vec::new());
    }

    log::trace!(
        "Found {} lines of Cargo.lock changes",
        cargo_lock_changes.len()
    );

    // Parse the changes to extract dependency information
    let changed_deps = parse_cargo_lock_changes(&cargo_lock_changes);

    log::trace!(
        "Extracted {} changed dependencies: {:?}",
        changed_deps.len(),
        changed_deps
    );

    Ok(changed_deps)
}

/// Parse Cargo.lock changes to extract changed dependencies
fn parse_cargo_lock_changes(changes: &[(char, String)]) -> Vec<String> {
    let mut changed_dependencies = HashSet::new();
    let mut current_package = None;
    let mut in_package_block = false;

    for (origin, line) in changes {
        let line = line.trim();

        // Track package blocks
        if line == "[[package]]" {
            in_package_block = true;
            current_package = None;
            continue;
        }

        // Empty line ends the current package block
        if line.is_empty() {
            in_package_block = false;
            current_package = None;
            continue;
        }

        if !in_package_block {
            continue;
        }

        // Parse package name
        if line.starts_with("name = ") {
            if let Some(name) = line
                .strip_prefix("name = ")
                .and_then(|s| s.strip_prefix('"'))
                .and_then(|s| s.strip_suffix('"'))
            {
                current_package = Some(name.to_string());
                log::trace!("Found package: {name}");
            }
        }

        // Parse version changes
        if line.starts_with("version = ") && current_package.is_some() {
            match origin {
                '+' | '-' => {
                    // This is a version change for the current package
                    if let Some(package_name) = &current_package {
                        log::trace!("Version change for package {package_name}: {origin} {line}");
                        changed_dependencies.insert(package_name.clone());
                    }
                }
                _ => {}
            }
        }
    }

    changed_dependencies.into_iter().collect()
}

/// Parse Cargo.lock file content
#[allow(dead_code)]
pub fn parse_cargo_lock(content: &str) -> Result<CargoLock, Box<dyn std::error::Error>> {
    let cargo_lock: CargoLock = toml::from_str(content)?;
    Ok(cargo_lock)
}

/// Build a map of external dependencies to workspace packages that use them
pub fn build_external_dependency_map(
    workspace_root: &std::path::Path,
    workspace_members: &[String],
) -> Result<HashMap<String, Vec<String>>, Box<dyn std::error::Error>> {
    let mut external_dep_map: HashMap<String, Vec<String>> = HashMap::new();

    // First, parse workspace-level Cargo.toml to get workspace dependencies
    let workspace_cargo_path = workspace_root.join("Cargo.toml");
    let workspace_source = std::fs::read_to_string(&workspace_cargo_path)?;
    let workspace_value: toml::Value = toml::from_str(&workspace_source)?;

    // Get workspace dependencies (these are external packages that can be referenced with workspace = true)
    let mut workspace_external_deps = HashSet::new();
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
pub fn find_packages_affected_by_external_deps(
    external_dep_map: &HashMap<String, Vec<String>>,
    changed_external_deps: &[String],
) -> Vec<String> {
    let mut affected_packages = HashSet::new();

    for dep in changed_external_deps {
        if let Some(packages) = external_dep_map.get(dep) {
            for package in packages {
                affected_packages.insert(package.clone());
            }
        }
    }

    let mut result: Vec<String> = affected_packages.into_iter().collect();
    result.sort();

    log::trace!("External dependencies {changed_external_deps:?} affect packages: {result:?}");

    result
}
