//! Node.js workspace support for npm, pnpm, and bun monorepos.
//!
//! This module provides workspace detection and package analysis for Node.js projects
//! using various package managers:
//!
//! - **npm**: Uses `package.json` `workspaces` field with `package-lock.json`
//! - **pnpm**: Uses `pnpm-workspace.yaml` with `pnpm-lock.yaml`
//! - **bun**: Uses `package.json` `workspaces` field with `bun.lock`
//!
//! # Example
//!
//! ```rust,ignore
//! use clippier::workspace::node::{NodeWorkspace, NodePackageManager};
//! use clippier::workspace::Workspace;
//!
//! async fn analyze() {
//!     // Auto-detect package manager
//!     let ws = NodeWorkspace::detect(Path::new(".")).await.unwrap().unwrap();
//!     println!("Using {:?}", ws.package_manager());
//!     
//!     // List all packages
//!     for pkg in ws.packages().await.unwrap() {
//!         println!("  - {}", pkg.name());
//!     }
//! }
//! ```

mod context;
pub mod lockfile;
mod package;

pub use context::NodeWorkspace;
pub use lockfile::{NodeLockDiffParser, NodeLockEntry, NodeLockfile};
pub use package::{NodePackage, parse_dependencies, read_package_name, read_package_version};

/// Supported Node.js package managers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodePackageManager {
    /// npm - Node Package Manager
    ///
    /// Uses `package-lock.json` for lockfile and `package.json` `workspaces` field.
    Npm,

    /// pnpm - Performant npm
    ///
    /// Uses `pnpm-lock.yaml` for lockfile and `pnpm-workspace.yaml` for workspace config.
    Pnpm,

    /// bun - Fast all-in-one JavaScript runtime
    ///
    /// Uses `bun.lock` for lockfile and `package.json` `workspaces` field.
    Bun,
}

impl NodePackageManager {
    /// Returns the lockfile name for this package manager.
    #[must_use]
    pub const fn lockfile_name(self) -> &'static str {
        match self {
            Self::Npm => "package-lock.json",
            Self::Pnpm => "pnpm-lock.yaml",
            Self::Bun => "bun.lock",
        }
    }

    /// Returns the workspace config file name for this package manager.
    ///
    /// Note: npm and bun use `package.json`, while pnpm uses `pnpm-workspace.yaml`.
    #[must_use]
    pub const fn workspace_config_name(self) -> &'static str {
        match self {
            Self::Npm | Self::Bun => "package.json",
            Self::Pnpm => "pnpm-workspace.yaml",
        }
    }

    /// Returns the display name for this package manager.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Bun => "bun",
        }
    }
}

impl std::fmt::Display for NodePackageManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_manager_lockfile_names() {
        assert_eq!(NodePackageManager::Npm.lockfile_name(), "package-lock.json");
        assert_eq!(NodePackageManager::Pnpm.lockfile_name(), "pnpm-lock.yaml");
        assert_eq!(NodePackageManager::Bun.lockfile_name(), "bun.lock");
    }

    #[test]
    fn test_package_manager_workspace_config() {
        assert_eq!(
            NodePackageManager::Npm.workspace_config_name(),
            "package.json"
        );
        assert_eq!(
            NodePackageManager::Pnpm.workspace_config_name(),
            "pnpm-workspace.yaml"
        );
        assert_eq!(
            NodePackageManager::Bun.workspace_config_name(),
            "package.json"
        );
    }

    #[test]
    fn test_package_manager_display() {
        assert_eq!(NodePackageManager::Npm.to_string(), "npm");
        assert_eq!(NodePackageManager::Pnpm.to_string(), "pnpm");
        assert_eq!(NodePackageManager::Bun.to_string(), "bun");
    }
}
