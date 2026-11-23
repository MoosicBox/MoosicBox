//! What a simulation links: every crate its package reaches through normal dependencies, for
//! the host (the platform simulations build for).
//!
//! Build dependencies and proc macros run at compile time, not in a run, so the walk does not
//! enter them; development dependencies are not linked at all.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use cargo_metadata::{DependencyKind, MetadataCommand, PackageId, TargetKind};

/// A crate of the workspace (or another local path) that a simulation links.
#[derive(Debug, Clone)]
pub struct LocalCrate {
    /// Its name.
    pub name: String,
    /// Its directory.
    pub dir: PathBuf,
    /// Its manifest's `[package.metadata]`.
    pub metadata: serde_json::Value,
}

/// A crate from a registry or git repository that a simulation links.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExternalCrate {
    /// Its name.
    pub name: String,
    /// Its version.
    pub version: semver::Version,
    /// Where it comes from.
    pub source: String,
}

/// Everything a simulation links.
#[derive(Debug, Clone, Default)]
pub struct Graph {
    /// Local crates, by name.
    pub local: Vec<LocalCrate>,
    /// Crates from registries and git repositories, by name and version.
    pub external: Vec<ExternalCrate>,
}

/// The host's target triple, as `rustc -vV` reports it.
fn host() -> Result<String, String> {
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let output = std::process::Command::new(&rustc)
        .arg("-vV")
        .output()
        .map_err(|e| format!("cannot run rustc -vV: {e}"))?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(str::to_owned)
        .ok_or_else(|| "rustc -vV reported no host".to_owned())
}

/// Everything package `root` of the workspace at `manifest_path` links.
///
/// # Errors
///
/// * If `cargo metadata` fails, or the workspace has no package `root`
pub fn resolve(manifest_path: &Path, root: &str) -> Result<Graph, String> {
    let metadata = MetadataCommand::new()
        .manifest_path(manifest_path)
        .other_options(vec!["--filter-platform".to_owned(), host()?])
        .exec()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    let packages: BTreeMap<&PackageId, &cargo_metadata::Package> =
        metadata.packages.iter().map(|p| (&p.id, p)).collect();
    let resolve = metadata
        .resolve
        .as_ref()
        .ok_or("cargo metadata resolved no dependencies")?;
    let nodes: BTreeMap<&PackageId, &cargo_metadata::Node> =
        resolve.nodes.iter().map(|n| (&n.id, n)).collect();
    let start = metadata
        .workspace_members
        .iter()
        .find(|id| packages[id].name.as_str() == root)
        .ok_or_else(|| format!("the workspace has no package {root}"))?;

    let mut seen: BTreeSet<&PackageId> = BTreeSet::new();
    let mut stack = vec![start];
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let package = packages[id];
        let proc_macro = package
            .targets
            .iter()
            .any(|t| t.kind.contains(&TargetKind::ProcMacro));
        if proc_macro {
            continue;
        }
        for dep in &nodes[id].deps {
            if dep
                .dep_kinds
                .iter()
                .any(|kind| kind.kind == DependencyKind::Normal)
            {
                stack.push(&dep.pkg);
            }
        }
    }

    let mut graph = Graph::default();
    for id in seen {
        let package = packages[id];
        if package
            .targets
            .iter()
            .any(|t| t.kind.contains(&TargetKind::ProcMacro))
        {
            continue;
        }
        match &package.source {
            None => graph.local.push(LocalCrate {
                name: package.name.to_string(),
                dir: package
                    .manifest_path
                    .parent()
                    .map_or_else(PathBuf::new, |dir| dir.as_std_path().to_path_buf()),
                metadata: package.metadata.clone(),
            }),
            Some(source) => graph.external.push(ExternalCrate {
                name: package.name.to_string(),
                version: package.version.clone(),
                source: source.repr.clone(),
            }),
        }
    }
    graph.local.sort_by(|a, b| a.name.cmp(&b.name));
    graph.external.sort();
    Ok(graph)
}
