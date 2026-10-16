//! Supplemental dependency diagnostics. These never replace the full-package check.

#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
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

use super::super::registry::index_path;

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

// Bound subprocesses, filesystem staging and network requests without multiplying
// Cargo homes/downloads. Cargo may still serialize on its shared registry locks.
const PROBE_WORKERS: usize = 4;

/// Dynamically schedule bounded work; merge results in input order, never completion order.
fn parallel_ordered<T: Sync, R: Send>(
    jobs: &[T],
    workers: usize,
    run: impl Fn(&T) -> R + Sync,
) -> Result<Vec<R>, BoxError> {
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        let handles = (0..workers.max(1).min(jobs.len()))
            .map(|_| {
                scope.spawn(|| {
                    let mut results = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(job) = jobs.get(index) else { break };
                        results.push((index, run(job)));
                    }
                    results
                })
            })
            .collect::<Vec<_>>();
        let mut results = Vec::with_capacity(jobs.len());
        let mut panicked = false;
        for handle in handles {
            match handle.join() {
                Ok(batch) => results.extend(batch),
                Err(_) => panicked = true,
            }
        }
        if panicked {
            return Err("Dependency diagnostic worker panicked".into());
        }
        results.sort_unstable_by_key(|(index, _)| *index);
        Ok(results.into_iter().map(|(_, result)| result).collect())
    })
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
        let started = Instant::now();
        let pending_set = pending.iter().cloned().collect::<BTreeSet<_>>();
        let mut jobs = Vec::new();
        let mut scheduled = BTreeSet::new();
        let mut entries = Vec::new();
        let mut cache_hits = 0;
        for dependency in dependencies(&manifest) {
            let probe = probe_manifest(&manifest, &dependency);
            let mut needed = BTreeSet::new();
            if dependency.crates_io() && pending_set.contains(dependency.name()) {
                let requirement = dependency
                    .requirement()
                    .ok_or("Missing version requirement")?;
                if VersionReq::parse(requirement)?.matches(&cargo_metadata::semver::Version::parse(
                    &packages[dependency.name()].version,
                )?) {
                    pending_dependencies(dependency.name(), packages, &pending_set, &mut needed);
                }
            }
            let key = format!("{}\n{:?}", toml::to_string(&probe)?, needed);
            if !self.probes.contains_key(&key) && scheduled.insert(key.clone()) {
                eprintln!(
                    "  Dependency probe queued: {name} -> {}",
                    dependency.label()
                );
                jobs.push((
                    key.clone(),
                    dependency.clone(),
                    probe,
                    needed.iter().cloned().collect::<Vec<_>>(),
                ));
            } else {
                cache_hits += 1;
            }
            entries.push((key, dependency, !needed.is_empty()));
        }
        let results = parallel_ordered(&jobs, PROBE_WORKERS, |(_, dependency, probe, needed)| {
            run_probe(
                workspace_root,
                packages,
                needed,
                dependency,
                probe.clone(),
                config,
            )
            .map_err(|error| error.to_string())
        })?;
        for ((key, _, _, _), result) in jobs.iter().zip(results) {
            self.probes.insert(key.clone(), result);
        }
        let index_fetches = self.prefetch_indexes(&entries)?;
        eprintln!(
            "  Dependency diagnostics: {} probes, {cache_hits} cache hits, {} index fetches; up to {PROBE_WORKERS} workers; {:.2}s",
            jobs.len(),
            index_fetches,
            started.elapsed().as_secs_f64()
        );
        Ok(self.collect_findings(name, entries))
    }

    fn collect_findings(
        &mut self,
        name: &str,
        entries: Vec<(String, Dependency, bool)>,
    ) -> Vec<String> {
        let mut findings = Vec::new();
        for (key, dependency, staged_pending) in entries {
            let result = &self.probes[&key];
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
        findings
    }

    fn prefetch_indexes(
        &mut self,
        entries: &[(String, Dependency, bool)],
    ) -> Result<usize, BoxError> {
        // Fetch only failed direct crates.io requirements. Deduplicate by crate, not
        // requirement: one immutable index response serves every requirement this run.
        let index_names = entries
            .iter()
            .filter(|(key, dependency, staged_pending)| {
                self.probes[key].is_err()
                    && !staged_pending
                    && dependency.crates_io()
                    && dependency.requirement().is_some()
                    && !self.indexes.contains_key(dependency.name())
            })
            .map(|(_, dependency, _)| dependency.name().to_string())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if !index_names.is_empty() {
            // Construct AND drop the blocking client off the caller's async runtime.
            let indexes = std::thread::scope(|scope| {
                scope
                    .spawn(|| {
                        let client = reqwest::blocking::Client::builder()
                            .timeout(Duration::from_secs(15))
                            .user_agent(concat!("clippier/", env!("CARGO_PKG_VERSION")))
                            .build()
                            .map_err(|error| error.to_string());
                        parallel_ordered(&index_names, PROBE_WORKERS, |name| match &client {
                            Ok(client) => {
                                Self::fetch_index(client, name).map_err(|error| error.to_string())
                            }
                            Err(error) => Err(error.clone()),
                        })
                    })
                    .join()
                    .map_err(|_| "Registry-index diagnostic thread panicked")?
            })?;
            self.indexes
                .extend(index_names.iter().cloned().zip(indexes));
        }
        Ok(index_names.len())
    }

    fn audit(&self, name: &str, requirement: &str) -> Result<Option<String>, BoxError> {
        match &self.indexes[name] {
            Ok(versions) => audit_requirement(requirement, versions),
            Err(error) => Err(error.clone().into()),
        }
    }

    fn fetch_index(
        client: &reqwest::blocking::Client,
        name: &str,
    ) -> Result<Vec<IndexVersion>, BoxError> {
        let response = client
            .get(format!("https://index.crates.io/{}", index_path(name)?))
            .send()?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        let text = response.error_for_status()?.text()?;
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).map_err(Into::into))
            .collect()
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
