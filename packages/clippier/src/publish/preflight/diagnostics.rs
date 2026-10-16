//! Supplemental dependency diagnostics. These never replace the full-package check.

#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use cargo_metadata::semver::VersionReq;
use serde::Deserialize;

use super::super::sanitized_manifest;
use super::{
    BoxError, PublishConfig, PublishPackageInfo, SanitizedPackage, audit_cargo_configuration,
    copy_dir, pending_dependencies, prepare_workspace, read_toml,
};

#[derive(Clone, Debug)]
struct Dependency {
    alias: String,
    target: Option<String>,
    section: String,
    specification: toml::Value,
}

impl Dependency {
    fn name(&self) -> &str {
        self.specification
            .get("package")
            .and_then(toml::Value::as_str)
            .unwrap_or(&self.alias)
    }

    fn requirement(&self) -> Option<&str> {
        self.specification.as_str().or_else(|| {
            self.specification
                .get("version")
                .and_then(toml::Value::as_str)
        })
    }

    fn crates_io(&self) -> bool {
        ["registry", "registry-index", "git", "path"]
            .iter()
            .all(|key| self.specification.get(key).is_none())
    }

    fn label(&self) -> String {
        format!(
            "{} [{}{}]",
            self.alias,
            self.target
                .as_ref()
                .map_or_else(String::new, |target| format!("target.{target}.")),
            self.section
        )
    }
}

fn dependencies(manifest: &toml::Value) -> Vec<Dependency> {
    let mut result = Vec::new();
    let mut sections = |table: &toml::Value, target: Option<&str>| {
        for section in ["dependencies", "build-dependencies"] {
            if let Some(entries) = table.get(section).and_then(toml::Value::as_table) {
                result.extend(entries.iter().map(|(alias, specification)| Dependency {
                    alias: alias.clone(),
                    target: target.map(str::to_string),
                    section: section.into(),
                    specification: specification.clone(),
                }));
            }
        }
    };
    sections(manifest, None);
    if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
        for (target, table) in targets {
            sections(table, Some(target));
        }
    }
    result
}

// Preserve root feature forwarding relevant to this dependency, including default,
// named feature chains and weak dep?/feature edges, without referencing removed deps.
fn projected_features(manifest: &toml::Value, alias: &str) -> toml::Value {
    let mut features = manifest
        .get("features")
        .cloned()
        .unwrap_or_else(|| toml::Value::Table(toml::map::Map::new()));
    let names = features
        .as_table()
        .unwrap()
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    for (_, entries) in features.as_table_mut().unwrap().iter_mut() {
        if let Some(entries) = entries.as_array_mut() {
            entries.retain(|entry| {
                let Some(entry) = entry.as_str() else {
                    return false;
                };
                if let Some(name) = entry.strip_prefix("dep:") {
                    return name == alias;
                }
                if let Some((name, _)) = entry.split_once('/') {
                    return name.trim_end_matches('?') == alias;
                }
                names.contains(entry) || entry == alias
            });
        }
    }
    features
}

fn probe_manifest(manifest: &toml::Value, dependency: &Dependency) -> toml::Value {
    let mut probe = toml::Value::Table(toml::toml! {
        [package]
        name = "clippier-dependency-probe"
        version = "0.0.0"
        edition = "2021"
        publish = false
    });
    for field in ["edition", "rust-version"] {
        if let Some(value) = manifest
            .get("package")
            .and_then(|package| package.get(field))
        {
            probe["package"]
                .as_table_mut()
                .unwrap()
                .insert(field.into(), value.clone());
        }
    }
    probe.as_table_mut().unwrap().insert(
        "features".into(),
        projected_features(manifest, &dependency.alias),
    );
    let mut entries = toml::map::Map::new();
    entries.insert(dependency.alias.clone(), dependency.specification.clone());
    let mut sections = toml::map::Map::new();
    sections.insert(dependency.section.clone(), toml::Value::Table(entries));
    if let Some(target) = &dependency.target {
        let mut targets = toml::map::Map::new();
        targets.insert(target.clone(), toml::Value::Table(sections));
        probe
            .as_table_mut()
            .unwrap()
            .insert("target".into(), toml::Value::Table(targets));
    } else {
        probe.as_table_mut().unwrap().extend(sections);
    }
    probe
}

#[derive(Debug, Deserialize)]
struct IndexVersion {
    vers: String,
    yanked: bool,
}

fn index_path(name: &str) -> Result<String, BoxError> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err("Invalid registry crate name".into());
    }
    let name = name.to_ascii_lowercase();
    Ok(match name.len() {
        1 => format!("1/{name}"),
        2 => format!("2/{name}"),
        3 => format!("3/{}/{name}", &name[..1]),
        _ => format!("{}/{}/{name}", &name[..2], &name[2..4]),
    })
}

fn audit_requirement(
    requirement: &str,
    versions: &[IndexVersion],
) -> Result<Option<String>, BoxError> {
    let requirement = VersionReq::parse(requirement)?;
    let mut matching = 0;
    for version in versions {
        if requirement.matches(&cargo_metadata::semver::Version::parse(&version.vers)?) {
            matching += 1;
            if !version.yanked {
                return Ok(None);
            }
        }
    }
    Ok(Some(if matching == 0 {
        format!("no published version matches {requirement}")
    } else {
        format!("all {matching} published version(s) matching {requirement} are yanked")
    }))
}

#[derive(Default)]
pub(super) struct Diagnostics {
    probes: BTreeMap<String, Result<(), String>>,
    indexes: BTreeMap<String, Result<Vec<IndexVersion>, String>>,
    emitted: BTreeSet<String>,
    failed_packages: BTreeSet<String>,
}

impl Diagnostics {
    pub(super) fn investigate(
        &mut self,
        workspace_root: &Path,
        packages: &BTreeMap<String, PublishPackageInfo>,
        pending: &[String],
        closure: &[String],
        config: &PublishConfig,
    ) -> Vec<String> {
        let mut findings = Vec::new();
        if let Err(error) = audit_cargo_configuration(workspace_root) {
            return vec![format!("Unresolved dependency diagnostics: {error}")];
        }
        for name in closure {
            if !self.failed_packages.insert(name.clone()) {
                continue;
            }
            let result = self.investigate_package(workspace_root, packages, pending, name, config);
            match result {
                Ok(package_findings) => findings.extend(package_findings),
                Err(error) => findings.push(format!(
                    "Unresolved dependency diagnostics for {name}: {error}"
                )),
            }
        }
        findings
    }

    fn investigate_package(
        &mut self,
        workspace_root: &Path,
        packages: &BTreeMap<String, PublishPackageInfo>,
        pending: &[String],
        name: &str,
        config: &PublishConfig,
    ) -> Result<Vec<String>, BoxError> {
        let path = packages[name]
            .package_path
            .as_ref()
            .ok_or("Missing package path")?
            .join("Cargo.toml");
        let manifest = sanitized_manifest(workspace_root, packages, &path)?;
        let mut findings = Vec::new();
        for dependency in dependencies(&manifest) {
            let probe = probe_manifest(&manifest, &dependency);
            let mut needed = BTreeSet::new();
            if dependency.crates_io() && pending.iter().any(|name| name == dependency.name()) {
                let requirement = dependency
                    .requirement()
                    .ok_or("Missing version requirement")?;
                if VersionReq::parse(requirement)?.matches(&cargo_metadata::semver::Version::parse(
                    &packages[dependency.name()].version,
                )?) {
                    pending_dependencies(
                        dependency.name(),
                        packages,
                        &pending.iter().cloned().collect(),
                        &mut needed,
                    );
                }
            }
            let staged_pending = !needed.is_empty();
            let key = format!("{}\n{:?}", toml::to_string(&probe)?, needed);
            let result = self.probes.entry(key.clone()).or_insert_with(|| {
                eprintln!("  Dependency probe: {name} -> {}", dependency.label());
                run_probe(
                    workspace_root,
                    packages,
                    &needed.into_iter().collect::<Vec<_>>(),
                    &dependency,
                    probe,
                    config,
                )
                .map_err(|error| error.to_string())
            });
            let probe_failed = result.is_err();
            if let Err(error) = result
                && self.emitted.insert(key)
            {
                findings.push(format!(
                    "Additional dependency-probe failure: {name} -> {}:\n{error}",
                    dependency.label()
                ));
            }
            if probe_failed
                && !staged_pending
                && dependency.crates_io()
                && let Some(requirement) = dependency.requirement()
            {
                let audit_key = format!("index:{}:{requirement}", dependency.name());
                if self.emitted.contains(&audit_key) {
                    continue;
                }
                match self.audit(dependency.name(), requirement) {
                    Ok(Some(issue)) => {
                        self.emitted.insert(audit_key);
                        findings.push(format!(
                            "Registry-index finding: {name} -> {}: {issue}",
                            dependency.label()
                        ));
                    }
                    Err(error) => {
                        self.emitted.insert(audit_key);
                        findings.push(format!(
                            "Unresolved registry-index check: {name} -> {}: {error}",
                            dependency.label()
                        ));
                    }
                    Ok(None) => {}
                }
            }
        }
        Ok(findings)
    }

    fn audit(&mut self, name: &str, requirement: &str) -> Result<Option<String>, BoxError> {
        if !self.indexes.contains_key(name) {
            let result = Self::fetch_index(name).map_err(|error| error.to_string());
            self.indexes.insert(name.into(), result);
        }
        match &self.indexes[name] {
            Ok(versions) => audit_requirement(requirement, versions),
            Err(error) => Err(error.clone().into()),
        }
    }

    fn fetch_index(name: &str) -> Result<Vec<IndexVersion>, BoxError> {
        let name = name.to_string();
        std::thread::spawn(move || {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(15))
                .user_agent(concat!("clippier/", env!("CARGO_PKG_VERSION")))
                .build()?;
            let response = client
                .get(format!("https://index.crates.io/{}", index_path(&name)?))
                .send()?;
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                return Ok(Vec::new());
            }
            let text = response.error_for_status()?.text()?;
            text.lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| serde_json::from_str(line).map_err(Into::into))
                .collect()
        })
        .join()
        .map_err(|_| "Registry-index diagnostic thread panicked")?
    }
}

fn run_probe(
    workspace_root: &Path,
    packages: &BTreeMap<String, PublishPackageInfo>,
    needed: &[String],
    dependency: &Dependency,
    manifest: toml::Value,
    config: &PublishConfig,
) -> Result<(), BoxError> {
    run_probe_with(
        workspace_root,
        packages,
        needed,
        dependency,
        manifest,
        config,
        |_| {},
    )
}

fn run_probe_with(
    workspace_root: &Path,
    packages: &BTreeMap<String, PublishPackageInfo>,
    needed: &[String],
    dependency: &Dependency,
    mut manifest: toml::Value,
    config: &PublishConfig,
    configure: impl FnOnce(&mut Command),
) -> Result<(), BoxError> {
    let staged = if needed.is_empty() {
        let root = std::env::temp_dir().join(format!(
            "clippier-dependency-probe-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        fs::create_dir(&root)?;
        let staged = SanitizedPackage {
            package_path: root.clone(),
            root,
        };
        fs::write(staged.manifest_path(), "[workspace]\nresolver = \"2\"\n")?;
        if workspace_root.join(".cargo").exists() {
            copy_dir(&workspace_root.join(".cargo"), &staged.root.join(".cargo"))?;
        }
        staged
    } else {
        prepare_workspace(workspace_root, packages, needed)?
    };
    if needed.iter().any(|name| name == dependency.name()) {
        let source = fs::canonicalize(
            packages[dependency.name()]
                .package_path
                .as_ref()
                .ok_or("Missing dependency path")?,
        )?;
        let root = fs::canonicalize(workspace_root)?;
        let path = staged.root.join("source").join(source.strip_prefix(root)?);
        let entry = if let Some(target) = &dependency.target {
            &mut manifest["target"][target][&dependency.section][&dependency.alias]
        } else {
            &mut manifest[&dependency.section][&dependency.alias]
        };
        if let Some(version) = entry.as_str() {
            *entry = toml::Value::Table(toml::toml! { version = version });
        }
        entry
            .as_table_mut()
            .ok_or("Invalid dependency entry")?
            .insert(
                "path".into(),
                toml::Value::String(path.to_string_lossy().into_owned()),
            );
    }
    let probe = staged.root.join("dependency-probe");
    fs::create_dir_all(probe.join("src"))?;
    fs::write(probe.join("src/lib.rs"), "")?;
    fs::write(probe.join("Cargo.toml"), toml::to_string_pretty(&manifest)?)?;
    let mut workspace = read_toml(&staged.manifest_path())?;
    let original = read_toml(&workspace_root.join("Cargo.toml"))?;
    if let Some(resolver) = original
        .get("workspace")
        .and_then(|workspace| workspace.get("resolver"))
    {
        workspace["workspace"]
            .as_table_mut()
            .unwrap()
            .insert("resolver".into(), resolver.clone());
    }
    let members = workspace["workspace"]
        .as_table_mut()
        .unwrap()
        .entry("members")
        .or_insert_with(|| toml::Value::Array(Vec::new()));
    members
        .as_array_mut()
        .unwrap()
        .push(toml::Value::String("dependency-probe".into()));
    fs::write(staged.manifest_path(), toml::to_string_pretty(&workspace)?)?;
    let mut command = Command::new("cargo");
    command
        .arg("generate-lockfile")
        .arg("--manifest-path")
        .arg(staged.manifest_path())
        .arg("--color")
        .arg(super::cargo_color_arg(config.color))
        .current_dir(&staged.root)
        .env("CARGO_TARGET_DIR", staged.target_dir());
    configure(&mut command);
    let output = command.output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
    }
}
