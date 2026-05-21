//! Cargo workspace version bumping support.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use cargo_metadata::{MetadataCommand, PackageId};
use clap::ValueEnum;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::OutputType;

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

    let dry_run = config.dry_run;
    let mut changed_files = BTreeSet::new();

    if update_workspace_version
        && update_manifest_file(&workspace_manifest, dry_run, |contents| {
            update_workspace_package_version(contents, &new_version)
        })?
    {
        changed_files.insert(workspace_manifest.clone());
    }

    for package in packages.values() {
        let package_selected = selected_names_set.contains(&package.name);
        if update_manifest_file(&package.manifest_path, dry_run, |contents| {
            let mut updated = contents.to_string();
            let changed_package_version = package_selected
                && !package.inherits_workspace_version
                && update_package_version_in_contents(&mut updated, &new_version);

            let changed_dependency_versions = update_dependency_versions_in_contents(
                &mut updated,
                &selected_names_set,
                &new_version,
                DependencyUpdateMode::PathOnly,
            );

            if changed_package_version || changed_dependency_versions {
                Some(updated)
            } else {
                None
            }
        })? {
            changed_files.insert(package.manifest_path.clone());
        }
    }

    if update_manifest_file(&workspace_manifest, dry_run, |contents| {
        let mut updated = contents.to_string();
        if update_dependency_versions_in_contents(
            &mut updated,
            &selected_names_set,
            &new_version,
            DependencyUpdateMode::WorkspaceDependencies,
        ) {
            Some(updated)
        } else {
            None
        }
    })? {
        changed_files.insert(workspace_manifest);
    }

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

fn normalize_workspace_root(path: &Path) -> PathBuf {
    if path.file_name().is_some_and(|name| name == "Cargo.toml") {
        path.parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    } else {
        path.to_path_buf()
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

fn update_manifest_file(
    path: &Path,
    dry_run: bool,
    update: impl FnOnce(&str) -> Option<String>,
) -> Result<bool, BoxError> {
    let contents = fs::read_to_string(path)?;
    let Some(updated) = update(&contents) else {
        return Ok(false);
    };

    if updated == contents {
        Ok(false)
    } else {
        if !dry_run {
            fs::write(path, updated)?;
        }
        Ok(true)
    }
}

fn update_workspace_package_version(contents: &str, new_version: &str) -> Option<String> {
    update_version_key_in_section(contents, "workspace.package", new_version)
}

fn update_package_version_in_contents(contents: &mut String, new_version: &str) -> bool {
    update_version_key_in_section(contents, "package", new_version).is_some_and(|updated| {
        *contents = updated;
        true
    })
}

fn update_version_key_in_section(
    contents: &str,
    target_section: &str,
    new_version: &str,
) -> Option<String> {
    let mut current_section = String::new();
    let mut changed = false;
    let lines = contents
        .lines()
        .map(|line| {
            if let Some(section) = parse_section(line) {
                current_section = section;
                return line.to_string();
            }

            if current_section == target_section
                && line_key(line).is_some_and(|key| key == "version")
                && let Some(updated) = replace_version_literal(line, new_version)
            {
                changed = true;
                return updated;
            }

            line.to_string()
        })
        .collect::<Vec<_>>();

    if changed {
        Some(join_lines_preserving_trailing_newline(&lines, contents))
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DependencyUpdateMode {
    PathOnly,
    WorkspaceDependencies,
}

fn update_dependency_versions_in_contents(
    contents: &mut String,
    selected_package_names: &BTreeSet<String>,
    new_version: &str,
    mode: DependencyUpdateMode,
) -> bool {
    let mut current_section = String::new();
    let mut changed = false;
    let lines = contents
        .lines()
        .map(|line| {
            if let Some(section) = parse_section(line) {
                current_section = section;
                return line.to_string();
            }

            if !is_dependency_section(&current_section, mode) {
                return line.to_string();
            }

            if dependency_line_targets_package(line, selected_package_names, mode)
                && let Some(updated) = replace_version_literal(line, new_version)
            {
                changed = true;
                return updated;
            }

            line.to_string()
        })
        .collect::<Vec<_>>();

    if changed {
        *contents = join_lines_preserving_trailing_newline(&lines, contents);
    }

    changed
}

fn is_dependency_section(section: &str, mode: DependencyUpdateMode) -> bool {
    match mode {
        DependencyUpdateMode::WorkspaceDependencies => section == "workspace.dependencies",
        DependencyUpdateMode::PathOnly => {
            section == "dependencies"
                || section == "dev-dependencies"
                || section == "build-dependencies"
                || section.ends_with(".dependencies")
                || section.ends_with(".dev-dependencies")
                || section.ends_with(".build-dependencies")
        }
    }
}

fn dependency_line_targets_package(
    line: &str,
    selected_package_names: &BTreeSet<String>,
    mode: DependencyUpdateMode,
) -> bool {
    let Some(key) = line_key(line) else {
        return false;
    };

    if !line.contains("version") {
        return false;
    }

    if mode == DependencyUpdateMode::PathOnly && !line.contains("path") {
        return false;
    }

    if selected_package_names.contains(key) {
        return true;
    }

    inline_table_string_value(line, "package")
        .is_some_and(|package| selected_package_names.contains(package.as_str()))
}

fn parse_section(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return None;
    }

    Some(
        trimmed
            .trim_start_matches('[')
            .trim_end_matches(']')
            .trim_matches('[')
            .trim_matches(']')
            .to_string(),
    )
}

fn line_key(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') {
        return None;
    }

    trimmed
        .split_once('=')
        .map(|(key, _value)| key.trim())
        .filter(|key| !key.is_empty())
}

fn replace_version_literal(line: &str, new_version: &str) -> Option<String> {
    let regex = Regex::new(r#"(version\s*=\s*)"[^"]+""#).expect("valid version regex");
    if regex.is_match(line) {
        Some(
            regex
                .replace(line, format!("$1\"{new_version}\""))
                .to_string(),
        )
    } else {
        None
    }
}

fn inline_table_string_value(line: &str, key: &str) -> Option<String> {
    let regex = Regex::new(&format!(r#"{}\s*=\s*"([^"]+)""#, regex::escape(key)))
        .expect("valid inline table regex");
    regex
        .captures(line)
        .and_then(|captures| captures.get(1))
        .map(|capture| capture.as_str().to_string())
}

fn join_lines_preserving_trailing_newline(lines: &[String], original: &str) -> String {
    let mut joined = lines.join("\n");
    if original.ends_with('\n') {
        joined.push('\n');
    }
    joined
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

    #[test]
    fn update_workspace_package_version_preserves_manifest_shape() {
        let manifest = r#"
[workspace.package]
edition = "2024"
version    = "0.2.0"

[workspace.dependencies]
foo = { version = "0.2.0", path = "packages/foo" }
"#;

        let updated = update_workspace_package_version(manifest, "0.3.0").unwrap();

        assert!(updated.contains("version    = \"0.3.0\""));
        assert!(updated.contains("foo = { version = \"0.2.0\""));
    }

    #[test]
    fn update_dependency_versions_updates_selected_path_dependencies() {
        let mut manifest = r#"
[dependencies]
foo = { version = "0.2.0", path = "../foo" }
bar = { version = "0.2.0", path = "../bar", package = "foo" }
serde = { version = "1.0.0" }
"#
        .to_string();
        let selected = BTreeSet::from(["foo".to_string()]);

        assert!(update_dependency_versions_in_contents(
            &mut manifest,
            &selected,
            "0.3.0",
            DependencyUpdateMode::PathOnly,
        ));

        assert!(manifest.contains("foo = { version = \"0.3.0\""));
        assert!(manifest.contains("bar = { version = \"0.3.0\""));
        assert!(manifest.contains("serde = { version = \"1.0.0\""));
    }
}
