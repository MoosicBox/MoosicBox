//! Cargo workspace context implementation.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use switchy_async::sync::RwLock;

use super::{
    lockfile::{CargoLockDiffParser, CargoLockfile},
    package::{CargoPackage, parse_dependencies, read_package_name},
};
use crate::workspace::{
    glob::expand_workspace_globs,
    traits::{Lockfile, LockfileDiffParser, Package, Workspace},
};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Cargo workspace context for Rust projects.
///
/// This provides the `Workspace` trait implementation for Cargo workspaces,
/// enabling package discovery, dependency analysis, and lockfile parsing.
#[derive(Debug)]
pub struct CargoWorkspace {
    /// Workspace root directory
    root: PathBuf,

    /// Expanded member patterns (directories, not globs)
    member_patterns: Vec<String>,

    /// Cache: package name -> package path
    member_cache: Arc<RwLock<BTreeMap<String, PathBuf>>>,

    /// Cache: canonical paths of known members
    path_cache: Arc<RwLock<BTreeSet<PathBuf>>>,

    /// Whether all members have been loaded
    fully_loaded: Arc<RwLock<bool>>,

    /// Cached packages (fully loaded with dependencies)
    packages_cache: Arc<RwLock<Option<Vec<CargoPackage>>>>,
}

impl CargoWorkspace {
    /// Attempts to detect a Cargo workspace at the given root path.
    ///
    /// Returns `Ok(Some(workspace))` if a Cargo workspace is detected,
    /// `Ok(None)` if no workspace is present, or an error.
    ///
    /// # Errors
    ///
    /// Returns an error if the `Cargo.toml` cannot be read or parsed.
    pub async fn detect(root: &Path) -> Result<Option<Self>, BoxError> {
        let cargo_toml = root.join("Cargo.toml");
        if !switchy_fs::unsync::exists(&cargo_toml).await {
            return Ok(None);
        }

        let content = switchy_fs::unsync::read_to_string(&cargo_toml).await?;
        let toml: toml::Value = toml::from_str(&content)?;

        // Check for [workspace] section
        if toml.get("workspace").is_some() {
            Ok(Some(Self::new(root).await?))
        } else {
            Ok(None)
        }
    }

    /// Creates a new Cargo workspace context.
    ///
    /// This reads the workspace Cargo.toml and expands member globs.
    ///
    /// # Errors
    ///
    /// Returns an error if the `Cargo.toml` cannot be read or parsed.
    pub async fn new(root: &Path) -> Result<Self, BoxError> {
        let workspace_cargo = root.join("Cargo.toml");
        let content = switchy_fs::unsync::read_to_string(&workspace_cargo).await?;
        let root_toml: toml::Value = toml::from_str(&content)?;

        let mut raw_patterns = Vec::new();

        if let Some(toml::Value::Table(workspace)) = root_toml.get("workspace")
            && let Some(toml::Value::Array(member_list)) = workspace.get("members")
        {
            for member in member_list {
                if let toml::Value::String(member_pattern) = member {
                    raw_patterns.push(member_pattern.clone());
                }
            }
        }

        // Expand glob patterns
        let patterns_ref: Vec<&str> = raw_patterns.iter().map(String::as_str).collect();
        let member_patterns = expand_workspace_globs(root, &patterns_ref, "Cargo.toml").await;

        log::debug!(
            "CargoWorkspace: expanded {} patterns to {} member paths",
            raw_patterns.len(),
            member_patterns.len()
        );

        Ok(Self {
            root: root.to_path_buf(),
            member_patterns,
            member_cache: Arc::new(RwLock::new(BTreeMap::new())),
            path_cache: Arc::new(RwLock::new(BTreeSet::new())),
            fully_loaded: Arc::new(RwLock::new(false)),
            packages_cache: Arc::new(RwLock::new(None)),
        })
    }

    /// Ensures all workspace members are loaded into the cache.
    async fn ensure_fully_loaded(&self) {
        {
            let loaded = self.fully_loaded.read().await;
            if *loaded {
                return;
            }
        }

        log::trace!(
            "Loading all {} workspace members",
            self.member_patterns.len()
        );
        let start = std::time::Instant::now();

        let mut member_cache = self.member_cache.write().await;
        let mut path_cache = self.path_cache.write().await;

        for pattern in &self.member_patterns {
            let member_path = self.root.join(pattern);
            if switchy_fs::unsync::exists(&member_path).await
                && let Ok(canonical) = switchy_fs::unsync::canonicalize(&member_path).await
                && !path_cache.contains(&canonical)
            {
                let cargo_toml = canonical.join("Cargo.toml");
                if let Some(name) = read_package_name(&cargo_toml).await {
                    member_cache.insert(name, canonical.clone());
                    path_cache.insert(canonical);
                }
            }
        }

        let member_count = member_cache.len();
        drop(member_cache);
        drop(path_cache);

        {
            let mut loaded = self.fully_loaded.write().await;
            *loaded = true;
        }

        log::trace!("Loaded {member_count} members in {:?}", start.elapsed());
    }

    /// Gets the set of all workspace member names.
    async fn member_names(&self) -> BTreeSet<String> {
        self.ensure_fully_loaded().await;
        let cache = self.member_cache.read().await;
        cache.keys().cloned().collect()
    }

    /// Loads a single package with its dependencies.
    async fn load_package(&self, name: &str, path: &Path) -> Result<CargoPackage, BoxError> {
        let cargo_toml_path = path.join("Cargo.toml");
        let content = switchy_fs::unsync::read_to_string(&cargo_toml_path).await?;
        let cargo_toml: toml::Value = toml::from_str(&content)?;

        let version = cargo_toml
            .get("package")
            .and_then(|p| p.get("version"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let workspace_members = self.member_names().await;
        let (workspace_deps, external_deps) = parse_dependencies(&cargo_toml, &workspace_members);

        Ok(CargoPackage::new(
            name.to_string(),
            version,
            path.to_path_buf(),
            workspace_deps,
            external_deps,
        ))
    }

    /// Parses lockfile content into a structured representation.
    ///
    /// # Errors
    ///
    /// Returns an error if the lockfile content cannot be parsed.
    pub fn parse_lockfile_content(content: &str) -> Result<CargoLockfile, BoxError> {
        CargoLockfile::parse(content)
    }
}

#[async_trait]
impl Workspace for CargoWorkspace {
    fn root(&self) -> &Path {
        &self.root
    }

    fn lockfile_path(&self) -> &'static str {
        "Cargo.lock"
    }

    fn member_patterns(&self) -> &[String] {
        &self.member_patterns
    }

    async fn is_member_by_path(&self, path: &Path) -> bool {
        let Ok(canonical) = switchy_fs::unsync::canonicalize(path).await else {
            return false;
        };

        // Check cache first
        {
            let cache = self.path_cache.read().await;
            if cache.contains(&canonical) {
                return true;
            }
        }

        // Check against patterns
        for pattern in &self.member_patterns {
            let member_path = self.root.join(pattern);
            if let Ok(member_canonical) = switchy_fs::unsync::canonicalize(&member_path).await
                && member_canonical == canonical
            {
                // Update caches
                let mut path_cache = self.path_cache.write().await;
                path_cache.insert(canonical.clone());
                drop(path_cache);

                let cargo_toml = canonical.join("Cargo.toml");
                if let Some(name) = read_package_name(&cargo_toml).await {
                    let mut member_cache = self.member_cache.write().await;
                    member_cache.insert(name, canonical);
                }

                return true;
            }
        }

        false
    }

    async fn is_member_by_name(&self, name: &str) -> bool {
        // Check cache first
        {
            let cache = self.member_cache.read().await;
            if cache.contains_key(name) {
                return true;
            }
        }

        // Ensure fully loaded and check again
        self.ensure_fully_loaded().await;
        let cache = self.member_cache.read().await;
        cache.contains_key(name)
    }

    async fn find_member(&self, name: &str) -> Option<PathBuf> {
        // Check cache first
        {
            let cache = self.member_cache.read().await;
            if let Some(path) = cache.get(name) {
                return Some(path.clone());
            }
        }

        // Ensure fully loaded and check again
        self.ensure_fully_loaded().await;
        let cache = self.member_cache.read().await;
        cache.get(name).cloned()
    }

    async fn packages(&self) -> Result<Vec<Box<dyn Package>>, BoxError> {
        // Check cache first
        {
            let cache = self.packages_cache.read().await;
            if let Some(packages) = cache.as_ref() {
                return Ok(packages
                    .iter()
                    .cloned()
                    .map(|p| Box::new(p) as Box<dyn Package>)
                    .collect());
            }
        }

        // Load all packages
        self.ensure_fully_loaded().await;

        let member_cache = self.member_cache.read().await;
        let mut packages = Vec::with_capacity(member_cache.len());

        for (name, path) in member_cache.iter() {
            match self.load_package(name, path).await {
                Ok(pkg) => packages.push(pkg),
                Err(e) => {
                    log::warn!("Failed to load package {name}: {e}");
                }
            }
        }
        drop(member_cache);

        // Cache the result
        {
            let mut cache = self.packages_cache.write().await;
            *cache = Some(packages.clone());
        }

        Ok(packages
            .into_iter()
            .map(|p| Box::new(p) as Box<dyn Package>)
            .collect())
    }

    async fn read_lockfile(&self) -> Result<Box<dyn Lockfile>, BoxError> {
        let path = self.root.join(self.lockfile_path());
        let content = switchy_fs::unsync::read_to_string(&path).await?;
        let lockfile = Self::parse_lockfile_content(&content)?;
        Ok(Box::new(lockfile))
    }

    fn diff_parser(&self) -> Box<dyn LockfileDiffParser> {
        Box::new(CargoLockDiffParser)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Full integration tests would use switchy_fs simulator
    // These are basic unit tests for the sync functions

    #[test]
    fn test_lockfile_path() {
        // This test just verifies the constant
        let ws = CargoWorkspace {
            root: PathBuf::from("/test"),
            member_patterns: vec![],
            member_cache: Arc::new(RwLock::new(BTreeMap::new())),
            path_cache: Arc::new(RwLock::new(BTreeSet::new())),
            fully_loaded: Arc::new(RwLock::new(false)),
            packages_cache: Arc::new(RwLock::new(None)),
        };

        assert_eq!(ws.lockfile_path(), "Cargo.lock");
    }
}
