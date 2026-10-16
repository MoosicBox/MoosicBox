//! Native Cargo batching with a conservative temporary-registry scope guard.

use std::{collections::BTreeSet, fs, io::Read as _, path::Path, time::Instant};

use super::{
    BTreeMap, BoxError, PublishConfig, PublishPackageInfo, SanitizedPackage,
    package_command_with_cache, pending_dependencies, prepare_workspace, run_streaming_command,
};

/// A batch is only credited after every archive has a complete lockfile and none
/// uses a pending candidate outside that package's original dependency closure.
/// Cargo resolves archive lockfiles independently, unlike `cargo build --workspace`.
pub(super) fn validate(
    workspace_root: &Path,
    packages: &BTreeMap<String, PublishPackageInfo>,
    pending: &[String],
    config: &PublishConfig,
) -> Result<(), BoxError> {
    let started = Instant::now();
    let staged = prepare_workspace(workspace_root, packages, pending)?;
    let stage_time = started.elapsed();
    let packaging = Instant::now();
    let mut packaging_config = config.clone();
    packaging_config.verify = false;
    let output =
        run_streaming_command(package_command_with_cache(&staged, &packaging_config, None))?;
    if !output.status.success() {
        return Err(format!(
            "Native batch packaging failed ({}); independent checks required",
            output.status
        )
        .into());
    }
    check_archives(&staged, packages, pending)?;
    eprintln!(
        "Preflight batch: {} packages staged once in {:.2}s; one Cargo invocation, {} archives validated in {:.2}s; compilation not requested in this phase",
        pending.len(),
        stage_time.as_secs_f64(),
        pending.len(),
        packaging.elapsed().as_secs_f64()
    );
    Ok(())
}

pub(super) fn check_archives(
    staged: &SanitizedPackage,
    packages: &BTreeMap<String, PublishPackageInfo>,
    pending: &[String],
) -> Result<(), BoxError> {
    let pending_set = pending.iter().cloned().collect::<BTreeSet<_>>();
    for name in pending {
        let package = &packages[name];
        let root = format!("{name}-{}", package.version);
        let archive = fs::File::open(
            staged
                .target_dir()
                .join("package")
                .join(format!("{root}.crate")),
        )?;
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(archive));
        let mut lock = None;
        for entry in archive.entries()? {
            let mut entry = entry?;
            if entry.path()? == Path::new(&root).join("Cargo.lock") {
                if lock.is_some() || !entry.header().entry_type().is_file() {
                    return Err(format!("Invalid packaged lockfile for {name}").into());
                }
                let mut text = String::new();
                entry
                    .by_ref()
                    .take(16 * 1024 * 1024 + 1)
                    .read_to_string(&mut text)?;
                if text.len() > 16 * 1024 * 1024 {
                    return Err(format!("Packaged lockfile too large for {name}").into());
                }
                lock = Some(toml::from_str::<toml::Value>(&text)?);
            }
        }
        let lock = lock.ok_or_else(|| format!("Missing packaged lockfile for {name}"))?;
        check_lock(name, &lock, packages, &pending_set)?;
    }
    Ok(())
}

fn check_lock(
    name: &str,
    lock: &toml::Value,
    packages: &BTreeMap<String, PublishPackageInfo>,
    pending: &BTreeSet<String>,
) -> Result<(), BoxError> {
    let mut allowed = BTreeSet::new();
    pending_dependencies(name, packages, pending, &mut allowed);
    if !matches!(
        lock.get("version").and_then(toml::Value::as_integer),
        Some(3 | 4)
    ) {
        return Err("Unsupported packaged lockfile version".into());
    }
    let entries = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or("Invalid packaged lockfile package list")?;
    if entries.is_empty() {
        return Err("Empty packaged lockfile".into());
    }
    let mut root_found = false;
    for entry in entries {
        let dependency = entry
            .get("name")
            .and_then(toml::Value::as_str)
            .ok_or("Invalid packaged lockfile dependency name")?;
        let version = entry
            .get("version")
            .and_then(toml::Value::as_str)
            .ok_or("Invalid packaged lockfile dependency version")?;
        cargo_metadata::semver::Version::parse(version)?;
        root_found |= dependency == name
            && version == packages[name].version
            && entry.get("source").is_none();
        if pending.contains(dependency) && !allowed.contains(dependency) {
            return Err(format!("Batch overlay scope differs for {name}: {dependency} is outside its pending dependency closure; independent checks required").into());
        }
    }
    if !root_found {
        return Err(format!("Packaged lockfile missing root identity for {name}").into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outside_pending_candidates_require_independent_checks() {
        let mut packages = BTreeMap::new();
        for name in ["app", "other"] {
            packages.insert(
                name.into(),
                PublishPackageInfo::new(name, "0.1.0", true, BTreeSet::new()),
            );
        }
        let pending = packages.keys().cloned().collect();
        let lock = toml::from_str("version = 4\n[[package]]\nname = 'app'\nversion = '0.1.0'\n[[package]]\nname = 'other'\nversion = '0.0.1'\n").unwrap();
        assert!(
            check_lock("app", &lock, &packages, &pending)
                .unwrap_err()
                .to_string()
                .contains("outside")
        );
        packages
            .get_mut("app")
            .unwrap()
            .publish_dependencies
            .insert("other".into());
        check_lock("app", &lock, &packages, &pending).unwrap();
        assert!(
            check_lock(
                "app",
                &toml::Value::String("invalid".into()),
                &packages,
                &pending
            )
            .is_err()
        );
    }
}
