//! Pure dependency discovery and manifest planning.
use super::{
    BoxError, DependencyKind, DependencyReport, LockfilePolicy, LockfileStatus, ManifestChange,
    PinnedPolicy, PrereleasePolicy, RegistryKey, SourcePolicy, UpgradeConfig, UpgradeInput,
    UpgradePlan, UpgradePolicy, UpgradeReport, UpgradeStatus, manifest::patterns,
    registry::Release,
};
use semver::{Op, Version, VersionReq};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};
use toml_edit::{DocumentMut, Item, Value};

#[derive(Clone)]
pub(super) struct RawDependency {
    pub(crate) key: Vec<String>,
    pub(crate) alias: String,
    pub(crate) item: Item,
    kind: DependencyKind,
    target: Option<String>,
    workspace: bool,
}

pub(super) fn raw_dependencies(document: &DocumentMut) -> Result<Vec<RawDependency>, BoxError> {
    let mut result = Vec::new();
    for (section, kind) in [
        ("dependencies", DependencyKind::Normal),
        ("dev-dependencies", DependencyKind::Dev),
        ("build-dependencies", DependencyKind::Build),
    ] {
        collect_section(
            document.get(section),
            vec![section.into()],
            kind,
            None,
            false,
            &mut result,
        )?;
        if let Some(targets) = document.get("target").and_then(Item::as_table_like) {
            for (target, value) in targets.iter() {
                collect_section(
                    value.get(section),
                    vec!["target".into(), target.into(), section.into()],
                    kind,
                    Some(target.into()),
                    false,
                    &mut result,
                )?;
            }
        }
    }
    collect_section(
        document
            .get("workspace")
            .and_then(|w| w.get("dependencies")),
        vec!["workspace".into(), "dependencies".into()],
        DependencyKind::Normal,
        None,
        true,
        &mut result,
    )?;
    Ok(result)
}

#[allow(clippy::needless_pass_by_value)] // Section ownership keeps target and prefix construction explicit.
fn collect_section(
    item: Option<&Item>,
    prefix: Vec<String>,
    kind: DependencyKind,
    target: Option<String>,
    workspace: bool,
    result: &mut Vec<RawDependency>,
) -> Result<(), BoxError> {
    if let Some(item) = item {
        let table = item
            .as_table_like()
            .ok_or("dependency section must be a table")?;
        for (alias, value) in table.iter() {
            let mut key = prefix.clone();
            key.push(alias.into());
            result.push(RawDependency {
                key,
                alias: alias.into(),
                item: value.clone(),
                kind,
                target: target.clone(),
                workspace,
            });
        }
    }
    Ok(())
}

#[derive(Clone)]
struct Occurrence {
    raw: RawDependency,
    owner: PathBuf,
    consumer: PathBuf,
    package: String,
    owner_key: Vec<String>,
    declaration: Item,
    selected: bool,
}

fn occurrences(input: &UpgradeInput) -> Result<Vec<Occurrence>, BoxError> {
    let dependencies = patterns(&input.config.dependencies)?;
    let packages = patterns(&input.config.packages)?;
    let excludes = patterns(&input.config.exclude)?;
    let root_path = input.root.join("Cargo.toml");
    let root: DocumentMut = input
        .manifests
        .get(&root_path)
        .ok_or("missing workspace manifest")?
        .parse()?;
    let workspace = raw_dependencies(&root)?
        .into_iter()
        .filter(|d| d.workspace)
        .map(|d| (d.alias.clone(), d))
        .collect::<BTreeMap<_, _>>();
    let mut result = Vec::new();
    let mut used = BTreeSet::new();
    for (path, text) in &input.manifests {
        let doc: DocumentMut = text.parse()?;
        let Some(package) = doc
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(Item::as_str)
        else {
            continue;
        };
        for raw in raw_dependencies(&doc)?.into_iter().filter(|d| !d.workspace) {
            let inherited = raw
                .item
                .get("workspace")
                .and_then(Item::as_bool)
                .unwrap_or(false);
            let (owner, owner_key, declaration) = if inherited {
                let declaration = workspace
                    .get(&raw.alias)
                    .ok_or_else(|| format!("workspace dependency '{}' is missing", raw.alias))?;
                used.insert(raw.alias.clone());
                (
                    root_path.clone(),
                    declaration.key.clone(),
                    declaration.item.clone(),
                )
            } else {
                (path.clone(), raw.key.clone(), raw.item.clone())
            };
            let name = string_field(&declaration, "package").unwrap_or(&raw.alias);
            let selected = (input.config.packages.is_empty() || packages.is_match(package))
                && (input.config.dependencies.is_empty()
                    || dependencies.is_match(name)
                    || dependencies.is_match(&raw.alias))
                && !excludes.is_match(name)
                && !excludes.is_match(&raw.alias)
                && (input.config.kinds.is_empty() || input.config.kinds.contains(&raw.kind));
            result.push(Occurrence {
                raw,
                owner,
                consumer: path.clone(),
                package: package.into(),
                owner_key,
                declaration,
                selected,
            });
        }
    }
    for (alias, raw) in workspace {
        if used.contains(&alias) {
            continue;
        }
        let name = string_field(&raw.item, "package").unwrap_or(&alias);
        let selected = input.config.packages.is_empty()
            && (input.config.dependencies.is_empty()
                || dependencies.is_match(name)
                || dependencies.is_match(&alias))
            && !excludes.is_match(name)
            && !excludes.is_match(&alias)
            && input.config.kinds.is_empty();
        result.push(Occurrence {
            owner: root_path.clone(),
            consumer: root_path.clone(),
            package: "workspace".into(),
            owner_key: raw.key.clone(),
            declaration: raw.item.clone(),
            raw,
            selected,
        });
    }
    result.sort_by(|a, b| (&a.consumer, &a.raw.key).cmp(&(&b.consumer, &b.raw.key)));
    for selector in &input.config.dependencies {
        let pattern = patterns(std::slice::from_ref(selector))?;
        if !result.iter().any(|occurrence| {
            pattern.is_match(&occurrence.raw.alias)
                || pattern.is_match(registry_key(occurrence).name)
        }) {
            return Err(format!("dependency selector '{selector}' matches no dependency").into());
        }
    }
    Ok(result)
}

fn string_field<'a>(item: &'a Item, name: &str) -> Option<&'a str> {
    item.get(name).and_then(Item::as_str)
}
fn requirement(item: &Item) -> Option<&str> {
    item.as_str().or_else(|| string_field(item, "version"))
}
fn registry_key(occurrence: &Occurrence) -> RegistryKey {
    RegistryKey {
        name: string_field(&occurrence.declaration, "package")
            .unwrap_or(&occurrence.raw.alias)
            .into(),
        registry: string_field(&occurrence.declaration, "registry").map(str::to_owned),
    }
}

fn skip_reason(config: &UpgradeConfig, declaration: &Item) -> Option<&'static str> {
    if declaration.get("git").is_some() {
        return Some("Git source preserved");
    }
    if declaration.get("path").is_some() && config.source == SourcePolicy::RegistryOnly {
        return Some("path source preserved");
    }
    let Some(text) = requirement(declaration) else {
        return Some("no registry version requirement");
    };
    if config.pinned == PinnedPolicy::Preserve {
        let req = VersionReq::parse(text).ok()?;
        if req.comparators.len() == 1
            && req.comparators.iter().any(|comparator| {
                matches!(
                    comparator.op,
                    Op::Exact | Op::Wildcard | Op::Less | Op::LessEq
                )
            })
        {
            return Some("pin preserved");
        }
    }
    None
}

pub(super) fn registry_requests(input: &UpgradeInput) -> Result<BTreeSet<RegistryKey>, BoxError> {
    let mut requests = BTreeSet::new();
    for occurrence in occurrences(input)? {
        let config = effective_config(input, &occurrence)?;
        if occurrence.selected && skip_reason(&config, &occurrence.declaration).is_none() {
            requests.insert(registry_key(&occurrence));
        }
    }
    Ok(requests)
}

/// Compute an exact native manifest plan from captured input and injected releases.
/// This function performs no IO and never invokes Cargo.
///
/// # Errors
/// * Invalid manifest shapes, inheritance, glob selectors or version requirements.
/// * Missing registry metadata for an eligible selected dependency.
#[allow(clippy::too_many_lines)] // Pure planner keeps owner decisions and consumer reporting in one pass.
pub fn plan_upgrade_with_releases(
    input: &UpgradeInput,
    releases: &BTreeMap<RegistryKey, Vec<Release>>,
) -> Result<UpgradePlan, BoxError> {
    let occurrences = occurrences(input)?;
    let selected_owners = occurrences
        .iter()
        .filter(|o| o.selected)
        .map(|o| (o.owner.clone(), o.owner_key.clone()))
        .collect::<BTreeSet<_>>();
    let mut owner_configs = BTreeMap::new();
    let mut owner_features: BTreeMap<OwnerKey, BTreeSet<String>> = BTreeMap::new();
    for occurrence in &occurrences {
        let config = effective_config(input, occurrence)?;
        let key = (occurrence.owner.clone(), occurrence.owner_key.clone());
        owner_features
            .entry(key.clone())
            .or_default()
            .extend(required_features(&occurrence.raw.item));
        owner_features
            .entry(key.clone())
            .or_default()
            .extend(required_features(&occurrence.declaration));
        let fingerprint = (
            config.policy,
            config.pinned,
            config.prerelease,
            config.source,
            config.msrv.clone(),
        );
        if let Some((_, previous)) = owner_configs.get(&key) {
            if previous != &fingerprint {
                return Err(format!(
                    "conflicting scoped policies for inherited workspace dependency '{}'",
                    occurrence.raw.alias
                )
                .into());
            }
        } else {
            owner_configs.insert(key, (config, fingerprint));
        }
    }
    let mut decisions: BTreeMap<OwnerKey, Decision> = BTreeMap::new();
    let mut documents = input
        .manifests
        .iter()
        .map(|(path, text)| Ok((path.clone(), text.parse::<DocumentMut>()?)))
        .collect::<Result<BTreeMap<_, _>, BoxError>>()?;
    let mut reports = Vec::new();
    let mut lock_targets = BTreeSet::new();
    let mut lock_versions = BTreeSet::new();
    for occurrence in occurrences {
        let key = (occurrence.owner.clone(), occurrence.owner_key.clone());
        let config = &owner_configs.get(&key).ok_or("missing effective policy")?.0;
        let previous = requirement(&occurrence.declaration).map(str::to_owned);
        let registry = registry_key(&occurrence);
        let (status, reason, next, selected) = if let Some(decision) = decisions.get(&key) {
            decision.clone()
        } else {
            let decision = if !selected_owners.contains(&key) {
                (
                    UpgradeStatus::Skipped,
                    "excluded by selection".into(),
                    previous.clone(),
                    None,
                )
            } else if let Some(reason) = skip_reason(config, &occurrence.declaration) {
                (
                    UpgradeStatus::Skipped,
                    reason.into(),
                    previous.clone(),
                    None,
                )
            } else {
                let old = previous.as_deref().ok_or("missing version requirement")?;
                let versions = releases
                    .get(&registry)
                    .ok_or_else(|| format!("missing release metadata for {}", registry.name))?;
                let req = VersionReq::parse(old)?;
                let features = owner_features.get(&key).ok_or("missing owner features")?;
                let mut eligible_releases = Vec::new();
                for release in versions {
                    if eligible(release, &req, config, input.lockfile_only)?
                        && features
                            .iter()
                            .all(|feature| release.features.contains(feature))
                    {
                        eligible_releases.push(release);
                    }
                }
                let choice = eligible_releases
                    .into_iter()
                    .max_by(|a, b| a.version.cmp(&b.version));
                if let Some(release) = choice {
                    reject_locked_downgrade(input, &registry, &req, &release.version)?;
                    lock_targets.insert(registry.clone());
                    lock_versions.insert(super::LockTarget {
                        key: registry.clone(),
                        previous: old.into(),
                        selected: release.version.to_string(),
                    });
                    let next = if input.lockfile_only {
                        old.into()
                    } else {
                        rewrite_requirement(old, &release.version)?
                    };
                    let status = if next == old {
                        UpgradeStatus::Unchanged
                    } else {
                        UpgradeStatus::Planned
                    };
                    (
                        status,
                        if input.lockfile_only {
                            "lockfile-only; requirement preserved"
                        } else {
                            "highest eligible non-yanked release"
                        }
                        .into(),
                        Some(next),
                        Some(release.version.to_string()),
                    )
                } else {
                    (
                        UpgradeStatus::Unavailable,
                        "no eligible non-yanked release with requested features".into(),
                        previous.clone(),
                        None,
                    )
                }
            };
            decisions.insert(key.clone(), decision.clone());
            if decision.0 == UpgradeStatus::Planned {
                let document = documents
                    .get_mut(&occurrence.owner)
                    .ok_or("missing owner manifest")?;
                let mut item = document.as_item_mut();
                for part in &occurrence.owner_key {
                    item = item
                        .get_mut(part)
                        .ok_or("dependency owner changed while planning")?;
                }
                replace_requirement(
                    item,
                    decision.2.as_deref().ok_or("missing planned requirement")?,
                )?;
            }
            decision
        };
        reports.push(DependencyReport {
            package: occurrence.package,
            alias: occurrence.raw.alias,
            name: registry.name,
            manifest: occurrence.owner,
            consumer_manifest: occurrence.consumer,
            kind: occurrence.raw.kind,
            target: occurrence.raw.target,
            previous,
            requirement: next,
            selected,
            status,
            reason: if !occurrence.selected && status == UpgradeStatus::Planned {
                format!("workspace owner update affects this consumer: {reason}")
            } else {
                reason
            },
        });
    }
    let changes = documents
        .into_iter()
        .filter_map(|(path, doc)| {
            let before = input.manifests.get(&path)?;
            let after = doc.to_string();
            (after != *before).then(|| ManifestChange {
                path,
                before: before.clone(),
                after,
            })
        })
        .collect::<Vec<_>>();
    let protected_names = reports
        .iter()
        .filter(|r| {
            matches!(
                r.status,
                UpgradeStatus::Skipped | UpgradeStatus::Unavailable
            )
        })
        .map(|r| r.name.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let report = UpgradeReport {
        version: 1,
        config: input.config.clone(),
        lockfile_only: input.lockfile_only,
        status: if changes.is_empty() && input.config.lockfile == LockfilePolicy::None {
            UpgradeStatus::Unchanged
        } else {
            UpgradeStatus::Planned
        },
        pending: !changes.is_empty() || input.config.lockfile != LockfilePolicy::None,
        blockers: Vec::new(),
        cancelled: false,
        workspace_root: input.root.clone(),
        dependencies: reports,
        manifests: changes.iter().map(|c| c.path.clone()).collect(),
        lockfile: if input.config.lockfile == LockfilePolicy::None {
            LockfileStatus::Disabled
        } else {
            LockfileStatus::Unresolved
        },
        applied: false,
    };
    Ok(UpgradePlan {
        root: input.root.clone(),
        snapshots: input.snapshots.clone(),
        changes,
        report,
        lockfile_policy: input.config.lockfile,
        offline: input.offline,
        lock_targets: lock_targets.into_iter().collect(),
        lock_versions: lock_versions.into_iter().collect(),
        protected_names,
        resolved_lockfile: None,
    })
}

fn reject_locked_downgrade(
    input: &UpgradeInput,
    key: &RegistryKey,
    requirement: &VersionReq,
    selected: &Version,
) -> Result<(), BoxError> {
    let path = input.root.join("Cargo.lock");
    let Some(text) = input
        .snapshots
        .iter()
        .find(|s| s.path == path)
        .and_then(|s| s.contents.as_deref())
    else {
        return Ok(());
    };
    let lock: toml::Value = toml::from_str(text)?;
    if let Some(packages) = lock.get("package").and_then(toml::Value::as_array) {
        for package in packages {
            if package.get("name").and_then(toml::Value::as_str) == Some(&key.name)
                && package
                    .get("source")
                    .and_then(toml::Value::as_str)
                    .is_some_and(|s| s.starts_with("registry+") || s.starts_with("sparse+"))
            {
                let version = Version::parse(
                    package
                        .get("version")
                        .and_then(toml::Value::as_str)
                        .ok_or("locked package has no version")?,
                )?;
                if requirement.matches(&version) && version > *selected {
                    return Err(format!("upgrade of {} would downgrade locked {version} to {selected}; no eligible non-downgrading release (check MSRV/features/policies)", key.name).into());
                }
            }
        }
    }
    Ok(())
}

type OwnerKey = (PathBuf, Vec<String>);
type Decision = (UpgradeStatus, String, Option<String>, Option<String>);

fn required_features(item: &Item) -> BTreeSet<String> {
    item.get("features")
        .and_then(Item::as_array)
        .map(|array| {
            array
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn eligible(
    release: &Release,
    req: &VersionReq,
    config: &UpgradeConfig,
    lockfile_only: bool,
) -> Result<bool, BoxError> {
    if release.yanked {
        return Ok(false);
    }
    if let (Some(ceiling), Some(required)) = (&config.msrv, &release.rust_version)
        && rust_version(required)? > rust_version(ceiling)?
    {
        return Ok(false);
    }
    if !release.version.pre.is_empty() && config.prerelease == PrereleasePolicy::Preserve {
        // Existing prereleases permit only the explicitly opted-in release line,
        // not unrelated major/minor/patch prereleases in incompatible mode.
        if !req.comparators.iter().any(|c| {
            !c.pre.is_empty()
                && c.major == release.version.major
                && c.minor == Some(release.version.minor)
                && c.patch == Some(release.version.patch)
        }) {
            return Ok(false);
        }
    }
    if let Some(minimum) = minimum(req)
        && release.version < minimum
    {
        return Ok(false);
    }
    if lockfile_only {
        return Ok(req.matches(&release.version));
    }
    // Complex ranges are constraints, not an invitation to silently broaden.
    if req.comparators.len() > 1
        || req
            .comparators
            .iter()
            .any(|c| matches!(c.op, Op::Greater | Op::GreaterEq | Op::Less | Op::LessEq))
    {
        return Ok(req.matches(&release.version));
    }
    if config.policy == UpgradePolicy::Incompatible {
        return Ok(true);
    }
    if req.comparators.len() == 1
        && req.comparators[0].op == Op::Exact
        && config.pinned == PinnedPolicy::Upgrade
    {
        return Ok(minimum(req).is_some_and(|v| {
            VersionReq::parse(&format!("^{v}")).is_ok_and(|r| r.matches(&release.version))
        }));
    }
    if config.prerelease == PrereleasePolicy::Allow && !release.version.pre.is_empty() {
        let mut stable = release.version.clone();
        stable.pre = semver::Prerelease::EMPTY;
        Ok(req.matches(&stable))
    } else {
        Ok(req.matches(&release.version))
    }
}

fn rust_version(text: &str) -> Result<Version, BoxError> {
    let count = text.split('.').count();
    Ok(Version::parse(&match count {
        1 => format!("{text}.0.0"),
        2 => format!("{text}.0"),
        _ => text.into(),
    })?)
}

fn effective_config(
    input: &UpgradeInput,
    occurrence: &Occurrence,
) -> Result<UpgradeConfig, BoxError> {
    let mut config = input.config.clone();
    if config.msrv.is_none() {
        let manifest: DocumentMut = input
            .manifests
            .get(&occurrence.consumer)
            .ok_or("consumer manifest missing")?
            .parse()?;
        if let Some(rust) = manifest.get("package").and_then(|p| p.get("rust-version")) {
            if let Some(version) = rust.as_str() {
                config.msrv = Some(version.into());
            } else if rust.get("workspace").and_then(Item::as_bool) == Some(true) {
                let root: DocumentMut = input
                    .manifests
                    .get(&input.root.join("Cargo.toml"))
                    .ok_or("workspace manifest missing")?
                    .parse()?;
                config.msrv = root
                    .get("workspace")
                    .and_then(|w| w.get("package"))
                    .and_then(|p| p.get("rust-version"))
                    .and_then(Item::as_str)
                    .map(str::to_owned);
                if config.msrv.is_none() {
                    return Err(
                        "inherited package rust-version missing from workspace.package".into(),
                    );
                }
            }
        }
    }
    let key = registry_key(occurrence);
    for rule in &input.config.rules {
        if rule.dependency.is_empty() {
            return Err("upgrade rule requires a nonempty dependency selector".into());
        }
        let matches = |pattern: &str, value: &str| -> Result<bool, BoxError> {
            Ok(patterns(&[pattern.into()])?.is_match(value))
        };
        if !matches(&rule.dependency, &key.name)? {
            continue;
        }
        if let Some(alias) = &rule.alias
            && !matches(alias, &occurrence.raw.alias)?
        {
            continue;
        }
        if let Some(package) = &rule.package
            && !matches(package, &occurrence.package)?
        {
            continue;
        }
        if let Some(registry) = &rule.registry
            && registry != key.registry.as_deref().unwrap_or("crates-io")
        {
            continue;
        }
        if let Some(target) = &rule.target
            && !occurrence
                .raw
                .target
                .as_deref()
                .map(|value| matches(target, value))
                .transpose()?
                .unwrap_or(false)
        {
            continue;
        }
        if !rule.kinds.is_empty() && !rule.kinds.contains(&occurrence.raw.kind) {
            continue;
        }
        macro_rules! apply { ($($field:ident),+) => { $(if let Some(value) = rule.$field { config.$field = value; })+ }; }
        apply!(policy, pinned, prerelease, source);
        if rule.msrv.is_some() {
            config.msrv.clone_from(&rule.msrv);
        }
    }
    macro_rules! apply_cli { ($($field:ident),+) => { $(if let Some(value) = input.overrides.$field { config.$field = value; })+ }; }
    apply_cli!(policy, pinned, prerelease, source);
    if input.overrides.msrv.is_some() {
        config.msrv.clone_from(&input.overrides.msrv);
    }
    if let Some(msrv) = &config.msrv {
        rust_version(msrv)?;
    }
    Ok(config)
}

fn minimum(req: &VersionReq) -> Option<Version> {
    req.comparators
        .iter()
        .filter(|c| !matches!(c.op, Op::Less | Op::LessEq))
        .map(|c| Version {
            major: c.major,
            minor: c.minor.unwrap_or(0),
            patch: c.patch.unwrap_or(0),
            pre: c.pre.clone(),
            build: semver::BuildMetadata::EMPTY,
        })
        .max()
}

#[allow(clippy::option_if_let_else)] // Preserve the visible wildcard/precision/semver cases.
fn rewrite_requirement(old: &str, version: &Version) -> Result<String, BoxError> {
    // Cargo ignores build metadata in requirements; registry versions may carry
    // it, but writing it into Cargo.toml produces warnings and loses precision.
    let version = Version {
        build: semver::BuildMetadata::EMPTY,
        ..version.clone()
    };
    let version = &version;
    let trimmed = old.trim();
    let req = VersionReq::parse(trimmed)?;
    if req.matches(version)
        && (req.comparators.is_empty()
            || req.comparators.len() > 1
            || req.comparators.iter().any(|c| {
                matches!(
                    c.op,
                    Op::Greater | Op::GreaterEq | Op::Less | Op::LessEq | Op::Wildcard
                )
            }))
    {
        return Ok(old.into());
    }
    let body = trimmed.trim_start_matches(['=', '~', '^']).trim();
    if req.comparators.len() > 1
        || req
            .comparators
            .iter()
            .any(|c| matches!(c.op, Op::Greater | Op::GreaterEq | Op::Less | Op::LessEq))
    {
        return Ok(old.into());
    }
    if body == "*" || body.eq_ignore_ascii_case("x") {
        return Ok(old.into());
    }
    let components = body.split('.').collect::<Vec<_>>();
    let numbers = [version.major, version.minor, version.patch];
    let wildcard = components
        .iter()
        .position(|part| *part == "*" || part.eq_ignore_ascii_case("x"));
    let next = if let Some(index) = wildcard {
        components
            .iter()
            .enumerate()
            .map(|(i, part)| {
                if i < index {
                    numbers[i].to_string()
                } else {
                    (*part).into()
                }
            })
            .collect::<Vec<String>>()
            .join(".")
    } else if version.pre.is_empty()
        && version.build.is_empty()
        && !body.contains('-')
        && !body.contains('+')
    {
        numbers
            .iter()
            .take(components.len().min(3))
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(".")
    } else {
        version.to_string()
    };
    let operator = if trimmed.starts_with('=') {
        "="
    } else if trimmed.starts_with('~') {
        "~"
    } else if trimmed.starts_with('^') {
        "^"
    } else {
        ""
    };
    Ok(format!("{operator}{next}"))
}

fn replace_requirement(item: &mut Item, requirement: &str) -> Result<(), BoxError> {
    let value = if item.is_str() {
        item.as_value_mut()
    } else {
        item.get_mut("version").and_then(Item::as_value_mut)
    }
    .ok_or("version must be a string")?;
    let decor = value.decor().clone();
    *value = Value::from(requirement);
    *value.decor_mut() = decor;
    Ok(())
}
