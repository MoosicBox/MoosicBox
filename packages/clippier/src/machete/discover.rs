//! Filesystem discovery, layered configuration and exact removal planning.
use super::{
    BoxError, DependencyKind, DependencyStatus, MacheteChange, MacheteConfig, MacheteDependency,
    MacheteOptions, MachetePlan, MacheteReport, source,
};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};
use toml_edit::{DocumentMut, Item};

#[derive(Default)]
struct Inventory {
    files: BTreeMap<PathBuf, Option<Vec<u8>>>,
    directories: BTreeMap<PathBuf, Vec<PathBuf>>,
    configs: BTreeMap<PathBuf, MacheteConfig>,
    explicit: Option<(PathBuf, MacheteConfig)>,
    omitted_directories: bool,
    target_roots: BTreeSet<PathBuf>,
    ignore_matchers: BTreeMap<PathBuf, ignore::gitignore::Gitignore>,
    no_ignore: Option<bool>,
    diagnostics: BTreeSet<String>,
}

pub(super) fn optional(path: &Path) -> Result<Option<Vec<u8>>, BoxError> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot read {}: {e}", path.display()).into()),
    }
}

fn patterns(values: &[String]) -> Result<GlobSet, BoxError> {
    let mut builder = GlobSetBuilder::new();
    for value in values {
        builder.add(GlobBuilder::new(value).literal_separator(true).build()?);
    }
    Ok(builder.build()?)
}
fn matches(values: &[String], value: &str) -> Result<bool, BoxError> {
    Ok(patterns(values)?.is_match(value))
}
fn selected(values: &[String], value: &str) -> Result<bool, BoxError> {
    Ok(values.is_empty() || matches(values, value)?)
}
fn dependency_matches(values: &[String], key: &str, package: &str) -> Result<bool, BoxError> {
    Ok(matches(values, key)? || matches(values, package)?)
}
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            _ => out.push(component.as_os_str()),
        }
    }
    out
}
fn directory_match(base: &Path, patterns_: &[String], path: &Path) -> Result<bool, BoxError> {
    let patterns_ = patterns(patterns_)?;
    for ancestor in path.ancestors() {
        if let Ok(relative) = ancestor.strip_prefix(base)
            && !relative.as_os_str().is_empty()
            && patterns_.is_match(relative)
        {
            return Ok(true);
        }
    }
    Ok(false)
}
fn config_at(path: &Path, inventory: &mut Inventory) -> Result<(), BoxError> {
    let file = path.join("clippier.toml");
    let bytes = optional(&file)?;
    if let Some(bytes) = &bytes {
        let value: toml::Value = toml::from_str(std::str::from_utf8(bytes)?)?;
        if let Some(value) = value.get("machete") {
            let config: MacheteConfig = value.clone().try_into()?;
            // Validate even patterns not exercised by this invocation.
            for list in [
                &config.ignored,
                &config.ignored_dirs,
                &config.packages,
                &config.dependencies,
                &config.exclude,
            ] {
                patterns(list)?;
            }
            for rule in &config.rules {
                for list in [
                    &rule.packages,
                    &rule.dependencies,
                    &rule.ignored,
                    &rule.ignored_dirs,
                ] {
                    patterns(list)?;
                }
            }
            inventory.configs.insert(path.to_owned(), config);
        }
    }
    inventory.files.insert(file, bytes);
    Ok(())
}
fn ignored_directory(path: &Path, inventory: &Inventory) -> Result<bool, BoxError> {
    let no_ignore = inventory
        .no_ignore
        .or_else(|| {
            layers(path, inventory)
                .iter()
                .rev()
                .find_map(|(_, config)| config.no_ignore)
        })
        .unwrap_or(false);
    if !no_ignore {
        let mut ignored = false;
        let mut matchers = inventory
            .ignore_matchers
            .iter()
            .filter(|(base, _)| path.starts_with(base))
            .collect::<Vec<_>>();
        matchers.sort_by_key(|(base, _)| base.components().count());
        for (_, matcher) in matchers {
            let matched = matcher.matched_path_or_any_parents(path, path.is_dir());
            if matched.is_ignore() {
                ignored = true;
            } else if matched.is_whitelist() {
                ignored = false;
            }
        }
        if ignored {
            return Ok(true);
        }
    }
    for (base, config) in &inventory.configs {
        if path.starts_with(base) && directory_match(base, &config.ignored_dirs, path)? {
            return Ok(true);
        }
    }
    if let Some((base, config)) = &inventory.explicit
        && directory_match(base, &config.ignored_dirs, path)?
    {
        return Ok(true);
    }
    Ok(false)
}
fn walk(directory: &Path, inventory: &mut Inventory) -> Result<(), BoxError> {
    config_at(directory, inventory)?;
    let mut builder = ignore::gitignore::GitignoreBuilder::new(directory);
    for name in [".gitignore", ".ignore"] {
        let path = directory.join(name);
        let bytes = optional(&path)?;
        if bytes.is_some()
            && let Some(error) = builder.add(&path)
        {
            return Err(format!("invalid ignore file {}: {error}", path.display()).into());
        }
        inventory.files.insert(path, bytes);
    }
    inventory
        .ignore_matchers
        .insert(directory.to_owned(), builder.build()?);
    let mut entries = fs::read_dir(directory)
        .map_err(|error| format!("cannot list {}: {error}", directory.display()))?
        .map(|entry| {
            entry.map(|e| e.path()).map_err(|error| {
                format!("cannot inspect entry in {}: {error}", directory.display())
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort();
    inventory
        .directories
        .insert(directory.to_owned(), entries.clone());
    for path in entries {
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if matches!(name, ".git" | "target" | "node_modules" | ".direnv") {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
        if ignored_directory(&path, inventory)? {
            inventory.omitted_directories = true;
            continue;
        }
        if metadata.file_type().is_symlink() {
            if path.is_dir()
                || name == "Cargo.toml"
                || name == "clippier.toml"
                || path.extension().is_some_and(|extension| extension == "rs")
            {
                inventory
                    .diagnostics
                    .insert(format!("unsupported symlink: {}", path.display()));
            }
            continue;
        }
        if metadata.is_dir() {
            if ignored_directory(&path, inventory)? {
                inventory.omitted_directories = true;
            } else {
                walk(&path, inventory)?;
            }
        } else if metadata.is_file()
            && (name == "Cargo.toml"
                || name == "Cargo.lock"
                || path.extension().is_some_and(|s| s == "rs"))
        {
            inventory.files.insert(path.clone(), optional(&path)?);
        }
    }
    Ok(())
}
fn text<'a>(inventory: &'a Inventory, path: &Path) -> Result<&'a str, BoxError> {
    let bytes = inventory
        .files
        .get(path)
        .and_then(Option::as_deref)
        .ok_or_else(|| format!("file not inventoried: {}", path.display()))?;
    Ok(std::str::from_utf8(bytes)
        .map_err(|error| format!("invalid UTF-8 in {}: {error}", path.display()))?)
}
fn document(inventory: &Inventory, path: &Path) -> Result<DocumentMut, BoxError> {
    Ok(text(inventory, path)?.parse()?)
}
fn strings(item: Option<&Item>) -> Vec<String> {
    item.and_then(Item::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}
fn root_manifest(requested: &Path, inventory: &mut Inventory) -> Result<PathBuf, BoxError> {
    let original: DocumentMut = fs::read_to_string(requested)?.parse()?;
    if original.get("workspace").is_some() {
        return Ok(requested.to_owned());
    }
    if let Some(explicit) = original
        .get("package")
        .and_then(|p| p.get("workspace"))
        .and_then(Item::as_str)
    {
        return Ok(fs::canonicalize(
            requested
                .parent()
                .ok_or("manifest has no parent")?
                .join(explicit)
                .join("Cargo.toml"),
        )?);
    }
    for ancestor in requested
        .parent()
        .and_then(Path::parent)
        .into_iter()
        .flat_map(Path::ancestors)
    {
        let candidate = ancestor.join("Cargo.toml");
        let bytes = optional(&candidate)?;
        let has_workspace = if let Some(bytes) = &bytes {
            std::str::from_utf8(bytes)?
                .parse::<DocumentMut>()?
                .get("workspace")
                .is_some()
        } else {
            false
        };
        inventory.files.insert(candidate.clone(), bytes);
        if has_workspace {
            return Ok(candidate);
        }
    }
    Ok(requested.to_owned())
}

fn manifests(
    root: &Path,
    root_path: &Path,
    inventory: &Inventory,
) -> Result<BTreeMap<PathBuf, DocumentMut>, BoxError> {
    let doc = document(inventory, root_path)?;
    let mut manifests = BTreeMap::new();
    let members = strings(doc.get("workspace").and_then(|w| w.get("members")));
    let excluded = strings(doc.get("workspace").and_then(|w| w.get("exclude")));
    let workspace_dependencies = doc
        .get("workspace")
        .and_then(|w| w.get("dependencies"))
        .cloned();
    if doc.get("package").is_some() {
        manifests.insert(root_path.to_owned(), doc.clone());
    }
    if doc.get("workspace").is_some() {
        for member in &members {
            if member.contains("..") && !normalize(&root.join(member)).starts_with(root) {
                return Err(format!("workspace member outside discovery root: {member}").into());
            }
            if !member.contains(['*', '?', '[', '{']) {
                let directory = normalize(&root.join(member));
                if !inventory.files.contains_key(&directory.join("Cargo.toml"))
                    && !ignored_directory(&directory, inventory)?
                    && !matches(&excluded, member)?
                {
                    return Err(
                        format!("workspace member is missing or unsupported: {member}").into(),
                    );
                }
            }
        }
        for path in inventory
            .files
            .keys()
            .filter(|p| p.file_name().is_some_and(|s| s == "Cargo.toml"))
        {
            let Some(dir) = path.parent() else {
                continue;
            };
            let Ok(relative) = dir.strip_prefix(root) else {
                continue;
            };
            if matches(&members, &relative.to_string_lossy())?
                && !matches(&excluded, &relative.to_string_lossy())?
            {
                let member = document(inventory, path)?;
                if member.get("workspace").is_none() && member.get("package").is_some() {
                    manifests.insert(path.clone(), member);
                }
            }
        }
        // Cargo includes in-tree path dependencies as implicit workspace members.
        loop {
            let mut additions = BTreeMap::new();
            for (path, doc) in &manifests {
                for (_, _, table) in sections(doc) {
                    for (key, item) in table.iter() {
                        let inherited = item.get("workspace").and_then(Item::as_bool) == Some(true);
                        let item = if inherited {
                            workspace_dependencies
                                .as_ref()
                                .and_then(|dependencies| dependencies.get(key))
                                .unwrap_or(item)
                        } else {
                            item
                        };
                        if let Some(relative) = item.get("path").and_then(Item::as_str) {
                            let base = if inherited {
                                root
                            } else {
                                path.parent().ok_or("manifest parent missing")?
                            };
                            let dir = normalize(&base.join(relative));
                            let candidate = dir.join("Cargo.toml");
                            if dir.starts_with(root)
                                && !manifests.contains_key(&candidate)
                                && !matches(&excluded, &dir.strip_prefix(root)?.to_string_lossy())?
                                && inventory.files.contains_key(&candidate)
                            {
                                let member = document(inventory, &candidate)?;
                                if member.get("workspace").is_none() {
                                    additions.insert(candidate, member);
                                }
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
    }
    if manifests.is_empty() {
        return Err("no discoverable Cargo packages".into());
    }
    Ok(manifests)
}

fn sections(doc: &DocumentMut) -> Vec<(DependencyKind, Option<String>, &dyn toml_edit::TableLike)> {
    let mut result = Vec::new();
    for kind in [
        DependencyKind::Normal,
        DependencyKind::Dev,
        DependencyKind::Build,
    ] {
        if let Some(table) = doc.get(kind.section()).and_then(Item::as_table_like) {
            result.push((kind, None, table));
        }
        if let Some(targets) = doc.get("target").and_then(Item::as_table_like) {
            for (target, item) in targets.iter() {
                if let Some(table) = item.get(kind.section()).and_then(Item::as_table_like) {
                    result.push((kind, Some(target.to_owned()), table));
                }
            }
        }
    }
    result
}
fn layers<'a>(path: &Path, inventory: &'a Inventory) -> Vec<(&'a PathBuf, &'a MacheteConfig)> {
    let mut result = inventory
        .configs
        .iter()
        .filter(|(base, _)| path.starts_with(base))
        .collect::<Vec<_>>();
    result.sort_by_key(|(base, _)| base.components().count());
    if let Some((base, config)) = &inventory.explicit {
        result.push((base, config));
    }
    result
}
fn effective(layers: &[(&PathBuf, &MacheteConfig)], options: &MacheteOptions) -> MacheteConfig {
    let mut out = MacheteConfig::default();
    for (_, config) in layers {
        out.ignored.extend(config.ignored.clone());
        out.exclude.extend(config.exclude.clone());
        if !config.packages.is_empty() {
            out.packages.clone_from(&config.packages);
        }
        if !config.dependencies.is_empty() {
            out.dependencies.clone_from(&config.dependencies);
        }
        if !config.kinds.is_empty() {
            out.kinds.clone_from(&config.kinds);
        }
    }
    if !options.packages.is_empty() {
        out.packages.clone_from(&options.packages);
    }
    if !options.dependencies.is_empty() {
        out.dependencies.clone_from(&options.dependencies);
    }
    if !options.kinds.is_empty() {
        out.kinds.clone_from(&options.kinds);
    }
    out.exclude.extend(options.exclude.clone());
    out
}

#[derive(Default)]
struct PackageEvidence {
    normal: BTreeSet<String>,
    dev: BTreeSet<String>,
    build: BTreeSet<String>,
    diagnostics: BTreeSet<String>,
}
fn add_source(
    path: &Path,
    root: &Path,
    inventory: &mut Inventory,
    names: &mut BTreeSet<String>,
    diagnostics: &mut BTreeSet<String>,
    seen: &mut BTreeSet<PathBuf>,
) -> Result<(), BoxError> {
    let path = normalize(path);
    if !path.starts_with(root) {
        diagnostics.insert(format!("source outside discovery root: {}", path.display()));
        return Ok(());
    }
    if !seen.insert(path.clone()) {
        return Ok(());
    }
    let canonical = fs::canonicalize(&path);
    if canonical.as_ref().is_ok_and(|p| *p != path) {
        diagnostics.insert(format!("symlinked source: {}", path.display()));
        return Ok(());
    }
    if path.is_dir() {
        diagnostics.insert(format!(
            "source path is a directory, not a Rust file: {}",
            path.display()
        ));
        return Ok(());
    }
    let bytes = optional(&path)?;
    inventory.files.insert(path.clone(), bytes.clone());
    let Some(bytes) = bytes else {
        diagnostics.insert(format!("missing source: {}", path.display()));
        return Ok(());
    };
    let source_text = std::str::from_utf8(&bytes)
        .map_err(|error| format!("invalid UTF-8 source {}: {error}", path.display()))?;
    let evidence = source::inspect(source_text);
    names.extend(evidence.names);
    for reason in evidence.incomplete {
        diagnostics.insert(format!("{}: {reason}", path.display()));
    }
    for reason in evidence.advisory {
        inventory
            .diagnostics
            .insert(format!("advisory: {}: {reason}", path.display()));
    }
    for reason in evidence.uncertain {
        diagnostics.insert(format!(
            "advisory: {}: {reason}; unreferenced candidates remain protected",
            path.display()
        ));
    }
    for module in evidence.modules {
        let directory = path.parent().ok_or("source has no parent")?;
        let target_directory = directory.file_name().is_some_and(|name| {
            name == "tests"
                || name == "examples"
                || name == "benches"
                || (name == "bin"
                    && directory
                        .parent()
                        .is_some_and(|parent| parent.file_name().is_some_and(|name| name == "src")))
        });
        let module_base = if inventory.target_roots.contains(&path)
            || target_directory
            || path.file_name().is_some_and(|name| {
                name == "lib.rs" || name == "main.rs" || name == "mod.rs" || name == "build.rs"
            }) {
            directory.to_owned()
        } else {
            directory.join(path.file_stem().ok_or("source stem missing")?)
        };
        let direct = module_base.join(format!("{module}.rs"));
        let nested = module_base.join(&module).join("mod.rs");
        let candidate = if direct.exists() { direct } else { nested };
        if !ignored_directory(&candidate, inventory)? {
            add_source(&candidate, root, inventory, names, diagnostics, seen)?;
        }
    }
    for include in evidence.includes {
        let include = normalize(&path.parent().ok_or("source has no parent")?.join(include));
        if include.is_dir() {
            diagnostics.insert(format!(
                "included source path is a directory: {}",
                include.display()
            ));
            continue;
        }
        if include.extension().is_some_and(|s| s == "rs") {
            add_source(&include, root, inventory, names, diagnostics, seen)?;
        } else if !include.starts_with(root) {
            diagnostics.insert(format!(
                "include outside discovery root: {}",
                include.display()
            ));
        } else {
            let bytes = optional(&include)?;
            if bytes.is_none() {
                diagnostics.insert(format!("missing included file: {}", include.display()));
            }
            inventory.files.insert(include, bytes);
        }
    }
    Ok(())
}

fn package_evidence(
    path: &Path,
    doc: &DocumentMut,
    manifests: &BTreeMap<PathBuf, DocumentMut>,
    root: &Path,
    inventory: &mut Inventory,
) -> Result<PackageEvidence, BoxError> {
    let directory = path.parent().ok_or("manifest has no parent")?;
    let mut evidence = PackageEvidence::default();
    let build = doc.get("package").and_then(|p| p.get("build"));
    let build_path = match build.and_then(Item::as_str) {
        Some(value) => Some(directory.join(value)),
        None if build.and_then(Item::as_bool) == Some(false) => None,
        None if directory.join("build.rs").exists() => Some(directory.join("build.rs")),
        None => None,
    };
    let mut sources = BTreeMap::new();
    for file in inventory
        .files
        .keys()
        .filter(|p| p.extension().is_some_and(|s| s == "rs") && p.starts_with(directory))
    {
        if manifests.keys().any(|other| {
            other != path
                && other
                    .parent()
                    .is_some_and(|dir| dir.starts_with(directory) && file.starts_with(dir))
        }) {
            continue;
        }
        let relative = file.strip_prefix(directory)?;
        if relative.starts_with("src") {
            sources.insert(file.clone(), DependencyKind::Normal);
        } else if ["tests", "examples", "benches"]
            .iter()
            .any(|dir| relative.starts_with(dir))
        {
            sources.insert(file.clone(), DependencyKind::Dev);
        }
    }
    if let Some(build_path) = build_path {
        inventory.target_roots.insert(build_path.clone());
        sources.insert(build_path, DependencyKind::Build);
    }
    if let Some(lib) = doc.get("lib")
        && let Some(value) = lib.get("path").and_then(Item::as_str)
    {
        let target_path = normalize(&directory.join(value));
        inventory.target_roots.insert(target_path.clone());
        sources.insert(target_path, DependencyKind::Normal);
    }
    for (section, kind, default_dir) in [
        ("bin", DependencyKind::Normal, "src/bin"),
        ("test", DependencyKind::Dev, "tests"),
        ("example", DependencyKind::Dev, "examples"),
        ("bench", DependencyKind::Dev, "benches"),
    ] {
        if let Some(targets) = doc.get(section).and_then(Item::as_array_of_tables) {
            for target in targets {
                if let Some(value) = target.get("path").and_then(Item::as_str) {
                    let target_path = normalize(&directory.join(value));
                    inventory.target_roots.insert(target_path.clone());
                    sources.insert(target_path, kind);
                } else if let Some(name) = target.get("name").and_then(Item::as_str) {
                    let conventional = directory.join(default_dir).join(format!("{name}.rs"));
                    let nested = directory.join(default_dir).join(name).join("main.rs");
                    if !conventional.exists()
                        && !nested.exists()
                        && (section != "bin" || !directory.join("src/main.rs").exists())
                    {
                        evidence.diagnostics.insert(format!(
                            "unresolved {section} target {name} in {}",
                            path.display()
                        ));
                    }
                }
            }
        }
    }
    if sources.is_empty() {
        evidence
            .diagnostics
            .insert(format!("no Rust targets discovered in {}", path.display()));
    }
    for (source, kind) in sources {
        // Explicitly ignored source trees remain excluded, even for named targets.
        if ignored_directory(&source, inventory)? {
            continue;
        }
        let names = match kind {
            DependencyKind::Normal => &mut evidence.normal,
            DependencyKind::Dev => &mut evidence.dev,
            DependencyKind::Build => &mut evidence.build,
        };
        add_source(
            &source,
            root,
            inventory,
            names,
            &mut evidence.diagnostics,
            &mut BTreeSet::new(),
        )?;
    }
    Ok(evidence)
}

fn feature_reference(doc: &DocumentMut, key: &str) -> bool {
    doc.get("features")
        .and_then(Item::as_table_like)
        .is_some_and(|features| {
            features.iter().any(|(_, item)| {
                item.as_array().is_some_and(|a| {
                    a.iter().filter_map(toml_edit::Value::as_str).any(|s| {
                        s == key
                            || s == format!("dep:{key}")
                            || s.split('/')
                                .next()
                                .is_some_and(|head| head.trim_end_matches('?') == key)
                    })
                })
            })
        })
}
fn implicit_reference(key: &str, doc: &DocumentMut) -> bool {
    key == "proc-macro"
        || (key == "std" || key == "core" || key == "alloc")
        || (key == "test"
            && doc
                .get("lib")
                .and_then(|v| v.get("harness"))
                .and_then(Item::as_bool)
                != Some(false))
}

/// Analyze native Rust source and manifests without Cargo, networking or writes.
///
/// # Errors
/// * Missing/inaccessible paths, invalid TOML/configuration, invalid glob patterns.
/// * Invalid workspace membership or unreadable/non-UTF-8 source.
pub fn analyze_machete(options: &MacheteOptions) -> Result<MacheteReport, BoxError> {
    Ok(plan_machete(options)?.report)
}

/// Capture a deterministic exact removal plan without Cargo, networking or writes.
///
/// Incomplete reports may contain candidate changes for review, but cannot be applied.
/// Feature-referenced dependencies are retained; workspace owners are removed only
/// when every discovered consumer occurrence is removed. Cargo.lock is preserved.
///
/// # Errors
/// * All discovery/analysis errors documented by [`analyze_machete`].
/// * Invalid manifest dependency tables or formatting-preserving edit failures.
#[allow(clippy::too_many_lines)] // A single read-only capture ties analysis evidence to its exact edits.
pub fn plan_machete(options: &MacheteOptions) -> Result<MachetePlan, BoxError> {
    for list in [&options.packages, &options.dependencies, &options.exclude] {
        patterns(list)?;
    }
    let requested = options
        .manifest_path
        .clone()
        .unwrap_or(std::env::current_dir()?);
    let requested = fs::canonicalize(if requested.is_dir() {
        requested.join("Cargo.toml")
    } else {
        requested
    })?;
    let mut inventory = Inventory {
        no_ignore: options.no_ignore,
        ..Default::default()
    };
    let root_path = root_manifest(&requested, &mut inventory)?;
    let root = root_path
        .parent()
        .ok_or("root manifest has no parent")?
        .to_owned();
    if let Some(config_path) = &options.config_path {
        let config_path = fs::canonicalize(config_path)?;
        let base = config_path
            .parent()
            .ok_or("configuration has no parent")?
            .to_owned();
        let bytes = fs::read(&config_path)?;
        let value: toml::Value = toml::from_str(std::str::from_utf8(&bytes)?)?;
        let config: MacheteConfig = value
            .get("machete")
            .cloned()
            .unwrap_or_else(|| toml::Value::Table(toml::map::Map::default()))
            .try_into()?;
        for list in [
            &config.ignored,
            &config.ignored_dirs,
            &config.packages,
            &config.dependencies,
            &config.exclude,
        ] {
            patterns(list)?;
        }
        for rule in &config.rules {
            for list in [
                &rule.packages,
                &rule.dependencies,
                &rule.ignored,
                &rule.ignored_dirs,
            ] {
                patterns(list)?;
            }
        }
        inventory.files.insert(config_path, Some(bytes));
        inventory.explicit = Some((base, config));
    }
    for ancestor in root.ancestors().skip(1) {
        config_at(ancestor, &mut inventory)?;
    }
    walk(&root, &mut inventory)?;
    // Also protect absence of the workspace lockfile.
    inventory
        .files
        .insert(root.join("Cargo.lock"), optional(&root.join("Cargo.lock"))?);
    let mut manifests = manifests(&root, &root_path, &inventory)?;
    if requested == root_path {
        for path in inventory.files.keys().filter(|path| {
            path.starts_with(&root) && path.file_name().is_some_and(|name| name == "Cargo.toml")
        }) {
            let doc = document(&inventory, path)?;
            if doc.get("package").is_some() && !manifests.contains_key(path) {
                manifests.insert(path.clone(), doc);
                // Independent packages may have different workspace owners; do
                // not prune the top-level shared owner based on a partial graph.
                inventory.omitted_directories = true;
            }
        }
    }
    if !manifests.contains_key(&requested) && requested != root_path {
        return Err(
            "requested manifest is not a discoverable workspace member (possibly excluded)".into(),
        );
    }
    let root_doc = document(&inventory, &root_path)?;
    if let Some(metadata) = &options.metadata {
        for (path, contents) in &metadata.manifests {
            if optional(path)?.as_deref() != Some(contents.as_bytes()) {
                return Err(format!("stale imported metadata manifest: {}", path.display()).into());
            }
            inventory
                .files
                .insert(path.clone(), Some(contents.as_bytes().to_vec()));
        }
    }
    let mut dependencies = Vec::new();
    for (path, doc) in &manifests {
        let mut owner_doc = root_doc.clone();
        let mut owner_directory = root.clone();
        for ancestor in path.parent().into_iter().flat_map(Path::ancestors) {
            let candidate = ancestor.join("Cargo.toml");
            if candidate.starts_with(&root)
                && inventory.files.get(&candidate).is_some_and(Option::is_some)
            {
                let doc = document(&inventory, &candidate)?;
                if doc.get("workspace").is_some() {
                    owner_doc = doc;
                    ancestor.clone_into(&mut owner_directory);
                    break;
                }
            }
        }
        let package = doc
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(Item::as_str)
            .ok_or("package name missing")?;
        let layers = layers(path, &inventory);
        let config = effective(&layers, options);
        let package_selected =
            (requested == root_path || !options.packages.is_empty() || *path == requested)
                && selected(&config.packages, package)?
                && !matches(&config.exclude, package)?;
        let rules = layers
            .iter()
            .flat_map(|(base, config)| {
                config
                    .rules
                    .iter()
                    .map(move |rule| ((*base).clone(), rule.clone()))
            })
            .collect::<Vec<_>>();
        let evidence = if package_selected {
            package_evidence(path, doc, &manifests, &root, &mut inventory)?
        } else {
            PackageEvidence::default()
        };
        inventory
            .diagnostics
            .extend(evidence.diagnostics.iter().cloned());
        for (kind, target, table) in sections(doc) {
            for (key, item) in table.iter() {
                let workspace = item.get("workspace").and_then(Item::as_bool) == Some(true);
                let owner = if workspace {
                    owner_doc
                        .get("workspace")
                        .and_then(|w| w.get("dependencies"))
                        .and_then(|d| d.get(key))
                        .ok_or_else(|| format!("missing workspace owner for {key}"))?
                } else {
                    item
                };
                let actual = owner.get("package").and_then(Item::as_str).unwrap_or(key);
                let mut name = if owner.get("path").is_some() {
                    key.replace('-', "_")
                } else {
                    options
                        .metadata
                        .as_ref()
                        .and_then(|metadata| metadata.crate_names.get(path))
                        .and_then(|names| names.get(&key.replace('-', "_")))
                        .cloned()
                        .unwrap_or_else(|| key.replace('-', "_"))
                };
                // An explicit alias is the extern-prelude name; otherwise Cargo
                // exposes the dependency's library target, not its package name.
                if key == actual
                    && let Some(relative) = owner.get("path").and_then(Item::as_str)
                {
                    let base = if workspace {
                        &owner_directory
                    } else {
                        path.parent().ok_or("manifest has no parent")?
                    };
                    let local_manifest = normalize(&base.join(relative).join("Cargo.toml"));
                    let bytes = optional(&local_manifest)?.ok_or_else(|| {
                        format!(
                            "local dependency manifest missing: {}",
                            local_manifest.display()
                        )
                    })?;
                    let local: DocumentMut = std::str::from_utf8(&bytes)
                        .map_err(|error| {
                            format!("invalid UTF-8 in {}: {error}", local_manifest.display())
                        })?
                        .parse()
                        .map_err(|error| {
                            format!(
                                "invalid dependency manifest {}: {error}",
                                local_manifest.display()
                            )
                        })?;
                    if let Some(lib_name) = local
                        .get("lib")
                        .and_then(|lib| lib.get("name"))
                        .and_then(Item::as_str)
                    {
                        name = lib_name.replace('-', "_");
                    }
                    inventory.files.insert(local_manifest, Some(bytes));
                }
                let is_selected = package_selected
                    && (config.dependencies.is_empty()
                        || dependency_matches(&config.dependencies, key, actual)?)
                    && if config.kinds.is_empty() {
                        kind != DependencyKind::Dev
                    } else {
                        config.kinds.contains(&kind)
                    };
                let mut ignored = dependency_matches(&config.ignored, key, actual)?;
                let mut rule_excluded = false;
                for (base, rule) in &rules {
                    if selected(&rule.packages, package)?
                        && (rule.dependencies.is_empty()
                            || dependency_matches(&rule.dependencies, key, actual)?)
                    {
                        ignored |= dependency_matches(&rule.ignored, key, actual)?;
                        // Scoped directory exclusions only suppress evidence from those
                        // files, conservatively retaining the affected occurrence.
                        rule_excluded |= !rule.ignored_dirs.is_empty()
                            && inventory.files.keys().any(|file| {
                                file.extension().is_some_and(|s| s == "rs")
                                    && file.starts_with(path.parent().unwrap_or(&root))
                                    && directory_match(base, &rule.ignored_dirs, file)
                                        .unwrap_or(false)
                            });
                    }
                }
                let used_derives = (key == "serde"
                    && ["Serialize", "Deserialize"].iter().any(|derive| {
                        evidence.normal.contains(*derive) || evidence.dev.contains(*derive)
                    }))
                    || (key == "thiserror"
                        && (evidence.normal.contains("Error") || evidence.dev.contains("Error")));
                let used = feature_reference(doc, key)
                    || implicit_reference(key, doc)
                    || rule_excluded
                    || used_derives
                    || match kind {
                        DependencyKind::Normal | DependencyKind::Dev => evidence
                            .normal
                            .iter()
                            .chain(&evidence.dev)
                            .any(|observed| observed.eq_ignore_ascii_case(&name)),
                        DependencyKind::Build => evidence
                            .build
                            .iter()
                            .any(|observed| observed.eq_ignore_ascii_case(&name)),
                    };
                let (status, reason) = if !is_selected {
                    (DependencyStatus::Skipped, "excluded by selector")
                } else if ignored {
                    (DependencyStatus::Ignored, "ignored by clippier.toml")
                } else if used {
                    (
                        DependencyStatus::Used,
                        "source, feature, implicit tool, or excluded-source reference",
                    )
                } else if !evidence.diagnostics.is_empty() {
                    (DependencyStatus::Incomplete, "incomplete source inventory")
                } else {
                    (
                        DependencyStatus::Unused,
                        "no lexical or structural reference found",
                    )
                };
                dependencies.push(MacheteDependency {
                    package: package.to_owned(),
                    manifest_path: path.clone(),
                    dependency: key.to_owned(),
                    dependency_package: actual.to_owned(),
                    kind,
                    target: target.clone(),
                    workspace,
                    status,
                    reason: reason.to_owned(),
                });
            }
        }
    }
    dependencies.sort_by(|a, b| {
        (&a.manifest_path, a.kind, &a.target, &a.dependency).cmp(&(
            &b.manifest_path,
            b.kind,
            &b.target,
            &b.dependency,
        ))
    });
    let mut edited = manifests.clone();
    edited.entry(root_path.clone()).or_insert(root_doc);
    for dependency in dependencies
        .iter()
        .filter(|d| d.status == DependencyStatus::Unused)
    {
        let doc = edited
            .get_mut(&dependency.manifest_path)
            .ok_or("missing editable manifest")?;
        let section = if let Some(target) = &dependency.target {
            doc.get_mut("target")
                .and_then(|t| t.get_mut(target))
                .and_then(|t| t.get_mut(dependency.kind.section()))
        } else {
            doc.get_mut(dependency.kind.section())
        };
        section
            .and_then(Item::as_table_like_mut)
            .ok_or("missing dependency section")?
            .remove(&dependency.dependency);
    }
    let inherited = dependencies
        .iter()
        .filter(|d| d.workspace)
        .map(|d| d.dependency.clone())
        .collect::<BTreeSet<_>>();
    for key in inherited {
        if inventory
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.starts_with("advisory:"))
            && !dependencies
                .iter()
                .any(|dependency| dependency.status == DependencyStatus::Incomplete)
            && !inventory.omitted_directories
            && dependencies
                .iter()
                .filter(|d| d.workspace && d.dependency == key)
                .all(|d| d.status == DependencyStatus::Unused)
        {
            // Root feature references are also ownership evidence.
            if !feature_reference(edited.get(&root_path).ok_or("root missing")?, &key)
                && let Some(table) = edited
                    .get_mut(&root_path)
                    .and_then(|d| d.get_mut("workspace"))
                    .and_then(|w| w.get_mut("dependencies"))
                    .and_then(Item::as_table_like_mut)
            {
                table.remove(&key);
            }
        }
    }
    let mut changes = Vec::new();
    for (path, doc) in edited {
        let before = text(&inventory, &path)?.to_owned();
        let after = doc.to_string();
        if before != after {
            let _: DocumentMut = after.parse()?;
            changes.push(MacheteChange {
                path,
                before,
                after,
            });
        }
    }
    let complete = inventory
        .diagnostics
        .iter()
        .all(|diagnostic| diagnostic.starts_with("advisory:"))
        && !dependencies
            .iter()
            .any(|dependency| dependency.status == DependencyStatus::Incomplete);
    let report = MacheteReport {
        root,
        dependencies,
        diagnostics: inventory.diagnostics.into_iter().collect(),
        complete,
        applied: false,
        lockfile_preserved: true,
    };
    let sealed_changes = changes
        .iter()
        .map(|c| (c.path.clone(), c.before.clone(), c.after.clone()))
        .collect();
    Ok(MachetePlan {
        report,
        changes,
        snapshots: inventory.files,
        directories: inventory.directories,
        sealed_changes,
        complete,
    })
}
