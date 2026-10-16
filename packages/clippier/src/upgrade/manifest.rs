//! Read-only workspace discovery and configuration capture.
use super::{BoxError, FileSnapshot, UpgradeConfig, UpgradeInput, UpgradeOptions};
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use toml_edit::DocumentMut;

pub(super) fn patterns(values: &[String]) -> Result<GlobSet, BoxError> {
    let mut builder = GlobSetBuilder::new();
    for value in values {
        builder.add(Glob::new(value)?);
    }
    Ok(builder.build()?)
}

pub(super) fn read_optional(path: &Path) -> Result<Option<String>, BoxError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn capture(
    snapshots: &mut BTreeMap<PathBuf, Option<String>>,
    path: PathBuf,
) -> Result<(), BoxError> {
    let contents = read_optional(&path)?;
    snapshots.insert(path, contents);
    Ok(())
}

/// Load manifests and typed configuration without running Cargo or writing files.
///
/// # Errors
/// * Invalid TOML/configuration, inaccessible files, invalid selectors.
/// * Unsupported manifests outside the workspace or ambiguous workspace roots.
#[allow(clippy::too_many_lines)] // Discovery and snapshot capture must remain one read-only transaction.
pub fn load_upgrade_input(options: &UpgradeOptions) -> Result<UpgradeInput, BoxError> {
    let path = options
        .manifest_path
        .clone()
        .unwrap_or(std::env::current_dir()?);
    let manifest = if path.is_dir() {
        path.join("Cargo.toml")
    } else {
        path
    };
    let manifest = fs::canonicalize(manifest)?;
    let initial: DocumentMut = fs::read_to_string(&manifest)?.parse()?;
    let directory = manifest.parent().ok_or("manifest has no parent")?;
    let explicit = initial
        .get("package")
        .and_then(|p| p.get("workspace"))
        .and_then(toml_edit::Item::as_str);
    let root = if let Some(workspace) = explicit {
        fs::canonicalize(directory.join(workspace))?
    } else if initial.get("workspace").is_some() {
        directory.to_path_buf()
    } else {
        let mut found = None;
        for parent in directory.ancestors().skip(1) {
            let candidate = parent.join("Cargo.toml");
            if let Some(text) = read_optional(&candidate)? {
                let document: DocumentMut = text.parse()?;
                if document.get("workspace").is_some() {
                    found = Some(parent.to_path_buf());
                    break;
                }
            }
        }
        found.unwrap_or_else(|| directory.to_path_buf())
    };
    let root_manifest = root.join("Cargo.toml");
    let root_text = fs::read_to_string(&root_manifest)?;
    let root_doc: DocumentMut = root_text.parse()?;
    let mut manifests = BTreeMap::from([(root_manifest, root_text)]);
    if let Some(workspace) = root_doc.get("workspace") {
        let members = string_array(workspace.get("members"))?;
        let excludes = string_array(workspace.get("exclude"))?;
        let includes = patterns(&members)?;
        let excludes = patterns(&excludes)?;
        let mut candidates = Vec::new();
        discover(&root, &root, &mut candidates)?;
        for candidate in candidates {
            let parent = candidate.parent().ok_or("manifest has no parent")?;
            let relative = parent.strip_prefix(&root)?;
            if includes.is_match(relative) && !excludes.is_match(relative) {
                manifests.insert(candidate.clone(), fs::read_to_string(&candidate)?);
            }
        }
        // Cargo automatically includes path dependencies under the workspace root.
        loop {
            let mut additions = BTreeMap::new();
            for (path, text) in &manifests {
                let document: DocumentMut = text.parse()?;
                for dependency in super::planner::raw_dependencies(&document)? {
                    if let Some(relative) = dependency
                        .item
                        .get("path")
                        .and_then(toml_edit::Item::as_str)
                    {
                        let candidate = fs::canonicalize(
                            path.parent()
                                .ok_or("manifest has no parent")?
                                .join(relative)
                                .join("Cargo.toml"),
                        )?;
                        if candidate.starts_with(&root)
                            && !manifests.contains_key(&candidate)
                            && !excludes.is_match(
                                candidate
                                    .parent()
                                    .ok_or("manifest has no parent")?
                                    .strip_prefix(&root)?,
                            )
                        {
                            let text = fs::read_to_string(&candidate)?;
                            let doc: DocumentMut = text.parse()?;
                            if doc.get("workspace").is_none() {
                                additions.insert(candidate, text);
                            }
                        }
                    }
                }
            }
            if additions.is_empty() {
                break;
            }
            manifests.extend(additions);
        }
        if !manifests.contains_key(&manifest) {
            return Err("requested package is not a workspace member".into());
        }
    }
    let mut snapshots = BTreeMap::new();
    for (path, text) in &manifests {
        snapshots.insert(path.clone(), Some(text.clone()));
    }
    capture(&mut snapshots, root.join("Cargo.lock"))?;
    let config_path = options
        .config_path
        .clone()
        .unwrap_or_else(|| root.join("clippier.toml"));
    let config_path = if config_path.is_absolute() {
        config_path
    } else {
        std::env::current_dir()?.join(config_path)
    };
    let config_text = read_optional(&config_path)?;
    if options.config_path.is_some() && config_text.is_none() {
        return Err(format!("configuration not found: {}", config_path.display()).into());
    }
    let mut config: UpgradeConfig = if let Some(text) = &config_text {
        let value: toml::Value = toml::from_str(text)?;
        value
            .get("upgrade")
            .cloned()
            .map(toml::Value::try_into)
            .transpose()?
            .unwrap_or_default()
    } else {
        UpgradeConfig::default()
    };
    snapshots.insert(config_path, config_text);
    // Cargo reads ancestor configuration and CARGO_HOME. Protect absent files too.
    for ancestor in root.ancestors() {
        for name in ["config", "config.toml"] {
            capture(&mut snapshots, ancestor.join(".cargo").join(name))?;
        }
    }
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")));
    if let Some(home) = cargo_home {
        for name in ["config", "config.toml"] {
            capture(&mut snapshots, home.join(name))?;
        }
    }
    macro_rules! override_policy { ($($field:ident),+) => { $(if let Some(value) = options.$field { config.$field = value; })+ }; }
    override_policy!(policy, pinned, prerelease, source, lockfile);
    if !options.dependencies.is_empty() {
        config.dependencies.clone_from(&options.dependencies);
    }
    if !options.packages.is_empty() {
        config.packages.clone_from(&options.packages);
    }
    if !options.exclude.is_empty() {
        config.exclude.clone_from(&options.exclude);
    }
    if !options.kinds.is_empty() {
        config.kinds.clone_from(&options.kinds);
    }
    patterns(&config.dependencies)?;
    patterns(&config.packages)?;
    patterns(&config.exclude)?;
    let defaults = super::RegistryOptions::default();
    macro_rules! registry_override { ($($field:ident),+) => { $(
        if options.registry.$field != defaults.$field { config.registry.$field = options.registry.$field; }
        if let Some(value) = options.registry_overrides.$field { config.registry.$field = value; }
    )+ }; }
    registry_override!(offline, timeout_seconds, max_retries, max_concurrency);
    if let Some(msrv) = &options.msrv {
        config.msrv = Some(msrv.clone());
    }
    let registry = config.registry.clone();
    Ok(UpgradeInput {
        root,
        manifests,
        snapshots: snapshots
            .into_iter()
            .map(|(path, contents)| FileSnapshot { path, contents })
            .collect(),
        config,
        lockfile_only: options.lockfile_only,
        resolve_lockfile: options.resolve_lockfile,
        offline: registry.offline,
        registry,
        overrides: options.clone(),
    })
}

fn string_array(item: Option<&toml_edit::Item>) -> Result<Vec<String>, BoxError> {
    item.map_or_else(
        || Ok(Vec::new()),
        |item| {
            item.as_array()
                .ok_or("workspace members/exclude must be arrays")?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| "workspace member must be a string".into())
                })
                .collect()
        },
    )
}

fn discover(root: &Path, directory: &Path, result: &mut Vec<PathBuf>) -> Result<(), BoxError> {
    let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            continue;
        }
        let name = entry.file_name();
        if kind.is_dir()
            && ![".git", "target", "node_modules", ".jj"]
                .iter()
                .any(|skip| name == *skip)
        {
            // Nested independent workspaces do not belong to the outer workspace.
            let candidate = entry.path().join("Cargo.toml");
            if let Some(text) = read_optional(&candidate)? {
                let doc: DocumentMut = text.parse()?;
                if doc.get("workspace").is_some() {
                    continue;
                }
            }
            discover(root, &entry.path(), result)?;
        } else if kind.is_file() && name == "Cargo.toml" && directory != root {
            result.push(entry.path());
        }
    }
    Ok(())
}
