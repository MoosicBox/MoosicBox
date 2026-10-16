//! Explicit Cargo-metadata import; never executes Cargo.
use super::BoxError;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::PathBuf};

/// Captured resolved library names and their manifest evidence.
///
/// Callers may explicitly obtain JSON using `cargo metadata --format-version 1
/// --offline --locked` (requires an existing, current lockfile and cached sources).
/// The default engine path does not execute Cargo or resolve dependencies.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct MacheteMetadata {
    /// Consumer manifest -> resolved dependency alias -> library target name.
    pub crate_names: BTreeMap<PathBuf, BTreeMap<String, String>>,
    /// Exact dependency/consumer manifest bytes captured at import time.
    pub manifests: BTreeMap<PathBuf, String>,
}
impl MacheteMetadata {
    /// Parse Cargo's format-version 1 JSON, resolving dependency IDs to libraries.
    ///
    /// # Errors
    /// * Invalid JSON, missing package/resolve data or malformed IDs/targets.
    /// * Inaccessible or non-UTF-8 manifests required as stale-plan evidence.
    pub fn from_cargo_json(text: &str) -> Result<Self, BoxError> {
        let value: serde_json::Value = serde_json::from_str(text)?;
        let packages = value
            .get("packages")
            .and_then(serde_json::Value::as_array)
            .ok_or("metadata packages missing")?;
        let mut libraries = BTreeMap::new();
        let mut paths = BTreeMap::new();
        let mut result = Self::default();
        for package in packages {
            let id = string(package, "id")?;
            let path = PathBuf::from(string(package, "manifest_path")?);
            let contents = fs::read_to_string(&path).map_err(|error| {
                format!(
                    "cannot capture metadata manifest {}: {error}",
                    path.display()
                )
            })?;
            result.manifests.insert(path.clone(), contents);
            paths.insert(id.to_owned(), path);
            let targets = package
                .get("targets")
                .and_then(serde_json::Value::as_array)
                .ok_or("metadata targets missing")?;
            for target in targets {
                if target
                    .get("kind")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|kinds| {
                        kinds
                            .iter()
                            .filter_map(serde_json::Value::as_str)
                            .any(|kind| {
                                matches!(
                                    kind,
                                    "lib"
                                        | "rlib"
                                        | "dylib"
                                        | "proc-macro"
                                        | "staticlib"
                                        | "cdylib"
                                )
                            })
                    })
                {
                    libraries.insert(id.to_owned(), string(target, "name")?.replace('-', "_"));
                    break;
                }
            }
        }
        let nodes = value
            .get("resolve")
            .and_then(|resolve| resolve.get("nodes"))
            .and_then(serde_json::Value::as_array)
            .ok_or("metadata resolved graph missing (do not use --no-deps)")?;
        for node in nodes {
            let consumer = paths
                .get(string(node, "id")?)
                .ok_or("resolved consumer package missing")?;
            let dependencies = node
                .get("deps")
                .and_then(serde_json::Value::as_array)
                .ok_or("resolved dependency edges missing")?;
            for dependency in dependencies {
                let alias = string(dependency, "name")?.replace('-', "_");
                let id = string(dependency, "pkg")?;
                if let Some(library) = libraries.get(id) {
                    // Resolve edge names already encode explicit aliases. A distinct
                    // alias is itself the crate name; package-name edges use lib name.
                    let package = packages
                        .iter()
                        .find(|package| {
                            package.get("id").and_then(serde_json::Value::as_str) == Some(id)
                        })
                        .ok_or("resolved package missing")?;
                    let package_name = string(package, "name")?.replace('-', "_");
                    let name = if alias == package_name || alias == *library {
                        library.clone()
                    } else {
                        alias.clone()
                    };
                    let names = result.crate_names.entry(consumer.clone()).or_default();
                    if alias == *library {
                        names.insert(package_name, library.clone());
                    }
                    names.insert(alias, name);
                }
            }
        }
        Ok(result)
    }
}
fn string<'a>(value: &'a serde_json::Value, key: &str) -> Result<&'a str, BoxError> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("metadata string field missing: {key}").into())
}
