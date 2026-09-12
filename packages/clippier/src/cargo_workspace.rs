//! Shared Cargo workspace metadata helpers.

#[cfg(feature = "release")]
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[cfg(feature = "release")]
use cargo_metadata::{Metadata, MetadataCommand, PackageId};

#[cfg(feature = "release")]
type BoxError = Box<dyn std::error::Error + Send + Sync>;

pub fn normalize_workspace_root(path: &Path) -> PathBuf {
    if path.file_name().is_some_and(|name| name == "Cargo.toml") {
        path.parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    } else {
        path.to_path_buf()
    }
}

#[cfg(feature = "release")]
pub fn load_metadata(workspace_root: &Path, no_deps: bool) -> Result<Metadata, BoxError> {
    let mut command = MetadataCommand::new();
    command.current_dir(workspace_root);
    if no_deps {
        command.no_deps();
    }
    Ok(command.exec()?)
}

#[cfg(feature = "release")]
pub fn workspace_packages(metadata: &Metadata) -> BTreeMap<String, &cargo_metadata::Package> {
    let members = metadata
        .workspace_members
        .iter()
        .cloned()
        .collect::<BTreeSet<PackageId>>();
    metadata
        .packages
        .iter()
        .filter(|package| members.contains(&package.id))
        .map(|package| (package.name.to_string(), package))
        .collect()
}

#[cfg(feature = "release")]
pub fn is_publishable(package: &cargo_metadata::Package) -> bool {
    package
        .publish
        .as_ref()
        .is_none_or(|registries| registries.iter().any(|registry| registry == "crates-io"))
}

#[cfg(feature = "release")]
pub fn publishable_workspace_packages(
    metadata: &Metadata,
) -> BTreeMap<String, &cargo_metadata::Package> {
    workspace_packages(metadata)
        .into_iter()
        .filter(|(_name, package)| is_publishable(package))
        .collect()
}
