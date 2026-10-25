//! Isolated Cargo resolution and checked publication with conservative rollback.
use super::{
    BoxError, LockfilePolicy, LockfileStatus, UpgradePlan, UpgradeReport, UpgradeStatus,
    manifest::read_optional,
};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};
use toml_edit::DocumentMut;

fn check_snapshots(plan: &UpgradePlan) -> Result<(), BoxError> {
    for snapshot in &plan.snapshots {
        if read_optional(&snapshot.path)? != snapshot.contents {
            return Err(format!(
                "stale upgrade plan: {} changed; re-plan before applying",
                snapshot.path.display()
            )
            .into());
        }
    }
    Ok(())
}

/// Apply a previously captured plan with isolated Cargo resolution.
///
/// All protected manifests, lockfile and configuration are checked before writes.
/// Failed publication rolls back only bytes still owned by this operation.
///
/// # Errors
/// * Stale snapshots or concurrent edits, invalid planned TOML, filesystem failures.
/// * Unsupported external/symlinked sources or path-bearing Cargo configuration.
/// * Cargo resolution failure, or resolution that changes a locked Git identity.
/// * Rollback failure (reported explicitly, never hidden by the original failure).
pub async fn apply_upgrade_plan(plan: &UpgradePlan) -> Result<UpgradeReport, BoxError> {
    check_snapshots(plan)?;
    let mut updates = plan
        .changes
        .iter()
        .map(|change| {
            (
                change.path.clone(),
                Some(change.before.clone()),
                change.after.clone(),
            )
        })
        .collect::<Vec<_>>();
    for (_, _, text) in &updates {
        let _: DocumentMut = text.parse()?;
    }
    let mut resolution_warnings = Vec::new();
    let mut resolution_diagnostics = Vec::new();
    let lock_path = plan.root.join("Cargo.lock");
    if plan.lockfile_policy != LockfilePolicy::None {
        let resolved = if let Some(text) = &plan.resolved_lockfile {
            text.clone()
        } else {
            let resolution = resolve_lockfile_with_warnings(plan)?;
            resolution_warnings = resolution.1;
            resolution_diagnostics = resolution.2;
            resolution.0
        };
        let _: DocumentMut = resolved.parse()?;
        let before = plan
            .snapshots
            .iter()
            .find(|s| s.path == lock_path)
            .ok_or("plan lacks lockfile snapshot")?
            .contents
            .clone();
        if before.as_deref() != Some(resolved.as_str()) {
            updates.push((lock_path.clone(), before, resolved));
        }
    }
    check_snapshots(plan)?;
    let mut completed = Vec::new();
    for (path, before, after) in &updates {
        // Validate all protected inputs again before each publication, accounting
        // for the exact bytes already published by this operation.
        let validation =
            check_progress(plan, &completed).and_then(|()| publish(path, before.as_deref(), after));
        if let Err(error) = validation {
            let rollback_errors = rollback(&completed);
            return Err(format!(
                "upgrade apply failed: {error}{}",
                if rollback_errors.is_empty() {
                    String::new()
                } else {
                    format!("; rollback incomplete: {}", rollback_errors.join("; "))
                }
            )
            .into());
        }
        completed.push((path.clone(), before.clone(), after.clone()));
    }
    if let Err(error) = check_progress(plan, &completed) {
        let rollback_errors = rollback(&completed);
        return Err(format!(
            "concurrent change after publication: {error}; rollback diagnostics: {}",
            rollback_errors.join("; ")
        )
        .into());
    }
    let mut report = plan.report.clone();
    report.warnings.extend(resolution_warnings);
    report.diagnostics.extend(resolution_diagnostics);
    report.applied = true;
    report.pending = false;
    report.status = if updates.is_empty() {
        UpgradeStatus::Unchanged
    } else {
        UpgradeStatus::Applied
    };
    report.lockfile = if plan.lockfile_policy == LockfilePolicy::None {
        LockfileStatus::Disabled
    } else if updates.iter().any(|(path, _, _)| *path == lock_path) {
        LockfileStatus::Applied
    } else {
        LockfileStatus::Unchanged
    };
    for dependency in &mut report.dependencies {
        if dependency.status == UpgradeStatus::Planned {
            dependency.status = UpgradeStatus::Applied;
        }
    }
    Ok(report)
}

type Published = (PathBuf, Option<String>, String);
fn check_progress(plan: &UpgradePlan, completed: &[Published]) -> Result<(), BoxError> {
    for snapshot in &plan.snapshots {
        let expected = completed
            .iter()
            .find(|(path, _, _)| *path == snapshot.path)
            .map_or(snapshot.contents.as_deref(), |(_, _, after)| {
                Some(after.as_str())
            });
        if read_optional(&snapshot.path)?.as_deref() != expected {
            return Err(format!("concurrent edit: {}", snapshot.path.display()).into());
        }
    }
    Ok(())
}

fn publish(path: &Path, expected: Option<&str>, after: &str) -> Result<(), BoxError> {
    let parent = path.parent().ok_or("file has no parent")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(after.as_bytes())?;
    temporary.as_file().sync_all()?;
    if let Ok(metadata) = fs::metadata(path) {
        temporary
            .as_file()
            .set_permissions(metadata.permissions())?;
    }
    if read_optional(path)?.as_deref() != expected {
        return Err(format!("concurrent edit: {}", path.display()).into());
    }
    // persist_noclobber protects absent files; existing files have an unavoidable
    // platform rename race, minimized by the immediately preceding byte check.
    if expected.is_none() {
        temporary.persist_noclobber(path).map_err(|e| e.error)?;
    } else {
        temporary.persist(path).map_err(|e| e.error)?;
    }
    Ok(())
}

fn rollback(completed: &[Published]) -> Vec<String> {
    let mut errors = Vec::new();
    for (path, before, after) in completed.iter().rev() {
        let result = (|| -> Result<(), BoxError> {
            if read_optional(path)?.as_deref() != Some(after) {
                return Err(
                    format!("{} was edited concurrently; left untouched", path.display()).into(),
                );
            }
            if let Some(before) = before {
                publish(path, Some(after), before)?;
            } else {
                fs::remove_file(path)?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            errors.push(error.to_string());
        }
    }
    errors
}

#[cfg(test)]
pub(super) fn resolve_lockfile(plan: &UpgradePlan) -> Result<String, BoxError> {
    resolve_lockfile_with_warnings(plan).map(|resolution| resolution.0)
}

#[allow(clippy::too_many_lines)] // Resolution is intentionally a single isolated transaction.
pub(super) fn resolve_lockfile_with_warnings(
    plan: &UpgradePlan,
) -> Result<(String, Vec<String>, Vec<String>), BoxError> {
    let mut warnings = Vec::new();
    let mut diagnostics = Vec::new();
    check_snapshots(plan)?;
    let temporary = tempfile::tempdir()?;
    // Reproduce the ancestor configuration hierarchy, not just the root config.
    let root = mirror_path(&temporary.path().join("snapshot"), &plan.root)?;
    fs::create_dir_all(&root)?;
    let mut budget = CopyBudget::default();
    copy_tree(&plan.root, &root, &mut budget)?;
    for snapshot in &plan.snapshots {
        if snapshot
            .path
            .file_name()
            .is_some_and(|n| n == "config" || n == "config.toml")
            && snapshot
                .path
                .parent()
                .is_some_and(|p| p.file_name().is_some_and(|n| n == ".cargo"))
            && let Some(text) = &snapshot.contents
        {
            // Cargo home remains at its real location, preserving its cache.
            if snapshot.path.starts_with(&plan.root)
                || plan
                    .root
                    .ancestors()
                    .any(|p| snapshot.path.parent().and_then(Path::parent) == Some(p))
            {
                validate_config(text, &snapshot.path, &plan.root)?;
                let target = mirror_path(&temporary.path().join("snapshot"), &snapshot.path)?;
                fs::create_dir_all(target.parent().ok_or("config has no parent")?)?;
                fs::write(target, text)?;
            }
        }
    }
    for change in &plan.changes {
        fs::write(
            root.join(change.path.strip_prefix(&plan.root)?),
            &change.after,
        )?;
    }
    for snapshot in &plan.snapshots {
        if snapshot
            .path
            .file_name()
            .is_some_and(|name| name == "Cargo.toml")
        {
            validate_paths(&root, &root.join(snapshot.path.strip_prefix(&plan.root)?))?;
        }
    }
    let original_lock = plan
        .snapshots
        .iter()
        .find(|s| s.path == plan.root.join("Cargo.lock"))
        .and_then(|s| s.contents.as_deref());
    if original_lock.is_none() && contains_git(&root)? {
        return Err("cannot preserve Git revisions without an existing Cargo.lock; create one explicitly before upgrade".into());
    }
    // Constrain only the selected declaration edges in the disposable snapshot.
    // Replacing an old package ID with --precise would also replace consumers
    // whose requirements still need that version (e.g. base64 0.22 and 0.23).
    // Keep these constraints during optional refresh, then restore the intended
    // manifest bytes. Exact selections satisfy the published broader requirements.
    let unpinned = pin_selected_declarations(plan, &root)?;
    if original_lock.is_none() {
        cargo(&root, plan.offline, &["generate-lockfile".into()])?;
    } else {
        cargo(
            &root,
            plan.offline,
            &["update".into(), "--workspace".into()],
        )?;
        if plan.lockfile_policy == LockfilePolicy::Refresh {
            let current: toml::Value =
                toml::from_str(&fs::read_to_string(root.join("Cargo.lock"))?)?;
            let mut candidates = Vec::new();
            let mut eligible_names = plan
                .lock_targets
                .iter()
                .map(|key| key.name.clone())
                .collect::<BTreeSet<_>>();
            if let Some(packages) = current.get("package").and_then(toml::Value::as_array) {
                loop {
                    let mut additions = BTreeSet::new();
                    for package in packages {
                        if package
                            .get("name")
                            .and_then(toml::Value::as_str)
                            .is_some_and(|n| eligible_names.contains(n))
                            && let Some(dependencies) =
                                package.get("dependencies").and_then(toml::Value::as_array)
                        {
                            additions.extend(
                                dependencies
                                    .iter()
                                    .filter_map(toml::Value::as_str)
                                    .filter_map(|s| s.split_whitespace().next())
                                    .map(str::to_owned),
                            );
                        }
                    }
                    let previous = eligible_names.len();
                    eligible_names.extend(additions);
                    if previous == eligible_names.len() {
                        break;
                    }
                }
                for package in packages {
                    let name = package
                        .get("name")
                        .and_then(toml::Value::as_str)
                        .ok_or("lock package lacks name")?;
                    let source = package
                        .get("source")
                        .and_then(toml::Value::as_str)
                        .unwrap_or_default();
                    if (source.starts_with("registry+") || source.starts_with("sparse+"))
                        && eligible_names.contains(name)
                        && !plan.lock_targets.iter().any(|key| key.name == name)
                        && !plan.protected_names.iter().any(|p| p == name)
                    {
                        candidates.push(format!(
                            "{source}#{name}@{}",
                            package
                                .get("version")
                                .and_then(toml::Value::as_str)
                                .ok_or("lock package lacks version")?
                        ));
                    }
                }
            }
            let baseline = fs::read_to_string(root.join("Cargo.lock"))?;
            // Requested direct selections are mandatory. Only the optional
            // transitive refresh may be held back to preserve the valid graph.
            validate_resolution(plan, original_lock, &baseline)?;
            refresh_compatible(
                plan,
                &root,
                original_lock,
                &candidates,
                &mut warnings,
                &mut diagnostics,
            )?;
        }
    }
    let resolved = fs::read_to_string(root.join("Cargo.lock"))?;
    validate_resolution(plan, original_lock, &resolved)?;
    for (path, text) in unpinned {
        fs::write(path, text)?;
    }
    check_snapshots(plan)?;
    Ok((resolved, warnings, diagnostics))
}

/// Pin selected owner declarations only inside the disposable resolver snapshot.
fn pin_selected_declarations(
    plan: &UpgradePlan,
    root: &Path,
) -> Result<Vec<(PathBuf, String)>, BoxError> {
    let mut documents = std::collections::BTreeMap::new();
    let mut originals = std::collections::BTreeMap::new();
    for target in &plan.lock_versions {
        let path = root.join(target.manifest.strip_prefix(&plan.root)?);
        if !documents.contains_key(&path) {
            let text = fs::read_to_string(&path)?;
            documents.insert(path.clone(), text.parse::<DocumentMut>()?);
            originals.insert(path.clone(), text);
        }
        let mut item = documents
            .get_mut(&path)
            .ok_or("missing resolver manifest")?
            .as_item_mut();
        for part in &target.declaration_key {
            item = item
                .get_mut(part)
                .ok_or("selected declaration missing from resolver manifest")?;
        }
        super::planner::replace_requirement(item, &format!("={}", target.selected))?;
    }
    for (path, document) in documents {
        fs::write(path, document.to_string())?;
    }
    Ok(originals.into_iter().collect())
}

fn validate_resolution(
    plan: &UpgradePlan,
    original_lock: Option<&str>,
    resolved: &str,
) -> Result<(), BoxError> {
    if git_identities(resolved)? != git_identities(original_lock.unwrap_or_default())?
        && original_lock.is_some()
    {
        return Err(
            "isolated Cargo resolution changed a skipped Git revision; no live files were written"
                .into(),
        );
    }
    if original_lock.is_some() {
        verify_protected(plan, original_lock.unwrap_or_default(), resolved)?;
    }
    if original_lock.is_some() {
        verify_no_downgrades(original_lock.unwrap_or_default(), resolved)?;
    }
    let resolved_doc: toml::Value = toml::from_str(resolved)?;
    for target in &plan.lock_versions {
        if !resolved_doc
            .get("package")
            .and_then(toml::Value::as_array)
            .is_some_and(|packages| {
                packages.iter().any(|package| {
                    package.get("name").and_then(toml::Value::as_str) == Some(&target.key.name)
                        && package.get("version").and_then(toml::Value::as_str)
                            == Some(&target.selected)
                        && package
                            .get("source")
                            .and_then(toml::Value::as_str)
                            .is_some_and(|source| {
                                target.registry_source.as_deref().is_some_and(|index| {
                                    super::sources::same_registry(source, index)
                                })
                            })
                })
            })
        {
            return Err(format!(
                "Cargo did not retain selected release {} {}; no live files written",
                target.key.name, target.selected
            )
            .into());
        }
    }
    Ok(())
}

// Try the whole eligible set first so mutually dependent upgrades can resolve
// together. On conflict, restore exact bytes before trying smaller batches.
// Every accepted batch must preserve both the original policy and previously
// accepted versions. The fallback is deterministic and bounded by 2*N-1 trials;
// it deliberately promises safe progress, not a globally maximal solution.
fn refresh_compatible(
    plan: &UpgradePlan,
    root: &Path,
    original_lock: Option<&str>,
    candidates: &[String],
    warnings: &mut Vec<String>,
    diagnostics: &mut Vec<String>,
) -> Result<(), BoxError> {
    if candidates.is_empty() {
        return Ok(());
    }
    let path = root.join("Cargo.lock");
    let before = fs::read_to_string(&path)?;
    let document: toml::Value = toml::from_str(&before)?;
    let identities = document
        .get("package")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|package| {
            Some(format!(
                "{}#{}@{}",
                package.get("source")?.as_str()?,
                package.get("name")?.as_str()?,
                package.get("version")?.as_str()?
            ))
        })
        .collect::<BTreeSet<_>>();
    // Earlier accepted batches can update/remove members of later batches.
    // Do not invoke Cargo with stale IDs or misreport those as held back.
    let candidates = candidates
        .iter()
        .filter(|id| identities.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Ok(());
    }
    let mut arguments = vec!["update".into()];
    for candidate in &candidates {
        arguments.extend(["--package".into(), candidate.clone()]);
    }
    let attempt = cargo(root, plan.offline, &arguments).and_then(|()| {
        let after = fs::read_to_string(&path)?;
        validate_resolution(plan, original_lock, &after)?;
        verify_no_downgrades(&before, &after)
    });
    if let Err(error) = attempt {
        fs::write(&path, before)?;
        if candidates.len() == 1 {
            warnings.push(format!(
                "Held back transitive refresh of {}: {error}",
                candidates[0]
            ));
        } else {
            diagnostics.push(format!(
                "Retrying transitive refresh batch ({} packages): {error}",
                candidates.len()
            ));
            let (left, right) = candidates.split_at(candidates.len() / 2);
            refresh_compatible(plan, root, original_lock, left, warnings, diagnostics)?;
            refresh_compatible(plan, root, original_lock, right, warnings, diagnostics)?;
        }
    }
    Ok(())
}

fn cargo(root: &Path, offline: bool, arguments: &[String]) -> Result<(), BoxError> {
    let mut command = Command::new("cargo");
    command
        .current_dir(root)
        .args(arguments)
        .arg("--manifest-path")
        .arg(root.join("Cargo.toml"));
    if offline {
        command.arg("--offline");
    }
    command.env("CARGO_TARGET_DIR", root.join("target"));
    let output = command.output()?;
    if !output.status.success() {
        return Err(format!(
            "isolated cargo {} failed ({}): {}",
            arguments.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}

fn git_identities(text: &str) -> Result<BTreeSet<(String, String, String)>, BoxError> {
    let doc: toml::Value = toml::from_str(text)?;
    let mut result = BTreeSet::new();
    if let Some(packages) = doc.get("package").and_then(toml::Value::as_array) {
        for package in packages {
            if let Some(source) = package
                .get("source")
                .and_then(toml::Value::as_str)
                .filter(|source| source.starts_with("git+"))
            {
                result.insert((
                    package
                        .get("name")
                        .and_then(toml::Value::as_str)
                        .ok_or("Git package lacks name")?
                        .into(),
                    package
                        .get("version")
                        .and_then(toml::Value::as_str)
                        .ok_or("Git package lacks version")?
                        .into(),
                    source.into(),
                ));
            }
        }
    }
    Ok(result)
}

#[derive(Default)]
struct CopyBudget {
    entries: usize,
    bytes: u64,
}
fn copy_tree(from: &Path, to: &Path, budget: &mut CopyBudget) -> Result<(), BoxError> {
    let mut entries = fs::read_dir(from)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        budget.entries += 1;
        if budget.entries > 50_000 {
            return Err(
                "isolated snapshot exceeds 50,000 entries; resolution unsupported for this tree"
                    .into(),
            );
        }
        let name = entry.file_name();
        if [
            "target",
            ".git",
            ".jj",
            "node_modules",
            ".venv",
            "dist",
            ".direnv",
        ]
        .iter()
        .any(|skip| name == *skip)
        {
            continue;
        }
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            if ["Cargo.toml", "Cargo.lock", ".cargo", "src", "build.rs"]
                .iter()
                .any(|input| name == *input)
            {
                return Err(format!(
                    "Cargo input symlink unsupported: {}",
                    entry.path().display()
                )
                .into());
            }
            continue;
        }
        let target = to.join(&name);
        if kind.is_dir() {
            fs::create_dir_all(&target)?;
            copy_tree(&entry.path(), &target, budget)?;
        } else if kind.is_file() {
            // Cargo resolution does not execute or compile sources. Copy only
            // manifests, Rust target inputs and vendor checksum metadata, never
            // arbitrary ignored assets, dotenv files or credentials.
            if !["Cargo.toml", "Cargo.lock", ".cargo-checksum.json"]
                .iter()
                .any(|input| name == *input)
                && entry
                    .path()
                    .extension()
                    .is_none_or(|extension| extension != "rs")
                && !(from
                    .file_name()
                    .is_some_and(|directory| directory == ".cargo")
                    && (name == "config" || name == "config.toml"))
            {
                continue;
            }
            budget.bytes += entry.metadata()?.len();
            if budget.bytes > 256 * 1024 * 1024 {
                return Err(
                    "isolated snapshot exceeds 256 MiB; resolution unsupported for this tree"
                        .into(),
                );
            }
            fs::copy(entry.path(), target)?;
        } else {
            return Err("isolated snapshot encountered a nonregular file".into());
        }
    }
    Ok(())
}

fn validate_config(text: &str, path: &Path, workspace_root: &Path) -> Result<(), BoxError> {
    fn check(value: &toml::Value) -> bool {
        value.as_table().is_some_and(|table| {
            table.iter().any(|(key, value)| {
                [
                    "local-registry",
                    "ssl-cainfo",
                    "ssl-cert",
                    "ssl-key",
                    "credential-provider",
                    "global-credential-providers",
                ]
                .contains(&key.as_str())
                    || check(value)
            })
        })
    }
    let value: toml::Value = toml::from_str(text)?;
    let table = value
        .as_table()
        .ok_or("Cargo configuration must be a table")?;
    for key in table.keys() {
        if ![
            "registries",
            "registry",
            "source",
            "net",
            "http",
            "term",
            "resolver",
            "env",
            "alias",
            "build",
        ]
        .contains(&key.as_str())
        {
            return Err(format!("isolated resolution unsupported for Cargo config key '{key}' in {}; relative paths/environment semantics cannot be preserved safely", path.display()).into());
        }
    }
    if let Some(env) = value.get("env").and_then(toml::Value::as_table) {
        for setting in env.values() {
            if setting.as_table().is_some_and(|t| {
                t.get("relative")
                    .and_then(toml::Value::as_bool)
                    .unwrap_or(false)
            }) {
                return Err(
                    "isolated resolution does not support relative Cargo env entries".into(),
                );
            }
        }
    }
    if let Some(sources) = value.get("source").and_then(toml::Value::as_table) {
        for source in sources.values() {
            if let Some(directory) = source.get("directory").and_then(toml::Value::as_str) {
                let base = path
                    .parent()
                    .and_then(Path::parent)
                    .ok_or("Cargo config has no base directory")?;
                if Path::new(directory).is_absolute()
                    || !fs::canonicalize(base.join(directory))?.starts_with(workspace_root)
                {
                    return Err("isolated resolution supports directory sources only at relative paths inside the workspace".into());
                }
            }
        }
    }
    // Built-in Cargo commands do not execute aliases or compile targets. Their
    // path-like strings therefore need no relocation; CARGO_TARGET_DIR is set
    // explicitly for every isolated Cargo invocation.
    let mut resolution_config = value.clone();
    if let Some(table) = resolution_config.as_table_mut() {
        table.remove("alias");
        table.remove("build");
    }
    if check(&resolution_config) {
        return Err(format!(
            "isolated resolution unsupported for path-bearing or credential-provider config in {}",
            path.display()
        )
        .into());
    }
    Ok(())
}

fn validate_paths(root: &Path, manifest: &Path) -> Result<(), BoxError> {
    fn walk(value: &toml::Value, root: &Path, directory: &Path) -> Result<(), BoxError> {
        if let Some(table) = value.as_table() {
            for (key, value) in table {
                if key == "path"
                    && let Some(path) = value.as_str()
                {
                    let joined = directory.join(path);
                    let canonical = fs::canonicalize(&joined).map_err(|e| {
                        format!(
                            "isolated Cargo path {} cannot be reproduced: {e}",
                            joined.display()
                        )
                    })?;
                    if Path::new(path).is_absolute() || !canonical.starts_with(root) {
                        return Err(format!(
                            "isolated resolution unsupported for external path '{path}'"
                        )
                        .into());
                    }
                }
                walk(value, root, directory)?;
            }
        } else if let Some(array) = value.as_array() {
            for value in array {
                walk(value, root, directory)?;
            }
        }
        Ok(())
    }
    let doc: toml::Value = toml::from_str(&fs::read_to_string(manifest)?)?;
    // Package metadata can contain arbitrary strings called path; it is not a
    // Cargo source input and must not cause false unsupported errors.
    let mut relevant = doc;
    if let Some(package) = relevant
        .get_mut("package")
        .and_then(toml::Value::as_table_mut)
    {
        package.remove("metadata");
    }
    if let Some(workspace) = relevant
        .get_mut("workspace")
        .and_then(toml::Value::as_table_mut)
    {
        workspace.remove("metadata");
    }
    walk(
        &relevant,
        root,
        manifest.parent().ok_or("manifest has no parent")?,
    )
}

fn mirror_path(base: &Path, original: &Path) -> Result<PathBuf, BoxError> {
    use std::path::Component;
    if !original.is_absolute() {
        return Err("snapshot paths must be absolute".into());
    }
    let mut result = base.to_path_buf();
    for component in original.components() {
        match component {
            Component::Prefix(prefix) => {
                // Preserve drive/UNC distinctions without introducing separators.
                use std::fmt::Write as _;
                let identity = prefix.as_os_str().to_string_lossy().as_bytes().iter().fold(
                    String::new(),
                    |mut text, byte| {
                        let _ = write!(text, "{byte:02x}");
                        text
                    },
                );
                result.push(format!("volume-{identity}"));
            }
            Component::RootDir | Component::CurDir => {}
            Component::Normal(name) => result.push(name),
            Component::ParentDir => return Err("snapshot path contains parent traversal".into()),
        }
    }
    Ok(result)
}

fn verify_no_downgrades(before: &str, after: &str) -> Result<(), BoxError> {
    let old: toml::Value = toml::from_str(before)?;
    let new: toml::Value = toml::from_str(after)?;
    if let (Some(previous), Some(current)) = (
        old.get("package").and_then(toml::Value::as_array),
        new.get("package").and_then(toml::Value::as_array),
    ) {
        for package in previous {
            let name = package.get("name");
            let source = package.get("source");
            let old_version = semver::Version::parse(
                package
                    .get("version")
                    .and_then(toml::Value::as_str)
                    .ok_or("locked package lacks version")?,
            )?;
            for replacement in current
                .iter()
                .filter(|p| p.get("name") == name && p.get("source") == source)
            {
                let version = semver::Version::parse(
                    replacement
                        .get("version")
                        .and_then(toml::Value::as_str)
                        .ok_or("locked package lacks version")?,
                )?;
                let same_line = if old_version.major > 0 {
                    version.major == old_version.major
                } else if old_version.minor > 0 {
                    version.major == 0 && version.minor == old_version.minor
                } else {
                    version.major == 0 && version.minor == 0 && version.patch == old_version.patch
                };
                if same_line
                    && version < old_version
                    && !current.iter().any(|p| {
                        p.get("name") == name
                            && p.get("source") == source
                            && p.get("version") == package.get("version")
                    })
                {
                    let name = name
                        .and_then(toml::Value::as_str)
                        .ok_or("locked package lacks name")?;
                    return Err(format!("Cargo resolution would downgrade {name} from {old_version} to {version}; no live files written").into());
                }
            }
        }
    }
    Ok(())
}

fn verify_protected(plan: &UpgradePlan, before: &str, after: &str) -> Result<(), BoxError> {
    fn identities(
        text: &str,
        names: &[String],
    ) -> Result<BTreeSet<(String, String, String)>, BoxError> {
        let doc: toml::Value = toml::from_str(text)?;
        let mut result = BTreeSet::new();
        if let Some(packages) = doc.get("package").and_then(toml::Value::as_array) {
            for package in packages {
                let name = package
                    .get("name")
                    .and_then(toml::Value::as_str)
                    .ok_or("lock package lacks name")?;
                if names.iter().any(|n| n == name) {
                    result.insert((
                        name.into(),
                        package
                            .get("version")
                            .and_then(toml::Value::as_str)
                            .ok_or("lock package lacks version")?
                            .into(),
                        package
                            .get("source")
                            .and_then(toml::Value::as_str)
                            .unwrap_or_default()
                            .into(),
                    ));
                }
            }
        }
        Ok(result)
    }
    let previous = identities(before, &plan.protected_names)?;
    let current = identities(after, &plan.protected_names)?;
    // A selected alias may add a new version alongside a protected alias of
    // the same package. Existing protected identities must still survive.
    let selected_additions = current
        .difference(&previous)
        .all(|(name, version, source)| {
            plan.lock_versions.iter().any(|target| {
                target.key.name == *name
                    && target.selected == *version
                    && target
                        .registry_source
                        .as_deref()
                        .is_some_and(|index| super::sources::same_registry(source, index))
            })
        });
    if !previous.is_subset(&current) || !selected_additions {
        return Err("Cargo resolution would advance a pinned, excluded, unavailable or skipped dependency; no live files were written; adjust policy explicitly".into());
    }
    Ok(())
}

fn contains_git(root: &Path) -> Result<bool, BoxError> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if contains_git(&entry.path())? {
                return Ok(true);
            }
        } else if entry.file_name() == "Cargo.toml" {
            let doc: DocumentMut = fs::read_to_string(entry.path())?.parse()?;
            if super::planner::raw_dependencies(&doc)?
                .iter()
                .any(|d| d.item.get("git").is_some())
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downgrade_message_uses_plain_package_name() {
        let lock = |version: &str| {
            format!(
                "version=4\n[[package]]\nname='psm'\nversion='{version}'\nsource='registry+https://github.com/rust-lang/crates.io-index'\n"
            )
        };
        let error = verify_no_downgrades(&lock("0.1.31"), &lock("0.1.24")).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Cargo resolution would downgrade psm from 0.1.31 to 0.1.24; no live files written"
        );
    }

    #[test]
    fn snapshot_does_not_copy_dotenv_credentials_or_assets() {
        let dir = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".env"), "SECRET=not-for-snapshot").unwrap();
        fs::write(dir.path().join("credentials.toml"), "token='secret'").unwrap();
        fs::write(dir.path().join("large.bin"), [0u8; 32]).unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        copy_tree(dir.path(), output.path(), &mut CopyBudget::default()).unwrap();
        assert!(output.path().join("Cargo.toml").exists());
        for name in [".env", "credentials.toml", "large.bin"] {
            assert!(!output.path().join(name).exists());
        }
    }

    #[test]
    fn rollback_restores_only_operation_owned_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first");
        let concurrent = directory.path().join("concurrent");
        fs::write(&first, "after").unwrap();
        fs::write(&concurrent, "third-party").unwrap();
        let errors = rollback(&[
            (first.clone(), Some("before".into()), "after".into()),
            (concurrent.clone(), Some("original".into()), "ours".into()),
        ]);
        assert_eq!(fs::read_to_string(first).unwrap(), "before");
        assert_eq!(fs::read_to_string(concurrent).unwrap(), "third-party");
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn failed_publish_does_not_clobber_concurrent_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("manifest");
        fs::write(&path, "concurrent").unwrap();
        assert!(publish(&path, Some("before"), "after").is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "concurrent");
    }

    #[test]
    fn repo_alias_and_build_configuration_are_inert_for_resolution() {
        let directory = tempfile::tempdir().unwrap();
        assert!(validate_config(
            "[alias]\ncheck-all='check --workspace'\nrelative='!./scripts/check.sh'\n[build]\ntarget-dir='target'\nrustc-wrapper='./scripts/wrapper'\n[env]\nRUST_LOG='info'\n",
            &directory.path().join(".cargo/config.toml"),
            directory.path(),
        ).is_ok());
    }

    #[test]
    fn config_env_and_relative_source_semantics() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            validate_config(
                "[env]\nRUST_LOG='info'\n",
                &dir.path().join(".cargo/config.toml"),
                dir.path()
            )
            .is_ok()
        );
        assert!(
            validate_config(
                "[env]\nDATA={value='data',relative=true}\n",
                &dir.path().join(".cargo/config.toml"),
                dir.path()
            )
            .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn irrelevant_symlinks_are_not_traversed() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("spec")).unwrap();
        symlink("/not/a/source", dir.path().join("spec/rfc.txt")).unwrap();
        symlink("/not/a/source", dir.path().join(".direnv")).unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        copy_tree(dir.path(), output.path(), &mut CopyBudget::default()).unwrap();
        assert!(!output.path().join("spec/rfc.txt").exists());
        assert!(output.path().join("Cargo.toml").exists());
    }
}
