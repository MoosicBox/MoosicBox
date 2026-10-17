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

use crate::{
    ColorMode, OutputType,
    cargo_workspace::normalize_workspace_root,
    preferences::{PreferencesOverrides, PurityPolicy, resolve_preferences},
};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

mod preflight;
mod registry;
use registry::RegistryClient;
pub(crate) mod purity;
#[cfg(test)]
mod retry_tests;

pub use purity::{PublishPurityError, PurityAssessment, PurityStatus};

const CRATES_IO_API: &str = "https://crates.io/api/v1";

/// Configuration for publishing a Cargo workspace.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)] // Independent CLI switches, not mutually exclusive states.
pub struct PublishConfig {
    /// Path to the workspace root or workspace `Cargo.toml`.
    pub workspace_root: PathBuf,
    /// Specific packages to publish. Normal/build workspace dependencies are included automatically.
    pub packages: Option<Vec<String>>,
    /// Override workspace/user/environment purity policy; `None` resolves preferences.
    pub purity: Option<PurityPolicy>,
    /// Package and resolve the pending queue without uploading. Compilation requires `verify`.
    pub dry_run: bool,
    /// Override queue preflight for real publishes; `None` inherits preferences.
    pub preflight: Option<bool>,
    /// Seed staging from the local workspace lockfile; defaults to true.
    pub use_lockfile: Option<bool>,
    /// Block on meaningful published lockfile differences; defaults to false.
    pub require_lockfile_match: Option<bool>,
    /// Stop publish preflight after its first failure instead of checking every package.
    pub fail_fast: bool,
    /// Compile packaged contents during preflight and verify again before upload.
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
            purity: None,
            dry_run: false,
            preflight: None,
            use_lockfile: None,
            require_lockfile_match: None,
            fail_fast: false,
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
    /// The package passed packaging/resolution preflight without being uploaded.
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
    /// Whether Cargo packaging and fresh dependency resolution succeeded this run.
    #[serde(default)]
    pub packaging_validated: bool,
    /// Whether packaged contents were compiled during preflight this run.
    #[serde(default)]
    pub compilation_verified: bool,
    /// Exact registry artifact comparison, when this version was already published.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purity: Option<PurityAssessment>,
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
            packaging_validated: false,
            compilation_verified: false,
            purity: None,
        }
    }
}

/// Summary produced by `clippier publish`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishReport {
    /// Packages in the order they were considered.
    pub packages: Vec<PublishPackageReport>,
    /// Number of exact artifact comparisons that matched.
    pub purity_pure: usize,
    /// Number of exact artifact comparisons that differed.
    pub purity_impure: usize,
    /// Number of exact artifact comparisons that could not be completed.
    pub purity_unknown: usize,
}

impl PublishReport {
    #[must_use]
    fn new(packages: Vec<PublishPackageReport>) -> Self {
        let count = |status| {
            packages
                .iter()
                .filter(|package| {
                    package
                        .purity
                        .as_ref()
                        .is_some_and(|assessment| assessment.status == status)
                })
                .count()
        };
        Self {
            purity_pure: count(PurityStatus::Pure),
            purity_impure: count(PurityStatus::Impure),
            purity_unknown: count(PurityStatus::Unknown),
            packages,
        }
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
        if self
            .packages
            .iter()
            .any(|package| package.packaging_validated)
        {
            lines.push("Packaging and fresh dependency resolution: validated".to_string());
            lines.push(
                if self
                    .packages
                    .iter()
                    .any(|package| package.compilation_verified)
                {
                    "Compilation verification: passed".to_string()
                } else {
                    "Compilation verification: not requested (use --verify)".to_string()
                },
            );
        }

        if self.packages.is_empty() {
            lines.push("No workspace packages matched the publish request".to_string());
        } else {
            lines.push(paint(color, "1", "Publish results:"));
            for package in &self.packages {
                let (code, label) = match package.status {
                    PublishPackageStatus::PublishDisabled => ("90", "skip publish=false"),
                    PublishPackageStatus::AlreadyPublished => ("33", "skip already published"),
                    PublishPackageStatus::DryRun => ("35", "validated (not uploaded)"),
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

        lines.push(format!(
            "  purity: {} pure, {} impure, {} unknown",
            self.purity_pure, self.purity_impure, self.purity_unknown
        ));
        for assessment in self
            .packages
            .iter()
            .filter_map(|package| package.purity.as_ref())
        {
            lines.push(format!("  {}", purity::summary(assessment)));
        }
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
/// * If Cargo packaging, fresh dependency resolution, or requested compilation fails
/// * If Cargo source overrides prevent registry-faithful preflight validation
/// * If shared preferences cannot be resolved or strict purity is impure/unknown
/// * If `cargo publish` fails
#[allow(clippy::too_many_lines)]
pub async fn handle_publish_command(
    config: PublishConfig,
    output: OutputType,
) -> Result<String, BoxError> {
    let workspace_root = normalize_workspace_root(&config.workspace_root);
    let mut config = config;
    let preferences = resolve_preferences(
        &workspace_root,
        PreferencesOverrides {
            purity: config.purity,
            preflight: config.preflight,
            use_lockfile: config.use_lockfile,
            require_lockfile_match: config.require_lockfile_match,
        },
    )?;
    let purity_policy = preferences.publish.purity;
    config.preflight = Some(preferences.publish.preflight);
    config.use_lockfile = Some(preferences.publish.use_lockfile);
    config.require_lockfile_match = Some(preferences.publish.require_lockfile_match);
    config.purity = Some(purity_policy);
    eprintln!(
        "{} {}",
        paint(config.color, "34", "Loading workspace metadata from"),
        paint(config.color, "36;1", &workspace_root.display().to_string())
    );
    let (workspace_root, packages) = load_publish_packages(&workspace_root)?;
    let client = RegistryClient::new(config.rate_limit_retries)?;

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
    let mut needs_publish = BTreeSet::new();
    let mut preflight_failures = Vec::new();

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

    if !should_run_preflight(&config) {
        publish_pending_queue(
            &workspace_root,
            &packages,
            &initial_plan.publish_order,
            &config,
            &client,
            &mut reports,
        )
        .await?;
        let report = PublishReport::new(reports);
        return match output {
            OutputType::Raw => Ok(report.to_raw_string(config.color)),
            OutputType::Json => Ok(serde_json::to_string_pretty(&report)?),
        };
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
        let exists = match crate_version_exists(&client, &package.name, &package.version).await {
            Ok(exists) => exists,
            Err(error) if !config.fail_fast && !error.is::<registry::RateLimitExhausted>() => {
                preflight_failures.push(format!(
                    "{name}: registry availability could not be checked: {error}"
                ));
                // Still diagnose this package, but never upload when registry state is unknown.
                needs_publish.insert(name.clone());
                continue;
            }
            Err(error) => return Err(error),
        };
        if exists {
            if config.require_lockfile_match.unwrap_or(false) {
                let pending = initial_plan
                    .publish_order
                    .iter()
                    .filter(|name| *name != &package.name)
                    .cloned()
                    .collect();
                let assessment = purity::assess(
                    &workspace_root,
                    &packages,
                    package,
                    &pending,
                    &config,
                    &client,
                )
                .await;
                purity::enforce_config(&config, std::slice::from_ref(&assessment))?;
                let mut report = PublishPackageReport::new(
                    &package.name,
                    &package.version,
                    PublishPackageStatus::AlreadyPublished,
                );
                report.purity = Some(assessment);
                reports.push(report);
                continue;
            }
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
        } else {
            needs_publish.insert(name.clone());
        }
    }

    assess_remaining_dependencies(
        &packages,
        &needs_publish,
        &config,
        &mut reports,
        |package| {
            let client = &client;
            let packages = &packages;
            let workspace_root = &workspace_root;
            let pending = &needs_publish;
            let config = &config;
            async move {
                if !crate_version_exists(client, &package.name, &package.version).await? {
                    return Ok(None);
                }
                Ok(Some(
                    purity::assess(workspace_root, packages, &package, pending, config, client)
                        .await,
                ))
            }
        },
    )
    .await?;

    let assessments = reports
        .iter()
        .filter_map(|report| report.purity.clone())
        .collect::<Vec<_>>();
    // Check every relevant dependency before uploading any pending package.
    purity::enforce_config(&config, &assessments)?;

    for name in &needs_publish {
        let selected = BTreeSet::from([name.clone()]);
        let mut record = |result: Result<(), BoxError>| -> Result<(), BoxError> {
            if let Err(error) = result {
                if config.fail_fast || error.is::<registry::RateLimitExhausted>() {
                    return Err(error);
                }
                preflight_failures.push(format!("{name}: {error}"));
            }
            Ok(())
        };
        record(validate_publish_manifests(
            &workspace_root,
            &packages,
            &selected,
        ))?;
        record(
            validate_publish_dependencies_available(&client, &packages, &selected, &needs_publish)
                .await,
        )?;
        record(validate_package_categories_available(&client, &packages, &selected).await)?;
    }

    let packages_to_publish = filter_packages_to_publish(&packages, &needs_publish);
    let final_plan = build_publish_plan(&packages_to_publish, None)?;

    let publish_order = final_plan.publish_order;
    if !preflight_failures.is_empty() {
        if !should_run_preflight(&config) {
            return Err(format!(
                "Publish validation failed (no packages uploaded):\n  {}",
                preflight_failures.join("\n  ")
            )
            .into());
        }
        // Still run Cargo checks to discover independent packaging failures.
        return preflight::validate_with_failures(
            &workspace_root,
            &packages,
            &publish_order,
            &config,
            preflight_failures,
        )
        .map(|()| String::new());
    }
    publish_pending_queue(
        &workspace_root,
        &packages,
        &publish_order,
        &config,
        &client,
        &mut reports,
    )
    .await?;

    for report in &mut reports {
        if publish_order.contains(&report.name) {
            report.packaging_validated = true;
            report.compilation_verified = config.verify;
        }
    }
    let report = PublishReport::new(reports);
    match output {
        OutputType::Raw => Ok(report.to_raw_string(config.color)),
        OutputType::Json => Ok(serde_json::to_string_pretty(&report)?),
    }
}

/// Purity reachability is deliberately separate from upload planning: disabled
/// nodes stop publication, but cannot hide registry-backed transitive inputs.
fn pending_consumer_closure(
    packages: &BTreeMap<String, PublishPackageInfo>,
    pending: &BTreeSet<String>,
) -> BTreeMap<String, Vec<String>> {
    let mut consumers = BTreeMap::<String, Vec<String>>::new();
    for consumer in pending {
        let mut stack = vec![consumer.as_str()];
        let mut seen = BTreeSet::new();
        while let Some(name) = stack.pop() {
            if !seen.insert(name) {
                continue;
            }
            if name != consumer.as_str() {
                consumers
                    .entry(name.to_string())
                    .or_default()
                    .push(consumer.clone());
            }
            if let Some(package) = packages.get(name) {
                stack.extend(package.publish_dependencies.iter().map(String::as_str));
            }
        }
    }
    consumers
}

/// The lookup seam returns None only for a confirmed absent exact version.
/// Errors become UNKNOWN assessments, preserving strict fail-closed behavior.
async fn assess_remaining_dependencies<F, Fut>(
    packages: &BTreeMap<String, PublishPackageInfo>,
    pending: &BTreeSet<String>,
    config: &PublishConfig,
    reports: &mut Vec<PublishPackageReport>,
    mut lookup: F,
) -> Result<(), BoxError>
where
    F: FnMut(PublishPackageInfo) -> Fut,
    Fut: std::future::Future<Output = Result<Option<PurityAssessment>, BoxError>>,
{
    let policy = config.purity.unwrap_or_default();
    for (name, consumers) in pending_consumer_closure(packages, pending) {
        if pending.contains(&name)
            || reports
                .iter()
                .any(|report| report.name == name && report.purity.is_some())
        {
            continue;
        }
        let Some(package) = packages.get(&name) else {
            continue;
        };
        let assessment = match lookup(package.clone()).await {
            Ok(None) => continue,
            Ok(Some(mut assessment)) => {
                assessment.affected_pending_consumers = consumers;
                assessment
            }
            Err(error) => PurityAssessment {
                name: name.clone(),
                version: package.version.clone(),
                status: PurityStatus::Unknown,
                affected_pending_consumers: consumers,
                added: Vec::new(),
                removed: Vec::new(),
                changed: Vec::new(),
                lockfile_changed: false,
                diagnostics: vec![format!(
                    "Registry availability could not be checked: {error}"
                )],
            },
        };
        purity::warn(policy, &assessment, config.color);
        if let Some(report) = reports.iter_mut().find(|report| report.name == name) {
            report.purity = Some(assessment);
        } else {
            let status = if package.publishable {
                PublishPackageStatus::AlreadyPublished
            } else {
                PublishPackageStatus::PublishDisabled
            };
            let mut report = PublishPackageReport::new(&name, &package.version, status);
            report.purity = Some(assessment);
            reports.push(report);
        }
        if config.fail_fast {
            let assessments = reports
                .iter()
                .filter_map(|report| report.purity.clone())
                .collect::<Vec<_>>();
            purity::enforce_config(config, &assessments)?;
        }
    }
    Ok(())
}

fn should_run_preflight(config: &PublishConfig) -> bool {
    config.dry_run || config.preflight.unwrap_or(false)
}

async fn publish_pending_queue(
    workspace_root: &Path,
    packages: &BTreeMap<String, PublishPackageInfo>,
    publish_order: &[String],
    config: &PublishConfig,
    client: &RegistryClient,
    reports: &mut Vec<PublishPackageReport>,
) -> Result<(), BoxError> {
    // Repeat the gate at the upload seam so direct queue tests cannot bypass it.
    let assessments = reports
        .iter()
        .filter_map(|report| report.purity.clone())
        .collect::<Vec<_>>();
    purity::enforce_config(config, &assessments)?;
    if should_run_preflight(config) {
        preflight::validate(workspace_root, packages, publish_order, config)?;
    }
    for (index, name) in publish_order.iter().enumerate() {
        log_queue_progress(config.color, publish_order, index, reports);
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

        if !should_run_preflight(config) {
            let preparation = prepare_next_upload(
                workspace_root,
                packages,
                package,
                &publish_order[index + 1..],
                config,
                client,
                reports,
            )
            .await
            .inspect_err(|_| {
                print_failure_summary(
                    config.color,
                    package,
                    &publish_order[index + 1..],
                    packages,
                    reports,
                );
            })?;
            if let Some(report) = preparation {
                reports.push(report);
                continue;
            }
        }

        let publish_attempt = attempt_next_upload(
            workspace_root,
            packages,
            package,
            &publish_order[index + 1..],
            config,
            reports,
            &queue_progress(publish_order, index, reports),
        )?;

        match publish_attempt {
            PublishAttempt::AlreadyPublished => {
                reports.push(
                    assess_publish_race(
                        workspace_root,
                        packages,
                        package,
                        &publish_order[index + 1..],
                        config,
                        client,
                    )
                    .await?,
                );
            }
            PublishAttempt::Published => {
                record_published(package, config, client, reports)
                    .await
                    .inspect_err(|_| {
                        print_failure_summary(
                            config.color,
                            package,
                            &publish_order[index + 1..],
                            packages,
                            reports,
                        );
                    })?;
            }
        }
    }

    log_queue_progress(config.color, publish_order, publish_order.len(), reports);
    Ok(())
}

#[must_use]
fn queue_progress(order: &[String], completed: usize, reports: &[PublishPackageReport]) -> String {
    // Count only completed entries in this queue. Preflight may already have
    // skipped other packages; disabled and purity-only reports are not uploads.
    let count = |status| {
        order[..completed]
            .iter()
            .filter(|name| {
                reports
                    .iter()
                    .any(|report| &report.name == *name && report.status == status)
            })
            .count()
    };
    format!(
        "Publish progress: {completed}/{} packages complete; {} published, {} already published/skipped, {} dry-run; {} remaining",
        order.len(),
        count(PublishPackageStatus::Published),
        count(PublishPackageStatus::AlreadyPublished),
        count(PublishPackageStatus::DryRun),
        order.len() - completed,
    )
}

fn log_queue_progress(
    color: ColorMode,
    order: &[String],
    completed: usize,
    reports: &[PublishPackageReport],
) {
    eprintln!(
        "{}",
        paint(color, "34;1", &queue_progress(order, completed, reports))
    );
}

fn attempt_next_upload(
    workspace_root: &Path,
    packages: &BTreeMap<String, PublishPackageInfo>,
    package: &PublishPackageInfo,
    remaining: &[String],
    config: &PublishConfig,
    reports: &[PublishPackageReport],
    progress: &str,
) -> Result<PublishAttempt, BoxError> {
    log_publish_event(
        config.color,
        "35;1",
        "Publishing",
        &package.name,
        &package.version,
    );
    publish_package(workspace_root, packages, package, config, progress).inspect_err(|_| {
        print_failure_summary(config.color, package, remaining, packages, reports);
    })
}

async fn record_published(
    package: &PublishPackageInfo,
    config: &PublishConfig,
    client: &RegistryClient,
    reports: &mut Vec<PublishPackageReport>,
) -> Result<(), BoxError> {
    wait_for_crate_version(
        client,
        &package.name,
        &package.version,
        config.publish_timeout,
        config.publish_poll_interval,
    )
    .await?;
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
    Ok(())
}

async fn prepare_next_upload(
    workspace_root: &Path,
    packages: &BTreeMap<String, PublishPackageInfo>,
    package: &PublishPackageInfo,
    remaining: &[String],
    config: &PublishConfig,
    client: &RegistryClient,
    reports: &mut Vec<PublishPackageReport>,
) -> Result<Option<PublishPackageReport>, BoxError> {
    if crate_version_exists(client, &package.name, &package.version).await? {
        return Ok(Some(
            assess_publish_race(workspace_root, packages, package, remaining, config, client)
                .await?,
        ));
    }
    let pending = remaining
        .iter()
        .cloned()
        .chain(std::iter::once(package.name.clone()))
        .collect();
    assess_remaining_dependencies(packages, &pending, config, reports, |dependency| {
        let pending = &pending;
        async move {
            if !crate_version_exists(client, &dependency.name, &dependency.version).await? {
                return Ok(None);
            }
            Ok(Some(
                purity::assess(
                    workspace_root,
                    packages,
                    &dependency,
                    pending,
                    config,
                    client,
                )
                .await,
            ))
        }
    })
    .await?;
    let assessments = reports
        .iter()
        .filter_map(|report| report.purity.clone())
        .collect::<Vec<_>>();
    purity::enforce_config(config, &assessments)?;
    let selected = BTreeSet::from([package.name.clone()]);
    validate_publish_manifests(workspace_root, packages, &selected)?;
    validate_publish_dependencies_available(client, packages, &selected, &pending).await?;
    validate_package_categories_available(client, packages, &selected).await?;
    Ok(None)
}

// A version may appear after preflight. Check its artifact before any subsequent upload.
async fn assess_publish_race(
    workspace_root: &Path,
    packages: &BTreeMap<String, PublishPackageInfo>,
    package: &PublishPackageInfo,
    remaining: &[String],
    config: &PublishConfig,
    client: &RegistryClient,
) -> Result<PublishPackageReport, BoxError> {
    let pending = remaining.iter().cloned().collect();
    let mut report = PublishPackageReport::new(
        &package.name,
        &package.version,
        PublishPackageStatus::AlreadyPublished,
    );
    if config.require_lockfile_match.unwrap_or(false)
        || pending_consumer_closure(packages, &pending).contains_key(&package.name)
    {
        let assessment =
            purity::assess(workspace_root, packages, package, &pending, config, client).await;
        let policy = config.purity.unwrap_or_default();
        purity::warn(policy, &assessment, config.color);
        purity::enforce_config(config, std::slice::from_ref(&assessment))?;
        report.purity = Some(assessment);
    }
    log_publish_event_with_detail(
        config.color,
        "33",
        "Skipping",
        &package.name,
        &package.version,
        "(already published)",
    );
    Ok(report)
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

fn load_publish_packages(
    workspace_root: &Path,
) -> Result<(PathBuf, BTreeMap<String, PublishPackageInfo>), BoxError> {
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

    Ok((metadata.workspace_root.into_std_path_buf(), packages))
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
    client: &RegistryClient,
    packages: &BTreeMap<String, PublishPackageInfo>,
    needs_publish: &BTreeSet<String>,
    already_published: &BTreeSet<String>,
) -> Result<(), BoxError> {
    let mut failures = Vec::new();
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

            match crate_version_exists(
                client,
                &dependency_package.name,
                &dependency_package.version,
            )
            .await
            {
                Ok(true) => continue,
                Err(error) if error.is::<registry::RateLimitExhausted>() => return Err(error),
                Err(error) => {
                    failures.push(format!("{name}: cannot check {dependency}: {error}"));
                    continue;
                }
                Ok(false) => {}
            }
            failures.push(format!(
                "Package '{name}' depends on '{dependency}@{}', but that version is not published and is not publishable in this workspace. Normal and build dependencies, including optional dependencies with disabled features, must be available on crates.io. For test-only dependencies, move the tests into a separate unpublished test-runner package",
                dependency_package.version
            ));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n").into())
    }
}

async fn crate_version_exists(
    client: &RegistryClient,
    name: &str,
    version: &str,
) -> Result<bool, BoxError> {
    client.version_exists(name, version).await
}

async fn validate_package_categories_available(
    client: &RegistryClient,
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
    client: &RegistryClient,
    category: &str,
) -> Result<bool, BoxError> {
    client
        .exists(&format!("{CRATES_IO_API}/categories/{category}"), true)
        .await
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
    progress: &str,
) -> Result<PublishAttempt, BoxError> {
    let sanitized = SanitizedPackage::prepare(workspace_root, packages, package)?;
    seed_publish_lockfile(workspace_root, &sanitized, config)?;

    publish_with_retry(
        package,
        config,
        || {
            run_streaming_command(build_publish_command(
                sanitized.manifest_path(),
                sanitized.root(),
                sanitized.target_dir(),
                config,
            ))
        },
        |wait| {
            eprintln!("{}", paint(config.color, "34;1", progress));
            thread::sleep(wait);
        },
    )
}

// Injection keeps retry/termination coverage independent of registry uploads and
// wall-clock sleeps. Every retry still runs the real Cargo publication command.
fn publish_with_retry(
    package: &PublishPackageInfo,
    config: &PublishConfig,
    mut publish: impl FnMut() -> Result<CommandOutput, BoxError>,
    mut sleep: impl FnMut(Duration),
) -> Result<PublishAttempt, BoxError> {
    let mut attempts = 0;
    let mut network_attempts = 0;
    loop {
        let output = publish()?;
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
            sleep(rate_limit.wait);
            continue;
        }

        // Require both source/download context with a transient transport cause
        // and evidence that upload has not started. Preparation failures alone
        // can be invalid packages; never blindly replay an uncertain upload.
        if preflight::output::transient_network(&output.stderr)
            && (output.stderr.contains("download of config.json failed")
                || output
                    .stderr
                    .contains("failed to prepare local package for uploading"))
            && network_attempts < 3
        {
            let wait = Duration::from_secs(1 << network_attempts);
            network_attempts += 1;
            eprintln!(
                "Network error before upload for {}@{}; retrying in {}s ({network_attempts}/3)",
                package.name,
                package.version,
                wait.as_secs()
            );
            sleep(wait);
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
        .arg("--registry")
        .arg("crates-io")
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
    parse_rate_limit_at(stderr, Utc::now())
}

fn parse_rate_limit_at(stderr: &str, now: DateTime<Utc>) -> Option<RateLimit> {
    // Match Cargo's status diagnostics, not arbitrary occurrences of "429" in a
    // package name, URL or warning. libcurl's header-only response omits the reason.
    let rate_limited = stderr.lines().any(|line| {
        let line = line.trim();
        [
            "status 429",
            "failed to get a 200 OK response, got 429",
            "HTTP/1.1 429",
            "HTTP/2 429",
        ]
        .iter()
        .any(|marker| {
            line.match_indices(marker).any(|(start, marker)| {
                line[start + marker.len()..]
                    .chars()
                    .next()
                    .is_none_or(|next| next.is_whitespace() || matches!(next, ')' | ':' | ','))
            })
        })
    });
    if !rate_limited {
        return None;
    }

    let header = stderr
        .lines()
        .filter_map(|line| line.trim().split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("retry-after"))
        .map(|(_, value)| value.trim());
    Some(retry_timing(parse_retry_after(stderr), header, now))
}

fn retry_timing(
    body_date: Option<DateTime<Utc>>,
    header: Option<&str>,
    now: DateTime<Utc>,
) -> RateLimit {
    let header_date = header
        .and_then(|value| DateTime::parse_from_rfc2822(value).ok())
        .map(|date| date.with_timezone(&Utc));
    let retry_at = match (body_date, header_date) {
        (Some(body), Some(header)) => Some(body.max(header)),
        (body, header) => body.or(header),
    };
    let dated_wait = retry_at.map(|date| {
        date.signed_duration_since(now)
            .to_std()
            .unwrap_or(Duration::ZERO)
            .saturating_add(Duration::from_secs(5))
    });
    let seconds_wait = header
        .and_then(|value| value.parse::<u64>().ok())
        .map(|seconds| Duration::from_secs(seconds).saturating_add(Duration::from_secs(5)));
    let wait = match (dated_wait, seconds_wait) {
        (Some(date), Some(seconds)) => date.max(seconds),
        (date, seconds) => date.or(seconds).unwrap_or(Duration::from_mins(1)),
    };
    RateLimit { retry_at, wait }
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

fn seed_publish_lockfile(
    workspace_root: &Path,
    staged: &SanitizedPackage,
    config: &PublishConfig,
) -> Result<(), BoxError> {
    if config.use_lockfile.unwrap_or(true) && workspace_root.join("Cargo.lock").is_file() {
        fs::copy(
            workspace_root.join("Cargo.lock"),
            staged.package_path.join("Cargo.lock"),
        )?;
    }
    Ok(())
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
        // Install cleanup before staging can fail, including unknown purity checks.
        let staged = Self {
            root,
            package_path: sanitized_package_path,
        };

        if workspace_root.join(".cargo").exists() {
            copy_dir(&workspace_root.join(".cargo"), &staged.root.join(".cargo"))?;
        }
        copy_dir(&package_path, &staged.package_path)?;
        sanitize_manifest(&workspace_root, packages, &staged.manifest_path())?;

        Ok(staged)
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
    copy_dir_contents(from, to).map_err(|error| {
        format!(
            "Failed to copy {} to {}: {error}",
            from.display(),
            to.display()
        )
        .into()
    })
}

fn copy_dir_contents(from: &Path, to: &Path) -> Result<(), BoxError> {
    fs::create_dir_all(to)?;

    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let destination = to.join(entry.file_name());
        let file_type = entry.file_type()?;
        // Never inherit a lockfile that admits yanked versions or copy build/VCS state.
        if entry.file_name() == "Cargo.lock"
            || entry.file_name() == "target"
            || entry.file_name() == ".git"
            || entry.file_name() == ".cargo"
            || entry.file_name() == "node_modules"
        {
            continue;
        }

        if file_type.is_dir() {
            copy_dir(&source, &destination)?;
        } else if file_type.is_file() {
            fs::copy(&source, &destination).map_err(|error| {
                format!(
                    "Failed to copy file {} to {}: {error}",
                    source.display(),
                    destination.display()
                )
            })?;
        } else if file_type.is_symlink() {
            let target = fs::read_link(&source)?;
            create_symlink(&target, &destination).map_err(|error| {
                format!(
                    "Failed to copy symlink {} to {} (target {}): {error}",
                    source.display(),
                    destination.display(),
                    target.display()
                )
            })?;
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
    let original = read_toml(manifest_path)?;
    let mut manifest = sanitized_manifest(workspace_root, packages, manifest_path)?;
    stage_inherited_package_files(workspace_root, manifest_path, &original, &mut manifest)?;
    fs::write(manifest_path, toml::to_string_pretty(&manifest)?)?;
    Ok(())
}

// Cargo resolves inherited README/license-file paths relative to the workspace,
// not the member. Keep staged packages self-contained without overwriting a
// member's own files (which can have the same basename but different contents).
fn stage_inherited_package_files(
    workspace_root: &Path,
    manifest_path: &Path,
    original: &toml::Value,
    manifest: &mut toml::Value,
) -> Result<(), BoxError> {
    for field in ["readme", "license-file"] {
        if !is_workspace_inherited(original.get("package").and_then(|p| p.get(field))) {
            continue;
        }
        let Some(value) = manifest["package"].get(field).cloned() else {
            continue;
        };
        let source = if let Some(path) = value.as_str() {
            workspace_root.join(path)
        } else if field == "readme" && value.as_bool() == Some(true) {
            let Some(path) = ["README.md", "README.txt", "README"]
                .iter()
                .map(|name| workspace_root.join(name))
                .find(|path| path.is_file())
            else {
                continue;
            };
            path
        } else {
            continue;
        };
        let relative = PathBuf::from("clippier-workspace-assets").join(field).join(
            source
                .file_name()
                .ok_or("Missing workspace package file name")?,
        );
        let destination = manifest_path
            .parent()
            .ok_or("Missing manifest directory")?
            .join(&relative);
        // A pre-existing reserved path must not be silently replaced.
        if destination.exists() {
            return Err(format!(
                "Workspace package file staging collision: {}",
                destination.display()
            )
            .into());
        }
        fs::create_dir_all(destination.parent().ok_or("Missing asset directory")?)?;
        fs::copy(&source, &destination).map_err(|error| {
            format!(
                "Failed to stage inherited {field} {}: {error}",
                source.display()
            )
        })?;
        manifest["package"][field] = toml::Value::String(relative.to_string_lossy().into_owned());
    }
    Ok(())
}

fn sanitized_manifest(
    workspace_root: &Path,
    packages: &BTreeMap<String, PublishPackageInfo>,
    manifest_path: &Path,
) -> Result<toml::Value, BoxError> {
    let workspace_toml = read_toml(&workspace_root.join("Cargo.toml"))?;
    let mut manifest = read_toml(manifest_path)?;
    let package_versions = packages
        .iter()
        .map(|(name, package)| (name.clone(), package.version.clone()))
        .collect::<BTreeMap<_, _>>();

    resolve_workspace_package_fields(&mut manifest, &workspace_toml)?;
    resolve_workspace_lints(&mut manifest, &workspace_toml)?;
    sanitize_dependency_sections(&mut manifest, &workspace_toml, &package_versions)?;
    if let Some(table) = manifest.as_table_mut() {
        table.remove("workspace");
        table.remove("patch");
        table.remove("replace");
    }

    if let Some(package) = manifest
        .get_mut("package")
        .and_then(toml::Value::as_table_mut)
    {
        package.remove("workspace");
    }
    Ok(manifest)
}

fn validate_publish_manifests(
    workspace_root: &Path,
    packages: &BTreeMap<String, PublishPackageInfo>,
    package_names: &BTreeSet<String>,
) -> Result<(), BoxError> {
    let mut failures = Vec::new();
    for name in package_names {
        let package = packages
            .get(name)
            .ok_or_else(|| format!("Unknown workspace package '{name}'"))?;
        let manifest_path = package
            .package_path
            .as_ref()
            .ok_or_else(|| format!("Missing package path for '{name}'"))?
            .join("Cargo.toml");
        match sanitized_manifest(workspace_root, packages, &manifest_path) {
            Ok(manifest) => {
                for dependency in missing_publish_dependency_versions(&manifest) {
                    failures.push(format!(
                        "{}@{}: dependency {dependency} does not specify a nonempty version requirement",
                        package.name, package.version
                    ));
                }
            }
            Err(error) => failures.push(format!("{}@{}: {error}", package.name, package.version)),
        }
    }
    if failures.is_empty() {
        return Ok(());
    }
    Err(format!(
        "Publish manifest preflight failed (no packages uploaded):\n  {}\nNormal and build dependencies require versions, including optional and target-specific dependencies. Git/path sources are not substitutes for registry version requirements.",
        failures.join("\n  ")
    )
    .into())
}

fn missing_publish_dependency_versions(manifest: &toml::Value) -> Vec<String> {
    let mut failures = Vec::new();
    let mut check_sections = |table: &toml::Value, prefix: &str| {
        for section in ["dependencies", "build-dependencies"] {
            if let Some(dependencies) = table.get(section).and_then(toml::Value::as_table) {
                for (name, dependency) in dependencies {
                    let version = dependency
                        .as_str()
                        .or_else(|| dependency.get("version").and_then(toml::Value::as_str));
                    if version.is_none_or(|version| version.trim().is_empty()) {
                        failures.push(format!("'{name}' in [{prefix}{section}]"));
                    }
                }
            }
        }
    };
    check_sections(manifest, "");
    if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
        for (target, table) in targets {
            check_sections(table, &format!("target.{target}."));
        }
    }
    failures
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

fn resolve_workspace_lints(
    manifest: &mut toml::Value,
    workspace_toml: &toml::Value,
) -> Result<(), BoxError> {
    if !is_workspace_inherited(manifest.get("lints")) {
        return Ok(());
    }

    let workspace_lints = workspace_toml
        .get("workspace")
        .and_then(|workspace| workspace.get("lints"))
        .ok_or("Missing workspace.lints")?
        .clone();

    if let Some(table) = manifest.as_table_mut() {
        table.insert("lints".to_string(), workspace_lints);
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
    client: &RegistryClient,
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
    fn publish_metadata_keeps_unpublished_test_runner_out_of_library_dependencies() {
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(
            workspace.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"decoder\", \"vectors\", \"runner\"]\nresolver = \"2\"\n",
        )
        .unwrap();
        for (name, extra) in [
            ("decoder", ""),
            ("vectors", "publish = false\n"),
            (
                "runner",
                "publish = false\n[dev-dependencies]\ndecoder = { path = \"../decoder\" }\nvectors = { path = \"../vectors\" }\n",
            ),
        ] {
            let directory = workspace.path().join(name);
            std::fs::create_dir_all(directory.join("src")).unwrap();
            std::fs::write(directory.join("src/lib.rs"), "").unwrap();
            std::fs::write(
                directory.join("Cargo.toml"),
                format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n{extra}"),
            )
            .unwrap();
        }

        let (_, packages) = load_publish_packages(workspace.path()).unwrap();
        assert!(packages["decoder"].publish_dependencies.is_empty());
        assert!(!packages["runner"].publishable);
        assert!(!packages["vectors"].publishable);
        let plan = build_publish_plan(&packages, None).unwrap();
        assert_eq!(plan.publish_order, ["decoder"]);
    }

    #[test]
    fn publish_lockfile_input_is_default_opt_out_and_non_mutating() {
        let root = tempfile::tempdir().unwrap();
        let staging = tempfile::tempdir().unwrap();
        let staged = SanitizedPackage {
            root: staging.path().into(),
            package_path: staging.path().into(),
        };
        seed_publish_lockfile(root.path(), &staged, &PublishConfig::default()).unwrap();
        assert!(!staged.manifest_path().with_file_name("Cargo.lock").exists());
        fs::write(root.path().join("Cargo.lock"), "version = 4\n").unwrap();
        seed_publish_lockfile(
            root.path(),
            &staged,
            &PublishConfig {
                use_lockfile: Some(false),
                ..PublishConfig::default()
            },
        )
        .unwrap();
        assert!(!staging.path().join("Cargo.lock").exists());
        seed_publish_lockfile(root.path(), &staged, &PublishConfig::default()).unwrap();
        assert_eq!(
            fs::read_to_string(staging.path().join("Cargo.lock")).unwrap(),
            "version = 4\n"
        );
        assert_eq!(
            fs::read_to_string(root.path().join("Cargo.lock")).unwrap(),
            "version = 4\n"
        );
    }

    #[test]
    fn publish_manifest_preflight_reports_all_missing_versions_after_sanitization() {
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(
            workspace.path().join("Cargo.toml"),
            r#"[workspace]
[workspace.dependencies]
bmux_keyboard = { git = "https://example.com/bmux", optional = true }
versioned = { version = "1.0.0", git = "https://example.com/valid" }
"#,
        )
        .unwrap();
        let mut packages = BTreeMap::new();
        for name in ["first", "second"] {
            let directory = workspace.path().join(name);
            std::fs::create_dir(&directory).unwrap();
            std::fs::write(
                directory.join("Cargo.toml"),
                format!(
                    r#"[package]
name = "{name}"
version = "0.1.0"
[dependencies]
bmux_keyboard = {{ workspace = true, optional = true }}
versioned = {{ workspace = true }}
plain = "1.0.0"
empty = {{ version = "" }}
[build-dependencies]
local_tool = {{ path = "../local_tool" }}
[dev-dependencies]
local_tests = {{ path = "../local_tests" }}
[target.'cfg(windows)'.dependencies]
renamed = {{ package = "actual_name", git = "https://example.com/windows" }}
[target.'cfg(unix)'.build-dependencies]
build_tool = {{ git = "https://example.com/build" }}
[target.'cfg(unix)'.dev-dependencies]
ignored = {{ path = "../ignored" }}
"#
                ),
            )
            .unwrap();
            packages.insert(
                name.to_string(),
                package(name, true, []).with_package_path(directory),
            );
        }
        let names = packages.keys().cloned().collect();
        let error = validate_publish_manifests(workspace.path(), &packages, &names)
            .unwrap_err()
            .to_string();
        for name in ["first", "second"] {
            assert!(error.contains(&format!("{name}@0.1.0")));
        }
        for dependency in [
            "bmux_keyboard",
            "empty",
            "local_tool",
            "renamed",
            "build_tool",
        ] {
            assert_eq!(error.matches(&format!("'{dependency}'")).count(), 2);
        }
        assert!(error.contains("target.cfg(windows).dependencies"));
        assert!(error.contains("target.cfg(unix).build-dependencies"));
        for ignored in ["versioned", "plain", "local_tests", "ignored"] {
            assert!(!error.contains(&format!("'{ignored}'")));
        }
        // Preflight must not rewrite the source manifests.
        let original = std::fs::read_to_string(workspace.path().join("first/Cargo.toml")).unwrap();
        assert!(original.contains("workspace = true"));
        assert!(original.contains("[dev-dependencies]"));
    }

    #[test]
    fn publish_manifest_preflight_accepts_inferred_workspace_path_versions() {
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        std::fs::write(
            workspace.path().join("app.toml"),
            "[dependencies]\ncore = { path = \"../core\" }\n",
        )
        .unwrap();
        let packages = BTreeMap::from([("core".to_string(), package("core", true, []))]);
        let manifest = sanitized_manifest(
            workspace.path(),
            &packages,
            &workspace.path().join("app.toml"),
        )
        .unwrap();
        assert_eq!(
            missing_publish_dependency_versions(&manifest),
            Vec::<String>::new()
        );
        assert_eq!(
            manifest["dependencies"]["core"]["version"].as_str(),
            Some("0.1.0")
        );
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
    #[allow(clippy::too_many_lines)]
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

                [workspace.lints.clippy]
                all = "warn"
                pedantic = "warn"
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

                [lints]
                workspace = true

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
        let lints = sanitized_toml.get("lints").unwrap();
        let clippy_lints = lints.get("clippy").unwrap();
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
        assert!(lints.get("workspace").is_none());
        assert_eq!(clippy_lints.get("all").unwrap().as_str(), Some("warn"));
        assert_eq!(clippy_lints.get("pedantic").unwrap().as_str(), Some("warn"));

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
        assert_eq!(target_only.len(), 0);
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
