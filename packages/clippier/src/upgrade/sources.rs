//! Classify manifest overrides before creating registry requests or lock targets.
use super::{BoxError, UpgradeInput, UpgradeStatus};
use semver::VersionReq;
use toml_edit::{DocumentMut, Item};

pub(super) fn patch_policy(
    input: &UpgradeInput,
    name: &str,
    registry: Option<&str>,
    requirement: Option<&str>,
) -> Result<Option<(UpgradeStatus, String)>, BoxError> {
    let root: DocumentMut = input
        .manifests
        .get(&input.root.join("Cargo.toml"))
        .ok_or("missing workspace manifest")?
        .parse()?;
    let Some(patches) = root.get("patch").and_then(Item::as_table_like) else {
        return Ok(None);
    };
    let req = requirement.map(VersionReq::parse).transpose()?;
    let mut uncertain = false;
    for (scope, entries) in patches.iter() {
        let scope_match = matches_registry(input, scope, registry)?;
        if scope_match == Some(false) {
            continue;
        }
        let Some(entries) = entries.as_table_like() else {
            continue;
        };
        for (alias, patch) in entries.iter() {
            let package = patch.get("package").and_then(Item::as_str).unwrap_or(alias);
            if package != name {
                continue;
            }
            // Matching a patch name alone cannot establish that Cargo selected
            // it. Multiple versions/sources and unused patches remain ambiguous.
            uncertain = true;
            let lock = input
                .snapshots
                .iter()
                .find(|s| s.path == input.root.join("Cargo.lock"))
                .and_then(|s| s.contents.as_deref())
                .map(str::parse::<DocumentMut>)
                .transpose()?;

            if scope_match != Some(true) {
                continue;
            }
            let matching = lock
                .as_ref()
                .and_then(|l| l.get("package"))
                .and_then(Item::as_array_of_tables)
                .into_iter()
                .flatten()
                .filter(|p| p.get("name").and_then(Item::as_str) == Some(name))
                .filter(|p| {
                    p.get("version")
                        .and_then(Item::as_str)
                        .and_then(|v| v.parse::<semver::Version>().ok())
                        .is_some_and(|v| req.as_ref().is_some_and(|req| req.matches(&v)))
                })
                .collect::<Vec<_>>();
            if matching.len() != 1 {
                continue;
            }
            let source = matching[0].get("source").and_then(Item::as_str);
            let git = patch.get("git").and_then(Item::as_str);
            let path = patch.get("path").and_then(Item::as_str);
            let git_matches = git.is_some_and(|url| {
                source.is_some_and(|s| {
                    s.strip_prefix("git+")
                        .is_some_and(|s| s.split(['?', '#']).next() == Some(url))
                })
            });
            if git_matches {
                return Ok(Some((
                    UpgradeStatus::Skipped,
                    "resolved through Git patch; source and requirement preserved".into(),
                )));
            }
            if path.is_some() && source.is_none() {
                return Ok(Some((
                    UpgradeStatus::Skipped,
                    "resolved through path patch; source and requirement preserved".into(),
                )));
            }
        }
    }
    Ok(uncertain.then(|| (UpgradeStatus::Unavailable,
        "applicable patch may affect resolution; preserving declaration because its effective source is uncertain".into())))
}

fn matches_registry(
    input: &UpgradeInput,
    scope: &str,
    registry: Option<&str>,
) -> Result<Option<bool>, BoxError> {
    if scope == registry.unwrap_or("crates-io") {
        return Ok(Some(true));
    }
    Ok(registry_index(input, registry)?.map(|index| same_registry(&index, scope)))
}

pub(super) fn same_registry(a: &str, b: &str) -> bool {
    fn normalize(value: &str) -> &str {
        let value = value.strip_prefix("registry+").unwrap_or(value);
        let value = value
            .strip_prefix("sparse+")
            .unwrap_or(value)
            .trim_end_matches('/');
        match value {
            "crates-io" | "https://index.crates.io" => {
                "https://github.com/rust-lang/crates.io-index"
            }
            _ => value,
        }
    }
    normalize(a) == normalize(b)
}

pub(super) fn registry_index(
    input: &UpgradeInput,
    registry: Option<&str>,
) -> Result<Option<String>, BoxError> {
    let name = registry.unwrap_or("crates-io");
    let env_key = format!(
        "CARGO_REGISTRIES_{}_INDEX",
        name.to_ascii_uppercase().replace('-', "_")
    );
    if let Some(value) = input.registry_indexes.get(&env_key) {
        return Ok(Some(value.clone()));
    }
    let mut index = None;
    for snapshot in &input.snapshots {
        if snapshot
            .path
            .file_name()
            .is_some_and(|n| n == "config" || n == "config.toml")
            && let Some(text) = &snapshot.contents
        {
            let config: DocumentMut = text.parse()?;
            if let Some(value) = config
                .get("registries")
                .and_then(|r| r.get(name))
                .and_then(|r| r.get("index"))
                .and_then(Item::as_str)
            {
                if index.as_deref().is_some_and(|previous| previous != value) {
                    return Ok(None);
                }
                index = Some(value.to_owned());
            }
        }
    }
    Ok(index.or_else(|| {
        (name == "crates-io").then(|| "https://github.com/rust-lang/crates.io-index".into())
    }))
}
