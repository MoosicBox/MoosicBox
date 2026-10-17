//! Cargo-native packaging preflight for a pending publish queue.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use super::{
    BoxError, PublishConfig, PublishPackageInfo, SanitizedPackage, cargo_color_arg, copy_dir,
    format_package_name, paint, read_toml, sanitize_manifest,
};

mod batch;
mod diagnostics;
mod output;

#[cfg(test)]
mod reuse_tests;

#[cfg(test)]
mod tests;

/// Stage the pending queue together so Cargo can package unpublished dependencies.
/// Versioned path links are staging-only: Cargo removes them from published manifests.
fn prepare_workspace(
    workspace_root: &Path,
    packages: &BTreeMap<String, PublishPackageInfo>,
    pending: &[String],
) -> Result<SanitizedPackage, BoxError> {
    let workspace_root = fs::canonicalize(workspace_root)?;
    audit_cargo_configuration(&workspace_root)?;
    let root = std::env::temp_dir().join(format!(
        "clippier-preflight-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    fs::create_dir_all(&root)?;
    // Install the cleanup guard before copying or sanitizing can fail.
    let staged = SanitizedPackage {
        package_path: root.clone(),
        root,
    };
    let mut paths = BTreeMap::new();
    let mut sources = BTreeMap::new();
    for name in pending {
        let package = &packages[name];
        let source = fs::canonicalize(
            package
                .package_path
                .as_ref()
                .ok_or_else(|| format!("Missing package path for {name}"))?,
        )?;
        let relative = source.strip_prefix(&workspace_root)?;
        // Keep the original package-relative layout, including a root package.
        let destination = staged.root.join("source").join(relative);
        sources.insert(source, destination.clone());
        paths.insert(name.clone(), destination);
    }
    // A parent copy already includes its nested packages. Copy only outermost
    // roots, independent of package-name/order, before sanitizing any manifests.
    for (source, destination) in &sources {
        if !source
            .ancestors()
            .skip(1)
            .any(|parent| sources.contains_key(parent))
        {
            copy_dir(source, destination)?;
        }
    }
    if workspace_root.join(".cargo").exists() {
        copy_dir(&workspace_root.join(".cargo"), &staged.root.join(".cargo"))?;
    }
    for path in paths.values() {
        let manifest_path = path.join("Cargo.toml");
        sanitize_manifest(&workspace_root, packages, &manifest_path)?;
        let mut manifest = read_toml(&manifest_path)?;
        link_pending_dependencies(&mut manifest, &paths, packages)?;
        fs::write(manifest_path, toml::to_string_pretty(&manifest)?)?;
    }
    let manifest = toml::toml! {
        [workspace]
        resolver = "2"
    };
    let mut manifest = toml::Value::Table(manifest);
    let original = read_toml(&workspace_root.join("Cargo.toml"))?;
    if let Some(resolver) = original.get("workspace").and_then(|v| v.get("resolver")) {
        manifest["workspace"]["resolver"] = resolver.clone();
    }
    manifest["workspace"].as_table_mut().unwrap().insert(
        "members".into(),
        toml::Value::Array(
            paths
                .values()
                .map(|p| {
                    toml::Value::String(
                        p.strip_prefix(&staged.root)
                            .unwrap()
                            .to_string_lossy()
                            .into_owned(),
                    )
                })
                .collect(),
        ),
    );
    fs::write(staged.manifest_path(), toml::to_string_pretty(&manifest)?)?;
    Ok(staged)
}

fn link_pending_dependencies(
    manifest: &mut toml::Value,
    paths: &BTreeMap<String, PathBuf>,
    packages: &BTreeMap<String, PublishPackageInfo>,
) -> Result<(), BoxError> {
    for section in ["dependencies", "build-dependencies"] {
        if let Some(dependencies) = manifest
            .get_mut(section)
            .and_then(toml::Value::as_table_mut)
        {
            for (alias, dependency) in dependencies {
                let name = dependency
                    .get("package")
                    .and_then(toml::Value::as_str)
                    .unwrap_or(alias);
                let Some(path) = paths.get(name) else {
                    continue;
                };
                // Do not redirect explicitly selected alternate registries or git dependencies.
                if dependency.get("registry").is_some() || dependency.get("git").is_some() {
                    continue;
                }
                let requirement = dependency
                    .as_str()
                    .or_else(|| dependency.get("version").and_then(toml::Value::as_str));
                if let Some(requirement) = requirement {
                    let requirement = cargo_metadata::semver::VersionReq::parse(requirement)?;
                    let version = cargo_metadata::semver::Version::parse(&packages[name].version)?;
                    if !requirement.matches(&version) {
                        continue;
                    }
                }
                if let Some(version) = dependency.as_str() {
                    let mut table = toml::map::Map::new();
                    table.insert("version".into(), toml::Value::String(version.to_string()));
                    *dependency = toml::Value::Table(table);
                }
                if let Some(table) = dependency.as_table_mut() {
                    table.insert(
                        "path".into(),
                        toml::Value::String(path.to_string_lossy().into_owned()),
                    );
                }
            }
        }
    }
    if let Some(targets) = manifest
        .get_mut("target")
        .and_then(toml::Value::as_table_mut)
    {
        for (_, target) in targets.iter_mut() {
            link_pending_dependencies(target, paths, packages)?;
        }
    }
    Ok(())
}

#[cfg(test)]
fn package_command(staged: &SanitizedPackage, config: &PublishConfig) -> Command {
    package_command_with_cache(staged, config, None)
}

fn package_command_with_cache(
    staged: &SanitizedPackage,
    config: &PublishConfig,
    verification_cache: Option<&Path>,
) -> Command {
    let mut command = Command::new("cargo");
    command
        .arg("package")
        .arg("--registry")
        .arg("crates-io")
        .arg("--workspace")
        .arg("--manifest-path")
        .arg(staged.manifest_path())
        .arg("--color")
        .arg(cargo_color_arg(config.color))
        .current_dir(staged.root())
        .env("CARGO_TARGET_DIR", staged.target_dir());
    if let Some(cache) = verification_cache.filter(|_| config.verify) {
        command.env("CARGO_BUILD_BUILD_DIR", cache);
    }
    if !config.verify {
        command.arg("--no-verify");
    }
    if config.allow_dirty {
        command.arg("--allow-dirty");
    }
    command
}

pub(super) fn validate(
    workspace_root: &Path,
    packages: &BTreeMap<String, PublishPackageInfo>,
    pending: &[String],
    config: &PublishConfig,
) -> Result<(), BoxError> {
    validate_with_failures(workspace_root, packages, pending, config, Vec::new())
}

pub(super) fn validate_with_failures(
    workspace_root: &Path,
    packages: &BTreeMap<String, PublishPackageInfo>,
    pending: &[String],
    config: &PublishConfig,
    failures: Vec<String>,
) -> Result<(), BoxError> {
    if pending.is_empty() {
        return if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("\n\n").into())
        };
    }
    if config.fail_fast || pending.len() == 1 {
        return validate_independent(workspace_root, packages, pending, config, failures);
    }
    let packaging_config = PublishConfig {
        verify: false,
        ..config.clone()
    };
    // Keep fail-fast attribution and ordering unchanged. Otherwise Cargo's native
    // batch prepares each archive once; failed/unsafe batches get fresh per-root
    // resolution, never partially credited artifacts or a synthetic root failure.
    eprintln!(
        "Preflight packaging batch: {} pending packages (no compilation)",
        pending.len()
    );
    let packaging_passed =
        match batch::validate(workspace_root, packages, pending, &packaging_config) {
            Ok(()) => true,
            Err(error) => {
                eprintln!(
                    "Preflight batch not credited: {error}; falling back to independent checks"
                );
                false
            }
        };
    if !packaging_passed {
        validate_independent(
            workspace_root,
            packages,
            pending,
            &packaging_config,
            failures,
        )?;
    } else if !failures.is_empty() {
        return collect_package_checks(packages, pending, &packaging_config, failures, |_| Ok(()));
    }
    if config.verify {
        eprintln!("Packaging/resolution complete; starting independent compilation verification");
        validate_independent(workspace_root, packages, pending, config, Vec::new())?;
    }
    Ok(())
}

fn validate_independent(
    workspace_root: &Path,
    packages: &BTreeMap<String, PublishPackageInfo>,
    pending: &[String],
    config: &PublishConfig,
    failures: Vec<String>,
) -> Result<(), BoxError> {
    let mut diagnostics = diagnostics::Diagnostics::default();
    // Own only a run-scoped cache. Cargo's build directory can also contain its
    // extracted package work area; sources/manifests/lockfiles are still freshly
    // staged and Cargo re-extracts each archive before verifying it.
    let verification_cache = config.verify.then(verification_cache).transpose()?;
    collect_package_checks(
        packages,
        pending,
        config,
        failures,
        |closure| match validate_package(
            workspace_root,
            packages,
            closure,
            config,
            verification_cache.as_ref().map(SanitizedPackage::root),
        ) {
            Ok(()) => Ok(()),
            Err(error) if !config.fail_fast => {
                let findings =
                    diagnostics.investigate(workspace_root, packages, pending, closure, config);
                Err(format!("Full-package check: {error}\n{}", findings.join("\n\n")).into())
            }
            Err(error) => Err(error),
        },
    )
}

fn collect_package_checks(
    packages: &BTreeMap<String, PublishPackageInfo>,
    pending: &[String],
    config: &PublishConfig,
    mut failures: Vec<String>,
    mut check: impl FnMut(&[String]) -> Result<(), BoxError>,
) -> Result<(), BoxError> {
    let pending_set = pending.iter().cloned().collect::<BTreeSet<_>>();
    let mut failed = BTreeSet::new();
    let mut passed = BTreeSet::new();
    for (index, name) in pending.iter().enumerate() {
        let mut closure = BTreeSet::new();
        pending_dependencies(name, packages, &pending_set, &mut closure);
        let blocked_by = closure.intersection(&failed).cloned().collect::<Vec<_>>();
        eprintln!(
            "Preflight {}/{}: {name} (packaging/resolution; compilation {})",
            index + 1,
            pending.len(),
            if config.verify {
                "enabled"
            } else {
                "not requested"
            }
        );
        // Separate workspaces prevent one broken dependency graph from hiding
        // errors in independent packages. Include pending dependencies so Cargo
        // can stage their unpublished versions natively, without registry patches.
        let result = check(&closure.into_iter().collect::<Vec<_>>());
        match result {
            Ok(()) => {
                passed.insert(name.clone());
                eprintln!(
                    "  {}: {}",
                    paint(config.color, "35", "validated (not uploaded)"),
                    format_package_name(config.color, name, &packages[name].version),
                );
            }
            Err(error) => {
                let context = if blocked_by.is_empty() {
                    "failed".to_string()
                } else {
                    format!(
                        "potentially blocked by previously failed pending dependencies: {}",
                        blocked_by.join(", ")
                    )
                };
                failures.push(format!(
                    "{name}@{} ({context}):\n{error}",
                    packages[name].version
                ));
                failed.insert(name.clone());
                if config.fail_fast {
                    break;
                }
            }
        }
    }
    if failures.is_empty() {
        return Ok(());
    }
    let summary = failure_summary(
        packages,
        pending,
        config,
        &passed,
        &failed,
        failures.len() - failed.len(),
    );
    Err(format!(
        "Publish preflight failed (no packages uploaded):\n\n{}\n\n{summary}",
        failures.join("\n\n"),
    )
    .into())
}

fn failure_summary(
    packages: &BTreeMap<String, PublishPackageInfo>,
    pending: &[String],
    config: &PublishConfig,
    passed: &BTreeSet<String>,
    failed: &BTreeSet<String>,
    additional_failures: usize,
) -> String {
    let color = config.color;
    let not_checked = pending.len() - passed.len() - failed.len();
    let mut lines = vec![
        paint(
            color,
            "31;1",
            "Publish preflight summary (no packages uploaded)",
        ),
        paint(
            color,
            "90",
            if not_checked > 0 {
                "Stopped early (--fail-fast)."
            } else {
                "Checked every pending package."
            },
        ),
        String::new(),
        paint(color, "1", "Preflight results:"),
    ];
    for name in pending {
        let package = &packages[name];
        let (code, label) = if failed.contains(name) {
            ("31", "failed/blocked")
        } else if passed.contains(name) {
            ("35", "validated (not uploaded)")
        } else {
            ("90", "not checked")
        };
        lines.push(format!(
            "  {}: {}",
            paint(color, code, label),
            format_package_name(color, name, &package.version),
        ));
    }
    lines.extend([
        String::new(),
        paint(color, "1", "Summary:"),
        format!("  {}", paint(color, "35", &format!("Packaging validated: {}", passed.len()))),
        format!("  {}", paint(color, "31", &format!("failed/blocked: {}", failed.len()))),
        format!("  {}", paint(color, "90", &format!("not checked: {not_checked}"))),
        format!("  {}", paint(color, "31", &format!("Additional validation failures: {additional_failures}"))),
        String::new(),
        paint(color, "90", "Full-package checks are authoritative; dependency probes are supplemental diagnostics."),
        paint(color, "90", "Cargo can report only the first resolver error in each dependency graph; blocked checks may expose further issues after their dependencies are fixed."),
        paint(color, "31;1", "Result: FAILED (no packages uploaded)."),
    ]);
    lines.join("\n")
}

fn pending_dependencies(
    name: &str,
    packages: &BTreeMap<String, PublishPackageInfo>,
    pending: &BTreeSet<String>,
    closure: &mut BTreeSet<String>,
) {
    if !closure.insert(name.to_string()) {
        return;
    }
    for dependency in &packages[name].publish_dependencies {
        if pending.contains(dependency) {
            pending_dependencies(dependency, packages, pending, closure);
        }
    }
}

/// Own the run-scoped Cargo build directory; manifests and lockfiles are freshly
/// staged for every check and packaged contents are re-extracted by Cargo.
fn verification_cache() -> Result<SanitizedPackage, BoxError> {
    let root = std::env::temp_dir().join(format!(
        "clippier-verification-cache-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    fs::create_dir(&root)?;
    Ok(SanitizedPackage {
        package_path: root.clone(),
        root,
    })
}

fn validate_package(
    workspace_root: &Path,
    packages: &BTreeMap<String, PublishPackageInfo>,
    pending: &[String],
    config: &PublishConfig,
    verification_cache: Option<&Path>,
) -> Result<(), BoxError> {
    let started = Instant::now();
    let staged = prepare_workspace(workspace_root, packages, pending)?;
    let staged_in = started.elapsed();
    let command = package_command_with_cache(&staged, config, verification_cache);
    let output = output::run(command, config.color, &pending.join(", "))?;
    eprintln!(
        "Preflight independent check: {} package attempts; staging {:.2}s, Cargo {:.2}s; compilation {}",
        pending.len(),
        staged_in.as_secs_f64(),
        started.elapsed().saturating_sub(staged_in).as_secs_f64(),
        if config.verify { "enabled" } else { "disabled" }
    );
    if !output.status.success() {
        return Err(format!(
            "Cargo packaging/resolution failed with status {}:\n{}",
            output.status, output.stderr
        )
        .into());
    }
    Ok(())
}

// Cargo configuration is inherited from directories and CARGO_HOME. Overrides can
// conceal registry failures or make the preflight resolve a different source.
pub(super) fn audit_cargo_configuration(workspace_root: &Path) -> Result<(), BoxError> {
    let mut directories = workspace_root
        .ancestors()
        .map(|p| p.join(".cargo"))
        .collect::<BTreeSet<_>>();
    for parent in std::env::temp_dir().ancestors() {
        directories.insert(parent.join(".cargo"));
    }
    if let Some(home) = std::env::var_os("CARGO_HOME") {
        directories.insert(PathBuf::from(home));
    } else if let Some(home) = std::env::var_os("HOME") {
        directories.insert(PathBuf::from(home).join(".cargo"));
    }
    for directory in directories {
        for file in ["config", "config.toml"] {
            let path = directory.join(file);
            if !path.exists() {
                continue;
            }
            let config = read_toml(&path)?;
            if ["source", "patch", "paths"]
                .iter()
                .any(|key| config.get(key).is_some())
                || config
                    .get("registries")
                    .and_then(|v| v.get("crates-io"))
                    .and_then(|v| v.get("index"))
                    .is_some()
            {
                return Err(format!("Cargo source overrides in {} prevent registry-faithful publish validation; remove them before publishing", path.display()).into());
            }
        }
    }
    for (key, _) in std::env::vars_os() {
        let key = key.to_string_lossy();
        if key.starts_with("CARGO_SOURCE_")
            || key.starts_with("CARGO_PATCH_")
            || key == "CARGO_REGISTRIES_CRATES_IO_INDEX"
        {
            return Err(format!(
                "Cargo source override {key} prevents registry-faithful publish validation"
            )
            .into());
        }
    }
    Ok(())
}
