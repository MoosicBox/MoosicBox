//! Cargo workspace publishing support.
//!
//! This module publishes Cargo workspace crates in dependency order while skipping
//! versions that already exist on crates.io. Development dependencies are ignored
//! for ordering so dev-dependency cycles do not block publication.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use cargo_metadata::{DependencyKind as CargoDependencyKind, MetadataCommand, PackageId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{ColorMode, OutputType};

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
    /// Color mode for cargo publish output.
    pub color: ColorMode,
    /// Maximum time to wait for each newly published crate version to appear on crates.io.
    pub publish_timeout: Duration,
    /// Delay between crates.io availability checks.
    pub publish_poll_interval: Duration,
    /// Number of times to retry a package after crates.io rate limiting.
    pub rate_limit_retries: u16,
}

impl Default for PublishConfig {
    fn default() -> Self {
        Self {
            workspace_root: PathBuf::from("."),
            packages: None,
            dry_run: false,
            verify: false,
            allow_dirty: false,
            color: ColorMode::Auto,
            publish_timeout: Duration::from_mins(5),
            publish_poll_interval: Duration::from_secs(10),
            rate_limit_retries: 3,
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
    fn to_raw_string(&self, color: ColorMode) -> String {
        let mut lines = Vec::new();

        if self.packages.is_empty() {
            lines.push("No workspace packages matched the publish request".to_string());
        } else {
            lines.push(paint(color, "1", "Publish results:"));
            for package in &self.packages {
                let (code, label) = match package.status {
                    PublishPackageStatus::PublishDisabled => ("90", "skip publish=false"),
                    PublishPackageStatus::AlreadyPublished => ("33", "skip already published"),
                    PublishPackageStatus::DryRun => ("35", "dry-run publish"),
                    PublishPackageStatus::Published => ("32", "published"),
                };
                lines.push(format!(
                    "  {}: {}",
                    paint(color, code, label),
                    format_package_name(color, &package.name, &package.version)
                ));
            }
        }

        lines.push(String::new());
        lines.push(paint(color, "1", "Summary:"));
        lines.push(format!(
            "  {}: {}",
            paint(color, "90", "publish disabled"),
            self.count(PublishPackageStatus::PublishDisabled)
        ));
        lines.push(format!(
            "  {}: {}",
            paint(color, "33", "already published"),
            self.count(PublishPackageStatus::AlreadyPublished)
        ));
        lines.push(format!(
            "  {}: {}",
            paint(color, "35", "dry-run"),
            self.count(PublishPackageStatus::DryRun)
        ));
        lines.push(format!(
            "  {}: {}",
            paint(color, "32", "published"),
            self.count(PublishPackageStatus::Published)
        ));

        lines.join("\n")
    }
}

#[must_use]
const fn color_enabled(color: ColorMode) -> bool {
    !matches!(color, ColorMode::Never)
}

#[must_use]
fn paint(color: ColorMode, code: &str, text: &str) -> String {
    if color_enabled(color) {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

#[must_use]
fn format_package_name(color: ColorMode, name: &str, version: &str) -> String {
    paint(color, "36;1", &format!("{name}@{version}"))
}

fn log_publish_event(color: ColorMode, code: &str, label: &str, name: &str, version: &str) {
    eprintln!(
        "{} {}",
        paint(color, code, label),
        format_package_name(color, name, version)
    );
}

fn log_publish_event_with_detail(
    color: ColorMode,
    code: &str,
    label: &str,
    name: &str,
    version: &str,
    detail: &str,
) {
    eprintln!(
        "{} {} {}",
        paint(color, code, label),
        format_package_name(color, name, version),
        paint(color, "90", detail)
    );
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
    /// Package directory containing `Cargo.toml`.
    pub package_path: Option<PathBuf>,
    /// crates.io category slugs declared by the package.
    pub categories: Vec<String>,
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
            package_path: None,
            categories: Vec::new(),
        }
    }

    #[must_use]
    fn with_package_path(mut self, package_path: PathBuf) -> Self {
        self.package_path = Some(package_path);
        self
    }

    #[must_use]
    fn with_categories(mut self, categories: Vec<String>) -> Self {
        self.categories = categories;
        self
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
#[allow(clippy::too_many_lines)]
pub async fn handle_publish_command(
    config: PublishConfig,
    output: OutputType,
) -> Result<String, BoxError> {
    let workspace_root = normalize_workspace_root(&config.workspace_root);
    eprintln!(
        "{} {}",
        paint(config.color, "34", "Loading workspace metadata from"),
        paint(config.color, "36;1", &workspace_root.display().to_string())
    );
    let packages = load_publish_packages(&workspace_root)?;
    let client = crates_io_client()?;

    let initial_plan = build_publish_plan(&packages, config.packages.as_deref())?;
    eprintln!(
        "{} {} publishable package(s), {} publish-disabled package(s)",
        paint(config.color, "34", "Planned"),
        paint(
            config.color,
            "36;1",
            &initial_plan.publish_order.len().to_string()
        ),
        paint(
            config.color,
            "90;1",
            &initial_plan.publish_disabled.len().to_string()
        )
    );
    let mut reports = Vec::new();
    let mut already_published = BTreeSet::new();
    let mut needs_publish = BTreeSet::new();

    for name in &initial_plan.publish_disabled {
        if let Some(package) = packages.get(name) {
            log_publish_event_with_detail(
                config.color,
                "90",
                "Skipping",
                &package.name,
                &package.version,
                "(publish disabled)",
            );
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

        log_publish_event_with_detail(
            config.color,
            "34",
            "Checking",
            &package.name,
            &package.version,
            "on crates.io",
        );
        if crate_version_exists(&client, &package.name, &package.version).await? {
            log_publish_event_with_detail(
                config.color,
                "33",
                "Skipping",
                &package.name,
                &package.version,
                "(already published)",
            );
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

    if !needs_publish.is_empty() {
        validate_package_categories_available(&client, &packages, &needs_publish).await?;
    }

    let packages_to_publish = filter_packages_to_publish(&packages, &needs_publish);
    let final_plan = build_publish_plan(&packages_to_publish, None)?;

    let publish_order = final_plan.publish_order;
    for (index, name) in publish_order.iter().enumerate() {
        let package = packages
            .get(name)
            .ok_or_else(|| format!("Unknown workspace package '{name}'"))?;

        if config.dry_run {
            log_publish_event(
                config.color,
                "35",
                "Would publish",
                &package.name,
                &package.version,
            );
            reports.push(PublishPackageReport::new(
                &package.name,
                &package.version,
                PublishPackageStatus::DryRun,
            ));
            continue;
        }

        log_publish_event(
            config.color,
            "35;1",
            "Publishing",
            &package.name,
            &package.version,
        );
        let publish_attempt = match publish_package(&workspace_root, &packages, package, &config) {
            Ok(attempt) => attempt,
            Err(error) => {
                print_failure_summary(
                    config.color,
                    package,
                    &publish_order[index + 1..],
                    &packages,
                    &reports,
                );
                return Err(error);
            }
        };

        match publish_attempt {
            PublishAttempt::AlreadyPublished => {
                log_publish_event_with_detail(
                    config.color,
                    "33",
                    "Skipping",
                    &package.name,
                    &package.version,
                    "(already published)",
                );
                reports.push(PublishPackageReport::new(
                    &package.name,
                    &package.version,
                    PublishPackageStatus::AlreadyPublished,
                ));
            }
            PublishAttempt::Published => {
                if let Err(error) = wait_for_crate_version(
                    &client,
                    &package.name,
                    &package.version,
                    config.publish_timeout,
                    config.publish_poll_interval,
                )
                .await
                {
                    print_failure_summary(
                        config.color,
                        package,
                        &publish_order[index + 1..],
                        &packages,
                        &reports,
                    );
                    return Err(error);
                }

                log_publish_event(
                    config.color,
                    "32;1",
                    "Published",
                    &package.name,
                    &package.version,
                );
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
        OutputType::Raw => Ok(report.to_raw_string(config.color)),
        OutputType::Json => Ok(serde_json::to_string_pretty(&report)?),
    }
}

fn print_failure_summary(
    color: ColorMode,
    failed_package: &PublishPackageInfo,
    remaining_queue: &[String],
    packages: &BTreeMap<String, PublishPackageInfo>,
    reports: &[PublishPackageReport],
) {
    eprintln!();
    eprintln!("{}", paint(color, "31;1", "Publish failed"));
    eprintln!(
        "  {} {}",
        paint(color, "31", "failed:"),
        format_package_name(color, &failed_package.name, &failed_package.version)
    );
    eprintln!(
        "  {} {}",
        paint(color, "32", "published this run:"),
        reports
            .iter()
            .filter(|report| report.status == PublishPackageStatus::Published)
            .count()
    );
    eprintln!(
        "  {} {}",
        paint(color, "33", "already published/skipped:"),
        reports
            .iter()
            .filter(|report| report.status == PublishPackageStatus::AlreadyPublished)
            .count()
    );
    eprintln!(
        "  {} {}",
        paint(color, "90", "publish disabled:"),
        reports
            .iter()
            .filter(|report| report.status == PublishPackageStatus::PublishDisabled)
            .count()
    );
    eprintln!(
        "  {} {}",
        paint(color, "31;1", "not published in remaining queue:"),
        remaining_queue.len() + 1
    );

    eprintln!();
    eprintln!("{}", paint(color, "1", "Not published:"));
    eprintln!(
        "  {} {}",
        paint(color, "31", "failed"),
        format_package_name(color, &failed_package.name, &failed_package.version)
    );
    for name in remaining_queue {
        if let Some(package) = packages.get(name) {
            eprintln!(
                "  {} {}",
                paint(color, "35", "pending"),
                format_package_name(color, &package.name, &package.version)
            );
        } else {
            eprintln!("  {} {name}", paint(color, "35", "pending"));
        }
    }
    eprintln!();
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

        let manifest_path = package.manifest_path.clone().into_std_path_buf();
        let package_path = manifest_path
            .parent()
            .map_or_else(|| workspace_root.to_path_buf(), Path::to_path_buf);

        packages.insert(
            package.name.to_string(),
            PublishPackageInfo::new(
                package.name.to_string(),
                package.version.to_string(),
                publishable,
                publish_dependencies,
            )
            .with_package_path(package_path)
            .with_categories(package.categories.clone()),
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

async fn validate_package_categories_available(
    client: &reqwest::Client,
    packages: &BTreeMap<String, PublishPackageInfo>,
    package_names: &BTreeSet<String>,
) -> Result<(), BoxError> {
    let mut categories = BTreeSet::new();
    for name in package_names {
        let package = packages
            .get(name)
            .ok_or_else(|| format!("Unknown workspace package '{name}'"))?;
        categories.extend(package.categories.iter().cloned());
    }

    let mut supported_categories = BTreeSet::new();
    for category in categories {
        if crates_io_category_exists(client, &category).await? {
            supported_categories.insert(category);
        }
    }

    validate_package_categories(packages, package_names, &supported_categories)
}

async fn crates_io_category_exists(
    client: &reqwest::Client,
    category: &str,
) -> Result<bool, BoxError> {
    let response = client
        .get(format!("{CRATES_IO_API}/categories/{category}"))
        .send()
        .await?;

    if response.status().is_success() {
        Ok(true)
    } else if response.status() == reqwest::StatusCode::NOT_FOUND {
        Ok(false)
    } else {
        Err(format!(
            "Failed to query crates.io category '{category}': HTTP {}",
            response.status()
        )
        .into())
    }
}

fn validate_package_categories(
    packages: &BTreeMap<String, PublishPackageInfo>,
    package_names: &BTreeSet<String>,
    supported_categories: &BTreeSet<String>,
) -> Result<(), BoxError> {
    let mut errors = Vec::new();

    for name in package_names {
        let package = packages
            .get(name)
            .ok_or_else(|| format!("Unknown workspace package '{name}'"))?;
        let unsupported = package
            .categories
            .iter()
            .filter(|category| !supported_categories.contains(category.as_str()))
            .cloned()
            .collect::<Vec<_>>();

        if unsupported.is_empty() {
            continue;
        }

        let manifest_path = package
            .package_path
            .as_ref()
            .map(|path| path.join("Cargo.toml"));
        let manifest = manifest_path.as_ref().map_or_else(
            || "<unknown manifest>".to_string(),
            |path| path.display().to_string(),
        );
        let suggestions = unsupported
            .iter()
            .filter_map(|category| unsupported_category_suggestion(category))
            .collect::<Vec<_>>();
        let suggestion = if suggestions.is_empty() {
            String::new()
        } else {
            format!("; {}", suggestions.join(", "))
        };

        errors.push(format!(
            "  {}: unsupported categor{} {} ({manifest}){suggestion}",
            format_package_name(ColorMode::Never, &package.name, &package.version),
            if unsupported.len() == 1 { "y" } else { "ies" },
            unsupported.join(", "),
        ));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Unsupported crates.io category slugs:\n{}\nSee https://crates.io/category_slugs for supported slugs.",
            errors.join("\n")
        )
        .into())
    }
}

fn unsupported_category_suggestion(category: &str) -> Option<String> {
    match category {
        "testing" => Some("use development-tools::testing instead of testing".to_string()),
        "codec" => Some("use encoding or multimedia instead of codec".to_string()),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PublishAttempt {
    AlreadyPublished,
    Published,
}

#[derive(Debug, Clone)]
struct RateLimit {
    retry_at: Option<DateTime<Utc>>,
    wait: Duration,
}

fn publish_package(
    workspace_root: &Path,
    packages: &BTreeMap<String, PublishPackageInfo>,
    package: &PublishPackageInfo,
    config: &PublishConfig,
) -> Result<PublishAttempt, BoxError> {
    let sanitized = SanitizedPackage::prepare(workspace_root, packages, package)?;

    let mut attempts = 0;
    loop {
        let output = run_streaming_command(build_publish_command(
            sanitized.manifest_path(),
            sanitized.root(),
            sanitized.target_dir(),
            config,
        ))?;
        if output.status.success() {
            return Ok(PublishAttempt::Published);
        }

        if output.stderr.contains("already exists") {
            return Ok(PublishAttempt::AlreadyPublished);
        }

        if let Some(rate_limit) = parse_rate_limit(&output.stderr)
            && attempts < config.rate_limit_retries
        {
            attempts += 1;
            log_rate_limit_wait(
                config.color,
                package,
                &rate_limit,
                attempts,
                config.rate_limit_retries,
            );
            thread::sleep(rate_limit.wait);
            continue;
        }

        return Err(format!(
            "cargo publish failed for {}@{} with status {} (cargo output was streamed above)",
            package.name, package.version, output.status
        )
        .into());
    }
}

fn build_publish_command(
    manifest_path: PathBuf,
    root: &Path,
    target_dir: PathBuf,
    config: &PublishConfig,
) -> Command {
    let mut command = Command::new("cargo");
    command
        .arg("publish")
        .arg("--manifest-path")
        .arg(manifest_path)
        .arg("--color")
        .arg(cargo_color_arg(config.color))
        .current_dir(root)
        .env("CARGO_TARGET_DIR", target_dir);

    if !config.verify {
        command.arg("--no-verify");
    }
    if config.allow_dirty {
        command.arg("--allow-dirty");
    }

    command
}

const fn cargo_color_arg(color: ColorMode) -> &'static str {
    match color {
        ColorMode::Auto | ColorMode::Always => "always",
        ColorMode::Never => "never",
    }
}

fn log_rate_limit_wait(
    color: ColorMode,
    package: &PublishPackageInfo,
    rate_limit: &RateLimit,
    attempt: u16,
    max_attempts: u16,
) {
    let wait = format_duration(rate_limit.wait);
    let retry_at = rate_limit.retry_at.map_or_else(
        || "unknown retry time".to_string(),
        |retry_at| retry_at.to_rfc2822(),
    );

    eprintln!(
        "{} {} {} {} ({attempt}/{max_attempts})",
        paint(color, "33;1", "Rate limited"),
        format_package_name(color, &package.name, &package.version),
        paint(color, "33", &format!("waiting {wait}")),
        paint(color, "90", &format!("until {retry_at}")),
    );
}

#[must_use]
fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let minutes = seconds / 60;
    let remaining_seconds = seconds % 60;

    if minutes == 0 {
        format!("{remaining_seconds}s")
    } else {
        format!("{minutes}m {remaining_seconds}s")
    }
}

fn parse_rate_limit(stderr: &str) -> Option<RateLimit> {
    if !stderr.contains("429 Too Many Requests") {
        return None;
    }

    let retry_at = parse_retry_after(stderr);
    let wait = retry_at.map_or(Duration::from_mins(1), |retry_at| {
        retry_at
            .signed_duration_since(Utc::now())
            .to_std()
            .unwrap_or(Duration::ZERO)
            + Duration::from_secs(5)
    });

    Some(RateLimit { retry_at, wait })
}

fn parse_retry_after(stderr: &str) -> Option<DateTime<Utc>> {
    let retry_text = stderr.split("try again after ").nth(1)?;
    let date = retry_text
        .split(" and see ")
        .next()
        .unwrap_or(retry_text)
        .lines()
        .next()?
        .trim()
        .trim_end_matches('.');

    DateTime::parse_from_rfc2822(date)
        .ok()
        .map(|date| date.with_timezone(&Utc))
}

#[derive(Debug)]
struct CommandOutput {
    status: std::process::ExitStatus,
    stderr: String,
}

fn run_streaming_command(mut command: Command) -> Result<CommandOutput, BoxError> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or("Failed to capture cargo publish stdout")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Failed to capture cargo publish stderr")?;

    let stdout_handle = thread::spawn(move || stream_output(stdout, false));
    let stderr_handle = thread::spawn(move || stream_output(stderr, true));

    let status = child.wait()?;
    stdout_handle
        .join()
        .map_err(|_| "Failed to join stdout streaming thread")??;
    let stderr = stderr_handle
        .join()
        .map_err(|_| "Failed to join stderr streaming thread")??;

    Ok(CommandOutput { status, stderr })
}

fn stream_output(mut reader: impl std::io::Read, stderr: bool) -> Result<String, std::io::Error> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];

    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        if stderr {
            std::io::stderr().write_all(&buffer[..read])?;
            std::io::stderr().flush()?;
        } else {
            std::io::stdout().write_all(&buffer[..read])?;
            std::io::stdout().flush()?;
        }
    }

    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[derive(Debug)]
struct SanitizedPackage {
    root: PathBuf,
    package_path: PathBuf,
}

impl SanitizedPackage {
    fn prepare(
        workspace_root: &Path,
        packages: &BTreeMap<String, PublishPackageInfo>,
        package: &PublishPackageInfo,
    ) -> Result<Self, BoxError> {
        let package_path = package
            .package_path
            .as_ref()
            .ok_or_else(|| format!("Missing package path for {}", package.name))?;
        let workspace_root = fs::canonicalize(workspace_root)?;
        let package_path = fs::canonicalize(package_path)?;
        let relative_package_path = package_path.strip_prefix(&workspace_root).map_err(|_| {
            format!(
                "Package path {} is not inside workspace root {}",
                package_path.display(),
                workspace_root.display()
            )
        })?;
        let root = std::env::temp_dir().join(format!(
            "clippier-publish-root-{}-{}-{}",
            package.name,
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        let sanitized_package_path = root.join(relative_package_path);

        if workspace_root.join(".cargo").exists() {
            copy_dir(&workspace_root.join(".cargo"), &root.join(".cargo"))?;
        }
        copy_dir(&package_path, &sanitized_package_path)?;
        sanitize_manifest(
            &workspace_root,
            packages,
            &sanitized_package_path.join("Cargo.toml"),
        )?;

        Ok(Self {
            root,
            package_path: sanitized_package_path,
        })
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn manifest_path(&self) -> PathBuf {
        self.package_path.join("Cargo.toml")
    }

    fn target_dir(&self) -> PathBuf {
        self.root.join("target")
    }
}

impl Drop for SanitizedPackage {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.root) {
            log::warn!(
                "Failed to remove sanitized publish directory {}: {error}",
                self.root.display()
            );
        }
    }
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), BoxError> {
    fs::create_dir_all(to)?;

    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let destination = to.join(entry.file_name());
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            copy_dir(&source, &destination)?;
        } else if file_type.is_file() {
            fs::copy(&source, &destination)?;
        } else if file_type.is_symlink() {
            let target = fs::read_link(&source)?;
            create_symlink(&target, &destination)?;
        }
    }

    Ok(())
}

#[cfg(unix)]
fn create_symlink(target: &Path, destination: &Path) -> Result<(), BoxError> {
    std::os::unix::fs::symlink(target, destination)?;
    Ok(())
}

#[cfg(windows)]
fn create_symlink(target: &Path, destination: &Path) -> Result<(), BoxError> {
    if target.is_dir() {
        std::os::windows::fs::symlink_dir(target, destination)?;
    } else {
        std::os::windows::fs::symlink_file(target, destination)?;
    }
    Ok(())
}

fn sanitize_manifest(
    workspace_root: &Path,
    packages: &BTreeMap<String, PublishPackageInfo>,
    manifest_path: &Path,
) -> Result<(), BoxError> {
    let workspace_toml = read_toml(&workspace_root.join("Cargo.toml"))?;
    let mut manifest = read_toml(manifest_path)?;
    let package_versions = packages
        .iter()
        .map(|(name, package)| (name.clone(), package.version.clone()))
        .collect::<BTreeMap<_, _>>();

    resolve_workspace_package_fields(&mut manifest, &workspace_toml)?;
    sanitize_dependency_sections(&mut manifest, &workspace_toml, &package_versions)?;
    if let Some(table) = manifest.as_table_mut() {
        table.remove("workspace");
    }

    fs::write(manifest_path, toml::to_string_pretty(&manifest)?)?;
    Ok(())
}

fn read_toml(path: &Path) -> Result<toml::Value, BoxError> {
    Ok(toml::from_str(&fs::read_to_string(path)?)?)
}

fn resolve_workspace_package_fields(
    manifest: &mut toml::Value,
    workspace_toml: &toml::Value,
) -> Result<(), BoxError> {
    let Some(package_table) = manifest
        .get_mut("package")
        .and_then(toml::Value::as_table_mut)
    else {
        return Ok(());
    };
    let workspace_package = workspace_toml
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(toml::Value::as_table);

    let keys = package_table.keys().cloned().collect::<Vec<_>>();
    for key in keys {
        if !is_workspace_inherited(package_table.get(&key)) {
            continue;
        }

        let value = workspace_package
            .and_then(|workspace_package| workspace_package.get(&key))
            .ok_or_else(|| format!("Missing workspace.package.{key}"))?
            .clone();
        package_table.insert(key, value);
    }

    Ok(())
}

fn sanitize_dependency_sections(
    manifest: &mut toml::Value,
    workspace_toml: &toml::Value,
    package_versions: &BTreeMap<String, String>,
) -> Result<(), BoxError> {
    let removed_dev_dependencies = collect_removed_dev_dependency_keys(manifest);

    sanitize_dependency_table(manifest, "dependencies", workspace_toml, package_versions)?;
    sanitize_dependency_table(
        manifest,
        "build-dependencies",
        workspace_toml,
        package_versions,
    )?;
    remove_table_key(manifest, "dev-dependencies");

    if let Some(targets) = manifest
        .get_mut("target")
        .and_then(toml::Value::as_table_mut)
    {
        for (_target_name, target) in targets.iter_mut() {
            sanitize_dependency_table(target, "dependencies", workspace_toml, package_versions)?;
            sanitize_dependency_table(
                target,
                "build-dependencies",
                workspace_toml,
                package_versions,
            )?;
            remove_table_key(target, "dev-dependencies");
        }
    }

    sanitize_features(manifest, &removed_dev_dependencies);

    Ok(())
}

fn sanitize_dependency_table(
    manifest: &mut toml::Value,
    section: &str,
    workspace_toml: &toml::Value,
    package_versions: &BTreeMap<String, String>,
) -> Result<(), BoxError> {
    let Some(dependencies) = manifest
        .get_mut(section)
        .and_then(toml::Value::as_table_mut)
    else {
        return Ok(());
    };

    let keys = dependencies.keys().cloned().collect::<Vec<_>>();
    for key in keys {
        let Some(value) = dependencies.get_mut(&key) else {
            continue;
        };

        if is_workspace_inherited(Some(value)) {
            *value = resolve_workspace_dependency(&key, value, workspace_toml)?;
        }

        strip_workspace_path_dependency(&key, value, package_versions);
    }

    Ok(())
}

fn resolve_workspace_dependency(
    key: &str,
    value: &toml::Value,
    workspace_toml: &toml::Value,
) -> Result<toml::Value, BoxError> {
    let workspace_dependencies = workspace_toml
        .get("workspace")
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(toml::Value::as_table)
        .ok_or("Missing workspace.dependencies")?;
    let workspace_dependency = workspace_dependencies
        .get(key)
        .ok_or_else(|| format!("Missing workspace dependency '{key}'"))?;

    let Some(local_table) = value.as_table() else {
        return Ok(workspace_dependency.clone());
    };

    let mut resolved = match workspace_dependency {
        toml::Value::Table(table) => table.clone(),
        toml::Value::String(version) => {
            let mut table = toml::map::Map::new();
            table.insert("version".to_string(), toml::Value::String(version.clone()));
            table
        }
        other => return Ok(other.clone()),
    };

    for (local_key, local_value) in local_table {
        if local_key != "workspace" {
            resolved.insert(local_key.clone(), local_value.clone());
        }
    }

    Ok(toml::Value::Table(resolved))
}

fn strip_workspace_path_dependency(
    key: &str,
    value: &mut toml::Value,
    package_versions: &BTreeMap<String, String>,
) {
    let toml::Value::Table(table) = value else {
        return;
    };

    let package_name = table
        .get("package")
        .and_then(toml::Value::as_str)
        .unwrap_or(key);

    let Some(version) = package_versions.get(package_name) else {
        return;
    };

    table.remove("path");
    table.remove("workspace");
    table
        .entry("version".to_string())
        .or_insert_with(|| toml::Value::String(version.clone()));
}

fn collect_removed_dev_dependency_keys(manifest: &toml::Value) -> BTreeSet<String> {
    let mut removed = dependency_table_keys(manifest, "dev-dependencies");

    if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
        for target in targets.values() {
            removed.extend(dependency_table_keys(target, "dev-dependencies"));
        }
    }

    removed.retain(|dependency| {
        !dependency_exists_as_normal_or_build_dependency(manifest, dependency)
    });
    removed
}

fn dependency_table_keys(value: &toml::Value, key: &str) -> BTreeSet<String> {
    value
        .get(key)
        .and_then(toml::Value::as_table)
        .map(|table| table.keys().cloned().collect())
        .unwrap_or_default()
}

fn dependency_exists_as_normal_or_build_dependency(
    manifest: &toml::Value,
    dependency: &str,
) -> bool {
    dependency_table_contains(manifest, "dependencies", dependency)
        || dependency_table_contains(manifest, "build-dependencies", dependency)
        || manifest
            .get("target")
            .and_then(toml::Value::as_table)
            .is_some_and(|targets| {
                targets.values().any(|target| {
                    dependency_table_contains(target, "dependencies", dependency)
                        || dependency_table_contains(target, "build-dependencies", dependency)
                })
            })
}

fn dependency_table_contains(value: &toml::Value, key: &str, dependency: &str) -> bool {
    value
        .get(key)
        .and_then(toml::Value::as_table)
        .is_some_and(|table| table.contains_key(dependency))
}

fn sanitize_features(manifest: &mut toml::Value, removed_dev_dependencies: &BTreeSet<String>) {
    if removed_dev_dependencies.is_empty() {
        return;
    }

    let Some(features) = manifest
        .get_mut("features")
        .and_then(toml::Value::as_table_mut)
    else {
        return;
    };

    let feature_names = features.keys().cloned().collect::<BTreeSet<_>>();

    for (_feature_name, feature_values) in features.iter_mut() {
        let Some(feature_values) = feature_values.as_array_mut() else {
            continue;
        };

        feature_values.retain(|feature_value| {
            feature_value.as_str().is_none_or(|feature| {
                !removed_dev_dependencies.iter().any(|dependency| {
                    feature_references_removed_dependency(feature, dependency, &feature_names)
                })
            })
        });
    }
}

fn feature_references_removed_dependency(
    feature: &str,
    dependency: &str,
    feature_names: &BTreeSet<String>,
) -> bool {
    if feature == format!("dep:{dependency}") {
        return true;
    }

    if feature == dependency {
        return !feature_names.contains(feature);
    }

    feature
        .strip_prefix(dependency)
        .is_some_and(|suffix| suffix.starts_with('/') || suffix.starts_with("?/"))
}

fn remove_table_key(value: &mut toml::Value, key: &str) {
    if let Some(table) = value.as_table_mut() {
        table.remove(key);
    }
}

fn is_workspace_inherited(value: Option<&toml::Value>) -> bool {
    value
        .and_then(toml::Value::as_table)
        .and_then(|table| table.get("workspace"))
        .and_then(toml::Value::as_bool)
        .unwrap_or(false)
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

    fn package_with_categories(
        name: &str,
        categories: impl IntoIterator<Item = &'static str>,
    ) -> PublishPackageInfo {
        package(name, true, []).with_categories(
            categories
                .into_iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
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

    #[test]
    fn validate_package_categories_reports_all_unsupported_categories() {
        let packages = BTreeMap::from([
            (
                "ok".to_string(),
                package_with_categories("ok", ["development-tools"]),
            ),
            (
                "bad".to_string(),
                package_with_categories("bad", ["development-tools", "testing", "codec"]),
            ),
        ]);
        let package_names = BTreeSet::from(["ok".to_string(), "bad".to_string()]);
        let supported_categories = BTreeSet::from(["development-tools".to_string()]);

        let error = validate_package_categories(&packages, &package_names, &supported_categories)
            .unwrap_err()
            .to_string();

        assert!(error.contains("bad@0.1.0"));
        assert!(error.contains("testing, codec"));
        assert!(error.contains("development-tools::testing"));
        assert!(error.contains("encoding or multimedia"));
        assert!(!error.contains("ok@0.1.0"));
    }

    #[test]
    fn parse_rate_limit_uses_utc_retry_after_timestamp() {
        let stderr = "error: failed\nCaused by:\n  the remote server responded with an error (status 429 Too Many Requests): You have published too many new crates in a short period of time. Please try again after Wed, 20 May 2026 03:27:27 GMT and see https://crates.io/docs/rate-limits for more details.";

        let rate_limit = parse_rate_limit(stderr).unwrap();

        assert_eq!(
            rate_limit.retry_at.unwrap().to_rfc3339(),
            "2026-05-20T03:27:27+00:00"
        );
    }

    #[test]
    fn parse_rate_limit_without_timestamp_uses_fallback_wait() {
        let stderr = "status 429 Too Many Requests";

        let rate_limit = parse_rate_limit(stderr).unwrap();

        assert!(rate_limit.retry_at.is_none());
        assert_eq!(rate_limit.wait, Duration::from_mins(1));
    }

    #[test]
    fn sanitize_manifest_strips_dev_dependencies_and_resolves_workspace_values() {
        let root = std::env::temp_dir().join(format!(
            "clippier-publish-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let package_dir = root.join("packages/app");
        fs::create_dir_all(&package_dir).unwrap();

        fs::write(
            root.join("Cargo.toml"),
            r#"
                [workspace]
                members = ["packages/app", "packages/models", "packages/test-utils"]

                [workspace.package]
                edition = "2024"
                license = "MPL-2.0"
                repository = "https://example.com/repo"
                version = "1.2.3"

                [workspace.dependencies]
                models = { version = "1.2.3", path = "packages/models", default-features = false }
                test-utils = { version = "1.2.3", path = "packages/test-utils" }
                optional-runtime = { version = "1.2.3", path = "packages/optional-runtime" }
                serde = "1.0.228"
            "#,
        )
        .unwrap();

        fs::write(
            package_dir.join("Cargo.toml"),
            r#"
                [package]
                edition = { workspace = true }
                license = { workspace = true }
                name = "app"
                repository = { workspace = true }
                version = { workspace = true }

                [dependencies]
                models = { workspace = true, features = ["api"] }
                optional-runtime = { workspace = true, optional = true }
                serde = { workspace = true, features = ["derive"] }

                [dev-dependencies]
                optional-runtime = { workspace = true, features = ["macros"] }
                test-utils = { workspace = true }

                [target.'cfg(unix)'.dev-dependencies]
                test-utils = { workspace = true }

                [features]
                runtime = ["dep:optional-runtime", "optional-runtime?/tokio"]
            "#,
        )
        .unwrap();

        let packages = BTreeMap::from([
            ("app".to_string(), package("app", true, ["models"])),
            ("models".to_string(), package("models", true, [])),
            ("test-utils".to_string(), package("test-utils", true, [])),
        ]);

        sanitize_manifest(&root, &packages, &package_dir.join("Cargo.toml")).unwrap();
        let sanitized = fs::read_to_string(package_dir.join("Cargo.toml")).unwrap();
        let sanitized_toml: toml::Value = toml::from_str(&sanitized).unwrap();
        let package = sanitized_toml.get("package").unwrap();
        let dependencies = sanitized_toml.get("dependencies").unwrap();
        let models = dependencies.get("models").unwrap();
        let optional_runtime = dependencies.get("optional-runtime").unwrap();
        let serde = dependencies.get("serde").unwrap();
        let runtime_feature = sanitized_toml
            .get("features")
            .unwrap()
            .get("runtime")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(toml::Value::as_str)
            .collect::<Option<Vec<_>>>()
            .unwrap();

        assert_eq!(package.get("edition").unwrap().as_str(), Some("2024"));
        assert_eq!(package.get("version").unwrap().as_str(), Some("1.2.3"));
        assert!(sanitized_toml.get("dev-dependencies").is_none());
        assert!(
            sanitized_toml
                .get("target")
                .unwrap()
                .get("cfg(unix)")
                .unwrap()
                .get("dev-dependencies")
                .is_none()
        );
        assert!(models.get("path").is_none());
        assert_eq!(models.get("version").unwrap().as_str(), Some("1.2.3"));
        assert_eq!(
            optional_runtime.get("version").unwrap().as_str(),
            Some("1.2.3")
        );
        assert_eq!(
            runtime_feature,
            ["dep:optional-runtime", "optional-runtime?/tokio"]
        );
        assert_eq!(serde.get("version").unwrap().as_str(), Some("1.0.228"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sanitize_features_prunes_removed_dev_dependency_references() {
        let mut manifest = toml::from_str(
            r#"
                [dev-dependencies]
                test-utils = { path = "../test-utils" }

                [target.'cfg(unix)'.dev-dependencies]
                target-test-utils = { path = "../target-test-utils" }

                [features]
                default = ["decimal", "test-utils"]
                decimal = [
                    "models/api",
                    "test-utils/decimal",
                    "test-utils?/uuid",
                    "dep:test-utils",
                    "serde/derive",
                ]
                target-only = ["target-test-utils/helpers"]
                test-utils = []
            "#,
        )
        .unwrap();

        let removed_dev_dependencies = collect_removed_dev_dependency_keys(&manifest);
        sanitize_features(&mut manifest, &removed_dev_dependencies);

        let features = manifest.get("features").unwrap();
        let decimal = feature_values(features, "decimal");
        let target_only = feature_values(features, "target-only");
        let default = feature_values(features, "default");

        assert_eq!(decimal, ["models/api", "serde/derive"]);
        assert!(target_only.is_empty());
        assert_eq!(default, ["decimal", "test-utils"]);
    }

    fn feature_values<'a>(features: &'a toml::Value, name: &str) -> Vec<&'a str> {
        features
            .get(name)
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(toml::Value::as_str)
            .collect::<Option<Vec<_>>>()
            .unwrap()
    }
}
