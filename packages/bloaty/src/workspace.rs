//! Workspace package and final-target resolution.

use std::collections::BTreeSet;

use anyhow::{Result, bail};
use cargo_metadata::{Metadata, Package, Target, TargetKind};

use crate::{ArtifactKind, TargetSelection};

/// Resolves one package and final artifact target.
///
/// When no target is specified, resolution succeeds only when the package has exactly one
/// supported final artifact target.
///
/// # Errors
///
/// * The package is absent or ambiguous
/// * The target is absent, ambiguous, or unsupported
pub fn resolve_target(
    metadata: &Metadata,
    package_name: &str,
    target_name: Option<&str>,
) -> Result<TargetSelection> {
    let packages = metadata
        .workspace_packages()
        .into_iter()
        .filter(|package| package.name == package_name)
        .collect::<Vec<_>>();
    let package = match packages.as_slice() {
        [] => bail!("workspace package '{package_name}' was not found"),
        [package] => *package,
        _ => bail!("workspace package name '{package_name}' is ambiguous"),
    };

    let supported = package
        .targets
        .iter()
        .filter_map(|target| target_kind(target).map(|kind| (target, kind)))
        .collect::<Vec<_>>();
    let matches = supported
        .iter()
        .filter(|(target, _)| target_name.is_none_or(|name| target.name == name))
        .copied()
        .collect::<Vec<_>>();
    let (target, kind) = match matches.as_slice() {
        [] => {
            let candidates = supported
                .iter()
                .map(|(target, _)| target.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            bail!(
                "supported target '{}' was not found in package '{package_name}'; candidates: {candidates}",
                target_name.unwrap_or("<unspecified>")
            );
        }
        [selection] => *selection,
        _ => {
            let candidates = matches
                .iter()
                .map(|(target, _)| target.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            bail!(
                "package '{package_name}' has multiple supported targets; select one with --target: {candidates}"
            );
        }
    };

    Ok(selection(package, target, kind))
}

fn selection(package: &Package, target: &Target, kind: ArtifactKind) -> TargetSelection {
    TargetSelection {
        package_id: package.id.clone(),
        package_name: package.name.to_string(),
        target_name: target.name.clone(),
        kind,
        required_features: target.required_features.clone(),
        available_features: package.features.keys().cloned().collect::<BTreeSet<_>>(),
    }
}

fn target_kind(target: &Target) -> Option<ArtifactKind> {
    if target.kind.contains(&TargetKind::Bin) {
        Some(ArtifactKind::Binary)
    } else if target.kind.contains(&TargetKind::CDyLib) {
        Some(ArtifactKind::Cdylib)
    } else if target.kind.contains(&TargetKind::DyLib) {
        Some(ArtifactKind::Dylib)
    } else if target.kind.contains(&TargetKind::StaticLib) {
        Some(ArtifactKind::Staticlib)
    } else {
        None
    }
}
