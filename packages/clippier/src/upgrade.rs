//! Native, source-aware Cargo dependency upgrades.
//!
//! Planning reads manifests without invoking Cargo. Registry access is injectable through
//! [`plan_upgrade_with_releases`]. Confirmation belongs to the caller, not this module.

mod apply;
mod manifest;
mod planner;
pub mod registry;
#[cfg(test)]
mod tests;

use std::{collections::BTreeMap, path::PathBuf};

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

pub use apply::apply_upgrade_plan;
pub use manifest::load_upgrade_input;
pub use planner::plan_upgrade_with_releases;
pub use registry::{BoxError, RegistryOptions};

macro_rules! policy {
    ($name:ident, $default:ident, $default_alias:literal, {$($variant:ident => $alias:literal),+}) => {
        #[doc = concat!("Policy for native upgrades (", stringify!($name), ").")]
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
        #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
        pub enum $name {
            #[doc = "Default policy."]
            #[default]
            #[serde(alias = $default_alias)]
            $default,
            $(#[doc = "Explicit alternative policy."] #[serde(alias = $alias)] $variant,)+
        }
    };
}
policy!(UpgradePolicy, Compatible, "compatible", {Incompatible => "incompatible"});
policy!(PinnedPolicy, Preserve, "preserve", {Upgrade => "upgrade"});
policy!(PrereleasePolicy, Preserve, "preserve", {Allow => "allow"});
policy!(SourcePolicy, RegistryOnly, "registry-only", {IncludePath => "include-path"});
policy!(LockfilePolicy, Refresh, "refresh", {Resolve => "resolve", None => "none"});

/// Cargo dependency section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DependencyKind {
    /// `[dependencies]`.
    #[serde(alias = "normal")]
    Normal,
    /// `[dev-dependencies]`.
    #[serde(alias = "dev")]
    Dev,
    /// `[build-dependencies]`.
    #[serde(alias = "build")]
    Build,
}

/// CLI overrides. Empty lists defer to `[upgrade]` configuration; absent policies
/// defer to configuration and then their safe defaults.
#[derive(Debug, Clone, Default)]
pub struct UpgradeOptions {
    /// Workspace directory or a Cargo manifest (defaults to the current directory).
    pub manifest_path: Option<PathBuf>,
    /// Explicit clippier.toml; otherwise the workspace root's file is used.
    pub config_path: Option<PathBuf>,
    /// Dependency package names or manifest aliases to include (glob patterns).
    pub dependencies: Vec<String>,
    /// Workspace package names to include (glob patterns).
    pub packages: Vec<String>,
    /// Dependency names or aliases to exclude (glob patterns).
    pub exclude: Vec<String>,
    /// Dependency kinds to include; empty means all kinds.
    pub kinds: Vec<DependencyKind>,
    /// Compatibility override.
    pub policy: Option<UpgradePolicy>,
    /// Exact-pin override.
    pub pinned: Option<PinnedPolicy>,
    /// Prerelease override.
    pub prerelease: Option<PrereleasePolicy>,
    /// Source override.
    pub source: Option<SourcePolicy>,
    /// Lockfile update override.
    pub lockfile: Option<LockfilePolicy>,
    /// Explicit Rust compiler version ceiling for candidate releases.
    pub msrv: Option<String>,
    /// Update only the lockfile; do not change manifest requirements.
    pub lockfile_only: bool,
    /// Resolve the preview's lockfile in an isolated snapshot.
    pub resolve_lockfile: bool,
    /// Native registry transport options.
    pub registry: RegistryOptions,
    /// Only explicitly supplied CLI registry flags belong here.
    pub registry_overrides: RegistryOverrides,
}

/// Presence-tracked registry overrides; absent flags never erase file configuration.
#[derive(Debug, Clone, Default)]
pub struct RegistryOverrides {
    /// Override offline mode.
    pub offline: Option<bool>,
    /// Override request timeout.
    pub timeout_seconds: Option<u64>,
    /// Override retry count.
    pub max_retries: Option<u32>,
    /// Override request concurrency.
    pub max_concurrency: Option<usize>,
}

/// Typed `[upgrade]` configuration. CLI options take precedence.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct UpgradeConfig {
    /// Compatibility policy.
    pub policy: UpgradePolicy,
    /// Exact pins are preserved unless explicitly opted in.
    pub pinned: PinnedPolicy,
    /// Prereleases are not introduced unless explicitly opted in.
    pub prerelease: PrereleasePolicy,
    /// Git is always skipped; path requirements require opt-in.
    pub source: SourcePolicy,
    /// Refresh is the default, including when no manifest changes are needed.
    pub lockfile: LockfilePolicy,
    /// Dependency name/alias glob selectors.
    pub dependencies: Vec<String>,
    /// Workspace package glob selectors.
    pub packages: Vec<String>,
    /// Dependency name/alias exclusion globs.
    pub exclude: Vec<String>,
    /// Section filters.
    pub kinds: Vec<DependencyKind>,
    /// Rust compiler version ceiling; releases requiring newer Rust are skipped.
    pub msrv: Option<String>,
    /// Ordered scoped overrides; explicit CLI policies apply after these rules.
    pub rules: Vec<DependencyRule>,
    /// Native registry transport settings.
    pub registry: RegistryOptions,
}

/// Source-qualified per-dependency policies. Selectors restrict applicability;
/// they never broaden the global dependency/package/kind selection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct DependencyRule {
    /// Required actual registry package name glob (empty selectors are rejected).
    pub dependency: String,
    /// Optional manifest alias glob.
    pub alias: Option<String>,
    /// Optional consumer package name glob.
    pub package: Option<String>,
    /// Registry name; `crates-io` explicitly matches the default registry.
    pub registry: Option<String>,
    /// Optional target expression glob; use `*` to match target-specific declarations.
    pub target: Option<String>,
    /// Empty means all kinds.
    pub kinds: Vec<DependencyKind>,
    /// Scoped compatibility policy.
    pub policy: Option<UpgradePolicy>,
    /// Scoped pin policy.
    pub pinned: Option<PinnedPolicy>,
    /// Scoped prerelease policy.
    pub prerelease: Option<PrereleasePolicy>,
    /// Scoped source policy.
    pub source: Option<SourcePolicy>,
    /// Scoped Rust compiler ceiling.
    pub msrv: Option<String>,
}

/// A registry name and actual (unaliased) package name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RegistryKey {
    /// Real Cargo package name.
    pub name: String,
    /// Manifest registry name; `None` means Cargo's default registry.
    pub registry: Option<String>,
}

/// Exact file bytes protected by stale-plan checks, including absent files.
#[derive(Debug, Clone, Serialize)]
pub struct FileSnapshot {
    /// Absolute path.
    pub path: PathBuf,
    /// `None` records absence.
    pub contents: Option<String>,
}

/// Immutable, read-only input captured before registry queries.
#[derive(Debug, Clone)]
pub struct UpgradeInput {
    pub(crate) root: PathBuf,
    pub(crate) manifests: BTreeMap<PathBuf, String>,
    pub(crate) snapshots: Vec<FileSnapshot>,
    pub(crate) config: UpgradeConfig,
    pub(crate) lockfile_only: bool,
    pub(crate) resolve_lockfile: bool,
    pub(crate) offline: bool,
    pub(crate) registry: RegistryOptions,
    pub(crate) overrides: UpgradeOptions,
}

/// Outcome of a dependency or file operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UpgradeStatus {
    /// A manifest change is planned.
    Planned,
    /// Changes have been written successfully.
    Applied,
    /// No manifest change is necessary.
    Unchanged,
    /// Excluded by a policy or filter.
    Skipped,
    /// No eligible release exists.
    Unavailable,
    /// An operation failed; never used as a successful apply result.
    Failed,
    /// The caller declined confirmation.
    Cancelled,
}

/// A dependency occurrence, including workspace consumers affected by owner edits.
#[derive(Debug, Clone, Serialize)]
pub struct DependencyReport {
    /// Owning package, or `workspace` for an unused workspace declaration.
    pub package: String,
    /// Dependency alias in the manifest.
    pub alias: String,
    /// Actual registry package.
    pub name: String,
    /// Source declaration's manifest (workspace root for inherited dependencies).
    pub manifest: PathBuf,
    /// Consumer's manifest.
    pub consumer_manifest: PathBuf,
    /// Dependency kind.
    pub kind: DependencyKind,
    /// Target expression, if any.
    pub target: Option<String>,
    /// Existing requirement, if any.
    pub previous: Option<String>,
    /// Planned requirement (or unchanged previous requirement).
    pub requirement: Option<String>,
    /// Selected release.
    pub selected: Option<String>,
    /// Deterministic outcome.
    pub status: UpgradeStatus,
    /// Human-readable policy explanation.
    pub reason: String,
}

/// Lockfile plan status; unresolved does not claim an exact lockfile diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LockfileStatus {
    /// Disabled by policy.
    Disabled,
    /// Cargo resolution remains to be performed during apply.
    Unresolved,
    /// Exact preview was resolved in isolation.
    Resolved,
    /// Lockfile changed during apply.
    Applied,
    /// Lockfile bytes were unchanged.
    Unchanged,
    /// Lockfile resolution failed.
    Failed,
    /// Confirmation was declined.
    Cancelled,
}

/// Deterministic, serializable report, independent of terminal output.
#[derive(Debug, Clone, Serialize)]
#[allow(clippy::struct_excessive_bools)] // Independent wire-format facts: scope, pending work, cancellation and publication.
pub struct UpgradeReport {
    /// Report schema version.
    pub version: u32,
    /// Effective global configuration; scoped rules remain explicit in this value.
    pub config: UpgradeConfig,
    /// Manifest requirements are preserved in this mode.
    pub lockfile_only: bool,
    /// Overall operation status.
    pub status: UpgradeStatus,
    /// Includes unresolved lockfile work even when no manifests change.
    pub pending: bool,
    /// Explicit blockers; hard planning errors are returned as errors instead.
    pub blockers: Vec<String>,
    /// The caller declined confirmation.
    pub cancelled: bool,
    /// Canonical Cargo workspace root.
    pub workspace_root: PathBuf,
    /// Dependency occurrences in manifest/section/alias order.
    pub dependencies: Vec<DependencyReport>,
    /// Changed manifests.
    pub manifests: Vec<PathBuf>,
    /// Lockfile action.
    pub lockfile: LockfileStatus,
    /// Whether this is an applied report.
    pub applied: bool,
}

/// Exact manifest replacement, retaining original formatting outside changed values.
#[derive(Debug, Clone, Serialize)]
pub struct ManifestChange {
    /// Absolute manifest path.
    pub path: PathBuf,
    /// Original bytes.
    pub before: String,
    /// Proposed bytes.
    pub after: String,
}

/// Source- and requirement-qualified Cargo lock update.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(crate) struct LockTarget {
    pub(crate) key: RegistryKey,
    pub(crate) previous: String,
    pub(crate) selected: String,
}

/// Immutable plan. Protected fields cannot be reconstructed from untrusted JSON.
#[derive(Debug, Clone, Serialize)]
pub struct UpgradePlan {
    pub(crate) root: PathBuf,
    pub(crate) snapshots: Vec<FileSnapshot>,
    pub(crate) changes: Vec<ManifestChange>,
    pub(crate) report: UpgradeReport,
    pub(crate) lockfile_policy: LockfilePolicy,
    pub(crate) offline: bool,
    pub(crate) lock_targets: Vec<RegistryKey>,
    pub(crate) lock_versions: Vec<LockTarget>,
    pub(crate) protected_names: Vec<String>,
    pub(crate) resolved_lockfile: Option<String>,
}

impl UpgradePlan {
    /// Obtain the structured report without performing IO.
    #[must_use]
    pub const fn report(&self) -> &UpgradeReport {
        &self.report
    }

    /// Exact planned manifest replacements.
    #[must_use]
    pub fn changes(&self) -> &[ManifestChange] {
        &self.changes
    }

    /// Render a deterministic full-file unified diff. An unresolved lockfile is
    /// explicitly labeled rather than displayed as a fictitious exact diff.
    #[must_use]
    pub fn diff(&self) -> String {
        use std::fmt::Write;
        let mut result = String::new();
        for change in &self.changes {
            render_diff(&mut result, &change.path, &change.before, &change.after);
        }
        if let Some(after) = &self.resolved_lockfile {
            let path = self.root.join("Cargo.lock");
            let before = self
                .snapshots
                .iter()
                .find(|s| s.path == path)
                .and_then(|s| s.contents.as_deref())
                .unwrap_or_default();
            if before != after {
                render_diff(&mut result, &path, before, after);
            }
        } else if self.report.lockfile == LockfileStatus::Unresolved {
            let _ = writeln!(
                result,
                "# Cargo.lock: unresolved ({:?}); resolved during apply",
                self.lockfile_policy
            );
        }
        result
    }
}

fn render_diff(output: &mut String, path: &std::path::Path, before: &str, after: &str) {
    use std::fmt::Write;
    let _ = writeln!(
        output,
        "--- a/{}\n+++ b/{}\n@@ -1,{} +1,{} @@",
        path.display(),
        path.display(),
        before.lines().count(),
        after.lines().count()
    );
    for (prefix, text) in [('-', before), ('+', after)] {
        for line in text.lines() {
            let _ = writeln!(output, "{prefix}{line}");
        }
        if !text.is_empty() && !text.ends_with('\n') {
            let _ = writeln!(output, "\\ No newline at end of file");
        }
    }
}

/// Build a read-only native upgrade plan, querying only eligible registries.
///
/// # Errors
/// * Invalid manifests, configuration, selectors or version requirements.
/// * Registry transport/metadata failures.
/// * Unsupported or failed isolated lockfile resolution when explicitly requested.
pub async fn build_upgrade_plan(options: &UpgradeOptions) -> Result<UpgradePlan, BoxError> {
    let input = load_upgrade_input(options)?;
    let requests = planner::registry_requests(&input)?;
    let client = registry::RegistryClient::new(&input.root, input.registry.clone())?;
    let mut releases = BTreeMap::new();
    for key in requests {
        let versions = client.releases(&key.name, key.registry.as_deref()).await?;
        releases.insert(key, versions);
    }
    let mut plan = plan_upgrade_with_releases(&input, &releases)?;
    if input.resolve_lockfile {
        if plan.lockfile_policy == LockfilePolicy::None {
            return Err("resolve-lockfile cannot be combined with lockfile policy NONE".into());
        }
        plan.resolved_lockfile = Some(apply::resolve_lockfile(&plan)?);
        plan.report.lockfile = LockfileStatus::Resolved;
    }
    Ok(plan)
}
