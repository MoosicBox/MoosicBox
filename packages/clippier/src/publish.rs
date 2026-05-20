//! Cargo workspace publishing support.
//!
//! This module publishes Cargo workspace crates in dependency order while skipping
//! versions that already exist on crates.io. Development dependencies are ignored
//! for ordering so dev-dependency cycles do not block publication.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

use cargo_metadata::{DependencyKind as CargoDependencyKind, MetadataCommand, PackageId};
use serde::{Deserialize, Serialize};

use crate::OutputType;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

const CRATES_IO_API: &str = "https://crates.io/api/v1";

/// Configuration for publishing a Cargo workspace.
#[derive(Debug, Clone)]
pub struct PublishConfig {
    /// Path to the workspace root or workspace `Cargo.toml`.
    pub workspace_root: PathBuf,
    /// Specific packages to publish. Normal/build workspace dependencies are included automatically.
    pub packages: Option<Vec<String>>,
    /// Compute and print the publish plan without running `cargo publish`.
    pub dry_run: bool,
    /// Run Cargo's local verification step.
    pub verify: bool,
    /// Pass `--allow-dirty` to `cargo publish`.
    pub allow_dirty: bool,
    /// Maximum time to wait for each newly published crate version to appear on crates.io.
    pub publish_timeout: Duration,
    /// Delay between crates.io availability checks.
    pub publish_poll_interval: Duration,
}

impl Default for PublishConfig {
    fn default() -> Self {
        Self {
            workspace_root: PathBuf::from("."),
            packages: None,
            dry_run: false,
            verify: false,
            allow_dirty: false,
            publish_timeout: Duration::from_mins(5),
            publish_poll_interval: Duration::from_secs(10),
        }
    }
}

/// Publish status for a single workspace package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PublishPackageStatus {
    /// The package manifest disables crates.io publishing.
    PublishDisabled,
    /// The same package version already exists on crates.io.
    AlreadyPublished,
    /// The package would be published, but `--dry-run` was used.
    DryRun,
    /// The package was published successfully.
    Published,
}

/// A single package entry in a publish report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishPackageReport {
    /// Package name.
    pub name: String,
    /// Package version.
    pub version: String,
    /// Resulting status.
    pub status: PublishPackageStatus,
}

impl PublishPackageReport {
    #[must_use]
    fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        status: PublishPackageStatus,
    ) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            status,
        }
    }
}

/// Summary produced by `clippier publish`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishReport {
    /// Packages in the order they were considered.
    pub packages: Vec<PublishPackageReport>,
}

impl PublishReport {
    #[must_use]
    const fn new(packages: Vec<PublishPackageReport>) -> Self {
        Self { packages }
    }

    #[must_use]
    fn count(&self, status: PublishPackageStatus) -> usize {
        self.packages
            .iter()
            .filter(|package| package.status == status)
            .count()
    }

    #[must_use]
    fn to_raw_string(&self) -> String {
        let mut lines = Vec::new();

        if self.packages.is_empty() {
            lines.push("No workspace packages matched the publish request".to_string());
        } else {
            lines.push("Publish results:".to_string());
            for package in &self.packages {
                let label = match package.status {
                    PublishPackageStatus::PublishDisabled => "skip publish=false",
                    PublishPackageStatus::AlreadyPublished => "skip already published",
                    PublishPackageStatus::DryRun => "dry-run publish",
                    PublishPackageStatus::Published => "published",
                };
                lines.push(format!("  {label}: {}@{}", package.name, package.version));
            }
        }

        lines.push(String::new());
        lines.push("Summary:".to_string());
        lines.push(format!(
            "  publish disabled: {}",
            self.count(PublishPackageStatus::PublishDisabled)
        ));
        lines.push(format!(
            "  already published: {}",
            self.count(PublishPackageStatus::AlreadyPublished)
        ));
        lines.push(format!(
            "  dry-run: {}",
            self.count(PublishPackageStatus::DryRun)
        ));
        lines.push(format!(
            "  published: {}",
            self.count(PublishPackageStatus::Published)
        ));

        lines.join("\n")
    }
}

/// Internal package data needed for publish planning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishPackageInfo {
    /// Package name.
    pub name: String,
    /// Package version.
    pub version: String,
    /// Whether the package can be published to crates.io.
    pub publishable: bool,
    /// Workspace dependencies that must exist before this package can be published.
    pub publish_dependencies: BTreeSet<String>,
}

impl PublishPackageInfo {
    /// Creates package info for publish planning.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        publishable: bool,
        publish_dependencies: BTreeSet<String>,
    ) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            publishable,
            publish_dependencies,
        }
    }
}

/// A deterministic publish plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishPlan {
    /// Packages that are not publishable to crates.io.
    pub publish_disabled: Vec<String>,
    /// Publishable package order, dependencies first.
    pub publish_order: Vec<String>,
}

/// Build a publish plan from package metadata.
///
/// Development dependencies are ignored. Normal and build dependencies are ordered before dependents.
///
/// # Errors
///
/// * If a requested package does not exist
/// * If a normal/build workspace dependency cycle is detected
/// * If a requested package depends on an unknown workspace package
pub fn build_publish_plan(
    packages: &BTreeMap<String, PublishPackageInfo>,
    requested_packages: Option<&[String]>,
) -> Result<PublishPlan, BoxError> {
    let requested = collect_requested_packages(packages, requested_packages)?;
    let mut publish_disabled = Vec::new();
    let mut publishable_requested = BTreeSet::new();

    for name in &requested {
        let package = packages
            .get(name)
            .ok_or_else(|| format!("Unknown workspace package '{name}'"))?;
        if package.publishable {
            publishable_requested.insert(name.clone());
        } else {
            publish_disabled.push(name.clone());
        }
    }

    let mut temporary = BTreeSet::new();
    let mut permanent = BTreeSet::new();
    let mut publish_order = Vec::new();

    for name in &publishable_requested {
        visit_publish_package(
            name,
            packages,
            &publishable_requested,
            &mut temporary,
            &mut permanent,
            &mut publish_order,
        )?;
    }

    Ok(PublishPlan {
        publish_disabled,
        publish_order,
    })
}

fn collect_requested_packages(
    packages: &BTreeMap<String, PublishPackageInfo>,
    requested_packages: Option<&[String]>,
) -> Result<BTreeSet<String>, BoxError> {
    let mut requested = BTreeSet::new();

    if let Some(package_names) = requested_packages {
        for name in package_names {
            if !packages.contains_key(name) {
                return Err(format!("Unknown workspace package '{name}'").into());
            }
            collect_with_publish_dependencies(name, packages, &mut requested)?;
        }
    } else {
        requested.extend(packages.keys().cloned());
    }

    Ok(requested)
}

fn collect_with_publish_dependencies(
    name: &str,
    packages: &BTreeMap<String, PublishPackageInfo>,
    requested: &mut BTreeSet<String>,
) -> Result<(), BoxError> {
    if !requested.insert(name.to_string()) {
        return Ok(());
    }

    let package = packages
        .get(name)
        .ok_or_else(|| format!("Unknown workspace package '{name}'"))?;

    for dependency in &package.publish_dependencies {
        collect_with_publish_dependencies(dependency, packages, requested)?;
    }

    Ok(())
}

fn visit_publish_package(
    name: &str,
    packages: &BTreeMap<String, PublishPackageInfo>,
    publishable_requested: &BTreeSet<String>,
    temporary: &mut BTreeSet<String>,
    permanent: &mut BTreeSet<String>,
    publish_order: &mut Vec<String>,
) -> Result<(), BoxError> {
    if permanent.contains(name) {
        return Ok(());
    }
    if !temporary.insert(name.to_string()) {
        return Err(format!(
            "Normal/build workspace dependency cycle detected while planning publication at '{name}'"
        )
        .into());
    }

    let package = packages
        .get(name)
        .ok_or_else(|| format!("Unknown workspace package '{name}'"))?;

    for dependency in &package.publish_dependencies {
        let dependency_package = packages
            .get(dependency)
            .ok_or_else(|| format!("Unknown workspace package '{dependency}'"))?;

        if !dependency_package.publishable {
            continue;
        }

        if publishable_requested.contains(dependency) {
            visit_publish_package(
                dependency,
                packages,
                publishable_requested,
                temporary,
                permanent,
                publish_order,
            )?;
        }
    }

    temporary.remove(name);
    permanent.insert(name.to_string());
    publish_order.push(name.to_string());
    Ok(())
}

/// Handles the `publish` command.
///
/// # Errors
///
/// * If workspace metadata cannot be loaded
/// * If a requested package does not exist
/// * If a publishable package has an unpublished non-publishable workspace dependency
/// * If crates.io cannot be queried
/// * If `cargo publish` fails
pub async fn handle_publish_command(
    config: PublishConfig,
    output: OutputType,
) -> Result<String, BoxError> {
    let workspace_root = normalize_workspace_root(&config.workspace_root);
    let packages = load_publish_packages(&workspace_root)?;
    let client = crates_io_client()?;

    let initial_plan = build_publish_plan(&packages, config.packages.as_deref())?;
    let mut reports = Vec::new();
    let mut already_published = BTreeSet::new();
    let mut needs_publish = BTreeSet::new();

    for name in &initial_plan.publish_disabled {
        if let Some(package) = packages.get(name) {
            reports.push(PublishPackageReport::new(
                &package.name,
                &package.version,
                PublishPackageStatus::PublishDisabled,
            ));
        }
    }

    for name in &initial_plan.publish_order {
        let package = packages
            .get(name)
            .ok_or_else(|| format!("Unknown workspace package '{name}'"))?;

        if crate_version_exists(&client, &package.name, &package.version).await? {
            already_published.insert(name.clone());
            reports.push(PublishPackageReport::new(
                &package.name,
                &package.version,
                PublishPackageStatus::AlreadyPublished,
            ));
        } else {
            needs_publish.insert(name.clone());
        }
    }

    validate_publish_dependencies_available(&client, &packages, &needs_publish, &already_published)
        .await?;

    let packages_to_publish = filter_packages_to_publish(&packages, &needs_publish);
    let final_plan = build_publish_plan(&packages_to_publish, None)?;

    for name in final_plan.publish_order {
        let package = packages
            .get(&name)
            .ok_or_else(|| format!("Unknown workspace package '{name}'"))?;

        if config.dry_run {
            reports.push(PublishPackageReport::new(
                &package.name,
                &package.version,
                PublishPackageStatus::DryRun,
            ));
            continue;
        }

        match publish_package(&workspace_root, package, &config)? {
            PublishAttempt::AlreadyPublished => {
                reports.push(PublishPackageReport::new(
                    &package.name,
                    &package.version,
                    PublishPackageStatus::AlreadyPublished,
                ));
            }
            PublishAttempt::Published => {
                wait_for_crate_version(
                    &client,
                    &package.name,
                    &package.version,
                    config.publish_timeout,
                    config.publish_poll_interval,
                )
                .await?;

                reports.push(PublishPackageReport::new(
                    &package.name,
                    &package.version,
                    PublishPackageStatus::Published,
                ));
            }
        }
    }

    let report = PublishReport::new(reports);
    match output {
        OutputType::Raw => Ok(report.to_raw_string()),
        OutputType::Json => Ok(serde_json::to_string_pretty(&report)?),
    }
}

fn normalize_workspace_root(path: &Path) -> PathBuf {
    if path.file_name().is_some_and(|name| name == "Cargo.toml") {
        path.parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    } else {
        path.to_path_buf()
    }
}

fn load_publish_packages(
    workspace_root: &Path,
) -> Result<BTreeMap<String, PublishPackageInfo>, BoxError> {
    let mut command = MetadataCommand::new();
    command.current_dir(workspace_root).no_deps();
    let metadata = command.exec()?;

    let workspace_members = metadata
        .workspace_members
        .iter()
        .cloned()
        .collect::<BTreeSet<PackageId>>();

    let workspace_package_names = metadata
        .packages
        .iter()
        .filter(|package| workspace_members.contains(&package.id))
        .map(|package| package.name.to_string())
        .collect::<BTreeSet<_>>();

    let mut packages = BTreeMap::new();

    for package in metadata
        .packages
        .iter()
        .filter(|package| workspace_members.contains(&package.id))
    {
        let publish_dependencies = package
            .dependencies
            .iter()
            .filter(|dependency| {
                matches!(
                    dependency.kind,
                    CargoDependencyKind::Normal | CargoDependencyKind::Build
                ) && workspace_package_names.contains(dependency.name.as_str())
            })
            .map(|dependency| dependency.name.clone())
            .collect::<BTreeSet<_>>();

        let publishable = package
            .publish
            .as_ref()
            .is_none_or(|registries| registries.iter().any(|registry| registry == "crates-io"));

        packages.insert(
            package.name.to_string(),
            PublishPackageInfo::new(
                package.name.to_string(),
                package.version.to_string(),
                publishable,
                publish_dependencies,
            ),
        );
    }

    Ok(packages)
}

fn filter_packages_to_publish(
    packages: &BTreeMap<String, PublishPackageInfo>,
    needs_publish: &BTreeSet<String>,
) -> BTreeMap<String, PublishPackageInfo> {
    packages
        .iter()
        .filter(|(name, _package)| needs_publish.contains(*name))
        .map(|(name, package)| {
            let mut package = package.clone();
            package
                .publish_dependencies
                .retain(|dependency| needs_publish.contains(dependency));
            (name.clone(), package)
        })
        .collect()
}

async fn validate_publish_dependencies_available(
    client: &reqwest::Client,
    packages: &BTreeMap<String, PublishPackageInfo>,
    needs_publish: &BTreeSet<String>,
    already_published: &BTreeSet<String>,
) -> Result<(), BoxError> {
    for name in needs_publish {
        let package = packages
            .get(name)
            .ok_or_else(|| format!("Unknown workspace package '{name}'"))?;

        for dependency in &package.publish_dependencies {
            if needs_publish.contains(dependency) || already_published.contains(dependency) {
                continue;
            }

            let dependency_package = packages
                .get(dependency)
                .ok_or_else(|| format!("Unknown workspace package '{dependency}'"))?;

            if crate_version_exists(
                client,
                &dependency_package.name,
                &dependency_package.version,
            )
            .await?
            {
                continue;
            }

            return Err(format!(
                "Package '{name}' depends on '{dependency}@{}', but that version is not published and is not publishable in this workspace",
                dependency_package.version
            )
            .into());
        }
    }

    Ok(())
}

fn crates_io_client() -> Result<reqwest::Client, BoxError> {
    Ok(reqwest::Client::builder()
        .user_agent(concat!(
            "clippier/",
            env!("CARGO_PKG_VERSION"),
            " (https://github.com/MoosicBox/MoosicBox)"
        ))
        .build()?)
}

async fn crate_version_exists(
    client: &reqwest::Client,
    name: &str,
    version: &str,
) -> Result<bool, BoxError> {
    let response = client
        .get(format!("{CRATES_IO_API}/crates/{name}/{version}"))
        .send()
        .await?;

    if response.status().is_success() {
        Ok(true)
    } else if response.status() == reqwest::StatusCode::NOT_FOUND {
        Ok(false)
    } else {
        Err(format!(
            "Failed to query crates.io for {name}@{version}: HTTP {}",
            response.status()
        )
        .into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PublishAttempt {
    AlreadyPublished,
    Published,
}

fn publish_package(
    workspace_root: &Path,
    package: &PublishPackageInfo,
    config: &PublishConfig,
) -> Result<PublishAttempt, BoxError> {
    let mut command = Command::new("cargo");
    command
        .arg("publish")
        .arg("-p")
        .arg(&package.name)
        .current_dir(workspace_root);

    if !config.verify {
        command.arg("--no-verify");
    }
    if config.allow_dirty {
        command.arg("--allow-dirty");
    }

    let output = command.output()?;
    if output.status.success() {
        return Ok(PublishAttempt::Published);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if stderr.contains("already exists") {
        return Ok(PublishAttempt::AlreadyPublished);
    }

    Err(format!(
        "cargo publish failed for {}@{}\nstdout:\n{}\nstderr:\n{}",
        package.name, package.version, stdout, stderr
    )
    .into())
}

async fn wait_for_crate_version(
    client: &reqwest::Client,
    name: &str,
    version: &str,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<(), BoxError> {
    let started = Instant::now();

    while started.elapsed() <= timeout {
        if crate_version_exists(client, name, version).await? {
            return Ok(());
        }
        std::thread::sleep(poll_interval);
    }

    Err(format!(
        "Timed out waiting for {name}@{version} to appear on crates.io after {} seconds",
        timeout.as_secs()
    )
    .into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(
        name: &str,
        publishable: bool,
        dependencies: impl IntoIterator<Item = &'static str>,
    ) -> PublishPackageInfo {
        PublishPackageInfo::new(
            name,
            "0.1.0",
            publishable,
            dependencies
                .into_iter()
                .map(ToString::to_string)
                .collect::<BTreeSet<_>>(),
        )
    }

    #[test]
    fn publish_plan_orders_dependencies_before_dependents() {
        let packages = BTreeMap::from([
            ("app".to_string(), package("app", true, ["core"])),
            ("core".to_string(), package("core", true, ["models"])),
            ("models".to_string(), package("models", true, [])),
        ]);

        let plan = build_publish_plan(&packages, None).unwrap();

        assert_eq!(plan.publish_order, ["models", "core", "app"]);
    }

    #[test]
    fn publish_plan_includes_transitive_dependencies_for_requested_package() {
        let packages = BTreeMap::from([
            ("app".to_string(), package("app", true, ["core"])),
            ("core".to_string(), package("core", true, ["models"])),
            ("models".to_string(), package("models", true, [])),
            ("unrelated".to_string(), package("unrelated", true, [])),
        ]);

        let plan = build_publish_plan(&packages, Some(&["app".to_string()])).unwrap();

        assert_eq!(plan.publish_order, ["models", "core", "app"]);
    }

    #[test]
    fn publish_plan_rejects_normal_dependency_cycle() {
        let packages = BTreeMap::from([
            ("a".to_string(), package("a", true, ["b"])),
            ("b".to_string(), package("b", true, ["a"])),
        ]);

        let error = build_publish_plan(&packages, None).unwrap_err().to_string();

        assert!(error.contains("cycle"));
    }

    #[test]
    fn publish_plan_skips_publish_disabled_packages() {
        let packages = BTreeMap::from([
            ("publishable".to_string(), package("publishable", true, [])),
            ("private".to_string(), package("private", false, [])),
        ]);

        let plan = build_publish_plan(&packages, None).unwrap();

        assert_eq!(plan.publish_disabled, ["private"]);
        assert_eq!(plan.publish_order, ["publishable"]);
    }

    #[test]
    fn publish_plan_skips_private_dependencies_for_publish_order() {
        let packages = BTreeMap::from([
            ("app".to_string(), package("app", true, ["private"])),
            ("private".to_string(), package("private", false, [])),
        ]);

        let plan = build_publish_plan(&packages, None).unwrap();

        assert_eq!(plan.publish_disabled, ["private"]);
        assert_eq!(plan.publish_order, ["app"]);
    }
}
