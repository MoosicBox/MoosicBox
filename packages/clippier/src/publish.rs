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
    /// Package directory containing `Cargo.toml`.
    pub package_path: Option<PathBuf>,
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
        }
    }

    #[must_use]
    fn with_package_path(mut self, package_path: PathBuf) -> Self {
        self.package_path = Some(package_path);
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
        "Loading workspace metadata from {}",
        workspace_root.display()
    );
    let packages = load_publish_packages(&workspace_root)?;
    let client = crates_io_client()?;

    let initial_plan = build_publish_plan(&packages, config.packages.as_deref())?;
    eprintln!(
        "Planned {} publishable package(s), {} publish-disabled package(s)",
        initial_plan.publish_order.len(),
        initial_plan.publish_disabled.len()
    );
    let mut reports = Vec::new();
    let mut already_published = BTreeSet::new();
    let mut needs_publish = BTreeSet::new();

    for name in &initial_plan.publish_disabled {
        if let Some(package) = packages.get(name) {
            eprintln!(
                "Skipping {}@{} (publish disabled)",
                package.name, package.version
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

        eprintln!("Checking {}@{} on crates.io", package.name, package.version);
        if crate_version_exists(&client, &package.name, &package.version).await? {
            eprintln!(
                "Skipping {}@{} (already published)",
                package.name, package.version
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

    let packages_to_publish = filter_packages_to_publish(&packages, &needs_publish);
    let final_plan = build_publish_plan(&packages_to_publish, None)?;

    for name in final_plan.publish_order {
        let package = packages
            .get(&name)
            .ok_or_else(|| format!("Unknown workspace package '{name}'"))?;

        if config.dry_run {
            eprintln!("Would publish {}@{}", package.name, package.version);
            reports.push(PublishPackageReport::new(
                &package.name,
                &package.version,
                PublishPackageStatus::DryRun,
            ));
            continue;
        }

        eprintln!("Publishing {}@{}", package.name, package.version);
        match publish_package(&workspace_root, &packages, package, &config)? {
            PublishAttempt::AlreadyPublished => {
                eprintln!(
                    "Skipping {}@{} (already published)",
                    package.name, package.version
                );
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

                eprintln!("Published {}@{}", package.name, package.version);
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
            .with_package_path(package_path),
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
    packages: &BTreeMap<String, PublishPackageInfo>,
    package: &PublishPackageInfo,
    config: &PublishConfig,
) -> Result<PublishAttempt, BoxError> {
    let sanitized = SanitizedPackage::prepare(workspace_root, packages, package)?;

    let mut command = Command::new("cargo");
    command
        .arg("publish")
        .arg("--manifest-path")
        .arg(sanitized.manifest_path())
        .current_dir(sanitized.root())
        .env("CARGO_TARGET_DIR", sanitized.target_dir());

    if !config.verify {
        command.arg("--no-verify");
    }
    if config.allow_dirty {
        command.arg("--allow-dirty");
    }

    let output = run_streaming_command(command)?;
    if output.status.success() {
        return Ok(PublishAttempt::Published);
    }

    if output.stderr.contains("already exists") {
        return Ok(PublishAttempt::AlreadyPublished);
    }

    Err(format!(
        "cargo publish failed for {}@{} with status {} (cargo output was streamed above)",
        package.name, package.version, output.status
    )
    .into())
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
                serde = { workspace = true, features = ["derive"] }

                [dev-dependencies]
                test-utils = { workspace = true }

                [target.'cfg(unix)'.dev-dependencies]
                test-utils = { workspace = true }
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
        let serde = dependencies.get("serde").unwrap();

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
        assert_eq!(serde.get("version").unwrap().as_str(), Some("1.0.228"));

        fs::remove_dir_all(root).unwrap();
    }
}
