//! Workspace abstraction for monorepo package management.
//!
//! This module provides a unified interface for working with different workspace types:
//! - Cargo workspaces (Rust)
//! - Node.js workspaces (npm, pnpm, bun)
//!
//! # Architecture
//!
//! The workspace abstraction is built around several core traits:
//!
//! - [`Workspace`] - Main trait for workspace operations (package enumeration, lockfile access)
//! - [`Package`] - Represents a single package within a workspace
//! - [`Lockfile`] - Parsed lockfile with dependency information
//! - [`LockfileDiffParser`] - Parses git diffs of lockfiles to detect changed dependencies
//!
//! ## Detection
//!
//! Use [`detect_workspaces`] to detect all workspaces at a given path, optionally filtered
//! by workspace type. Use [`select_primary_workspace`] to select the highest-priority
//! workspace when only one is needed.
//!
//! ## Workspace Types
//!
//! The [`WorkspaceType`] enum identifies workspace types for filtering and CLI integration.
//! All workspaces implement the dyn-compatible [`Workspace`] trait, allowing for runtime
//! polymorphism via `Box<dyn Workspace>`.
//!
//! # Features
//!
//! - `cargo-workspace` - Support for Cargo/Rust workspaces (enabled by default)
//! - `node-workspace` - Support for Node.js workspaces (npm, pnpm, bun)
//!
//! # Example - Detecting Workspaces
//!
//! ```rust,ignore
//! use clippier::workspace::{detect_workspaces, select_primary_workspace, WorkspaceType};
//!
//! async fn analyze_workspace(root: &Path) -> Result<(), BoxError> {
//!     // Detect all workspaces (no filter)
//!     let workspaces = detect_workspaces(root, None).await?;
//!     
//!     // Or filter to specific types
//!     let cargo_only = detect_workspaces(root, Some(&[WorkspaceType::Cargo])).await?;
//!     
//!     // Get the primary (highest priority) workspace
//!     if let Some(workspace) = select_primary_workspace(workspaces) {
//!         let packages = workspace.package_name_to_path().await?;
//!         for (name, path) in packages {
//!             println!("Package: {} at {}", name, path);
//!         }
//!     }
//!     
//!     Ok(())
//! }
//! ```
//!
//! # Example - Lockfile Diff Parsing
//!
//! ```rust,ignore
//! use clippier::workspace::Workspace;
//!
//! fn analyze_lockfile_changes(
//!     workspace: &dyn Workspace,
//!     diff_lines: &[(char, String)],
//! ) -> Vec<String> {
//!     // Returns package names that changed in the lockfile
//!     workspace.parse_lockfile_diff(diff_lines)
//! }
//! ```

use std::path::Path;

pub mod analysis;
pub mod git;
pub mod glob;
pub mod traits;
pub mod types;

#[cfg(feature = "cargo-workspace")]
pub mod cargo;

#[cfg(feature = "node-workspace")]
pub mod node;

// Re-export commonly used items
pub use analysis::{
    build_dependency_map, find_affected_by_dependencies, find_packages_using_dependencies,
    find_transitive_dependents,
};
#[cfg(feature = "git-diff")]
pub use analysis::{get_affected_from_git, get_affected_with_transitive_analysis};
pub use glob::{contains_glob_chars, expand_workspace_globs, get_or_compile_glob};
pub use traits::{
    Lockfile, LockfileDiffParser, LockfileEntry, Package, Workspace, parse_dependency_name,
};
pub use types::{AffectedPackageInfo, DependencyKind, ExternalDependency, WorkspaceDependency};

#[cfg(feature = "cargo-workspace")]
pub use cargo::{CargoLockDiffParser, CargoLockfile, CargoPackage, CargoWorkspace};

#[cfg(feature = "node-workspace")]
pub use node::{NodeLockDiffParser, NodeLockfile, NodePackage, NodePackageManager, NodeWorkspace};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Workspace type identifier for filtering and CLI integration.
///
/// This enum is used for:
/// - Filtering which workspace types to detect
/// - CLI argument parsing via clap
/// - Priority ordering (lower ordinal = higher priority)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, clap::ValueEnum)]
pub enum WorkspaceType {
    /// Cargo (Rust) workspace - highest priority
    Cargo,
    /// Node.js workspace (npm, pnpm, bun)
    Node,
}

impl WorkspaceType {
    /// Returns the display name for this workspace type.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Cargo => "Cargo",
            Self::Node => "Node.js",
        }
    }
}

impl std::fmt::Display for WorkspaceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Detects all workspaces at the given root path, optionally filtered by type.
///
/// Workspaces are returned in priority order (Cargo first, then Node.js).
/// Use [`select_primary_workspace`] to get the highest-priority workspace.
///
/// # Arguments
///
/// * `root` - The potential workspace root directory
/// * `filter` - Optional filter to restrict which workspace types to detect.
///   If `None`, all workspace types are detected.
///
/// # Returns
///
/// A vector of detected workspaces, sorted by priority (highest first).
/// Returns an empty vector if no workspaces are found.
///
/// # Errors
///
/// Returns an error if there's an I/O error reading workspace configuration files.
///
/// # Example
///
/// ```rust,ignore
/// // Detect all workspace types
/// let all = detect_workspaces(root, None).await?;
///
/// // Detect only Cargo workspaces
/// let cargo_only = detect_workspaces(root, Some(&[WorkspaceType::Cargo])).await?;
///
/// // Detect Cargo or Node workspaces
/// let both = detect_workspaces(root, Some(&[WorkspaceType::Cargo, WorkspaceType::Node])).await?;
/// ```
pub async fn detect_workspaces(
    root: &Path,
    filter: Option<&[WorkspaceType]>,
) -> Result<Vec<Box<dyn Workspace>>, BoxError> {
    log::debug!("Detecting workspaces at: {}", root.display());

    let mut workspaces: Vec<Box<dyn Workspace>> = Vec::new();

    // Cargo workspace detection (highest priority)
    #[cfg(feature = "cargo-workspace")]
    if filter.is_none_or(|f| f.contains(&WorkspaceType::Cargo))
        && let Some(ws) = CargoWorkspace::detect(root).await?
    {
        log::info!("Detected Cargo workspace at: {}", root.display());
        workspaces.push(Box::new(ws));
    }

    // Node.js workspace detection
    #[cfg(feature = "node-workspace")]
    if filter.is_none_or(|f| f.contains(&WorkspaceType::Node))
        && let Some(ws) = NodeWorkspace::detect(root).await?
    {
        log::info!(
            "Detected Node.js workspace ({:?}) at: {}",
            ws.package_manager(),
            root.display()
        );
        workspaces.push(Box::new(ws));
    }

    log::debug!("Found {} workspace(s)", workspaces.len());
    Ok(workspaces)
}

/// Selects the highest-priority workspace from a list.
///
/// This is a convenience function for the common case where only one workspace
/// should be used. The first workspace in the list is returned, which corresponds
/// to the highest priority due to the detection order in [`detect_workspaces`].
///
/// # Arguments
///
/// * `workspaces` - A vector of detected workspaces
///
/// # Returns
///
/// The highest-priority workspace, or `None` if the list is empty.
///
/// # Example
///
/// ```rust,ignore
/// let workspaces = detect_workspaces(root, None).await?;
/// if let Some(workspace) = select_primary_workspace(workspaces) {
///     // Use the primary workspace
///     let packages = workspace.packages().await?;
/// }
/// ```
#[must_use]
pub fn select_primary_workspace(workspaces: Vec<Box<dyn Workspace>>) -> Option<Box<dyn Workspace>> {
    workspaces.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_type_display() {
        assert_eq!(WorkspaceType::Cargo.to_string(), "Cargo");
        assert_eq!(WorkspaceType::Node.to_string(), "Node.js");
    }

    #[test]
    fn test_workspace_type_ordering() {
        // Cargo should have higher priority (lower ordinal)
        assert!(WorkspaceType::Cargo < WorkspaceType::Node);
    }

    #[test]
    fn test_select_primary_workspace_empty() {
        let workspaces: Vec<Box<dyn Workspace>> = vec![];
        assert!(select_primary_workspace(workspaces).is_none());
    }
}
