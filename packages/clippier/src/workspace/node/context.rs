//! Node.js workspace context implementation.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use switchy_async::sync::RwLock;

use super::{
    NodePackageManager,
    lockfile::{NodeLockDiffParser, NodeLockfile},
    package::{NodePackage, parse_dependencies, read_package_name},
};
use crate::workspace::{
    glob::expand_workspace_globs,
    traits::{Lockfile, LockfileDiffParser, Package, Workspace},
};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Node.js workspace context for monorepo projects.
///
/// Supports npm, pnpm, and bun workspaces.
#[derive(Debug)]
pub struct NodeWorkspace {
    /// Workspace root directory
    root: PathBuf,

    /// Detected package manager
    package_manager: NodePackageManager,

    /// Expanded member patterns (directories, not globs)
    member_patterns: Vec<String>,

    /// Cache: package name -> package path
    member_cache: Arc<RwLock<BTreeMap<String, PathBuf>>>,

    /// Cache: canonical paths of known members
    path_cache: Arc<RwLock<BTreeSet<PathBuf>>>,

    /// Whether all members have been loaded
    fully_loaded: Arc<RwLock<bool>>,

    /// Cached packages (fully loaded with dependencies)
    packages_cache: Arc<RwLock<Option<Vec<NodePackage>>>>,
}

impl NodeWorkspace {
    /// Returns the detected package manager.
    #[must_use]
    pub const fn package_manager(&self) -> NodePackageManager {
        self.package_manager
    }

    /// Attempts to detect a Node.js workspace at the given root path.
    ///
    /// Returns `Ok(Some(workspace))` if a Node.js workspace is detected,
    /// `Ok(None)` if no workspace is present, or an error.
    ///
    /// # Errors
    ///
    /// Returns an error if the workspace configuration cannot be read or parsed.
    pub async fn detect(root: &Path) -> Result<Option<Self>, BoxError> {
        // 1. Check for pnpm (has its own workspace file)
        let pnpm_workspace = root.join("pnpm-workspace.yaml");
        if switchy_fs::unsync::exists(&pnpm_workspace).await {
            return Ok(Some(Self::new(root, NodePackageManager::Pnpm).await?));
        }

        // 2. Check for package.json with workspaces field
        let package_json = root.join("package.json");
        if !switchy_fs::unsync::exists(&package_json).await {
            return Ok(None);
        }

        let content = switchy_fs::unsync::read_to_string(&package_json).await?;
        let json: serde_json::Value = serde_json::from_str(&content)?;

        // Check if workspaces field exists
        if json.get("workspaces").is_none() {
            return Ok(None);
        }

        // 3. Detect package manager by lockfile
        let manager = if switchy_fs::unsync::exists(&root.join("bun.lock")).await {
            NodePackageManager::Bun
        } else if switchy_fs::unsync::exists(&root.join("package-lock.json")).await {
            NodePackageManager::Npm
        } else {
            // Default to npm if no lockfile found
            NodePackageManager::Npm
        };

        Ok(Some(Self::new(root, manager).await?))
    }

    /// Creates a new Node workspace with the specified package manager.
    ///
    /// # Errors
    ///
    /// Returns an error if the workspace configuration cannot be read or parsed.
    pub async fn new(root: &Path, manager: NodePackageManager) -> Result<Self, BoxError> {
        let member_patterns = match manager {
            NodePackageManager::Pnpm => parse_pnpm_workspaces(root).await?,
            NodePackageManager::Npm | NodePackageManager::Bun => {
                parse_package_json_workspaces(root).await?
            }
        };

        // Expand glob patterns
        let patterns_ref: Vec<&str> = member_patterns.iter().map(String::as_str).collect();
        let expanded = expand_workspace_globs(root, &patterns_ref, "package.json").await;

        log::debug!(
            "NodeWorkspace ({:?}): expanded {} patterns to {} member paths",
            manager,
            member_patterns.len(),
            expanded.len()
        );

        Ok(Self {
            root: root.to_path_buf(),
            package_manager: manager,
            member_patterns: expanded,
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
                let package_json = canonical.join("package.json");
                if let Some(name) = read_package_name(&package_json).await {
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
    async fn load_package(&self, name: &str, path: &Path) -> Result<NodePackage, BoxError> {
        let package_json_path = path.join("package.json");
        let content = switchy_fs::unsync::read_to_string(&package_json_path).await?;
        let package_json: serde_json::Value = serde_json::from_str(&content)?;

        let version = package_json
            .get("version")
            .and_then(|v| v.as_str())
            .map(String::from);

        let workspace_members = self.member_names().await;
        let (workspace_deps, external_deps) = parse_dependencies(&package_json, &workspace_members);

        Ok(NodePackage::new(
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
    pub fn parse_lockfile_content(&self, content: &str) -> Result<NodeLockfile, BoxError> {
        NodeLockfile::parse(content, self.package_manager)
    }
}

#[async_trait]
impl Workspace for NodeWorkspace {
    fn root(&self) -> &Path {
        &self.root
    }

    fn lockfile_path(&self) -> &'static str {
        match self.package_manager {
            NodePackageManager::Npm => "package-lock.json",
            NodePackageManager::Pnpm => "pnpm-lock.yaml",
            NodePackageManager::Bun => "bun.lock",
        }
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

                let package_json = canonical.join("package.json");
                if let Some(name) = read_package_name(&package_json).await {
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
        let lockfile = self.parse_lockfile_content(&content)?;
        Ok(Box::new(lockfile))
    }

    fn diff_parser(&self) -> Box<dyn LockfileDiffParser> {
        Box::new(NodeLockDiffParser::new(self.package_manager))
    }
}

/// Parses workspaces from pnpm-workspace.yaml.
async fn parse_pnpm_workspaces(root: &Path) -> Result<Vec<String>, BoxError> {
    let pnpm_workspace = root.join("pnpm-workspace.yaml");
    let content = switchy_fs::unsync::read_to_string(&pnpm_workspace).await?;
    let yaml: serde_yaml::Value = serde_yaml::from_str(&content)?;

    let packages = yaml
        .get("packages")
        .and_then(|p| p.as_sequence())
        .ok_or("Missing packages field in pnpm-workspace.yaml")?;

    let patterns: Vec<String> = packages
        .iter()
        .filter_map(|p| p.as_str().map(String::from))
        .filter(|p| !p.starts_with('!')) // Skip exclusion patterns for now
        .collect();

    Ok(patterns)
}

/// Parses workspaces from package.json.
async fn parse_package_json_workspaces(root: &Path) -> Result<Vec<String>, BoxError> {
    let package_json = root.join("package.json");
    let content = switchy_fs::unsync::read_to_string(&package_json).await?;
    let json: serde_json::Value = serde_json::from_str(&content)?;

    match json.get("workspaces") {
        Some(serde_json::Value::Array(arr)) => Ok(arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect()),
        Some(serde_json::Value::Object(obj)) => {
            // Yarn-style: { "packages": [...] }
            obj.get("packages")
                .and_then(|p| p.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .ok_or_else(|| "Invalid workspaces format in package.json".into())
        }
        _ => Err("Missing workspaces field in package.json".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::traits::Workspace;
    use std::path::PathBuf;

    fn fixture_path(name: &str) -> PathBuf {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/workspace/node/fixtures")
            .join(name);

        // Seed the fixture directory into the simulator filesystem if enabled
        if switchy_fs::is_simulator_enabled() {
            switchy_fs::seed_from_real_fs_same_path(&path)
                .expect("Failed to seed fixture into simulator filesystem");
        }

        path
    }

    #[switchy_async::test]
    async fn test_detect_npm_workspace() {
        let path = fixture_path("npm-workspace");
        let ws = NodeWorkspace::detect(&path).await.unwrap();
        assert!(ws.is_some());
        let ws = ws.unwrap();
        assert_eq!(ws.package_manager(), NodePackageManager::Npm);
        assert_eq!(ws.lockfile_path(), "package-lock.json");
    }

    #[switchy_async::test]
    async fn test_detect_pnpm_workspace() {
        let path = fixture_path("pnpm-workspace");
        let ws = NodeWorkspace::detect(&path).await.unwrap();
        assert!(ws.is_some());
        let ws = ws.unwrap();
        assert_eq!(ws.package_manager(), NodePackageManager::Pnpm);
        assert_eq!(ws.lockfile_path(), "pnpm-lock.yaml");
    }

    #[switchy_async::test]
    async fn test_detect_bun_workspace() {
        let path = fixture_path("bun-workspace");
        let ws = NodeWorkspace::detect(&path).await.unwrap();
        assert!(ws.is_some());
        let ws = ws.unwrap();
        assert_eq!(ws.package_manager(), NodePackageManager::Bun);
        assert_eq!(ws.lockfile_path(), "bun.lock");
    }

    #[switchy_async::test]
    async fn test_npm_workspace_packages() {
        let path = fixture_path("npm-workspace");
        let ws = NodeWorkspace::detect(&path).await.unwrap().unwrap();
        let packages = ws.packages().await.unwrap();

        assert_eq!(packages.len(), 3);

        let names: std::collections::BTreeSet<_> =
            packages.iter().map(|p| p.name().to_string()).collect();
        assert!(names.contains("@myorg/api"));
        assert!(names.contains("@myorg/client"));
        assert!(names.contains("@myorg/models"));
    }

    #[switchy_async::test]
    async fn test_pnpm_workspace_packages() {
        let path = fixture_path("pnpm-workspace");
        let ws = NodeWorkspace::detect(&path).await.unwrap().unwrap();
        let packages = ws.packages().await.unwrap();

        assert_eq!(packages.len(), 3);

        let names: std::collections::BTreeSet<_> =
            packages.iter().map(|p| p.name().to_string()).collect();
        assert!(names.contains("@myorg/api"));
        assert!(names.contains("@myorg/client"));
        assert!(names.contains("@myorg/models"));
    }

    #[switchy_async::test]
    async fn test_bun_workspace_packages() {
        let path = fixture_path("bun-workspace");
        let ws = NodeWorkspace::detect(&path).await.unwrap().unwrap();
        let packages = ws.packages().await.unwrap();

        assert_eq!(packages.len(), 3);

        let names: std::collections::BTreeSet<_> =
            packages.iter().map(|p| p.name().to_string()).collect();
        assert!(names.contains("@myorg/api"));
        assert!(names.contains("@myorg/client"));
        assert!(names.contains("@myorg/models"));
    }

    #[switchy_async::test]
    async fn test_workspace_dependencies() {
        let path = fixture_path("npm-workspace");
        let ws = NodeWorkspace::detect(&path).await.unwrap().unwrap();
        let packages = ws.packages().await.unwrap();

        // Find the api package
        let api = packages.iter().find(|p| p.name() == "@myorg/api").unwrap();

        // Check workspace dependencies
        assert!(
            api.workspace_dependencies()
                .iter()
                .any(|d| d.name == "@myorg/models")
        );

        // Check external dependencies
        assert!(
            api.external_dependencies()
                .iter()
                .any(|d| d.name == "express")
        );
    }

    #[switchy_async::test]
    async fn test_is_member_by_name() {
        let path = fixture_path("npm-workspace");
        let ws = NodeWorkspace::detect(&path).await.unwrap().unwrap();

        assert!(ws.is_member_by_name("@myorg/api").await);
        assert!(ws.is_member_by_name("@myorg/models").await);
        assert!(!ws.is_member_by_name("nonexistent").await);
    }

    #[switchy_async::test]
    async fn test_find_member() {
        let path = fixture_path("npm-workspace");
        let ws = NodeWorkspace::detect(&path).await.unwrap().unwrap();

        let api_path = ws.find_member("@myorg/api").await;
        assert!(api_path.is_some());
        assert!(api_path.unwrap().ends_with("packages/api"));

        let nonexistent = ws.find_member("nonexistent").await;
        assert!(nonexistent.is_none());
    }

    #[switchy_async::test]
    async fn test_no_workspace_detected() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let ws = NodeWorkspace::detect(&path).await.unwrap();
        // The clippier package itself is not a node workspace
        assert!(ws.is_none());
    }
}
