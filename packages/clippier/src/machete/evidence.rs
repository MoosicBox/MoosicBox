//! Serializable, declaration-owned observations and analysis coverage.
use super::DependencyKind;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The origin of a dependency observation; feature wiring is not source usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvidenceKind {
    /// A qualified source path.
    Direct,
    /// A source import, including renamed imports.
    Import,
    /// A public source import.
    Reexport,
    /// An unexpanded macro token or attribute.
    Macro,
    /// Cargo feature forwarding or optional dependency activation.
    Feature,
    /// An implicit Cargo tool convention.
    Implicit,
    /// Explicitly captured generated Rust.
    Generated,
}

/// An observation resolved to a specific dependency declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyEvidence {
    /// Consumer manifest, identifying declaration ownership.
    pub manifest_path: PathBuf,
    /// Source file or manifest for non-source observations.
    pub file: PathBuf,
    /// One-based line; zero only when no source position is available.
    pub line: usize,
    /// One-based Unicode column; zero only when unavailable.
    pub column: usize,
    /// Observed syntax or manifest wiring.
    pub kind: EvidenceKind,
    /// Source target category; independent of the dependency declaration kind.
    pub source_target: DependencyKind,
    /// Cargo target category (`LIBRARY`, `BINARY`, `TEST`, `EXAMPLE`, `BENCH`, `BUILD_SCRIPT`,
    /// `MANIFEST`, or `UNRESOLVED` when ownership cannot be uniquely assigned).
    pub source_target_role: String,
    /// Cargo target name when identified; absent for shared/unresolved sources.
    pub source_target_name: Option<String>,
    /// Dependency declaration category.
    pub dependency_kind: DependencyKind,
    /// Target predicate attached to the declaration.
    pub target: Option<String>,
    /// Cargo dependency key, preserving aliases.
    pub alias: String,
    /// Actual dependency package name.
    pub resolved_package: String,
    /// Enclosing cfg predicates, not assumed globally exhaustive.
    pub conditions: Vec<String>,
    /// Why this observation is not fully resolved; empty means resolved syntax only.
    pub uncertainty: Vec<String>,
}

/// Coverage is deliberately distinct from a global compiler proof.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnalysisCoverage {
    /// Selected named profile, if conditional analysis was requested.
    pub profile: Option<String>,
    /// Whether structural lexical scope resolution was enabled.
    pub precision: bool,
    /// Finite profiles never establish global absence.
    pub profile_limited: bool,
    /// Exact unsupported constructs or unverified provenance encountered.
    pub gaps: Vec<String>,
}
