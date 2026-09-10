//! Cargo workspace version bumping support.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use cargo_metadata::{MetadataCommand, PackageId};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, TableLike, Value};

use crate::{OutputType, cargo_workspace::normalize_workspace_root};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Version bump kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum VersionBump {
    /// Increment the major version and reset minor/patch to zero.
    Major,
    /// Increment the minor version and reset patch to zero.
    Minor,
    /// Increment the patch version.
    Patch,
    /// Increment or create a prerelease suffix.
    Prerelease,
    /// Remove a prerelease suffix.
    Release,
}

/// Configuration for `clippier version`.
#[derive(Debug, Clone)]
pub struct VersionConfig {
    /// Path to the workspace root or workspace `Cargo.toml`.
    pub workspace_root: PathBuf,
    /// Specific packages to bump.
    pub packages: Option<Vec<String>>,
    /// Only consider packages publishable to crates.io.
    pub publishable_only: bool,
    /// Compute and print the bump plan without writing files.
    pub dry_run: bool,
    /// Version operation.
    pub operation: VersionOperation,
}

/// Version operation.
#[derive(Debug, Clone)]
pub enum VersionOperation {
    /// Set an exact version.
    Set(String),
    /// Bump the current version.
    Bump {
        /// Bump kind.
        kind: VersionBump,
        /// Prerelease identifier for prerelease bumps.
        pre: Option<String>,
    },
}

#[derive(Debug, Clone)]
struct WorkspacePackage {
    name: String,
    version: String,
    manifest_path: PathBuf,
    publishable: bool,
    inherits_workspace_version: bool,
}

/// Version bump report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionReport {
    /// Previous version used as the bump source.
    pub old_version: String,
    /// New version.
    pub new_version: String,
    /// Whether this was a dry run.
    pub dry_run: bool,
    /// Package names selected for the bump.
    pub packages: Vec<String>,
    /// Files that would be or were modified.
    pub files: Vec<String>,
}

impl VersionReport {
    #[must_use]
    fn to_raw_string(&self) -> String {
        let action = if self.dry_run { "Would bump" } else { "Bumped" };
        let mut lines = vec![format!(
            "{action} {} package(s) from {} to {}",
            self.packages.len(),
            self.old_version,
            self.new_version
        )];

        if self.files.is_empty() {
            lines.push("No files changed".to_string());
        } else {
            lines.push("Files:".to_string());
            for file in &self.files {
                lines.push(format!("  {file}"));
            }
        }

        lines.join("\n")
    }
}

/// Handles the `version` command.
///
/// # Errors
///
/// * If workspace metadata cannot be loaded
/// * If package selection is invalid
/// * If versions cannot be parsed
/// * If manifests cannot be read or written
pub fn handle_version_command(
    config: &VersionConfig,
    output: OutputType,
) -> Result<String, BoxError> {
    let workspace_root = normalize_workspace_root(&config.workspace_root);
    let workspace_manifest = workspace_root.join("Cargo.toml");
    let packages = load_workspace_packages(&workspace_root)?;
    let selected_names = select_packages(
        &packages,
        config.packages.as_deref(),
        config.publishable_only,
    )?;
    let old_version = determine_current_version(&workspace_manifest, &packages, &selected_names)?;
    let new_version = match &config.operation {
        VersionOperation::Set(version) => {
            validate_version(version)?;
            version.clone()
        }
        VersionOperation::Bump { kind, pre } => bump_version(&old_version, *kind, pre.as_deref())?,
    };

    validate_partial_workspace_inherited_bump(&packages, &selected_names)?;

    let selected_names_set = selected_names.iter().cloned().collect::<BTreeSet<_>>();
    let all_package_names = packages.keys().cloned().collect::<BTreeSet<_>>();
    let update_workspace_version = selected_names_set == all_package_names
        && workspace_package_version(&workspace_manifest)?.is_some();

    let planned = plan_manifest_updates(
        &workspace_manifest,
        &packages,
        &selected_names_set,
        &new_version,
        update_workspace_version,
    )?;
    // Complete parsing and dependency validation before writing any manifest.
    if !config.dry_run {
        for (path, contents) in &planned {
            fs::write(path, contents)?;
        }
    }
    let changed_files = planned.into_keys().collect::<BTreeSet<_>>();

    let report = VersionReport {
        old_version,
        new_version,
        dry_run: config.dry_run,
        packages: selected_names,
        files: changed_files
            .into_iter()
            .map(|path| path.display().to_string())
            .collect(),
    };

    match output {
        OutputType::Raw => Ok(report.to_raw_string()),
        OutputType::Json => Ok(serde_json::to_string_pretty(&report)?),
    }
}

fn load_workspace_packages(
    workspace_root: &Path,
) -> Result<BTreeMap<String, WorkspacePackage>, BoxError> {
    let mut command = MetadataCommand::new();
    command.current_dir(workspace_root).no_deps();
    let metadata = command.exec()?;
    let workspace_members = metadata
        .workspace_members
        .iter()
        .cloned()
        .collect::<BTreeSet<PackageId>>();
    let mut packages = BTreeMap::new();

    for package in metadata
        .packages
        .iter()
        .filter(|package| workspace_members.contains(&package.id))
    {
        let manifest_path = package.manifest_path.clone().into_std_path_buf();
        let manifest = read_toml(&manifest_path)?;
        let publishable = package
            .publish
            .as_ref()
            .is_none_or(|registries| registries.iter().any(|registry| registry == "crates-io"));
        let inherits_workspace_version = manifest
            .get("package")
            .and_then(|package| package.get("version"))
            .and_then(toml::Value::as_table)
            .and_then(|table| table.get("workspace"))
            .and_then(toml::Value::as_bool)
            .unwrap_or(false);

        packages.insert(
            package.name.to_string(),
            WorkspacePackage {
                name: package.name.to_string(),
                version: package.version.to_string(),
                manifest_path,
                publishable,
                inherits_workspace_version,
            },
        );
    }

    Ok(packages)
}

fn read_toml(path: &Path) -> Result<toml::Value, BoxError> {
    Ok(toml::from_str(&fs::read_to_string(path)?)?)
}

fn select_packages(
    packages: &BTreeMap<String, WorkspacePackage>,
    requested: Option<&[String]>,
    publishable_only: bool,
) -> Result<Vec<String>, BoxError> {
    let mut selected = Vec::new();

    if let Some(requested) = requested {
        for name in requested {
            let package = packages
                .get(name)
                .ok_or_else(|| format!("Unknown workspace package '{name}'"))?;
            if !publishable_only || package.publishable {
                selected.push(name.clone());
            }
        }
    } else {
        selected.extend(
            packages
                .values()
                .filter(|package| !publishable_only || package.publishable)
                .map(|package| package.name.clone()),
        );
    }

    if selected.is_empty() {
        return Err("No workspace packages matched the version bump request".into());
    }

    Ok(selected)
}

fn determine_current_version(
    workspace_manifest: &Path,
    packages: &BTreeMap<String, WorkspacePackage>,
    selected_names: &[String],
) -> Result<String, BoxError> {
    if let Some(version) = workspace_package_version(workspace_manifest)? {
        return Ok(version);
    }

    let versions = selected_names
        .iter()
        .map(|name| {
            packages
                .get(name)
                .map(|package| package.version.clone())
                .ok_or_else(|| format!("Unknown workspace package '{name}'"))
        })
        .collect::<Result<BTreeSet<_>, _>>()?;

    if versions.len() == 1 {
        Ok(versions
            .into_iter()
            .next()
            .expect("version set is not empty"))
    } else {
        Err(format!(
            "Selected packages have multiple versions: {}. Use `clippier version set <version>` instead.",
            versions.into_iter().collect::<Vec<_>>().join(", ")
        )
        .into())
    }
}

fn workspace_package_version(workspace_manifest: &Path) -> Result<Option<String>, BoxError> {
    Ok(read_toml(workspace_manifest)?
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .map(ToString::to_string))
}

fn validate_partial_workspace_inherited_bump(
    packages: &BTreeMap<String, WorkspacePackage>,
    selected_names: &[String],
) -> Result<(), BoxError> {
    if selected_names.len() == packages.len() {
        return Ok(());
    }

    let inherited = selected_names
        .iter()
        .filter(|name| {
            packages
                .get(*name)
                .is_some_and(|package| package.inherits_workspace_version)
        })
        .cloned()
        .collect::<Vec<_>>();

    if inherited.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Cannot bump a subset of packages that inherit workspace.package.version: {}. Bump the whole workspace or set package-specific versions first.",
            inherited.join(", ")
        )
        .into())
    }
}

fn plan_manifest_updates(
    workspace_manifest: &Path,
    packages: &BTreeMap<String, WorkspacePackage>,
    selected: &BTreeSet<String>,
    version: &str,
    update_workspace_version: bool,
) -> Result<BTreeMap<PathBuf, String>, BoxError> {
    let root = workspace_manifest.canonicalize()?;
    let mut manifests = BTreeSet::from([root.clone()]);
    let mut members = BTreeMap::new();
    for package in packages.values() {
        let path = package.manifest_path.canonicalize()?;
        manifests.insert(path.clone());
        members.insert(path, package);
    }
    let mut planned = BTreeMap::new();
    for path in manifests {
        let original = fs::read_to_string(&path)?;
        let mut document = original.parse::<DocumentMut>()?;
        if path == root && update_workspace_version {
            replace_string(&mut document["workspace"]["package"]["version"], version);
        }
        if let Some(package) = members.get(&path)
            && selected.contains(&package.name)
            && !package.inherits_workspace_version
        {
            replace_string(&mut document["package"]["version"], version);
        }
        update_dependency_sections(
            document.as_table_mut(),
            &path,
            &members,
            selected,
            version,
            false,
        )?;
        if let Some(workspace) = document
            .get_mut("workspace")
            .and_then(Item::as_table_like_mut)
        {
            update_dependency_sections(workspace, &path, &members, selected, version, true)?;
        }
        if let Some(targets) = document.get_mut("target").and_then(Item::as_table_like_mut) {
            for (_, target) in targets.iter_mut() {
                if let Some(table) = target.as_table_like_mut() {
                    update_dependency_sections(table, &path, &members, selected, version, false)?;
                }
            }
        }
        let updated = document.to_string();
        if updated != original {
            planned.insert(path, updated);
        }
    }
    Ok(planned)
}

fn replace_string(item: &mut Item, version: &str) {
    if let Some(value) = item.as_value_mut()
        && value.as_str().is_some_and(|old| old != version)
    {
        let decor = value.decor().clone();
        *value = Value::from(version);
        *value.decor_mut() = decor;
    }
}

fn update_dependency_sections(
    table: &mut dyn TableLike,
    manifest: &Path,
    members: &BTreeMap<PathBuf, &WorkspacePackage>,
    selected: &BTreeSet<String>,
    version: &str,
    workspace: bool,
) -> Result<(), BoxError> {
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if workspace && section != "dependencies" {
            continue;
        }
        let Some(dependencies) = table.get_mut(section).and_then(Item::as_table_like_mut) else {
            continue;
        };
        for (key, dependency) in dependencies.iter_mut() {
            let key = key.get();
            let Some(spec) = dependency.as_table_like_mut() else {
                // Preserve the existing workspace registry dependency behavior.
                if workspace && selected.contains(key) {
                    replace_string(dependency, version);
                }
                continue;
            };
            if spec.get("workspace").and_then(Item::as_bool) == Some(true) {
                continue;
            }
            let name = spec.get("package").and_then(Item::as_str).unwrap_or(key);
            let should_update = if let Some(relative) = spec.get("path").and_then(Item::as_str) {
                let target = manifest
                    .parent()
                    .ok_or("Manifest has no parent")?
                    .join(relative)
                    .join("Cargo.toml")
                    .canonicalize()
                    .map_err(|error| {
                        format!(
                            "{}: dependency '{key}' path '{relative}': {error}",
                            manifest.display()
                        )
                    })?;
                // Workspace declarations may be unused aliases without `package`.
                // Identify local members by path; leave Cargo's name validation to Cargo.
                members
                    .get(&target)
                    .is_some_and(|package| selected.contains(&package.name))
            } else {
                workspace && selected.contains(name)
            };
            if should_update && let Some(item) = spec.get_mut("version") {
                replace_string(item, version);
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedVersion {
    major: u64,
    minor: u64,
    patch: u64,
    pre: Option<String>,
}

fn bump_version(version: &str, kind: VersionBump, pre: Option<&str>) -> Result<String, BoxError> {
    let mut parsed = parse_version(version)?;

    match kind {
        VersionBump::Major => {
            parsed.major += 1;
            parsed.minor = 0;
            parsed.patch = 0;
            parsed.pre = None;
        }
        VersionBump::Minor => {
            parsed.minor += 1;
            parsed.patch = 0;
            parsed.pre = None;
        }
        VersionBump::Patch => {
            parsed.patch += 1;
            parsed.pre = None;
        }
        VersionBump::Prerelease => {
            let prefix = pre.unwrap_or("alpha");
            parsed.pre = Some(next_prerelease(parsed.pre.as_deref(), prefix));
        }
        VersionBump::Release => {
            parsed.pre = None;
        }
    }

    Ok(format_version(&parsed))
}

fn validate_version(version: &str) -> Result<(), BoxError> {
    parse_version(version).map(|_| ())
}

fn parse_version(version: &str) -> Result<ParsedVersion, BoxError> {
    let (core, pre) = version
        .split_once('-')
        .map_or((version, None), |(core, pre)| (core, Some(pre.to_string())));
    let mut parts = core.split('.');
    let major = parts
        .next()
        .ok_or_else(|| format!("Invalid version '{version}'"))?
        .parse::<u64>()?;
    let minor = parts
        .next()
        .ok_or_else(|| format!("Invalid version '{version}'"))?
        .parse::<u64>()?;
    let patch = parts
        .next()
        .ok_or_else(|| format!("Invalid version '{version}'"))?
        .parse::<u64>()?;

    if parts.next().is_some() {
        return Err(format!("Invalid version '{version}'").into());
    }

    Ok(ParsedVersion {
        major,
        minor,
        patch,
        pre,
    })
}

fn format_version(version: &ParsedVersion) -> String {
    let core = format!("{}.{}.{}", version.major, version.minor, version.patch);
    version
        .pre
        .as_ref()
        .map_or_else(|| core.clone(), |pre| format!("{core}-{pre}"))
}

fn next_prerelease(current: Option<&str>, prefix: &str) -> String {
    let Some(current) = current else {
        return format!("{prefix}.0");
    };

    current
        .strip_prefix(prefix)
        .and_then(|suffix| suffix.strip_prefix('.'))
        .and_then(|number| number.parse::<u64>().ok())
        .map_or_else(
            || format!("{prefix}.0"),
            |number| format!("{prefix}.{}", number + 1),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_version_updates_semver_parts() {
        assert_eq!(
            bump_version("1.2.3", VersionBump::Major, None).unwrap(),
            "2.0.0"
        );
        assert_eq!(
            bump_version("1.2.3", VersionBump::Minor, None).unwrap(),
            "1.3.0"
        );
        assert_eq!(
            bump_version("1.2.3", VersionBump::Patch, None).unwrap(),
            "1.2.4"
        );
    }

    #[test]
    fn bump_version_handles_prerelease() {
        assert_eq!(
            bump_version("1.2.3", VersionBump::Prerelease, Some("beta")).unwrap(),
            "1.2.3-beta.0"
        );
        assert_eq!(
            bump_version("1.2.3-beta.0", VersionBump::Prerelease, Some("beta")).unwrap(),
            "1.2.3-beta.1"
        );
        assert_eq!(
            bump_version("1.2.3-beta.1", VersionBump::Release, None).unwrap(),
            "1.2.3"
        );
    }

    fn fixture(extra: &str) -> (tempfile::TempDir, VersionConfig) {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("cli")).unwrap();
        fs::write(dir.path().join("cli/main.rs"), "fn main() {}\n").unwrap();
        fs::write(
            dir.path().join("cli/Cargo.toml"),
            r#"[package]
name = "sshenv"
version.workspace = true
[[bin]]
name = "sshenv"
path = "main.rs"
"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            format!(
                r#"[workspace]
members = ["cli"]
resolver = "2"
[workspace.package]
version    = "0.0.1-alpha.3" # retained
{extra}
"#
            ),
        )
        .unwrap();
        let config = VersionConfig {
            workspace_root: dir.path().to_path_buf(),
            packages: None,
            publishable_only: false,
            dry_run: false,
            operation: VersionOperation::Set("0.0.1-alpha.4".into()),
        };
        (dir, config)
    }

    #[test]
    fn updates_unused_aliases_and_preserves_comments() {
        let (dir, config) = fixture(
            r#"
[workspace.dependencies]
"sshenv_cli" = { package = "sshenv", path = "cli/../cli", version = '0.0.1-alpha.3' } # alias
[workspace.dependencies.other]
package = "sshenv"
path = "cli"
version = "0.0.1-alpha.3"
"#,
        );
        handle_version_command(&config, OutputType::Json).unwrap();
        let updated = fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
        assert!(!updated.contains("alpha.3"));
        assert!(updated.contains("# alias"));
        assert!(updated.contains("version    = \"0.0.1-alpha.4\" # retained"));
    }

    #[test]
    fn updates_unused_workspace_dependency_by_path_without_requiring_matching_name() {
        let (dir, mut config) = fixture(
            r#"
[workspace.dependencies]
sshenv_cli = { path = "cli", version = "0.0.1-alpha.3" }
"#,
        );
        let root = dir.path().join("Cargo.toml");
        let member = dir.path().join("cli/Cargo.toml");
        let original = fs::read_to_string(&root).unwrap();
        let original_member = fs::read_to_string(&member).unwrap();
        config.dry_run = true;
        handle_version_command(&config, OutputType::Json).unwrap();
        assert_eq!(fs::read_to_string(&root).unwrap(), original);
        config.dry_run = false;
        handle_version_command(&config, OutputType::Json).unwrap();
        assert_eq!(
            fs::read_to_string(&root).unwrap(),
            original.replace("0.0.1-alpha.3", "0.0.1-alpha.4")
        );
        assert_eq!(fs::read_to_string(&member).unwrap(), original_member);
    }

    #[test]
    fn package_override_takes_precedence_and_registry_members_stay_external() {
        let mut doc = r#"
[dependencies]
foo = { package = "bar", version = "1.0.0" }
bar = "1.0.0"
"#
        .parse::<DocumentMut>()
        .unwrap();
        let original = doc.to_string();
        let selected = BTreeSet::from(["foo".to_string()]);
        update_dependency_sections(
            doc.as_table_mut(),
            Path::new("Cargo.toml"),
            &BTreeMap::new(),
            &selected,
            "2.0.0",
            true,
        )
        .unwrap();
        assert_eq!(doc.to_string(), original);
        let selected = BTreeSet::from(["bar".to_string()]);
        update_dependency_sections(
            doc.as_table_mut(),
            Path::new("Cargo.toml"),
            &BTreeMap::new(),
            &selected,
            "2.0.0",
            false,
        )
        .unwrap();
        assert_eq!(doc.to_string(), original);
        update_dependency_sections(
            doc.as_table_mut(),
            Path::new("Cargo.toml"),
            &BTreeMap::new(),
            &selected,
            "2.0.0",
            true,
        )
        .unwrap();
        assert!(!doc.to_string().contains("1.0.0"));
    }

    #[test]
    fn target_dependencies_and_dry_run() {
        let (dir, mut config) = fixture("");
        let root = dir.path().join("Cargo.toml");
        let member = dir.path().join("cli/Cargo.toml");
        let original = fs::read_to_string(&member).unwrap()
            + r#"
[target.'cfg(unix)'.build-dependencies.self_alias]
package = "sshenv"
path = "."
version = "0.0.1-alpha.3"
"#;
        fs::write(&member, &original).unwrap();
        let original_root = fs::read_to_string(&root).unwrap();
        config.dry_run = true;
        handle_version_command(&config, OutputType::Json).unwrap();
        assert_eq!(fs::read_to_string(&root).unwrap(), original_root);
        assert_eq!(fs::read_to_string(&member).unwrap(), original);
        config.dry_run = false;
        handle_version_command(&config, OutputType::Json).unwrap();
        assert!(
            fs::read_to_string(&member)
                .unwrap()
                .contains("0.0.1-alpha.4")
        );
    }
}
