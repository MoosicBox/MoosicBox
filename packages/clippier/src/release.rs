//! Stateless Cargo release preparation support.
//!
//! The release domain reconstructs its decisions from the workspace and registry state. It does
//! not persist release plans. This module currently provides the one-time migration needed to
//! make publishable workspace packages independently versionable.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use cargo_metadata::{Metadata, PackageId};
use flate2::read::GzDecoder;
use semver::Version;
use serde::{Deserialize, Serialize};
use tar::Archive;
use toml_edit::{DocumentMut, value};

use crate::{
    ColorMode, OutputType,
    cargo_workspace::{
        is_publishable, load_metadata as load_cargo_metadata, normalize_workspace_root,
        publishable_workspace_packages, workspace_packages,
    },
};

use super::publish::{PublishConfig, handle_publish_command};

mod semver_checks;

pub use semver_checks::{Compatibility, SemverAnalysis};

const CRATES_IO_API: &str = "https://crates.io/api/v1";

/// `cargo-semver-checks` feature selection policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SemverFeaturePolicy {
    /// Upstream deterministic default heuristic.
    #[default]
    DefaultHeuristic,
    /// Enable every feature, including unstable/private-looking names.
    All,
    /// Enable only default features plus explicitly configured features.
    DefaultOnly,
    /// Disable defaults and enable only explicitly configured features.
    ExplicitOnly,
}

impl SemverFeaturePolicy {
    #[must_use]
    const fn as_str(self) -> &'static str {
        match self {
            Self::DefaultHeuristic => "cargo-semver-checks-default",
            Self::All => "all-features",
            Self::DefaultOnly => "default-features",
            Self::ExplicitOnly => "only-explicit-features",
        }
    }
}

/// Configuration for reconstructing stateless Cargo release status.
#[derive(Debug, Clone)]
pub struct ReleaseStatusConfig {
    /// Path to the workspace root or its `Cargo.toml`.
    pub workspace_root: PathBuf,
    /// Optional release roots. All publishable packages are considered when omitted.
    pub packages: Option<Vec<String>>,
    /// Optional Git base revision used only to prioritize registry-backed candidates.
    #[cfg(feature = "git-diff")]
    pub git_base: Option<String>,
    /// Optional Git head revision paired with `git_base`.
    #[cfg(feature = "git-diff")]
    pub git_head: Option<String>,
    /// Optional feature-policy override; otherwise `clippier.toml` or the upstream default applies.
    pub semver_feature_policy: Option<SemverFeaturePolicy>,
    /// Features explicitly enabled for both current and baseline analyses.
    pub semver_features: Vec<String>,
    /// Features enabled only for the current package.
    pub semver_current_features: Vec<String>,
    /// Features enabled only for the registry baseline.
    pub semver_baseline_features: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ReleaseFileConfig {
    #[serde(default)]
    semver: ReleaseSemverFileConfig,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ReleaseSemverFileConfig {
    feature_policy: Option<SemverFeaturePolicy>,
    #[serde(default)]
    features: Vec<String>,
    #[serde(default)]
    current_features: Vec<String>,
    #[serde(default)]
    baseline_features: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ReleaseConfigDocument {
    #[serde(default)]
    release: ReleaseFileConfig,
}

#[derive(Debug, Clone)]
struct EffectiveSemverConfig {
    feature_policy: SemverFeaturePolicy,
    features: Vec<String>,
    current_features: Vec<String>,
    baseline_features: Vec<String>,
}

fn effective_semver_config(
    workspace_root: &Path,
    config: &ReleaseStatusConfig,
) -> Result<EffectiveSemverConfig, BoxError> {
    let file_config = workspace_root.join("clippier.toml");
    let file = if file_config.is_file() {
        toml::from_str::<ReleaseConfigDocument>(&fs::read_to_string(file_config)?)?
            .release
            .semver
    } else {
        ReleaseSemverFileConfig::default()
    };
    Ok(EffectiveSemverConfig {
        feature_policy: config
            .semver_feature_policy
            .or(file.feature_policy)
            .unwrap_or_default(),
        features: if config.semver_features.is_empty() {
            file.features
        } else {
            config.semver_features.clone()
        },
        current_features: if config.semver_current_features.is_empty() {
            file.current_features
        } else {
            config.semver_current_features.clone()
        },
        baseline_features: if config.semver_baseline_features.is_empty() {
            file.baseline_features
        } else {
            config.semver_baseline_features.clone()
        },
    })
}

/// Release eligibility/status category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReleaseEligibility {
    /// Package can be published to crates.io.
    Publishable,
    /// Package explicitly disables crates.io publication.
    PublishDisabled,
}

/// Reconstructed status of one workspace package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleasePackageStatus {
    /// Cargo package name.
    pub name: String,
    /// Whether this package is eligible for crates.io publication.
    pub eligibility: ReleaseEligibility,
    /// Current local package version.
    pub local_version: String,
    /// Latest stable published baseline, if one exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_version: Option<String>,
    /// Minimum target version when release preparation is required.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_version: Option<String>,
    /// Whether normalized packageable contents differ from the baseline.
    pub changed: bool,
    /// Whether this package has never been published.
    pub unpublished: bool,
    /// Compatibility result for changed library packages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<Compatibility>,
    /// Deterministically sorted explanations for this status.
    pub reasons: Vec<String>,
    /// Semver engine diagnostics, when analysis ran.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semver: Option<SemverAnalysis>,
}

/// Stateless release status for selected publishable packages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseStatusReport {
    /// Package status sorted by Cargo package name.
    pub packages: Vec<ReleasePackageStatus>,
    /// Internal dependency requirements that must change, sorted deterministically.
    pub dependency_changes: Vec<DependencyRequirementChange>,
    /// Pending packages in dependency-first publication order.
    pub publish_order: Vec<String>,
}

/// One internal dependency requirement update induced by a package release.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DependencyRequirementChange {
    /// Package whose effective manifest changes.
    pub consumer: String,
    /// Workspace package being depended on.
    pub dependency: String,
    /// Dependency key used by the consumer, which may be a rename.
    pub alias: String,
    /// Cargo dependency kind.
    pub kind: ReleaseDependencyKind,
    /// Target expression for target-specific dependencies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Whether the dependency is optional.
    pub optional: bool,
    /// Existing effective requirement.
    pub current_requirement: String,
    /// Proposed minimum requirement.
    pub proposed_requirement: String,
    /// Workspace-relative manifest that owns the requirement.
    pub owner_manifest: String,
    /// Whether the consumer inherits this requirement from `[workspace.dependencies]`.
    pub inherited: bool,
}

/// Cargo dependency kinds that participate in release planning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReleaseDependencyKind {
    /// Runtime dependency.
    Normal,
    /// Build-time dependency.
    Build,
}

impl ReleaseStatusReport {
    #[must_use]
    fn to_raw_string(&self) -> String {
        if self.packages.is_empty() {
            return "No selected publishable packages".to_string();
        }
        let mut lines = self
            .packages
            .iter()
            .map(|package| {
                let target = package.target_version.as_deref().map_or_else(
                    || "unchanged".to_string(),
                    |version| format!("-> {version}"),
                );
                format!(
                    "{} {} {}: {}",
                    package.name,
                    package.local_version,
                    target,
                    package.reasons.join("; ")
                )
            })
            .collect::<Vec<_>>();
        if !self.dependency_changes.is_empty() {
            lines.push("Dependency requirement changes:".to_string());
            lines.extend(self.dependency_changes.iter().map(|change| {
                format!(
                    "  {}: {} {} -> {}",
                    change.consumer,
                    change.dependency,
                    change.current_requirement,
                    change.proposed_requirement
                )
            }));
        }
        if !self.publish_order.is_empty() {
            lines.push(format!("Publish order: {}", self.publish_order.join(", ")));
        }
        lines.join("\n")
    }
}

/// Reconstruct release requirements from packageable workspace contents and crates.io.
///
/// This command does not persist a plan. Each invocation obtains one in-memory registry snapshot,
/// packages each selected crate, compares normalized archives, and semver-checks changed library
/// crates against their exact published baseline.
///
/// # Errors
///
/// * If workspace metadata or registry state cannot be loaded
/// * If package selection is invalid
/// * If `cargo package` or archive normalization fails
/// * If required semver analysis fails
pub async fn handle_release_status_command(
    config: &ReleaseStatusConfig,
    output: OutputType,
) -> Result<String, BoxError> {
    let config = config.clone();
    switchy_async::task::spawn_blocking(move || {
        let report = reconstruct_release_status(&config)?;
        match output {
            OutputType::Raw => Ok(report.to_raw_string()),
            OutputType::Json => Ok(serde_json::to_string_pretty(&report)?),
        }
    })
    .await?
}

#[allow(clippy::too_many_lines)]
fn reconstruct_release_status(
    config: &ReleaseStatusConfig,
) -> Result<ReleaseStatusReport, BoxError> {
    let workspace_root = fs::canonicalize(normalize_workspace_root(&config.workspace_root))?;
    let semver_config = effective_semver_config(&workspace_root, config)?;
    let metadata = load_cargo_metadata(&workspace_root, true)?;
    let all_packages = publishable_workspace_packages(&metadata);
    let mut root_names = selected_publishable_packages(&metadata, config.packages.as_deref())?
        .into_iter()
        .map(|package| package.name.to_string())
        .collect::<BTreeSet<_>>();
    #[cfg(feature = "git-diff")]
    let git_candidates = if config.packages.is_none()
        && let (Some(base), Some(head)) = (config.git_base.as_deref(), config.git_head.as_deref())
    {
        let changed_files =
            crate::git_diff::get_changed_files_from_git(&workspace_root, base, head)?;
        release_git_candidates(&workspace_root, &all_packages, &changed_files)
    } else {
        BTreeSet::new()
    };
    #[cfg(not(feature = "git-diff"))]
    let git_candidates = BTreeSet::new();
    let client = crates_io_client()?;
    let temporary = switchy_fs::tempdir()?;
    let mut registry_versions = BTreeMap::new();
    for package in all_packages.values() {
        registry_versions.insert(
            package.name.to_string(),
            latest_stable_version(&crate_versions(&client, package.name.as_str())?),
        );
    }
    if config.packages.is_some() {
        include_prepared_consumers(
            &all_packages,
            &registry_versions,
            &metadata,
            &mut root_names,
        )?;
    }
    let mut statuses = BTreeMap::new();

    let ordered_roots = git_candidates
        .intersection(&root_names)
        .cloned()
        .chain(root_names.difference(&git_candidates).cloned())
        .collect::<Vec<_>>();
    for name in &ordered_roots {
        let package = all_packages
            .get(name)
            .expect("selected roots were loaded from all publishable packages");
        let baseline = registry_versions
            .get(name)
            .expect("registry versions were loaded for all packages")
            .clone();
        let local_version = package.version.to_string();
        if baseline.as_deref().is_some_and(|baseline| {
            Version::parse(baseline).ok() > Version::parse(&local_version).ok()
        }) {
            return Err(format!(
                "Registry baseline for '{}' ({}) is newer than the local version ({local_version}); refresh the checkout before releasing",
                package.name,
                baseline.as_deref().expect("baseline checked above")
            )
            .into());
        }
        let package_archive =
            cargo_package_archive(&workspace_root, package.name.as_str(), temporary.path())?;
        let local_contents = normalized_archive_entries(&package_archive)?;
        let mut reasons = Vec::new();
        let (changed, unpublished, target_version, compatibility, semver) =
            if let Some(baseline_version) = baseline.as_deref() {
                let published_archive = download_crate(
                    &client,
                    package.name.as_str(),
                    baseline_version,
                    temporary.path(),
                )?;
                let published_contents = normalized_archive_entries(&published_archive)?;
                if local_contents == published_contents {
                    reasons.push(format!(
                        "packageable contents match crates.io {baseline_version}"
                    ));
                    (false, false, None, None, None)
                } else {
                    reasons.push(format!(
                        "packageable contents differ from crates.io {baseline_version}"
                    ));
                    if package.targets.iter().any(checkable_library_target) {
                        let analysis = semver_checks::analyze(
                            &workspace_root,
                            package.name.as_str(),
                            baseline_version,
                            &semver_config,
                        )?;
                        let minimum = target_version(baseline_version, analysis.compatibility)?;
                        let target =
                            sufficient_local_target(&local_version, baseline_version, &minimum)?;
                        reasons.push(format!(
                            "{} requires {}",
                            analysis.feature_policy,
                            compatibility_name(analysis.compatibility)
                        ));
                        (
                            true,
                            false,
                            Some(target),
                            Some(analysis.compatibility),
                            Some(analysis),
                        )
                    } else {
                        reasons.push(
                        "package has no checkable library target; using patch-compatible release"
                            .to_string(),
                    );
                        let minimum = target_version(baseline_version, Compatibility::Patch)?;
                        (
                            true,
                            false,
                            Some(sufficient_local_target(
                                &local_version,
                                baseline_version,
                                &minimum,
                            )?),
                            Some(Compatibility::Patch),
                            None,
                        )
                    }
                }
            } else {
                reasons.push("package has never been published".to_string());
                (true, true, Some(local_version.clone()), None, None)
            };

        statuses.insert(
            package.name.to_string(),
            ReleasePackageStatus {
                name: package.name.to_string(),
                eligibility: ReleaseEligibility::Publishable,
                local_version,
                baseline_version: baseline,
                target_version,
                changed,
                unpublished,
                compatibility,
                reasons,
                semver,
            },
        );
    }
    let graph = ReleaseGraph::load(&workspace_root, &metadata)?;
    include_pending_dependencies(&all_packages, &registry_versions, &graph, &mut statuses)?;
    let dependency_changes = propagate_release_graph(
        &workspace_root,
        &all_packages,
        &registry_versions,
        &graph,
        &semver_config,
        &mut statuses,
    )?;
    let pending = statuses
        .values()
        .filter(|status| status.changed)
        .map(|status| status.name.clone())
        .collect::<BTreeSet<_>>();
    let publish_order = graph.publish_order(&pending)?;
    let report = ReleaseStatusReport {
        packages: {
            let mut packages = statuses.into_values().collect::<Vec<_>>();
            if config.packages.is_none() {
                packages.extend(
                    workspace_packages(&metadata)
                        .into_values()
                        .filter(|package| !is_publishable(package))
                        .map(|package| ReleasePackageStatus {
                            name: package.name.to_string(),
                            eligibility: ReleaseEligibility::PublishDisabled,
                            local_version: package.version.to_string(),
                            baseline_version: None,
                            target_version: None,
                            changed: false,
                            unpublished: false,
                            compatibility: None,
                            reasons: vec!["package disables crates.io publication".to_string()],
                            semver: None,
                        }),
                );
            }
            packages.sort_by(|left, right| left.name.cmp(&right.name));
            packages
        },
        dependency_changes,
        publish_order,
    };
    Ok(report)
}

#[derive(Debug, Deserialize)]
struct CratesIoVersionsResponse {
    versions: Vec<CratesIoVersion>,
}

#[derive(Debug, Deserialize)]
struct CratesIoVersion {
    num: String,
    yanked: bool,
}

fn crates_io_client() -> Result<reqwest::blocking::Client, BoxError> {
    Ok(reqwest::blocking::Client::builder()
        .user_agent(format!("clippier/{}", env!("CARGO_PKG_VERSION")))
        .build()?)
}

fn crate_versions(
    client: &reqwest::blocking::Client,
    name: &str,
) -> Result<Vec<CratesIoVersion>, BoxError> {
    let response = client
        .get(format!("{CRATES_IO_API}/crates/{name}"))
        .send()?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    Ok(response
        .error_for_status()?
        .json::<CratesIoVersionsResponse>()?
        .versions)
}

fn latest_stable_version(versions: &[CratesIoVersion]) -> Option<String> {
    versions
        .iter()
        .filter(|version| !version.yanked)
        .filter_map(|version| Version::parse(&version.num).ok())
        .filter(|version| version.pre.is_empty())
        .max()
        .map(|version| version.to_string())
}

#[derive(Debug, Clone)]
struct ReleaseDependency {
    consumer: String,
    dependency: String,
    alias: String,
    kind: ReleaseDependencyKind,
    target: Option<String>,
    optional: bool,
    requirement: semver::VersionReq,
    owner_manifest: String,
    inherited: bool,
}

#[derive(Debug, Default)]
struct ReleaseGraph {
    dependencies: BTreeMap<String, Vec<ReleaseDependency>>,
    consumers: BTreeMap<String, Vec<ReleaseDependency>>,
}

impl ReleaseGraph {
    fn load(workspace_root: &Path, metadata: &Metadata) -> Result<Self, BoxError> {
        let packages = publishable_workspace_packages(metadata);
        let package_names = packages.keys().cloned().collect::<BTreeSet<_>>();
        let mut graph = Self::default();
        for package in packages.values() {
            let manifest_path = package.manifest_path.clone().into_std_path_buf();
            let manifest = fs::read_to_string(&manifest_path)?.parse::<DocumentMut>()?;
            for dependency in &package.dependencies {
                let kind = match dependency.kind {
                    cargo_metadata::DependencyKind::Normal => ReleaseDependencyKind::Normal,
                    cargo_metadata::DependencyKind::Build => ReleaseDependencyKind::Build,
                    _ => continue,
                };
                if !package_names.contains(dependency.name.as_str()) {
                    continue;
                }
                let alias = dependency
                    .rename
                    .clone()
                    .unwrap_or_else(|| dependency.name.clone());
                let inherited = dependency_is_inherited(
                    &manifest,
                    &alias,
                    kind,
                    dependency
                        .target
                        .as_ref()
                        .map(ToString::to_string)
                        .as_deref(),
                );
                let owner = if inherited {
                    workspace_root.join("Cargo.toml")
                } else {
                    manifest_path.clone()
                };
                let edge = ReleaseDependency {
                    consumer: package.name.to_string(),
                    dependency: dependency.name.clone(),
                    alias,
                    kind,
                    target: dependency.target.as_ref().map(ToString::to_string),
                    optional: dependency.optional,
                    requirement: dependency.req.clone(),
                    owner_manifest: owner
                        .strip_prefix(workspace_root)
                        .unwrap_or(&owner)
                        .to_string_lossy()
                        .replace('\\', "/"),
                    inherited,
                };
                graph
                    .dependencies
                    .entry(edge.consumer.clone())
                    .or_default()
                    .push(edge.clone());
                graph
                    .consumers
                    .entry(edge.dependency.clone())
                    .or_default()
                    .push(edge);
            }
        }
        for edges in graph.dependencies.values_mut() {
            edges.sort_by(release_dependency_cmp);
        }
        for edges in graph.consumers.values_mut() {
            edges.sort_by(release_dependency_cmp);
        }
        Ok(graph)
    }

    fn publish_order(&self, pending: &BTreeSet<String>) -> Result<Vec<String>, BoxError> {
        let mut temporary = BTreeSet::new();
        let mut permanent = BTreeSet::new();
        let mut order = Vec::new();
        for package in pending {
            self.visit(package, pending, &mut temporary, &mut permanent, &mut order)?;
        }
        Ok(order)
    }

    fn visit(
        &self,
        package: &str,
        pending: &BTreeSet<String>,
        temporary: &mut BTreeSet<String>,
        permanent: &mut BTreeSet<String>,
        order: &mut Vec<String>,
    ) -> Result<(), BoxError> {
        if permanent.contains(package) {
            return Ok(());
        }
        if !temporary.insert(package.to_string()) {
            return Err(format!(
                "Normal/build workspace dependency cycle detected while ordering '{package}'"
            )
            .into());
        }
        if let Some(dependencies) = self.dependencies.get(package) {
            for dependency in dependencies {
                if pending.contains(&dependency.dependency) {
                    self.visit(&dependency.dependency, pending, temporary, permanent, order)?;
                }
            }
        }
        temporary.remove(package);
        permanent.insert(package.to_string());
        order.push(package.to_string());
        Ok(())
    }
}

fn release_dependency_cmp(
    left: &ReleaseDependency,
    right: &ReleaseDependency,
) -> std::cmp::Ordering {
    (
        &left.consumer,
        &left.dependency,
        &left.alias,
        left.kind,
        &left.target,
        left.optional,
    )
        .cmp(&(
            &right.consumer,
            &right.dependency,
            &right.alias,
            right.kind,
            &right.target,
            right.optional,
        ))
}

fn dependency_is_inherited(
    manifest: &DocumentMut,
    alias: &str,
    kind: ReleaseDependencyKind,
    target: Option<&str>,
) -> bool {
    let section = match kind {
        ReleaseDependencyKind::Normal => "dependencies",
        ReleaseDependencyKind::Build => "build-dependencies",
    };
    let table = target.map_or_else(
        || {
            manifest
                .get(section)
                .and_then(toml_edit::Item::as_table_like)
        },
        |target| {
            manifest
                .get("target")
                .and_then(|targets| targets.get(target))
                .and_then(|target| target.get(section))
                .and_then(toml_edit::Item::as_table_like)
        },
    );
    table
        .and_then(|table| table.get(alias))
        .and_then(dependency_workspace_value)
        .unwrap_or(false)
}

fn dependency_workspace_value(item: &toml_edit::Item) -> Option<bool> {
    item.as_inline_table()
        .and_then(|table| table.get("workspace"))
        .and_then(toml_edit::Value::as_bool)
        .or_else(|| {
            item.as_table()
                .and_then(|table| table.get("workspace"))
                .and_then(toml_edit::Item::as_bool)
        })
}

fn include_pending_dependencies(
    packages: &BTreeMap<String, &cargo_metadata::Package>,
    registry_versions: &BTreeMap<String, Option<String>>,
    graph: &ReleaseGraph,
    statuses: &mut BTreeMap<String, ReleasePackageStatus>,
) -> Result<(), BoxError> {
    let mut stack = statuses.keys().cloned().collect::<Vec<_>>();
    while let Some(consumer) = stack.pop() {
        for edge in graph.dependencies.get(&consumer).into_iter().flatten() {
            if statuses.contains_key(&edge.dependency) {
                continue;
            }
            let package = packages
                .get(&edge.dependency)
                .ok_or_else(|| format!("Unknown publishable dependency '{}'", edge.dependency))?;
            let baseline = registry_versions.get(&edge.dependency).cloned().flatten();
            let local_version = package.version.to_string();
            let needs_publish = baseline.as_deref() != Some(local_version.as_str());
            if needs_publish {
                statuses.insert(
                    edge.dependency.clone(),
                    ReleasePackageStatus {
                        name: edge.dependency.clone(),
                        eligibility: ReleaseEligibility::Publishable,
                        local_version: local_version.clone(),
                        baseline_version: baseline.clone(),
                        target_version: Some(local_version),
                        changed: true,
                        unpublished: baseline.is_none(),
                        compatibility: None,
                        reasons: vec![format!(
                            "pending normal/build dependency of {}",
                            edge.consumer
                        )],
                        semver: None,
                    },
                );
                stack.push(edge.dependency.clone());
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn propagate_release_graph(
    workspace_root: &Path,
    packages: &BTreeMap<String, &cargo_metadata::Package>,
    registry_versions: &BTreeMap<String, Option<String>>,
    graph: &ReleaseGraph,
    semver_config: &EffectiveSemverConfig,
    statuses: &mut BTreeMap<String, ReleasePackageStatus>,
) -> Result<Vec<DependencyRequirementChange>, BoxError> {
    let mut changes = BTreeSet::new();
    let mut pending = statuses
        .values()
        .filter(|status| status.changed)
        .map(|status| status.name.clone())
        .collect::<BTreeSet<_>>();
    let mut processed_targets = BTreeMap::new();

    loop {
        let mut progressed = false;
        for dependency_name in pending.clone() {
            let Some(target) = statuses
                .get(&dependency_name)
                .and_then(|status| status.target_version.clone())
            else {
                continue;
            };
            if processed_targets.get(&dependency_name) == Some(&target) {
                continue;
            }
            processed_targets.insert(dependency_name.clone(), target.clone());
            let parsed_target = Version::parse(&target)?;
            for edge in graph.consumers.get(&dependency_name).into_iter().flatten() {
                if edge.requirement.matches(&parsed_target) {
                    continue;
                }
                let proposed_requirement = target.clone();
                changes.insert(DependencyRequirementChange {
                    consumer: edge.consumer.clone(),
                    dependency: edge.dependency.clone(),
                    alias: edge.alias.clone(),
                    kind: edge.kind,
                    target: edge.target.clone(),
                    optional: edge.optional,
                    current_requirement: edge.requirement.to_string(),
                    proposed_requirement,
                    owner_manifest: edge.owner_manifest.clone(),
                    inherited: edge.inherited,
                });
                if pending.insert(edge.consumer.clone()) {
                    let package = packages.get(&edge.consumer).ok_or_else(|| {
                        format!("Unknown publishable workspace consumer '{}'", edge.consumer)
                    })?;
                    let baseline = registry_versions.get(&edge.consumer).cloned().flatten();
                    let local_version = package.version.to_string();
                    let checkable = package.targets.iter().any(checkable_library_target);
                    let (target_version, unpublished, compatibility, semver, mut reasons) =
                        if let Some(baseline) = baseline.as_deref() {
                            let analysis = if checkable {
                                Some(semver_checks::analyze(
                                    workspace_root,
                                    package.name.as_str(),
                                    baseline,
                                    semver_config,
                                )?)
                            } else {
                                None
                            };
                            let compatibility = analysis
                                .as_ref()
                                .map_or(Compatibility::Patch, |analysis| analysis.compatibility);
                            let minimum = target_version(baseline, compatibility)?;
                            let mut reasons = vec![format!(
                                "dependency requirement for {} no longer accepts {}",
                                edge.dependency, target
                            )];
                            if let Some(analysis) = &analysis {
                                reasons.push(format!(
                                    "{} requires {}",
                                    analysis.feature_policy,
                                    compatibility_name(analysis.compatibility)
                                ));
                            } else {
                                reasons.push(
                                    "package has no checkable library target; using patch-compatible release"
                                        .to_string(),
                                );
                            }
                            (
                                sufficient_local_target(&local_version, baseline, &minimum)?,
                                false,
                                compatibility,
                                analysis,
                                reasons,
                            )
                        } else {
                            (
                                local_version.clone(),
                                true,
                                Compatibility::Patch,
                                None,
                                vec![format!(
                                    "unpublished consumer requires dependency {} {}",
                                    edge.dependency, target
                                )],
                            )
                        };
                    reasons.sort();
                    reasons.dedup();
                    statuses.insert(
                        edge.consumer.clone(),
                        ReleasePackageStatus {
                            name: edge.consumer.clone(),
                            eligibility: ReleaseEligibility::Publishable,
                            local_version,
                            baseline_version: baseline,
                            target_version: Some(target_version),
                            changed: true,
                            unpublished,
                            compatibility: Some(compatibility),
                            reasons,
                            semver,
                        },
                    );
                    progressed = true;
                }
            }
        }
        if !progressed {
            break;
        }
    }

    Ok(changes.into_iter().collect())
}

#[cfg(feature = "git-diff")]
fn release_git_candidates(
    workspace_root: &Path,
    packages: &BTreeMap<String, &cargo_metadata::Package>,
    changed_files: &[String],
) -> BTreeSet<String> {
    if changed_files
        .iter()
        .any(|path| matches!(path.as_str(), "Cargo.toml" | "Cargo.lock"))
    {
        return packages.keys().cloned().collect();
    }
    packages
        .iter()
        .filter(|(_name, package)| {
            let package_root = package.manifest_path.parent().map_or(
                workspace_root,
                cargo_metadata::camino::Utf8Path::as_std_path,
            );
            changed_files
                .iter()
                .any(|changed| workspace_root.join(changed).starts_with(package_root))
        })
        .map(|(name, _package)| name.clone())
        .collect()
}

fn include_prepared_consumers(
    packages: &BTreeMap<String, &cargo_metadata::Package>,
    registry_versions: &BTreeMap<String, Option<String>>,
    metadata: &Metadata,
    roots: &mut BTreeSet<String>,
) -> Result<(), BoxError> {
    let graph = ReleaseGraph::load(
        &metadata.workspace_root.clone().into_std_path_buf(),
        metadata,
    )?;
    let mut stack = roots.iter().cloned().collect::<Vec<_>>();
    while let Some(dependency) = stack.pop() {
        for edge in graph.consumers.get(&dependency).into_iter().flatten() {
            let local = packages
                .get(&edge.consumer)
                .ok_or_else(|| format!("Unknown consumer '{}'", edge.consumer))?
                .version
                .to_string();
            let baseline = registry_versions
                .get(&edge.consumer)
                .and_then(Option::as_ref);
            if baseline
                .is_none_or(|baseline| Version::parse(&local).ok() > Version::parse(baseline).ok())
                && roots.insert(edge.consumer.clone())
            {
                stack.push(edge.consumer.clone());
            }
        }
    }
    Ok(())
}

fn selected_publishable_packages<'a>(
    metadata: &'a Metadata,
    requested: Option<&[String]>,
) -> Result<Vec<&'a cargo_metadata::Package>, BoxError> {
    let members = metadata.workspace_members.iter().collect::<BTreeSet<_>>();
    let packages = metadata
        .packages
        .iter()
        .filter(|package| members.contains(&package.id))
        .map(|package| (package.name.to_string(), package))
        .collect::<BTreeMap<_, _>>();
    let mut selected = Vec::new();
    if let Some(requested) = requested {
        for name in requested {
            let package = packages
                .get(name)
                .ok_or_else(|| format!("Unknown workspace package '{name}'"))?;
            if !is_publishable(package) {
                return Err(format!("Package '{name}' is not publishable to crates.io").into());
            }
            selected.push(*package);
        }
    } else {
        selected.extend(
            packages
                .values()
                .filter(|package| is_publishable(package))
                .copied(),
        );
    }
    selected.sort_by(|left, right| left.name.cmp(&right.name));
    selected.dedup_by(|left, right| left.name == right.name);
    Ok(selected)
}

fn cargo_package_archive(
    workspace_root: &Path,
    package: &str,
    target_directory: &Path,
) -> Result<PathBuf, BoxError> {
    let output = Command::new("cargo")
        .arg("package")
        .arg("--package")
        .arg(package)
        .arg("--allow-dirty")
        .arg("--no-verify")
        .arg("--target-dir")
        .arg(target_directory)
        .current_dir(workspace_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "cargo package failed for '{package}': {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let package_directory = target_directory.join("package");
    let expected_name = format!(
        "{package}-{}.crate",
        package_version(workspace_root, package)?
    );
    let expected = package_directory.join(expected_name);
    if expected.is_file() {
        Ok(expected)
    } else {
        Err(format!("cargo package produced no archive for '{package}'").into())
    }
}

fn package_version(workspace_root: &Path, package: &str) -> Result<String, BoxError> {
    let metadata = load_cargo_metadata(workspace_root, true)?;
    metadata
        .packages
        .iter()
        .find(|candidate| candidate.name == package)
        .map(|candidate| candidate.version.to_string())
        .ok_or_else(|| format!("Unknown workspace package '{package}'").into())
}

fn download_crate(
    client: &reqwest::blocking::Client,
    name: &str,
    version: &str,
    directory: &Path,
) -> Result<PathBuf, BoxError> {
    let path = directory.join(format!("registry-{name}-{version}.crate"));
    if !path.exists() {
        let bytes = client
            .get(format!("{CRATES_IO_API}/crates/{name}/{version}/download"))
            .send()?
            .error_for_status()?
            .bytes()?;
        fs::write(&path, bytes)?;
    }
    Ok(path)
}

fn normalized_archive_entries(path: &Path) -> Result<BTreeMap<String, Vec<u8>>, BoxError> {
    let decoder = GzDecoder::new(fs::File::open(path)?);
    let mut archive = Archive::new(decoder);
    let mut entries = BTreeMap::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path()?.to_string_lossy().replace('\\', "/");
        let relative = path.split_once('/').map_or(path.as_str(), |(_, path)| path);
        if relative == ".cargo_vcs_info.json" || relative == "Cargo.toml.orig" {
            continue;
        }
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut bytes)?;
        if relative == "Cargo.toml" {
            bytes = normalize_generated_manifest(&bytes)?;
        }
        entries.insert(relative.to_string(), bytes);
    }
    Ok(entries)
}

fn normalize_generated_manifest(bytes: &[u8]) -> Result<Vec<u8>, BoxError> {
    let mut manifest = toml::from_str::<toml::Value>(&String::from_utf8(bytes.to_vec())?)?;
    if let Some(package) = manifest
        .get_mut("package")
        .and_then(toml::Value::as_table_mut)
    {
        package.remove("version");
    }
    Ok(serde_json::to_vec(&manifest)?)
}

fn checkable_library_target(target: &cargo_metadata::Target) -> bool {
    target.is_lib()
        || target.kind.iter().any(|kind| {
            matches!(
                kind,
                cargo_metadata::TargetKind::RLib
                    | cargo_metadata::TargetKind::DyLib
                    | cargo_metadata::TargetKind::CDyLib
                    | cargo_metadata::TargetKind::StaticLib
            )
        })
}

fn target_version(baseline: &str, compatibility: Compatibility) -> Result<String, BoxError> {
    let mut version = Version::parse(baseline)?;
    match compatibility {
        Compatibility::Patch => version.patch += 1,
        Compatibility::Feature => {
            if version.major == 0 {
                version.patch += 1;
            } else {
                version.minor += 1;
                version.patch = 0;
            }
        }
        Compatibility::Breaking => {
            if version.major == 0 {
                if version.minor == 0 {
                    version.patch += 1;
                } else {
                    version.minor += 1;
                    version.patch = 0;
                }
            } else {
                version.major += 1;
                version.minor = 0;
                version.patch = 0;
            }
        }
    }
    version.pre = semver::Prerelease::EMPTY;
    version.build = semver::BuildMetadata::EMPTY;
    Ok(version.to_string())
}

fn sufficient_local_target(local: &str, baseline: &str, minimum: &str) -> Result<String, BoxError> {
    let local = Version::parse(local)?;
    let baseline = Version::parse(baseline)?;
    let minimum = Version::parse(minimum)?;
    if local > baseline && local >= minimum {
        Ok(local.to_string())
    } else {
        Ok(minimum.to_string())
    }
}

const fn compatibility_name(compatibility: Compatibility) -> &'static str {
    match compatibility {
        Compatibility::Patch => "a patch-compatible release",
        Compatibility::Feature => "a feature release",
        Compatibility::Breaking => "an incompatible release",
    }
}

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Configuration for materializing a reconstructed release.
#[derive(Debug, Clone)]
pub struct ReleasePrepareConfig {
    /// Status selection used to reconstruct the release.
    pub status: ReleaseStatusConfig,
    /// Report edits without writing files.
    pub dry_run: bool,
}

/// Configuration for verified stateless publication.
#[derive(Debug, Clone)]
pub struct ReleasePublishConfig {
    /// Status selection used to reconstruct pending packages.
    pub status: ReleaseStatusConfig,
    /// Compute and report publication without uploading crates.
    pub dry_run: bool,
    /// Run Cargo's local package verification before upload.
    pub verify: bool,
    /// Permit unrelated dirty workspace files during publishing.
    pub allow_dirty: bool,
    /// Color mode for publication output.
    pub color: ColorMode,
    /// Maximum seconds to wait for a newly published crate to become available.
    pub publish_timeout_secs: u64,
    /// Seconds between crates.io availability checks.
    pub publish_poll_secs: u64,
    /// Retries after crates.io rate limiting.
    pub rate_limit_retries: u16,
}

/// Result of release preparation or verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseMutationReport {
    /// Whether this was a preview.
    pub dry_run: bool,
    /// Whether verification succeeded without requiring edits.
    pub verified: bool,
    /// Deterministically sorted files that require or received edits.
    pub files: Vec<String>,
    /// Reconstructed release status.
    pub status: ReleaseStatusReport,
}

/// Verify and publish the currently reconstructed pending release closure.
///
/// Verification and package selection are recomputed immediately before delegating to Clippier's
/// existing idempotent dependency-ordered publisher. No release plan is loaded or persisted.
///
/// # Errors
///
/// * If release requirements are not fully prepared
/// * If package construction fails
/// * If crates.io publication or availability checks fail
pub async fn handle_release_publish_command(
    config: ReleasePublishConfig,
    output: OutputType,
) -> Result<String, BoxError> {
    let status_config = config.status.clone();
    let (workspace_root, status) = switchy_async::task::spawn_blocking(move || {
        let workspace_root =
            fs::canonicalize(normalize_workspace_root(&status_config.workspace_root))?;
        let status = reconstruct_release_status(&status_config)?;
        let edits = build_release_edits(&workspace_root, &status)?;
        if !edits.is_empty() {
            return Err(format!(
                "Release is not prepared; run `clippier release prepare` first. Required edits: {}",
                edits
                    .iter()
                    .map(|edit| edit.relative_path.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .into());
        }
        validate_prepared_packages(&workspace_root, &status)?;
        Ok::<_, BoxError>((workspace_root, status))
    })
    .await??;
    if status.publish_order.is_empty() {
        return match output {
            OutputType::Json => Ok(serde_json::to_string_pretty(&serde_json::json!({
                "packages": []
            }))?),
            OutputType::Raw => Ok("No pending packages to publish".to_string()),
        };
    }
    handle_publish_command(
        PublishConfig {
            workspace_root,
            packages: Some(status.publish_order),
            dry_run: config.dry_run,
            verify: config.verify,
            allow_dirty: config.allow_dirty,
            color: config.color,
            publish_timeout: std::time::Duration::from_secs(config.publish_timeout_secs),
            publish_poll_interval: std::time::Duration::from_secs(config.publish_poll_secs),
            rate_limit_retries: config.rate_limit_retries,
        },
        output,
    )
    .await
}

/// Apply reconstructed package versions and dependency requirements without persisting a plan.
///
/// # Errors
///
/// * If release status reconstruction fails
/// * If a required manifest shape cannot be edited safely
/// * If Cargo lockfile refresh or post-write metadata validation fails
/// * If original files cannot be restored after a failed preparation
pub async fn handle_release_prepare_command(
    config: &ReleasePrepareConfig,
    output: OutputType,
) -> Result<String, BoxError> {
    let config = config.clone();
    switchy_async::task::spawn_blocking(move || {
        let workspace_root =
            fs::canonicalize(normalize_workspace_root(&config.status.workspace_root))?;
        let status = reconstruct_release_status(&config.status)?;
        let edits = build_release_edits(&workspace_root, &status)?;
        let files = edits
            .iter()
            .map(|edit| edit.relative_path.clone())
            .collect::<Vec<_>>();
        if !config.dry_run && !edits.is_empty() {
            apply_release_edits(&workspace_root, &edits)?;
        }
        let report = ReleaseMutationReport {
            dry_run: config.dry_run,
            verified: edits.is_empty(),
            files,
            status,
        };
        format_release_mutation_report(&report, output)
    })
    .await?
}

/// Verify that reconstructed release requirements are completely materialized.
///
/// # Errors
///
/// * If status reconstruction fails
/// * If any package version or dependency requirement remains insufficient
/// * If Cargo metadata or package construction fails
pub async fn handle_release_verify_command(
    config: &ReleaseStatusConfig,
    output: OutputType,
) -> Result<String, BoxError> {
    let config = config.clone();
    switchy_async::task::spawn_blocking(move || {
        let workspace_root = fs::canonicalize(normalize_workspace_root(&config.workspace_root))?;
        let status = reconstruct_release_status(&config)?;
        let edits = build_release_edits(&workspace_root, &status)?;
        if !edits.is_empty() {
            return Err(format!(
                "Release is not prepared; required edits: {}",
                edits
                    .iter()
                    .map(|edit| edit.relative_path.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .into());
        }
        validate_prepared_packages(&workspace_root, &status)?;
        format_release_mutation_report(
            &ReleaseMutationReport {
                dry_run: false,
                verified: true,
                files: Vec::new(),
                status,
            },
            output,
        )
    })
    .await?
}

fn format_release_mutation_report(
    report: &ReleaseMutationReport,
    output: OutputType,
) -> Result<String, BoxError> {
    match output {
        OutputType::Json => Ok(serde_json::to_string_pretty(report)?),
        OutputType::Raw => {
            let prefix = if report.verified {
                "Release is fully prepared"
            } else if report.dry_run {
                "Would update release files"
            } else {
                "Updated release files"
            };
            if report.files.is_empty() {
                Ok(prefix.to_string())
            } else {
                Ok(format!("{prefix}: {}", report.files.join(", ")))
            }
        }
    }
}

#[derive(Debug, Clone)]
struct FileEdit {
    path: PathBuf,
    relative_path: String,
    original: Option<Vec<u8>>,
    updated: Vec<u8>,
}

fn build_release_edits(
    workspace_root: &Path,
    status: &ReleaseStatusReport,
) -> Result<Vec<FileEdit>, BoxError> {
    let metadata = load_cargo_metadata(workspace_root, true)?;
    let packages = publishable_workspace_packages(&metadata);
    let mut documents = BTreeMap::<PathBuf, DocumentMut>::new();
    for package_status in &status.packages {
        let Some(target) = package_status.target_version.as_deref() else {
            continue;
        };
        if package_status.local_version == target {
            continue;
        }
        let package = packages
            .get(&package_status.name)
            .ok_or_else(|| format!("Unknown package '{}'", package_status.name))?;
        let path = package.manifest_path.clone().into_std_path_buf();
        let document = load_edit_document(&mut documents, &path)?;
        let package_table = document
            .get_mut("package")
            .and_then(toml_edit::Item::as_table_mut)
            .ok_or_else(|| format!("Manifest '{}' has no [package] table", path.display()))?;
        package_table["version"] = value(target);
    }
    for change in &status.dependency_changes {
        let path = workspace_root.join(&change.owner_manifest);
        let document = load_edit_document(&mut documents, &path)?;
        update_dependency_requirement(document, change)?;
    }
    documents
        .into_iter()
        .filter_map(|(path, document)| {
            let original = fs::read(&path);
            match original {
                Ok(original) => {
                    let updated = document.to_string().into_bytes();
                    if original == updated {
                        None
                    } else {
                        Some(Ok(FileEdit {
                            relative_path: relative_path(workspace_root, &path),
                            path,
                            original: Some(original),
                            updated,
                        }))
                    }
                }
                Err(error) => Some(Err(error.into())),
            }
        })
        .collect()
}

fn load_edit_document<'a>(
    documents: &'a mut BTreeMap<PathBuf, DocumentMut>,
    path: &Path,
) -> Result<&'a mut DocumentMut, BoxError> {
    if !documents.contains_key(path) {
        documents.insert(path.to_path_buf(), fs::read_to_string(path)?.parse()?);
    }
    Ok(documents.get_mut(path).expect("document inserted above"))
}

fn update_dependency_requirement(
    document: &mut DocumentMut,
    change: &DependencyRequirementChange,
) -> Result<(), BoxError> {
    let section = match change.kind {
        ReleaseDependencyKind::Normal => "dependencies",
        ReleaseDependencyKind::Build => "build-dependencies",
    };
    let table = if change.inherited {
        document
            .get_mut("workspace")
            .and_then(|workspace| workspace.get_mut("dependencies"))
            .and_then(toml_edit::Item::as_table_like_mut)
    } else if let Some(target) = change.target.as_deref() {
        document
            .get_mut("target")
            .and_then(|targets| targets.get_mut(target))
            .and_then(|target| target.get_mut(section))
            .and_then(toml_edit::Item::as_table_like_mut)
    } else {
        document
            .get_mut(section)
            .and_then(toml_edit::Item::as_table_like_mut)
    };
    let item = table
        .and_then(|table| table.get_mut(&change.alias))
        .ok_or_else(|| {
            format!(
                "Dependency '{}' is missing from '{}'",
                change.alias, change.owner_manifest
            )
        })?;
    if let Some(inline) = item.as_inline_table_mut() {
        inline.insert(
            "version",
            toml_edit::Value::from(&change.proposed_requirement),
        );
    } else if let Some(table) = item.as_table_mut() {
        table["version"] = value(&change.proposed_requirement);
    } else if item.is_value() {
        *item = value(&change.proposed_requirement);
    } else {
        return Err(format!(
            "Unsupported dependency syntax for '{}' in '{}'",
            change.alias, change.owner_manifest
        )
        .into());
    }
    Ok(())
}

fn apply_release_edits(workspace_root: &Path, edits: &[FileEdit]) -> Result<(), BoxError> {
    let lock_path = workspace_root.join("Cargo.lock");
    let lock_original = fs::read(&lock_path).ok();
    let mut written = Vec::new();
    for edit in edits {
        let current = fs::read(&edit.path).ok();
        if current.as_deref() != edit.original.as_deref() {
            restore_file_edits(&written, &lock_path, lock_original.as_deref())?;
            return Err(format!(
                "File '{}' changed after release reconstruction; rerun preparation",
                edit.path.display()
            )
            .into());
        }
        if let Err(error) = atomic_write(&edit.path, &edit.updated) {
            restore_file_edits(&written, &lock_path, lock_original.as_deref())?;
            return Err(error);
        }
        written.push(edit);
    }
    let validation = Command::new("cargo")
        .arg("metadata")
        .arg("--format-version")
        .arg("1")
        .current_dir(workspace_root)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output();
    let validation = match validation {
        Ok(validation) => validation,
        Err(error) => {
            restore_file_edits(&written, &lock_path, lock_original.as_deref())?;
            return Err(error.into());
        }
    };
    if !validation.status.success() {
        restore_file_edits(&written, &lock_path, lock_original.as_deref())?;
        return Err(format!(
            "Cargo metadata validation failed after release edits: {}",
            String::from_utf8_lossy(&validation.stderr)
        )
        .into());
    }
    Ok(())
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), BoxError> {
    let temporary = path.with_extension(format!(
        "{}.clippier-tmp",
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
    ));
    fs::write(&temporary, contents)?;
    fs::rename(&temporary, path)?;
    Ok(())
}

fn restore_file_edits(
    edits: &[&FileEdit],
    lock_path: &Path,
    lock_original: Option<&[u8]>,
) -> Result<(), BoxError> {
    let mut failures = Vec::new();
    for edit in edits.iter().rev() {
        let result = edit.original.as_deref().map_or_else(
            || fs::remove_file(&edit.path),
            |original| fs::write(&edit.path, original),
        );
        if let Err(error) = result {
            failures.push(format!("{}: {error}", edit.path.display()));
        }
    }
    if let Some(original) = lock_original
        && let Err(error) = fs::write(lock_path, original)
    {
        failures.push(format!("{}: {error}", lock_path.display()));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("Failed to restore release files: {}", failures.join(", ")).into())
    }
}

fn validate_prepared_packages(
    workspace_root: &Path,
    status: &ReleaseStatusReport,
) -> Result<(), BoxError> {
    for package in &status.publish_order {
        let output = Command::new("cargo")
            .arg("package")
            .arg("--package")
            .arg(package)
            .arg("--allow-dirty")
            .arg("--no-verify")
            .arg("--list")
            .current_dir(workspace_root)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()?;
        if !output.status.success() {
            return Err(format!(
                "Package construction failed for '{package}': {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
    }
    Ok(())
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Configuration for converting inherited package versions to explicit versions.
#[derive(Debug, Clone)]
pub struct IndependentizeConfig {
    /// Path to the workspace root or its `Cargo.toml`.
    pub workspace_root: PathBuf,
    /// Optional publishable package names to migrate. All publishable packages are selected when
    /// omitted.
    pub packages: Option<Vec<String>>,
    /// Report changes without writing manifests.
    pub dry_run: bool,
}

/// One package whose inherited version is or was made explicit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndependentizedPackage {
    /// Cargo package name.
    pub name: String,
    /// Effective package version preserved by the migration.
    pub version: String,
    /// Workspace-relative manifest path.
    pub manifest_path: String,
}

/// Result of an independent-version migration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndependentizeReport {
    /// Whether the command only previewed changes.
    pub dry_run: bool,
    /// Packages whose manifests required migration.
    pub packages: Vec<IndependentizedPackage>,
}

impl IndependentizeReport {
    #[must_use]
    fn to_raw_string(&self) -> String {
        if self.packages.is_empty() {
            return "All selected publishable packages already have explicit versions".to_string();
        }

        let action = if self.dry_run { "Would make" } else { "Made" };
        let mut lines = vec![format!(
            "{action} {} publishable package(s) independently versioned:",
            self.packages.len()
        )];
        lines.extend(self.packages.iter().map(|package| {
            format!(
                "  {} {} ({})",
                package.name, package.version, package.manifest_path
            )
        }));
        lines.join("\n")
    }
}

#[derive(Debug, Clone)]
struct Migration {
    package: IndependentizedPackage,
    path: PathBuf,
    original: String,
    updated: String,
}

/// Materialize effective versions for publishable packages that inherit
/// `[workspace.package].version`.
///
/// The numeric package versions do not change. The operation validates that Cargo's effective
/// workspace package metadata is identical before and after the edits. If validation fails, every
/// edited manifest is restored before the error is returned.
///
/// # Errors
///
/// * If Cargo workspace metadata cannot be loaded
/// * If an explicitly requested package is missing or not publishable to crates.io
/// * If a package manifest cannot be parsed or written
/// * If effective Cargo package metadata changes during migration
/// * If a failed migration cannot restore an original manifest
pub fn handle_independentize_command(
    config: &IndependentizeConfig,
    output: OutputType,
) -> Result<String, BoxError> {
    let workspace_root = fs::canonicalize(normalize_workspace_root(&config.workspace_root))?;
    let before = load_cargo_metadata(&workspace_root, true)?;
    let before_snapshot = workspace_package_snapshot(&before)?;
    let migrations = build_migrations(&workspace_root, &before, config.packages.as_deref())?;

    let report = IndependentizeReport {
        dry_run: config.dry_run,
        packages: migrations
            .iter()
            .map(|migration| migration.package.clone())
            .collect(),
    };

    if !config.dry_run && !migrations.is_empty() {
        apply_migrations(&workspace_root, &migrations, &before_snapshot)?;
    }

    match output {
        OutputType::Raw => Ok(report.to_raw_string()),
        OutputType::Json => Ok(serde_json::to_string_pretty(&report)?),
    }
}

fn workspace_package_snapshot(
    metadata: &Metadata,
) -> Result<BTreeMap<String, serde_json::Value>, BoxError> {
    let members = metadata
        .workspace_members
        .iter()
        .cloned()
        .collect::<BTreeSet<PackageId>>();

    metadata
        .packages
        .iter()
        .filter(|package| members.contains(&package.id))
        .map(|package| Ok((package.name.to_string(), serde_json::to_value(package)?)))
        .collect()
}

fn build_migrations(
    workspace_root: &Path,
    metadata: &Metadata,
    requested: Option<&[String]>,
) -> Result<Vec<Migration>, BoxError> {
    let members = metadata
        .workspace_members
        .iter()
        .cloned()
        .collect::<BTreeSet<PackageId>>();
    let packages = metadata
        .packages
        .iter()
        .filter(|package| members.contains(&package.id))
        .map(|package| (package.name.to_string(), package))
        .collect::<BTreeMap<_, _>>();

    let selected = if let Some(requested) = requested {
        let mut selected = BTreeSet::new();
        for name in requested {
            let package = packages
                .get(name)
                .ok_or_else(|| format!("Unknown workspace package '{name}'"))?;
            if !is_publishable(package) {
                return Err(format!("Package '{name}' is not publishable to crates.io").into());
            }
            selected.insert(name.clone());
        }
        selected
    } else {
        packages
            .iter()
            .filter(|(_name, package)| is_publishable(package))
            .map(|(name, _package)| name.clone())
            .collect()
    };

    let mut migrations = Vec::new();
    for name in selected {
        let package = packages
            .get(&name)
            .expect("selected packages were validated above");
        let path = package.manifest_path.clone().into_std_path_buf();
        let original = fs::read_to_string(&path)?;
        let mut document = original.parse::<DocumentMut>()?;
        let package_table = document
            .get_mut("package")
            .and_then(toml_edit::Item::as_table_mut)
            .ok_or_else(|| format!("Manifest '{}' has no [package] table", path.display()))?;
        let inherits = package_table
            .get("version")
            .and_then(toml_edit::Item::as_inline_table)
            .and_then(|version| version.get("workspace"))
            .and_then(toml_edit::Value::as_bool)
            .unwrap_or(false);

        if !inherits {
            continue;
        }

        package_table["version"] = value(package.version.to_string());
        let updated = document.to_string();
        let manifest_path = path
            .strip_prefix(workspace_root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        migrations.push(Migration {
            package: IndependentizedPackage {
                name,
                version: package.version.to_string(),
                manifest_path,
            },
            path,
            original,
            updated,
        });
    }

    Ok(migrations)
}

fn apply_migrations(
    workspace_root: &Path,
    migrations: &[Migration],
    before_snapshot: &BTreeMap<String, serde_json::Value>,
) -> Result<(), BoxError> {
    let mut written = Vec::new();
    for migration in migrations {
        if let Err(error) = fs::write(&migration.path, &migration.updated) {
            restore_migrations(&written)?;
            return Err(error.into());
        }
        written.push(migration);
    }

    let validation = load_cargo_metadata(workspace_root, true)
        .and_then(|metadata| workspace_package_snapshot(&metadata))
        .and_then(|after_snapshot| {
            if &after_snapshot == before_snapshot {
                Ok(())
            } else {
                Err("Independent-version migration changed effective Cargo package metadata".into())
            }
        });

    if let Err(error) = validation {
        restore_migrations(&written)?;
        return Err(error);
    }

    Ok(())
}

fn restore_migrations(migrations: &[&Migration]) -> Result<(), BoxError> {
    let mut failures = Vec::new();
    for migration in migrations.iter().rev() {
        if let Err(error) = fs::write(&migration.path, &migration.original) {
            failures.push(format!("{}: {error}", migration.path.display()));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Failed to restore manifests after migration failure: {}",
            failures.join(", ")
        )
        .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_graph_models_aliases_and_orders_dependencies() {
        let directory = switchy_fs::tempdir().expect("temp graph workspace");
        for package in ["leaf", "middle", "root"] {
            fs::create_dir_all(directory.path().join(package).join("src")).expect("package dir");
            fs::write(directory.path().join(package).join("src/lib.rs"), "")
                .expect("package source");
        }
        fs::write(
            directory.path().join("Cargo.toml"),
            r#"[workspace]
members = ["leaf", "middle", "root"]
resolver = "2"

[workspace.dependencies]
renamed_leaf = { version = "0.4.0", path = "leaf", package = "leaf" }
middle = { version = "0.4.0", path = "middle" }
"#,
        )
        .expect("workspace manifest");
        fs::write(
            directory.path().join("leaf/Cargo.toml"),
            "[package]\nname = \"leaf\"\nversion = \"0.4.0\"\nedition = \"2024\"\n",
        )
        .expect("leaf manifest");
        fs::write(
            directory.path().join("middle/Cargo.toml"),
            "[package]\nname = \"middle\"\nversion = \"0.4.0\"\nedition = \"2024\"\n\n[dependencies]\nrenamed_leaf = { workspace = true, package = \"leaf\" }\n",
        )
        .expect("middle manifest");
        fs::write(
            directory.path().join("root/Cargo.toml"),
            "[package]\nname = \"root\"\nversion = \"0.4.0\"\nedition = \"2024\"\n\n[build-dependencies]\nmiddle = { workspace = true }\n",
        )
        .expect("root manifest");

        let metadata = load_cargo_metadata(directory.path(), true).expect("metadata");
        let graph = ReleaseGraph::load(directory.path(), &metadata).expect("graph");
        let middle_edge = &graph.dependencies["middle"][0];
        assert_eq!(middle_edge.alias, "renamed_leaf");
        assert!(middle_edge.inherited);
        assert_eq!(middle_edge.owner_manifest, "Cargo.toml");
        assert_eq!(middle_edge.kind, ReleaseDependencyKind::Normal);
        assert_eq!(
            graph
                .publish_order(&BTreeSet::from([
                    "leaf".to_string(),
                    "middle".to_string(),
                    "root".to_string(),
                ]))
                .expect("publish order"),
            vec!["leaf", "middle", "root"]
        );
    }

    #[test]
    fn release_graph_rejects_normal_dependency_cycles() {
        let edge = |consumer: &str, dependency: &str| ReleaseDependency {
            consumer: consumer.to_string(),
            dependency: dependency.to_string(),
            alias: dependency.to_string(),
            kind: ReleaseDependencyKind::Normal,
            target: None,
            optional: false,
            requirement: semver::VersionReq::STAR,
            owner_manifest: format!("{consumer}/Cargo.toml"),
            inherited: false,
        };
        let graph = ReleaseGraph {
            dependencies: BTreeMap::from([
                ("a".to_string(), vec![edge("a", "b")]),
                ("b".to_string(), vec![edge("b", "a")]),
            ]),
            consumers: BTreeMap::new(),
        };
        let error = graph
            .publish_order(&BTreeSet::from(["a".to_string(), "b".to_string()]))
            .expect_err("cycle should fail");
        assert!(error.to_string().contains("cycle detected"));
    }

    fn create_workspace() -> switchy_fs::TempDir {
        let directory = switchy_fs::tempdir().expect("temp workspace");
        fs::create_dir_all(directory.path().join("publishable/src")).expect("publishable dir");
        fs::create_dir_all(directory.path().join("private/src")).expect("private dir");
        fs::write(
            directory.path().join("Cargo.toml"),
            r#"[workspace]
members = ["publishable", "private"]
resolver = "2"

[workspace.package]
version = "0.4.0"
edition = "2024"
"#,
        )
        .expect("workspace manifest");
        fs::write(
            directory.path().join("publishable/Cargo.toml"),
            r#"[package]
name = "publishable"
version = { workspace = true }
edition = { workspace = true }
"#,
        )
        .expect("publishable manifest");
        fs::write(
            directory.path().join("private/Cargo.toml"),
            r#"[package]
name = "private"
version = { workspace = true }
edition = { workspace = true }
publish = false
"#,
        )
        .expect("private manifest");
        fs::write(directory.path().join("publishable/src/lib.rs"), "").expect("publishable lib");
        fs::write(directory.path().join("private/src/lib.rs"), "").expect("private lib");
        directory
    }

    #[test]
    fn independentize_is_dry_runnable_and_idempotent() {
        let workspace = create_workspace();
        let publishable_manifest = workspace.path().join("publishable/Cargo.toml");
        let original = fs::read_to_string(&publishable_manifest).expect("read original");
        let config = IndependentizeConfig {
            workspace_root: workspace.path().to_path_buf(),
            packages: None,
            dry_run: true,
        };

        let preview = handle_independentize_command(&config, OutputType::Json).expect("preview");
        let report: IndependentizeReport = serde_json::from_str(&preview).expect("preview report");
        assert_eq!(report.packages.len(), 1);
        assert_eq!(report.packages[0].name, "publishable");
        assert_eq!(
            fs::read_to_string(&publishable_manifest).expect("read after preview"),
            original
        );

        let config = IndependentizeConfig {
            dry_run: false,
            ..config
        };
        handle_independentize_command(&config, OutputType::Raw).expect("apply");
        let updated = fs::read_to_string(&publishable_manifest).expect("read updated");
        assert!(updated.contains("version = \"0.4.0\""));
        assert!(
            fs::read_to_string(workspace.path().join("private/Cargo.toml"))
                .expect("read private")
                .contains("version = { workspace = true }")
        );

        let second =
            handle_independentize_command(&config, OutputType::Json).expect("second apply");
        let report: IndependentizeReport = serde_json::from_str(&second).expect("second report");
        assert!(report.packages.is_empty());
    }

    #[test]
    fn structured_release_edits_update_package_and_inherited_dependency() {
        let directory = switchy_fs::tempdir().expect("temp release workspace");
        fs::create_dir_all(directory.path().join("leaf/src")).expect("leaf dir");
        fs::create_dir_all(directory.path().join("consumer/src")).expect("consumer dir");
        fs::write(directory.path().join("leaf/src/lib.rs"), "").expect("leaf source");
        fs::write(directory.path().join("consumer/src/lib.rs"), "").expect("consumer source");
        fs::write(
            directory.path().join("Cargo.toml"),
            r#"[workspace]
members = ["leaf", "consumer"]
resolver = "2"

[workspace.dependencies]
leaf_alias = { version = "0.4.0", path = "leaf", package = "leaf" }
"#,
        )
        .expect("workspace manifest");
        fs::write(
            directory.path().join("leaf/Cargo.toml"),
            "[package]\nname = \"leaf\"\nversion = \"0.4.0\"\nedition = \"2024\"\n",
        )
        .expect("leaf manifest");
        fs::write(
            directory.path().join("consumer/Cargo.toml"),
            "[package]\nname = \"consumer\"\nversion = \"0.4.0\"\nedition = \"2024\"\n\n[dependencies]\nleaf_alias = { workspace = true }\n",
        )
        .expect("consumer manifest");
        let status = ReleaseStatusReport {
            packages: vec![
                ReleasePackageStatus {
                    name: "consumer".to_string(),
                    eligibility: ReleaseEligibility::Publishable,
                    local_version: "0.4.0".to_string(),
                    baseline_version: Some("0.4.0".to_string()),
                    target_version: Some("0.4.1".to_string()),
                    changed: true,
                    unpublished: false,
                    compatibility: Some(Compatibility::Patch),
                    reasons: Vec::new(),
                    semver: None,
                },
                ReleasePackageStatus {
                    name: "leaf".to_string(),
                    eligibility: ReleaseEligibility::Publishable,
                    local_version: "0.4.0".to_string(),
                    baseline_version: Some("0.4.0".to_string()),
                    target_version: Some("0.5.0".to_string()),
                    changed: true,
                    unpublished: false,
                    compatibility: Some(Compatibility::Breaking),
                    reasons: Vec::new(),
                    semver: None,
                },
            ],
            dependency_changes: vec![DependencyRequirementChange {
                consumer: "consumer".to_string(),
                dependency: "leaf".to_string(),
                alias: "leaf_alias".to_string(),
                kind: ReleaseDependencyKind::Normal,
                target: None,
                optional: false,
                current_requirement: "^0.4.0".to_string(),
                proposed_requirement: "0.5.0".to_string(),
                owner_manifest: "Cargo.toml".to_string(),
                inherited: true,
            }],
            publish_order: vec!["leaf".to_string(), "consumer".to_string()],
        };

        let edits = build_release_edits(directory.path(), &status).expect("build edits");
        assert_eq!(edits.len(), 3);
        apply_release_edits(directory.path(), &edits).expect("apply edits");
        assert!(
            fs::read_to_string(directory.path().join("leaf/Cargo.toml"))
                .expect("leaf manifest")
                .contains("version = \"0.5.0\"")
        );
        assert!(
            fs::read_to_string(directory.path().join("consumer/Cargo.toml"))
                .expect("consumer manifest")
                .contains("version = \"0.4.1\"")
        );
        assert!(
            fs::read_to_string(directory.path().join("Cargo.toml"))
                .expect("workspace manifest")
                .contains("version = \"0.5.0\"")
        );
        assert!(
            build_release_edits(directory.path(), &status)
                .expect("idempotent edits")
                .is_empty()
        );
        assert!(directory.path().join("Cargo.lock").exists());
    }

    #[test]
    fn generated_manifest_normalization_ignores_version_and_formatting() {
        let compact = br#"[package]
name="demo"
version="1.0.0"
edition="2024"

[dependencies]
serde="1"
"#;
        let formatted = br#"[package]
name    = "demo"
version = "2.0.0"
edition = "2024"

[dependencies]
serde = "1"
"#;
        assert_eq!(
            normalize_generated_manifest(compact).unwrap(),
            normalize_generated_manifest(formatted).unwrap()
        );
    }

    #[test]
    fn cargo_compatible_target_versions_are_minimal() {
        assert_eq!(
            target_version("1.2.3", Compatibility::Patch).unwrap(),
            "1.2.4"
        );
        assert_eq!(
            target_version("1.2.3", Compatibility::Feature).unwrap(),
            "1.3.0"
        );
        assert_eq!(
            target_version("1.2.3", Compatibility::Breaking).unwrap(),
            "2.0.0"
        );
        assert_eq!(
            target_version("0.4.3", Compatibility::Feature).unwrap(),
            "0.4.4"
        );
        assert_eq!(
            target_version("0.4.3", Compatibility::Breaking).unwrap(),
            "0.5.0"
        );
        assert_eq!(
            target_version("0.0.3", Compatibility::Breaking).unwrap(),
            "0.0.4"
        );
    }

    #[test]
    fn sufficient_local_version_makes_preparation_idempotent() {
        assert_eq!(
            sufficient_local_target("0.4.1", "0.4.0", "0.4.1").unwrap(),
            "0.4.1"
        );
        assert_eq!(
            sufficient_local_target("0.4.0", "0.4.0", "0.4.1").unwrap(),
            "0.4.1"
        );
        assert_eq!(
            sufficient_local_target("1.3.0", "1.2.3", "1.2.4").unwrap(),
            "1.3.0"
        );
    }

    #[test]
    fn explicit_non_publishable_package_is_rejected() {
        let workspace = create_workspace();
        let error = handle_independentize_command(
            &IndependentizeConfig {
                workspace_root: workspace.path().to_path_buf(),
                packages: Some(vec!["private".to_string()]),
                dry_run: false,
            },
            OutputType::Raw,
        )
        .expect_err("private package should fail");

        assert!(error.to_string().contains("not publishable"));
    }
}
