//! Common types for workspace abstractions.
//!
//! This module defines types that are shared across different workspace implementations
//! (Cargo, Node.js) to provide a unified interface for dependency analysis.

#![allow(clippy::module_name_repetitions)]

use serde::{Deserialize, Serialize};

/// Normalized dependency kinds across ecosystems.
///
/// This enum represents the different categories of dependencies that exist
/// in both Cargo (Rust) and Node.js package managers. Some variants are
/// specific to one ecosystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DependencyKind {
    /// Regular runtime dependency.
    ///
    /// - Cargo: `[dependencies]`
    /// - Node: `dependencies`
    Normal,

    /// Development-only dependency.
    ///
    /// - Cargo: `[dev-dependencies]`
    /// - Node: `devDependencies`
    Dev,

    /// Build-time dependency.
    ///
    /// - Cargo: `[build-dependencies]`
    /// - Node: N/A (not a standard concept)
    Build,

    /// Peer dependency (Node.js only).
    ///
    /// - Cargo: N/A
    /// - Node: `peerDependencies`
    Peer,

    /// Optional dependency.
    ///
    /// - Cargo: dependency with `optional = true`
    /// - Node: `optionalDependencies`
    Optional,
}

impl DependencyKind {
    /// Returns `true` if this is a development dependency.
    #[must_use]
    pub const fn is_dev(&self) -> bool {
        matches!(self, Self::Dev)
    }

    /// Returns `true` if this is a normal (runtime) dependency.
    #[must_use]
    pub const fn is_normal(&self) -> bool {
        matches!(self, Self::Normal)
    }

    /// Returns `true` if this is an optional dependency.
    #[must_use]
    pub const fn is_optional(&self) -> bool {
        matches!(self, Self::Optional)
    }
}

/// An external dependency (from a registry or external source).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalDependency {
    /// Name of the dependency package.
    pub name: String,

    /// Kind of dependency (normal, dev, build, etc.).
    pub kind: DependencyKind,

    /// Whether this dependency is optional.
    ///
    /// Note: This is separate from `DependencyKind::Optional` because a dependency
    /// can be in the `[dependencies]` section but marked as `optional = true`.
    pub is_optional: bool,
}

impl ExternalDependency {
    /// Creates a new external dependency.
    #[must_use]
    pub fn new(name: impl Into<String>, kind: DependencyKind, is_optional: bool) -> Self {
        Self {
            name: name.into(),
            kind,
            is_optional,
        }
    }

    /// Creates a normal (runtime) dependency.
    #[must_use]
    pub fn normal(name: impl Into<String>) -> Self {
        Self::new(name, DependencyKind::Normal, false)
    }

    /// Creates a dev dependency.
    #[must_use]
    pub fn dev(name: impl Into<String>) -> Self {
        Self::new(name, DependencyKind::Dev, false)
    }

    /// Creates a build dependency.
    #[must_use]
    pub fn build(name: impl Into<String>) -> Self {
        Self::new(name, DependencyKind::Build, false)
    }
}

/// A workspace dependency (reference to another package in the workspace).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceDependency {
    /// Name of the workspace package being depended on.
    pub name: String,

    /// Kind of dependency (normal, dev, build, etc.).
    pub kind: DependencyKind,
}

impl WorkspaceDependency {
    /// Creates a new workspace dependency.
    #[must_use]
    pub fn new(name: impl Into<String>, kind: DependencyKind) -> Self {
        Self {
            name: name.into(),
            kind,
        }
    }

    /// Creates a normal (runtime) workspace dependency.
    #[must_use]
    pub fn normal(name: impl Into<String>) -> Self {
        Self::new(name, DependencyKind::Normal)
    }

    /// Creates a dev workspace dependency.
    #[must_use]
    pub fn dev(name: impl Into<String>) -> Self {
        Self::new(name, DependencyKind::Dev)
    }
}

/// Information about a package affected by changes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AffectedPackageInfo {
    /// Name of the affected package.
    pub name: String,

    /// Reasons why the package is affected (e.g., changed dependencies).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Vec<String>>,
}

impl AffectedPackageInfo {
    /// Creates a new affected package info with no reasoning.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            reasoning: None,
        }
    }

    /// Creates a new affected package info with reasoning.
    #[must_use]
    pub fn with_reasoning(name: impl Into<String>, reasoning: Vec<String>) -> Self {
        Self {
            name: name.into(),
            reasoning: Some(reasoning),
        }
    }
}
