//! Native, conservative unused-dependency analysis.
//!
//! Discovery, analysis and planning never invoke Cargo or access the network. A plan
//! owns its evidence; apply rechecks that evidence and preserves the lockfile.
mod apply;
mod discover;
mod metadata;
mod source;
pub use metadata::MacheteMetadata;
#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

pub use apply::apply_machete_plan;
pub use discover::{analyze_machete, plan_machete};

/// Error returned by discovery, planning and checked publication.
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Cargo dependency section, including target-specific occurrences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DependencyKind {
    /// Available to production and development targets.
    Normal,
    /// Available to tests, examples and benchmarks.
    Dev,
    /// Available to the build script.
    Build,
}
impl DependencyKind {
    const fn section(self) -> &'static str {
        match self {
            Self::Normal => "dependencies",
            Self::Dev => "dev-dependencies",
            Self::Build => "build-dependencies",
        }
    }
}

/// Read-only selectors. Empty lists select all; strings are glob patterns.
#[derive(Debug, Clone, Default)]
pub struct MacheteOptions {
    /// Cargo.toml or its directory; defaults to the current directory.
    pub manifest_path: Option<PathBuf>,
    /// Explicit clippier.toml layer; relative paths resolve from the current directory.
    pub config_path: Option<PathBuf>,
    /// Explicitly supplied Cargo metadata; never invokes Cargo in the engine.
    pub metadata: Option<MacheteMetadata>,
    /// Override ignore-file traversal; config directory exclusions still apply.
    pub no_ignore: Option<bool>,
    /// Package-name patterns.
    pub packages: Vec<String>,
    /// Dependency-key or actual package-name patterns.
    pub dependencies: Vec<String>,
    /// Package-name patterns to exclude.
    pub exclude: Vec<String>,
    /// Dependency kinds to inspect; empty defaults to NORMAL and BUILD.
    pub kinds: Vec<DependencyKind>,
}

/// Configuration read exclusively from the `machete` table in clippier.toml.
///
/// Layers accumulate ignored names/directories and rules; nonempty child selectors
/// replace parent selectors. Directory patterns are relative to their declaring file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct MacheteConfig {
    /// Disable .gitignore/.ignore filtering; None inherits the parent policy.
    pub no_ignore: Option<bool>,
    /// Dependency-key or package-name ignore patterns.
    pub ignored: Vec<String>,
    /// Declaring-directory-relative subtree paths or glob patterns.
    pub ignored_dirs: Vec<String>,
    /// Package-name selection patterns.
    pub packages: Vec<String>,
    /// Dependency-name selection patterns.
    pub dependencies: Vec<String>,
    /// Package-name exclusion patterns.
    pub exclude: Vec<String>,
    /// Selected dependency kinds; empty defaults to NORMAL and BUILD.
    pub kinds: Vec<DependencyKind>,
    /// Additional scoped ignore rules.
    pub rules: Vec<MacheteRule>,
}

/// A rule applies when both nonempty selectors match; it only adds ignores.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct MacheteRule {
    /// Package-name patterns (empty means all).
    pub packages: Vec<String>,
    /// Dependency-name patterns (empty means all).
    pub dependencies: Vec<String>,
    /// Additional ignored dependency patterns.
    pub ignored: Vec<String>,
    /// Additional declaring-directory-relative ignored subtrees.
    pub ignored_dirs: Vec<String>,
}

/// Conservative classification; only `Unused` occurrences may be removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DependencyStatus {
    /// A source, feature, or implicit tool reference exists.
    Used,
    /// No reference was found in the inspected source inventory.
    Unused,
    /// Explicitly ignored by configuration.
    Ignored,
    /// Excluded by a selector.
    Skipped,
    /// Source evidence cannot justify a removal.
    Incomplete,
}

/// An exact Cargo dependency declaration.
#[derive(Debug, Clone, Serialize)]
pub struct MacheteDependency {
    /// Owning package name.
    pub package: String,
    /// Absolute package manifest.
    pub manifest_path: PathBuf,
    /// Dependency key (possibly an alias).
    pub dependency: String,
    /// Actual dependency package name.
    pub dependency_package: String,
    /// Cargo dependency kind.
    pub kind: DependencyKind,
    /// Cargo target predicate, if any.
    pub target: Option<String>,
    /// Whether this occurrence inherits workspace ownership.
    pub workspace: bool,
    /// Classification.
    pub status: DependencyStatus,
    /// Human-readable evidence or exclusion explanation.
    pub reason: String,
}

/// Stable, serializable analysis/application result.
#[derive(Debug, Clone, Serialize)]
pub struct MacheteReport {
    /// Workspace or standalone package directory.
    pub root: PathBuf,
    /// Deterministically ordered declarations.
    pub dependencies: Vec<MacheteDependency>,
    /// Ordered diagnostics explaining incomplete evidence.
    pub diagnostics: Vec<String>,
    /// False for hard inventory failures or selected candidates with uncertain evidence.
    /// Deferred expansion alone is advisory when every declaration is used/ignored.
    pub complete: bool,
    /// True only after successful checked publication.
    pub applied: bool,
    /// This engine always preserves Cargo.lock without running resolution.
    pub lockfile_preserved: bool,
}

impl MacheteReport {
    /// Whether any selected declaration has no observed reference.
    #[must_use]
    pub fn has_unused(&self) -> bool {
        self.dependencies
            .iter()
            .any(|dependency| dependency.status == DependencyStatus::Unused)
    }
}

/// A formatting-preserving exact file replacement.
#[derive(Debug, Clone, Serialize)]
pub struct MacheteChange {
    /// Absolute manifest path.
    pub path: PathBuf,
    /// Expected original bytes (UTF-8).
    pub before: String,
    /// Exact planned bytes (UTF-8).
    pub after: String,
}

/// Immutable removal plan. Construct only through [`plan_machete`].
#[derive(Debug, Clone, Serialize)]
pub struct MachetePlan {
    /// Analysis that justifies these changes.
    pub report: MacheteReport,
    /// Exact replacements, sorted by path.
    pub changes: Vec<MacheteChange>,
    #[serde(skip)]
    pub(crate) snapshots: BTreeMap<PathBuf, Option<Vec<u8>>>,
    #[serde(skip)]
    pub(crate) directories: BTreeMap<PathBuf, Vec<PathBuf>>,
    #[serde(skip)]
    pub(crate) sealed_changes: Vec<(PathBuf, String, String)>,
    #[serde(skip)]
    pub(crate) complete: bool,
}
