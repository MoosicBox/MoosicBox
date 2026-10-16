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
    explicit_config_path: Option<PathBuf>,
    omitted_directories: bool,
    target_roots: BTreeSet<PathBuf>,
    ignore_matchers: BTreeMap<PathBuf, ignore::gitignore::Gitignore>,
    no_ignore: Option<bool>,
    diagnostics: BTreeSet<String>,
    precision: bool,
    profile: Option<super::AnalysisProfile>,
    source_kind: Option<DependencyKind>,
    conditions: super::conditions::ConditionQueries,
    source_evidence: BTreeMap<PathBuf, std::rc::Rc<source::Evidence>>,
    observations: Vec<(PathBuf, DependencyKind, super::resolution::Observation)>,
    observation_targets: BTreeMap<usize, (String, Option<String>)>,
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
    for (base, config) in layers {
        out.profiles.extend(config.profiles.clone());
        #[cfg(feature = "machete-verify")]
        if config.verification.is_some() {
            out.verification.clone_from(&config.verification);
        }
        if config.evidence_path.is_some() {
            out.evidence_path = config
                .evidence_path
                .as_ref()
                .map(|path| normalize(&base.join(path)));
        }
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
type ReferencePosition = (usize, usize, String);

fn resolved_import_gaps(
    syntax: &super::resolution::syntax::SyntaxIndex,
    instance: &super::module_graph::SourceInstance,
    graph: &super::module_graph::ModuleGraph,
) -> BTreeSet<String> {
    use super::module_graph::Namespace;
    use super::resolution::syntax::{Namespace as SyntaxNamespace, Resolution};
    let mut resolved_import_gaps = BTreeSet::<String>::new();
    let mut references_by_import = BTreeMap::<usize, Vec<_>>::new();
    for reference in &syntax.references {
        if let Resolution::Import(import) = reference.resolution {
            references_by_import
                .entry(import)
                .or_default()
                .push(reference);
        }
    }
    for (import_id, import) in syntax.imports.iter().enumerate() {
        if import.absolute || import.glob || !import.conditions.is_empty() {
            continue;
        }
        let Some((name, prefix)) = import.path.split_last() else {
            continue;
        };
        let Some(owner) = graph.child_module(instance.module, &syntax.scopes[import.scope].module)
        else {
            continue;
        };
        let resolved = [Namespace::Type, Namespace::Value, Namespace::Macro]
            .into_iter()
            .flat_map(|namespace| graph.imported(owner, prefix, name, namespace))
            .collect::<Vec<_>>();
        let required_namespaces_resolved = references_by_import
            .get(&import_id)
            .into_iter()
            .flatten()
            .all(|reference| {
                let namespace = match reference.namespace {
                    SyntaxNamespace::Type => Namespace::Type,
                    SyntaxNamespace::Value => Namespace::Value,
                    SyntaxNamespace::Macro => Namespace::Macro,
                    SyntaxNamespace::Lifetime => return false,
                };
                let bindings = graph.imported(owner, prefix, name, namespace);
                if bindings.is_empty()
                    && namespace == Namespace::Type
                    && graph.glob_resolved(owner, prefix)
                    && resolved
                        .iter()
                        .all(|binding| binding.definition.2 == Namespace::Value)
                {
                    return true;
                }
                !bindings.is_empty()
                    && bindings.iter().all(|binding| !binding.conditional)
                    && bindings
                        .iter()
                        .map(|binding| &binding.definition)
                        .collect::<BTreeSet<_>>()
                        .len()
                        == 1
            });
        if !resolved.is_empty()
            && resolved.iter().all(|binding| !binding.conditional)
            && required_namespaces_resolved
        {
            resolved_import_gaps.insert(format!(
                "import target namespace requires module graph resolution: {} ({}{} at {}:{})",
                import.path.join("::"),
                if import.public { "reexport " } else { "" },
                import.alias.as_deref().unwrap_or("*"),
                import.line,
                import.column
            ));
        }
    }
    resolved_import_gaps
}

/// An imported value does not occupy the type namespace used by crate roots.
fn value_only_external_references(
    syntax: &super::resolution::syntax::SyntaxIndex,
    instance: &super::module_graph::SourceInstance,
    graph: &super::module_graph::ModuleGraph,
) -> BTreeSet<ReferencePosition> {
    use super::module_graph::Namespace;
    use super::resolution::syntax::{Namespace as SyntaxNamespace, Resolution};
    let mut positions = BTreeSet::new();
    for reference in &syntax.references {
        let Resolution::Import(import_id) = reference.resolution else {
            continue;
        };
        if reference.namespace != SyntaxNamespace::Type || reference.path.len() < 2 {
            continue;
        }
        let import = &syntax.imports[import_id];
        if import.absolute || import.glob || !import.conditions.is_empty() {
            continue;
        }
        let Some((name, prefix)) = import.path.split_last() else {
            continue;
        };
        let Some(owner) = graph.child_module(instance.module, &syntax.scopes[import.scope].module)
        else {
            continue;
        };
        if !graph.glob_resolved(owner, prefix)
            || !graph
                .imported(owner, prefix, name, Namespace::Type)
                .is_empty()
            || !graph
                .imported(owner, prefix, name, Namespace::Macro)
                .is_empty()
        {
            continue;
        }
        let values = graph.imported(owner, prefix, name, Namespace::Value);
        if values.is_empty() || values.iter().any(|binding| binding.conditional) {
            continue;
        }
        positions.insert((reference.line, reference.column, reference.path[0].clone()));
    }
    positions
}

fn local_reference_positions(
    syntax: &super::resolution::syntax::SyntaxIndex,
    instance: &super::module_graph::SourceInstance,
    graph: &super::module_graph::ModuleGraph,
) -> BTreeSet<ReferencePosition> {
    use super::module_graph::Namespace;
    use super::resolution::syntax::{Namespace as SyntaxNamespace, Resolution};
    let path = &instance.path;
    let mut local_references = BTreeSet::new();
    for reference in &syntax.references {
        if reference.absolute
            || !matches!(
                reference.resolution,
                Resolution::External | Resolution::Import(_)
            )
        {
            continue;
        }
        let Some(name) = reference.path.first() else {
            continue;
        };
        let namespace = if reference.path.len() > 1 {
            Namespace::Type
        } else {
            match reference.namespace {
                SyntaxNamespace::Type => Namespace::Type,
                SyntaxNamespace::Value => Namespace::Value,
                SyntaxNamespace::Macro => Namespace::Macro,
                SyntaxNamespace::Lifetime => continue,
            }
        };
        let Some(owner) =
            graph.child_module(instance.module, &syntax.scopes[reference.scope].module)
        else {
            continue;
        };
        if graph
            .sources
            .iter()
            .filter(|source| source.path == *path)
            .count()
            == 1
            && graph.module_at(path, reference.line) != Some(owner)
        {
            continue;
        }
        let mut bindings = graph.lookup(owner, name, namespace);
        let mut scope = Some(reference.scope);
        while bindings.is_empty() {
            let Some(current) = scope else {
                break;
            };
            for import in syntax.imports.iter().filter(|import| {
                import.scope == current
                    && import.glob
                    && !import.absolute
                    && import.conditions.is_empty()
            }) {
                bindings.extend(graph.imported(owner, &import.path, name, namespace));
            }
            scope = syntax.scopes[current].parent;
        }
        if !bindings.is_empty()
            && bindings.iter().all(|binding| !binding.conditional)
            && bindings
                .iter()
                .map(|binding| &binding.definition)
                .collect::<BTreeSet<_>>()
                .len()
                == 1
        {
            local_references.insert((reference.line, reference.column, name.clone()));
        }
    }
    local_references
}

fn source_diagnostics(
    path: &Path,
    root: &Path,
    inventory: &mut Inventory,
    diagnostics: &mut BTreeSet<String>,
    context: (
        &source::Evidence,
        &super::module_graph::SourceInstance,
        &super::module_graph::ModuleGraph,
    ),
    resolved_import_gaps: &BTreeSet<String>,
) -> Result<(), BoxError> {
    let (evidence, instance, graph) = context;
    for reason in &evidence.incomplete {
        if resolved_import_gaps.contains(reason) {
            continue;
        }
        if reason == "glob import requires cross-file namespace resolution"
            && evidence.syntax.as_ref().is_some_and(|syntax| {
                syntax
                    .imports
                    .iter()
                    .filter(|import| import.glob)
                    .all(|import| {
                        !import.absolute
                            && import.conditions.is_empty()
                            && graph
                                .child_module(instance.module, &syntax.scopes[import.scope].module)
                                .is_some_and(|owner| graph.glob_resolved(owner, &import.path))
                    })
            })
        {
            continue;
        }
        diagnostics.insert(format!("{}: {reason}", path.display()));
    }
    for reason in &evidence.advisory {
        inventory
            .diagnostics
            .insert(format!("advisory: {}: {reason}", path.display()));
    }
    for reason in &evidence.uncertain {
        diagnostics.insert(format!(
            "advisory: {}: {reason}; unreferenced candidates remain protected",
            path.display()
        ));
    }
    // Rust module/include edges were traversed in semantic module contexts by
    // ModuleGraph. Retain non-Rust include snapshots for plan staleness checks.
    for include in &evidence.includes {
        let include = normalize(&path.parent().ok_or("source has no parent")?.join(include));
        if include.is_dir() {
            diagnostics.insert(format!(
                "included source path is a directory: {}",
                include.display()
            ));
            continue;
        }
        if include.extension().is_some_and(|s| s == "rs") {
            continue;
        }
        if include.starts_with(root) {
            let bytes = optional(&include)?;
            if bytes.is_none() {
                diagnostics.insert(format!("missing included file: {}", include.display()));
            }
            inventory.files.insert(include, bytes);
        } else {
            diagnostics.insert(format!(
                "include outside discovery root: {}",
                include.display()
            ));
        }
    }
    Ok(())
}

fn record_source_evidence(
    path: &Path,
    inventory: &mut Inventory,
    names: &mut BTreeSet<String>,
    diagnostics: &mut BTreeSet<String>,
    context: (&source::Evidence, &super::module_graph::SourceInstance),
    local_references: &BTreeSet<ReferencePosition>,
) {
    let (evidence, instance) = context;
    inventory
        .observations
        .extend(
            evidence
                .observations
                .iter()
                .cloned()
                .filter_map(|mut observation| {
                    if local_references.contains(&(
                        observation.line,
                        observation.column,
                        observation.name.clone(),
                    )) {
                        return None;
                    }
                    observation
                        .conditions
                        .extend(instance.conditions.iter().cloned());
                    if observation.conditions.iter().any(|condition| {
                        inventory.profile.as_ref().is_some_and(|profile| {
                            super::evaluate_condition(condition, profile)
                                == super::ConditionValue::False
                        })
                    }) {
                        return None;
                    }
                    let satisfiability =
                        inventory.conditions.satisfiability(&observation.conditions);
                    if satisfiability == super::conditions::Satisfiability::Exhausted {
                        diagnostics.insert("condition satisfiability budget exhausted".into());
                    }
                    (satisfiability != super::conditions::Satisfiability::Unsat).then(|| {
                        (
                            path.to_owned(),
                            inventory.source_kind.unwrap_or(DependencyKind::Normal),
                            observation,
                        )
                    })
                }),
        );
    let retained_names = inventory
        .observations
        .iter()
        .filter(|(file, _, _)| file == path)
        .map(|(_, _, observation)| observation)
        .filter(|observation| {
            !local_references.contains(&(
                observation.line,
                observation.column,
                observation.name.clone(),
            ))
        })
        .map(|observation| observation.name.as_str())
        .collect::<BTreeSet<_>>();
    names.extend(
        evidence
            .names
            .iter()
            .filter(|name| {
                !inventory.precision
                    || retained_names.contains(name.as_str())
                    || !evidence
                        .observations
                        .iter()
                        .any(|observation| observation.name == **name)
            })
            .cloned(),
    );
}

fn record_namespace_fallback(
    text: &str,
    inventory: &mut Inventory,
    names: &mut BTreeSet<String>,
    diagnostics: &mut BTreeSet<String>,
    evidence: &source::Evidence,
    instance: &super::module_graph::SourceInstance,
    graph: &super::module_graph::ModuleGraph,
) {
    let path = &instance.path;
    if let Some(syntax) = &evidence.syntax {
        let fallback = value_only_external_references(syntax, instance, graph);
        if !fallback.is_empty() {
            // Reuse tokenizer-owned cfg/span information rather than inventing
            // unconditional evidence for an AST reference in a conditional body.
            let mut lexical = source::inspect_with(text, false, inventory.profile.as_ref());
            lexical.observations.retain(|observation| {
                fallback.contains(&(
                    observation.line,
                    observation.column,
                    observation.name.clone(),
                ))
            });
            lexical.names = lexical
                .observations
                .iter()
                .map(|observation| observation.name.clone())
                .collect();
            record_source_evidence(
                path,
                inventory,
                names,
                diagnostics,
                (&lexical, instance),
                &BTreeSet::new(),
            );
        }
    }
}

fn add_source(
    path: &Path,
    root: &Path,
    inventory: &mut Inventory,
    names: &mut BTreeSet<String>,
    diagnostics: &mut BTreeSet<String>,
    instance: &super::module_graph::SourceInstance,
    graph: &super::module_graph::ModuleGraph,
) -> Result<(), BoxError> {
    let path = normalize(path);
    if !path.starts_with(root) {
        diagnostics.insert(format!("source outside discovery root: {}", path.display()));
        return Ok(());
    }
    if ignored_directory(&path, inventory)? {
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
    let bytes = if let Some(bytes) = inventory.files.get(&path) {
        bytes.clone()
    } else {
        let bytes = optional(&path)?;
        inventory.files.insert(path.clone(), bytes.clone());
        bytes
    };
    let Some(bytes) = bytes else {
        diagnostics.insert(format!("missing source: {}", path.display()));
        return Ok(());
    };
    let source_text = std::str::from_utf8(&bytes)
        .map_err(|error| format!("invalid UTF-8 source {}: {error}", path.display()))?;
    let evidence = if let Some(evidence) = inventory.source_evidence.get(&path) {
        std::rc::Rc::clone(evidence)
    } else {
        let evidence = std::rc::Rc::new(source::inspect_parsed(
            source_text,
            inventory.precision,
            inventory.profile.as_ref(),
            graph.parsed(&path),
        ));
        inventory
            .source_evidence
            .insert(path.clone(), std::rc::Rc::clone(&evidence));
        evidence
    };
    let (local_references, resolved_import_gaps) = evidence.syntax.as_ref().map_or_else(
        || (BTreeSet::new(), BTreeSet::new()),
        |syntax| {
            (
                local_reference_positions(syntax, instance, graph),
                resolved_import_gaps(syntax, instance, graph),
            )
        },
    );
    record_namespace_fallback(
        source_text,
        inventory,
        names,
        diagnostics,
        &evidence,
        instance,
        graph,
    );
    record_source_evidence(
        &path,
        inventory,
        names,
        diagnostics,
        (&evidence, instance),
        &local_references,
    );
    source_diagnostics(
        &path,
        root,
        inventory,
        diagnostics,
        (&evidence, instance, graph),
        &resolved_import_gaps,
    )
}

fn source_graph(
    source: &Path,
    root: &Path,
    inventory: &mut Inventory,
) -> Result<super::module_graph::ModuleGraph, BoxError> {
    let profile = inventory.profile.clone();
    let graph = super::module_graph::ModuleGraph::build(source, profile.as_ref(), |candidate| {
        let candidate = normalize(candidate);
        if !candidate.starts_with(root) {
            inventory.diagnostics.insert(format!(
                "source outside discovery root: {}",
                candidate.display()
            ));
            return Ok(None);
        }
        if ignored_directory(&candidate, inventory)? {
            return Ok(Some(String::new()));
        }
        if fs::canonicalize(&candidate)
            .as_ref()
            .is_ok_and(|actual| *actual != candidate)
        {
            inventory
                .diagnostics
                .insert(format!("symlinked source: {}", candidate.display()));
            return Ok(None);
        }
        if candidate.is_dir() {
            inventory.diagnostics.insert(format!(
                "source path is a directory, not a Rust file: {}",
                candidate.display()
            ));
            return Ok(None);
        }
        let bytes = if let Some(bytes) = inventory.files.get(&candidate) {
            bytes.clone()
        } else {
            let bytes = optional(&candidate)?;
            inventory.files.insert(candidate.clone(), bytes.clone());
            bytes
        };
        bytes
            .map(|bytes| {
                String::from_utf8(bytes).map_err(|error| {
                    format!("invalid UTF-8 source {}: {error}", candidate.display()).into()
                })
            })
            .transpose()
    })?;
    Ok(graph)
}

/// Validate orphan snapshots without inventing compilation units or use evidence.
fn validate_orphan_sources(
    manifest: &Path,
    manifests: &BTreeMap<PathBuf, DocumentMut>,
    inventory: &Inventory,
    reached: &BTreeSet<PathBuf>,
    diagnostics: &mut BTreeSet<String>,
) -> Result<(), BoxError> {
    let directory = manifest.parent().ok_or("manifest has no parent")?;
    let candidates = inventory
        .files
        .keys()
        .filter(|file| {
            file.extension().is_some_and(|extension| extension == "rs")
                && file.starts_with(directory)
                && !reached.contains(*file)
        })
        .filter(|file| {
            !manifests.keys().any(|other| {
                other != manifest
                    && other.parent().is_some_and(|nested| {
                        nested.starts_with(directory) && file.starts_with(nested)
                    })
            })
        })
        .cloned()
        .collect::<Vec<_>>();
    for file in candidates {
        if ignored_directory(&file, inventory)? {
            continue;
        }
        let relative = file.strip_prefix(directory)?;
        if !["src", "tests", "examples", "benches"]
            .iter()
            .any(|root| relative.starts_with(root))
        {
            continue;
        }
        if let Some(bytes) = inventory.files.get(&file).and_then(Option::as_ref) {
            let text = std::str::from_utf8(bytes)
                .map_err(|error| format!("invalid UTF-8 source {}: {error}", file.display()))?;
            if let Err(error) = syn::parse_file(text) {
                diagnostics.insert(format!(
                    "cannot parse orphan source {}: {error}",
                    file.display()
                ));
            }
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
    let mut evidence = PackageEvidence::default();
    let sources = package_sources(path, doc, manifests, inventory, &mut evidence.diagnostics)?;
    if sources.is_empty() {
        evidence
            .diagnostics
            .insert(format!("no Rust targets discovered in {}", path.display()));
    }
    let mut reached = BTreeSet::new();
    for (source, kind) in sources {
        // Explicitly ignored source trees remain excluded, even for named targets.
        if ignored_directory(&source, inventory)? {
            continue;
        }
        inventory.source_kind = Some(kind);
        let names = match kind {
            DependencyKind::Normal => &mut evidence.normal,
            DependencyKind::Dev => &mut evidence.dev,
            DependencyKind::Build => &mut evidence.build,
        };
        let observation_start = inventory.observations.len();
        let target_label = source_target_label(&source, path, doc, kind);
        let graph = source_graph(&source, root, inventory)?;
        evidence
            .diagnostics
            .extend(graph.diagnostics.iter().cloned());
        for instance in &graph.sources {
            reached.insert(instance.path.clone());
            add_source(
                &instance.path,
                root,
                inventory,
                names,
                &mut evidence.diagnostics,
                instance,
                &graph,
            )?;
        }
        for position in observation_start..inventory.observations.len() {
            inventory
                .observation_targets
                .insert(position, target_label.clone());
        }
    }
    validate_orphan_sources(
        path,
        manifests,
        inventory,
        &reached,
        &mut evidence.diagnostics,
    )?;
    if inventory.precision {
        for (_, kind, observation) in &mut inventory.observations {
            let without_test = inventory.conditions.without_test(&observation.conditions);
            if without_test == super::conditions::Satisfiability::Exhausted {
                evidence
                    .diagnostics
                    .insert("test attribution condition budget exhausted".into());
            }
            if *kind == DependencyKind::Normal
                && without_test == super::conditions::Satisfiability::Unsat
            {
                *kind = DependencyKind::Dev;
            }
        }
        // Rebuild source category sets from declaration-owned observations. Keep
        // string crate overrides and implicit bare derive retention conservatively.
        let observed = inventory
            .observations
            .iter()
            .map(|(_, _, observation)| observation.name.clone())
            .collect::<BTreeSet<_>>();
        evidence.normal.retain(|name| !observed.contains(name));
        evidence.dev.retain(|name| !observed.contains(name));
        evidence.build.retain(|name| !observed.contains(name));
        for (_, kind, observation) in &inventory.observations {
            match kind {
                DependencyKind::Normal => &mut evidence.normal,
                DependencyKind::Dev => &mut evidence.dev,
                DependencyKind::Build => &mut evidence.build,
            }
            .insert(observation.name.clone());
        }
    }
    inventory.source_evidence.clear();
    Ok(evidence)
}

fn dev_feature_contribution(doc: &DocumentMut, key: &str, dev: &Item) -> bool {
    let Some(normal) = doc
        .get("dependencies")
        .and_then(|dependencies| dependencies.get(key))
    else {
        return false;
    };
    let features = |item: &Item| {
        item.get("features")
            .and_then(Item::as_array)
            .map(|features| {
                features
                    .iter()
                    .filter_map(toml_edit::Value::as_str)
                    .map(str::to_owned)
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default()
    };
    let normal_features = features(normal);
    !features(dev).is_subset(&normal_features)
        || (normal.get("default-features").and_then(Item::as_bool) == Some(false)
            && dev.get("default-features").and_then(Item::as_bool) != Some(false))
        // Workspace-owned feature sets are not fully represented in this table.
        || (dev.get("workspace").and_then(Item::as_bool) == Some(true)
            && normal.get("workspace").and_then(Item::as_bool) != Some(true))
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
    Ok(capture(options, false, false, &mut |_| {})?.0)
}

/// Capture a removal plan while observing real discovery/package milestones.
///
/// The callback runs synchronously and does not change analysis or publication
/// authority. Package completion is provisional until the whole plan is captured.
///
/// # Errors
/// * All discovery, configuration and edit errors documented by [`plan_machete`].
pub fn plan_machete_with_progress(
    options: &MacheteOptions,
    progress: &mut impl FnMut(super::MacheteProgress),
) -> Result<MachetePlan, BoxError> {
    Ok(capture(options, false, false, progress)?.0)
}

#[allow(clippy::too_many_lines)]
fn capture(
    options: &MacheteOptions,
    audit_mode: bool,
    proposals: bool,
    progress: &mut dyn FnMut(super::MacheteProgress),
) -> Result<(MachetePlan, super::audit::AuditInput), BoxError> {
    progress(super::MacheteProgress::Discovering);
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
        inventory.explicit_config_path = Some(config_path.clone());
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
    progress(super::MacheteProgress::Discovered {
        packages: manifests.len(),
    });
    let mut dependencies = Vec::new();
    let mut structured = Vec::new();
    let mut audit = super::audit::AuditInput {
        review_date: options.review_date.clone(),
        ..Default::default()
    };
    for (path, doc) in &manifests {
        let dependency_start = dependencies.len();
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
        let generated = if package_selected || audit_mode {
            load_generated(options, &config, &root, &mut inventory)?
        } else {
            None
        };
        inventory.precision = options.precision;
        inventory.profile = options
            .profile
            .as_ref()
            .map(|name| {
                config
                    .profiles
                    .get(name)
                    .cloned()
                    .ok_or_else(|| format!("unknown machete analysis profile: {name}"))
            })
            .transpose()?;
        let activated = inventory
            .profile
            .as_ref()
            .map(|profile| super::conditions::activate(profile, doc));
        if let Some(profile) = &inventory.profile {
            let activation = super::conditions::activate_features(profile, doc);
            for feature in activation.unresolved_features {
                inventory
                    .diagnostics
                    .insert(format!("unresolved analysis profile feature: {feature}"));
            }
        }
        if let Some((profile, _)) = &activated {
            let mut profile = profile.clone();
            // An invalid requested feature still represents the user's cfg
            // assumption; retain it while reporting invalid Cargo activation.
            if let Some(requested) = &inventory.profile {
                profile.features.extend(requested.features.iter().cloned());
                profile.features.sort();
                profile.features.dedup();
            }
            inventory.profile = Some(profile);
        }
        inventory.observations.clear();
        inventory.observation_targets.clear();
        inventory.source_evidence.clear();
        let mut evidence = if package_selected || audit_mode {
            package_evidence(path, doc, &manifests, &root, &mut inventory)?
        } else {
            PackageEvidence::default()
        };
        if let Some(generated) = &generated {
            for supplied in &generated.sources {
                if root.join(&supplied.owner_manifest) != *path
                    || options
                        .profile
                        .as_ref()
                        .is_some_and(|profile| profile != &supplied.profile)
                {
                    continue;
                }
                let kind = match supplied.target_role {
                    super::generated::GeneratedTargetRole::Library
                    | super::generated::GeneratedTargetRole::Binary(_) => DependencyKind::Normal,
                    super::generated::GeneratedTargetRole::BuildScript => DependencyKind::Build,
                    _ => DependencyKind::Dev,
                };
                let names = match kind {
                    DependencyKind::Normal => &mut evidence.normal,
                    DependencyKind::Dev => &mut evidence.dev,
                    DependencyKind::Build => &mut evidence.build,
                };
                names.extend(supplied.names.clone());
                evidence
                    .diagnostics
                    .extend(supplied.diagnostics.iter().cloned());
                inventory.observations.extend(supplied.names.iter().map(|name| (root.join(&supplied.path), kind, super::resolution::Observation { name: name.clone(), line: 0, column: 0, kind: super::EvidenceKind::Generated, conditions: vec![format!("generated-profile={}", supplied.profile)], uncertainty: vec!["producer-declared fingerprints verified; producer input completeness is not proven".into()] })));
            }
        }
        inventory
            .diagnostics
            .extend(evidence.diagnostics.iter().cloned());
        let observations_by_name = inventory.observations.iter().enumerate().fold(
            BTreeMap::<String, Vec<usize>>::new(),
            |mut index, (position, (_, _, observation))| {
                index
                    .entry(observation.name.to_ascii_lowercase())
                    .or_default()
                    .push(position);
                index
            },
        );
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
                let feature_used = activated.as_ref().map_or_else(
                    || feature_reference(doc, key),
                    |(_, deps)| deps.contains(key),
                );
                let dev_contribution =
                    kind == DependencyKind::Dev && dev_feature_contribution(doc, key, item);
                let used = feature_used
                    || dev_contribution
                    || implicit_reference(key, doc)
                    || rule_excluded
                    || used_derives
                    || match kind {
                        DependencyKind::Dev if options.precision => {
                            evidence.dev.iter().any(|observed| observed == &name)
                        }
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
                } else if target.as_ref().zip(inventory.profile.as_ref()).is_some_and(
                    |(target, profile)| {
                        super::evaluate_condition(target, profile) == super::ConditionValue::False
                    },
                ) {
                    (
                        DependencyStatus::Incomplete,
                        "declaration inactive within selected profile; not globally removable",
                    )
                } else if used {
                    (
                        DependencyStatus::Used,
                        "source, feature, implicit tool, or excluded-source reference",
                    )
                } else if options.profile.is_some() {
                    (
                        DependencyStatus::Incomplete,
                        "unused only within a finite analysis profile; not globally removable",
                    )
                } else if !options.precision {
                    (
                        DependencyStatus::Incomplete,
                        "structural scope analysis disabled; absence unverified",
                    )
                } else if !evidence.diagnostics.is_empty() {
                    (
                        DependencyStatus::Incomplete,
                        if evidence
                            .diagnostics
                            .iter()
                            .all(|reason| reason.contains("unexpanded Rust macro"))
                        {
                            "unverified macro expansion; no global absence proof"
                        } else {
                            "incomplete source inventory"
                        },
                    )
                } else {
                    (
                        DependencyStatus::Unused,
                        "no lexical or structural reference found",
                    )
                };
                for position in observations_by_name
                    .get(&name.to_ascii_lowercase())
                    .into_iter()
                    .flatten()
                {
                    let (file, source_kind, observation) = &inventory.observations[*position];
                    let owns = match kind {
                        DependencyKind::Build => *source_kind == DependencyKind::Build,
                        DependencyKind::Dev => {
                            !options.precision || *source_kind == DependencyKind::Dev
                        }
                        DependencyKind::Normal => *source_kind != DependencyKind::Build,
                    };
                    if owns && observation.name.eq_ignore_ascii_case(&name) {
                        structured.push(super::DependencyEvidence {
                            manifest_path: path.clone(),
                            file: file.clone(),
                            line: observation.line,
                            column: observation.column,
                            kind: observation.kind,
                            source_target: *source_kind,
                            source_target_role: inventory
                                .observation_targets
                                .get(position)
                                .cloned()
                                .unwrap_or_else(|| {
                                    source_target_label(file, path, doc, *source_kind)
                                })
                                .0,
                            source_target_name: inventory
                                .observation_targets
                                .get(position)
                                .cloned()
                                .unwrap_or_else(|| {
                                    source_target_label(file, path, doc, *source_kind)
                                })
                                .1,
                            dependency_kind: kind,
                            target: target.clone(),
                            alias: key.to_owned(),
                            resolved_package: actual.to_owned(),
                            conditions: observation.conditions.clone(),
                            uncertainty: observation.uncertainty.clone(),
                        });
                    }
                }
                for (present, origin) in [
                    (
                        feature_used || dev_contribution,
                        super::EvidenceKind::Feature,
                    ),
                    (implicit_reference(key, doc), super::EvidenceKind::Implicit),
                ] {
                    if present {
                        structured.push(super::DependencyEvidence {
                            manifest_path: path.clone(),
                            file: path.clone(),
                            line: 0,
                            column: 0,
                            kind: origin,
                            source_target: kind,
                            source_target_role: "MANIFEST".into(),
                            source_target_name: None,
                            dependency_kind: kind,
                            target: target.clone(),
                            alias: key.to_owned(),
                            resolved_package: actual.to_owned(),
                            conditions: Vec::new(),
                            uncertainty: Vec::new(),
                        });
                    }
                }
                if audit_mode {
                    audit.declarations.push(super::audit::AuditDeclaration {
                        location: super::audit::AuditLocation {
                            path: path.clone(),
                            package: Some(package.to_owned()),
                            dependency: Some(key.to_owned()),
                            kind: Some(kind),
                            target: target.clone(),
                        },
                        dependency_package: actual.to_owned(),
                        specification: if workspace
                            && ["features", "default-features"]
                                .iter()
                                .any(|key| item.get(key).is_some())
                        {
                            None
                        } else {
                            audit_specification(owner)
                        },
                        optional: item
                            .get("optional")
                            .and_then(Item::as_bool)
                            .unwrap_or(false),
                        workspace,
                        selected: is_selected,
                        usage: super::audit::AuditUsage {
                            targets_complete: options.precision
                                && inventory.profile.is_none()
                                && evidence.diagnostics.is_empty(),
                            ..Default::default()
                        },
                    });
                }
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
        if package_selected {
            let selected = &dependencies[dependency_start..];
            progress(super::MacheteProgress::PackageAnalyzed {
                package: package.to_owned(),
                unused: selected
                    .iter()
                    .filter(|d| d.status == DependencyStatus::Unused)
                    .count(),
                complete: evidence
                    .diagnostics
                    .iter()
                    .all(|d| d.starts_with("advisory:"))
                    && !selected
                        .iter()
                        .any(|d| d.status == DependencyStatus::Incomplete),
            });
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
    edited
        .entry(root_path.clone())
        .or_insert_with(|| root_doc.clone());
    for dependency in dependencies.iter().filter(|d| {
        d.status == DependencyStatus::Unused
            || (proposals
                && d.status == DependencyStatus::Incomplete
                && d.reason == "unverified macro expansion; no global absence proof")
    }) {
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
    let mut report = MacheteReport {
        root,
        evidence: structured,
        coverage: super::AnalysisCoverage {
            profile: options.profile.clone(),
            precision: options.precision,
            profile_limited: options.profile.is_some(),
            gaps: inventory.diagnostics.iter().cloned().collect(),
        },
        dependencies,
        diagnostics: inventory.diagnostics.iter().cloned().collect(),
        complete,
        applied: false,
        lockfile_preserved: true,
    };
    report.canonicalize_evidence();
    let sealed_changes = changes
        .iter()
        .map(|c| (c.path.clone(), c.before.clone(), c.after.clone()))
        .collect();
    if audit_mode {
        fill_audit_inventory(&mut audit, &inventory, &manifests, &root_path, &root_doc);
        audit.add_report_evidence(&report);
    }
    Ok((
        MachetePlan {
            report,
            changes,
            snapshots: inventory.files,
            directories: inventory.directories,
            sealed_changes,
            complete,
        },
        audit,
    ))
}

/// Capture disposable compiler proposals, never an applicable removal plan.
#[cfg(feature = "machete-verify")]
pub(super) fn plan_machete_candidates(options: &MacheteOptions) -> Result<MachetePlan, BoxError> {
    let mut plan = capture(options, false, true, &mut |_| {})?.0;
    // Proposal state cannot pass apply, including if the public report is edited.
    plan.complete = false;
    Ok(plan)
}

/// Audit full discovered declarations before selectors, using unsuppressed evidence.
///
/// # Errors
/// * Discovery/configuration errors documented by [`analyze_machete`].
/// * Invalid suppression glob patterns or review date syntax.
pub fn audit_machete(options: &MacheteOptions) -> Result<super::audit::AuditReport, BoxError> {
    let (_, input) = capture(options, true, false, &mut |_| {})?;
    super::audit::audit_machete_evidence(&input)
}

fn audit_specification(item: &Item) -> Option<super::audit::AuditSpecification> {
    if ["path", "git", "workspace"]
        .iter()
        .any(|key| item.get(key).is_some())
    {
        return None;
    }
    let version = item
        .as_str()
        .or_else(|| item.get("version").and_then(Item::as_str))?;
    Some(super::audit::AuditSpecification {
        registry: item
            .get("registry")
            .and_then(Item::as_str)
            .unwrap_or("crates.io")
            .to_owned(),
        version: version.to_owned(),
        default_features: item
            .get("default-features")
            .and_then(Item::as_bool)
            .unwrap_or(true),
        features: strings(item.get("features")).into_iter().collect(),
    })
}

fn fill_audit_inventory(
    input: &mut super::audit::AuditInput,
    inventory: &Inventory,
    manifests: &BTreeMap<PathBuf, DocumentMut>,
    root_path: &Path,
    root_doc: &DocumentMut,
) {
    use super::audit::{AuditLocation, AuditSuppression};
    input.reference_inventory_complete = !inventory.omitted_directories
        && inventory
            .diagnostics
            .iter()
            .all(|message| message.starts_with("advisory:"));
    for (base, config) in inventory.configs.iter().chain(
        inventory
            .explicit
            .iter()
            .map(|(base, config)| (base, config)),
    ) {
        let scope = manifests
            .keys()
            .filter(|path| {
                path.starts_with(base)
                    || inventory
                        .explicit
                        .as_ref()
                        .is_some_and(|(explicit, _)| explicit == base)
            })
            .cloned()
            .collect::<BTreeSet<_>>();
        let config_path = inventory
            .explicit_config_path
            .as_ref()
            .filter(|_| {
                inventory
                    .explicit
                    .as_ref()
                    .is_some_and(|(explicit, _)| explicit == base)
            })
            .cloned()
            .unwrap_or_else(|| base.join("clippier.toml"));
        for pattern in &config.ignored {
            input.suppressions.push(AuditSuppression {
                location: AuditLocation {
                    path: config_path.clone(),
                    package: None,
                    dependency: Some(pattern.clone()),
                    kind: None,
                    target: None,
                },
                pattern: pattern.clone(),
                manifests: scope.clone(),
                occurrences: None,
                scope_complete: input.reference_inventory_complete,
                reason: None,
                review_after: None,
            });
        }
        fill_rule_suppressions(input, config, &scope, &config_path);
        for ignored in &config.ignored_dirs {
            input.excluded_directories.push(AuditLocation {
                path: base.join(ignored),
                package: None,
                dependency: None,
                kind: None,
                target: None,
            });
        }
    }
    fill_workspace_inventory(input, root_path, root_doc);
}

/// Read verification policy with the same validated ancestor/explicit config layers.
/// No Cargo process, source expansion, or network request is performed.
///
/// # Errors
/// * Missing manifest/config files, invalid TOML or unsupported configuration keys.
#[cfg(feature = "machete-verify")]
pub fn load_verification_options(
    options: &MacheteOptions,
) -> Result<Option<super::verify::VerificationOptions>, BoxError> {
    let requested = options
        .manifest_path
        .clone()
        .unwrap_or(std::env::current_dir()?);
    let requested = fs::canonicalize(if requested.is_dir() {
        requested.join("Cargo.toml")
    } else {
        requested
    })?;
    let mut inventory = Inventory::default();
    for ancestor in requested.parent().into_iter().flat_map(Path::ancestors) {
        config_at(ancestor, &mut inventory)?;
    }
    if let Some(path) = &options.config_path {
        let path = fs::canonicalize(path)?;
        let content = fs::read_to_string(&path)?;
        let value: toml::Value = toml::from_str(&content)?;
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
        inventory.explicit = Some((
            path.parent().ok_or("config has no parent")?.to_owned(),
            config,
        ));
    }
    Ok(effective(&layers(&requested, &inventory), options).verification)
}

fn fill_workspace_inventory(
    input: &mut super::audit::AuditInput,
    root_path: &Path,
    root_doc: &DocumentMut,
) {
    use super::audit::{AuditLocation, AuditWorkspaceDependency};
    if let Some(table) = root_doc
        .get("workspace")
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(Item::as_table_like)
    {
        for (alias, item) in table.iter() {
            input.workspace_dependencies.push(AuditWorkspaceDependency {
                location: AuditLocation {
                    path: root_path.to_owned(),
                    package: None,
                    dependency: Some(alias.to_owned()),
                    kind: None,
                    target: None,
                },
                specification: audit_specification(item),
                dependency_package: item
                    .get("package")
                    .and_then(Item::as_str)
                    .unwrap_or(alias)
                    .to_owned(),
            });
        }
    }
}

fn automatic_target_kind(
    file: &Path,
    directory: &Path,
    doc: &DocumentMut,
) -> Option<DependencyKind> {
    let relative = file.strip_prefix(directory).ok()?;
    let parts = relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let automatic = |flag: &str| {
        doc.get("package")
            .and_then(|package| package.get(flag))
            .and_then(Item::as_bool)
            != Some(false)
    };
    let normal = (relative == Path::new("src/lib.rs")
        && automatic("autolib")
        && doc.get("lib").and_then(|lib| lib.get("path")).is_none())
        || (relative == Path::new("src/main.rs") && automatic("autobins"))
        || (parts.first().is_some_and(|part| part == "src")
            && parts.get(1).is_some_and(|part| part == "bin")
            && automatic("autobins")
            && (parts.len() == 3 || (parts.len() == 4 && parts[3] == "main.rs")));
    let dev = [
        ("tests", "autotests"),
        ("examples", "autoexamples"),
        ("benches", "autobenches"),
    ]
    .iter()
    .any(|(directory, flag)| {
        parts.first().is_some_and(|part| part == directory)
            && automatic(flag)
            && (parts.len() == 2 || (parts.len() == 3 && parts[2] == "main.rs"))
    });
    if normal {
        Some(DependencyKind::Normal)
    } else if dev {
        Some(DependencyKind::Dev)
    } else {
        None
    }
}

fn package_sources(
    path: &Path,
    doc: &DocumentMut,
    manifests: &BTreeMap<PathBuf, DocumentMut>,
    inventory: &mut Inventory,
    diagnostics: &mut BTreeSet<String>,
) -> Result<BTreeMap<PathBuf, DependencyKind>, BoxError> {
    let directory = path.parent().ok_or("manifest has no parent")?;
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
        if let Some(kind) = automatic_target_kind(file, directory, doc) {
            inventory.target_roots.insert(file.clone());
            sources.insert(file.clone(), kind);
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
                    if conventional.exists()
                        || nested.exists()
                        || (section == "bin" && directory.join("src/main.rs").exists())
                    {
                        let target_path = if conventional.exists() {
                            conventional
                        } else if nested.exists() {
                            nested
                        } else {
                            directory.join("src/main.rs")
                        };
                        inventory.target_roots.insert(target_path.clone());
                        sources.insert(target_path, kind);
                    } else {
                        diagnostics.insert(format!(
                            "unresolved {section} target {name} in {}",
                            path.display()
                        ));
                    }
                }
            }
        }
    }
    Ok(sources)
}

fn fill_rule_suppressions(
    input: &mut super::audit::AuditInput,
    config: &MacheteConfig,
    scope: &BTreeSet<PathBuf>,
    config_path: &Path,
) {
    use super::audit::{AuditLocation, AuditSuppression};
    for rule in &config.rules {
        let applicable = input
            .declarations
            .iter()
            .filter(|declaration| scope.contains(&declaration.location.path))
            .filter_map(|declaration| {
                let package = declaration.location.package.as_deref().unwrap_or_default();
                let alias = declaration
                    .location
                    .dependency
                    .as_deref()
                    .unwrap_or_default();
                match (
                    selected(&rule.packages, package),
                    rule.dependencies.is_empty()
                        || dependency_matches(
                            &rule.dependencies,
                            alias,
                            &declaration.dependency_package,
                        )
                        .unwrap_or(false),
                ) {
                    (Ok(true), true) => Some(declaration.location.clone()),
                    _ => None,
                }
            })
            .collect::<BTreeSet<_>>();
        for pattern in &rule.ignored {
            input.suppressions.push(AuditSuppression {
                location: AuditLocation {
                    path: config_path.to_owned(),
                    package: None,
                    dependency: Some(pattern.clone()),
                    kind: None,
                    target: None,
                },
                pattern: pattern.clone(),
                manifests: applicable
                    .iter()
                    .map(|location| location.path.clone())
                    .collect(),
                occurrences: Some(applicable.iter().cloned().collect()),
                scope_complete: input.reference_inventory_complete,
                reason: rule.reason.clone(),
                review_after: rule.review_after.clone(),
            });
        }
    }
}

fn source_target_label(
    file: &Path,
    manifest: &Path,
    doc: &DocumentMut,
    kind: DependencyKind,
) -> (String, Option<String>) {
    let directory = manifest.parent().unwrap_or(manifest);
    let package = doc
        .get("package")
        .and_then(|package| package.get("name"))
        .and_then(Item::as_str)
        .unwrap_or_default();
    if kind == DependencyKind::Build {
        return ("BUILD_SCRIPT".into(), Some("build-script-build".into()));
    }
    if let Some(lib) = doc.get("lib") {
        let entry = lib
            .get("path")
            .and_then(Item::as_str)
            .unwrap_or("src/lib.rs");
        if file == directory.join(entry) {
            return (
                "LIBRARY".into(),
                Some(
                    lib.get("name")
                        .and_then(Item::as_str)
                        .unwrap_or(package)
                        .replace('-', "_"),
                ),
            );
        }
    } else if file == directory.join("src/lib.rs") {
        return ("LIBRARY".into(), Some(package.replace('-', "_")));
    }
    for (section, role) in [
        ("bin", "BINARY"),
        ("test", "TEST"),
        ("example", "EXAMPLE"),
        ("bench", "BENCH"),
    ] {
        if let Some(targets) = doc.get(section).and_then(Item::as_array_of_tables) {
            for target in targets {
                if let Some(entry) = target.get("path").and_then(Item::as_str)
                    && file == directory.join(entry)
                {
                    return (
                        role.into(),
                        target.get("name").and_then(Item::as_str).map(str::to_owned),
                    );
                }
            }
        }
    }
    if file == directory.join("src/main.rs") {
        return ("BINARY".into(), Some(package.into()));
    }
    if let Ok(relative) = file.strip_prefix(directory) {
        for (prefix, role) in [
            ("src/bin", "BINARY"),
            ("tests", "TEST"),
            ("examples", "EXAMPLE"),
            ("benches", "BENCH"),
        ] {
            if let Ok(relative) = relative.strip_prefix(prefix) {
                if relative.components().count() == 1 {
                    return (
                        role.into(),
                        relative
                            .file_stem()
                            .map(|name| name.to_string_lossy().into_owned()),
                    );
                }
                if relative.file_name().is_some_and(|name| name == "main.rs")
                    && relative.components().count() == 2
                {
                    return (
                        role.into(),
                        relative
                            .parent()
                            .and_then(Path::file_name)
                            .map(|name| name.to_string_lossy().into_owned()),
                    );
                }
            }
        }
    }
    ("UNRESOLVED".into(), None)
}

// Configured captures are ordinary offline inputs. Snapshot absence as well as
// bytes so a newly arriving capture invalidates the plan instead of being ignored.
fn load_generated(
    options: &MacheteOptions,
    config: &MacheteConfig,
    root: &Path,
    inventory: &mut Inventory,
) -> Result<Option<super::generated::GeneratedEvidenceReport>, BoxError> {
    let captured;
    let evidence = if let Some(evidence) = &options.generated {
        evidence
    } else if let Some(path) = &config.evidence_path {
        let bytes = optional(path)?;
        inventory.files.insert(path.clone(), bytes.clone());
        let Some(bytes) = bytes else {
            inventory.diagnostics.insert(format!(
                "advisory: configured generated evidence absent: {}",
                path.display()
            ));
            return Ok(None);
        };
        captured = serde_json::from_slice::<super::generated::GeneratedEvidence>(&bytes)
            .map_err(|error| format!("invalid generated evidence {}: {error}", path.display()))?;
        &captured
    } else {
        return Ok(None);
    };
    let validated = super::generated::validate_generated_evidence(root, evidence)?;
    for source in &evidence.sources {
        for fingerprint in source.inputs.iter().chain(std::iter::once(&source.output)) {
            inventory.files.insert(
                root.join(&fingerprint.path),
                Some(fingerprint.bytes.clone()),
            );
        }
    }
    Ok(Some(validated))
}

#[cfg(test)]
mod discovery_context_tests {
    use super::*;

    #[test]
    fn dev_features_and_default_features_are_not_removed_with_duplicate_normal_use() {
        let doc: DocumentMut = "[dependencies]\nshared={version='1.0.0',default-features=false,features=['base']}\n[dev-dependencies]\nshared={version='1.0.0',features=['base','testing']}\n".parse().unwrap();
        assert!(dev_feature_contribution(
            &doc,
            "shared",
            &doc["dev-dependencies"]["shared"]
        ));
        let doc: DocumentMut = "[dependencies]\nshared={version='1.0.0',features=['base']}\n[dev-dependencies]\nshared={version='1.0.0',features=['base']}\n".parse().unwrap();
        assert!(!dev_feature_contribution(
            &doc,
            "shared",
            &doc["dev-dependencies"]["shared"]
        ));
    }

    #[test]
    fn orphan_validation_adds_no_dependency_observations_and_keeps_snapshot() {
        let file = PathBuf::from("/pkg/src/orphan.rs");
        let manifest = PathBuf::from("/pkg/Cargo.toml");
        let mut inventory = Inventory::default();
        inventory
            .files
            .insert(file.clone(), Some(b"{ invalid".to_vec()));
        let mut diagnostics = BTreeSet::new();
        validate_orphan_sources(
            &manifest,
            &BTreeMap::new(),
            &inventory,
            &BTreeSet::new(),
            &mut diagnostics,
        )
        .unwrap();
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("orphan source"))
        );
        assert!(inventory.observations.is_empty());
        assert_eq!(inventory.files[&file], Some(b"{ invalid".to_vec()));
    }

    #[test]
    fn custom_target_root_labels_survive_child_source_ownership() {
        let doc: DocumentMut =
            "[package]\nname='pkg'\n[lib]\nname='custom'\npath='custom/root.rs'\n"
                .parse()
                .unwrap();
        assert_eq!(
            source_target_label(
                Path::new("/pkg/custom/root.rs"),
                Path::new("/pkg/Cargo.toml"),
                &doc,
                DependencyKind::Normal
            ),
            ("LIBRARY".into(), Some("custom".into()))
        );
    }
}
