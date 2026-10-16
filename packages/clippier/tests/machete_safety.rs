//! Evidence protection and filesystem invariants for native dependency removal.
#![cfg(feature = "machete")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use clippier::machete::{MacheteOptions, apply_machete_plan, plan_machete};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

fn write(root: &Path, path: &str, contents: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "Cargo.toml",
        "# untouched comment\n[package]\nname='safety'\nversion='0.1.0'\nedition='2021'\n[workspace]\n[dependencies]\nused='1.0.0'\nunused='1.0.0'\n",
    );
    write(dir.path(), "src/lib.rs", "pub use used::Thing;\n");
    dir
}

fn options(root: &Path) -> MacheteOptions {
    MacheteOptions {
        manifest_path: Some(root.join("Cargo.toml")),
        ..MacheteOptions::default()
    }
}

fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, directory: &Path, result: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, result);
            } else {
                result.insert(
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

#[test]
fn repeated_plans_are_deterministic_and_preserve_the_entire_tree() {
    let dir = fixture();
    let before = files(dir.path());
    let first = plan_machete(&options(dir.path())).unwrap();
    let second = plan_machete(&options(dir.path())).unwrap();
    assert!(first.report.complete);
    assert_eq!(first.changes.len(), 1);
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(second).unwrap()
    );
    assert_eq!(files(dir.path()), before);
}

#[test]
fn absent_lockfile_stays_absent_after_successful_application() {
    let dir = fixture();
    let plan = plan_machete(&options(dir.path())).unwrap();
    let result = apply_machete_plan(&plan).unwrap();
    assert!(result.applied && result.lockfile_preserved);
    assert!(!dir.path().join("Cargo.lock").exists());
    assert!(!dir.path().join("target").exists());
    let second = plan_machete(&options(dir.path())).unwrap();
    assert!(second.changes.is_empty());
    assert_eq!(files(dir.path()).len(), 2);
}

#[test]
fn new_lockfile_invalidates_captured_absence_without_live_writes() {
    let dir = fixture();
    let plan = plan_machete(&options(dir.path())).unwrap();
    write(dir.path(), "Cargo.lock", "new lockfile\n");
    let before = files(dir.path());
    assert!(apply_machete_plan(&plan).is_err());
    assert_eq!(files(dir.path()), before);
}

#[test]
fn deletion_of_source_evidence_is_rejected_before_any_manifest_write() {
    let dir = fixture();
    let plan = plan_machete(&options(dir.path())).unwrap();
    fs::remove_file(dir.path().join("src/lib.rs")).unwrap();
    let before = files(dir.path());
    assert!(apply_machete_plan(&plan).is_err());
    assert_eq!(files(dir.path()), before);
}

#[test]
fn new_ignore_files_invalidate_plans_even_when_no_source_bytes_changed() {
    for ignore in [".ignore", ".gitignore"] {
        let dir = fixture();
        let plan = plan_machete(&options(dir.path())).unwrap();
        write(dir.path(), ignore, "src/lib.rs\n");
        let before = files(dir.path());
        assert!(
            apply_machete_plan(&plan).is_err(),
            "new {ignore} must invalidate plan"
        );
        assert_eq!(files(dir.path()), before);
    }
}

#[test]
fn unrelated_manifest_fields_survive_removal_exactly() {
    let dir = fixture();
    write(
        dir.path(),
        "Cargo.toml",
        "# manifest comment\n[package]\nname = 'safety'\nversion = '0.1.0'\nedition = '2021'\n[workspace]\n[dependencies]\nused = { version = '1.0.0', default-features = false, features = ['a'] } # keep\nunused = '1.0.0'\n[package.metadata.custom]\nanswer = 42 # keep too\n",
    );
    let plan = plan_machete(&options(dir.path())).unwrap();
    let before = fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    apply_machete_plan(&plan).unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("Cargo.toml")).unwrap(),
        before.replace("unused = '1.0.0'\n", "")
    );
}

#[test]
fn dependency_feature_wiring_is_retained_without_an_invalid_removal() {
    let dir = fixture();
    write(
        dir.path(),
        "Cargo.toml",
        "[package]\nname='safety'\nversion='0.1.0'\nedition='2021'\n[workspace]\n[dependencies]\noptional={version='1.0.0',optional=true}\n[features]\nstrong=['optional/feature']\nweak=['optional?/feature']\nexplicit=['dep:optional']\n",
    );
    write(dir.path(), "src/lib.rs", "pub fn value() {}\n");
    let before = files(dir.path());
    let plan = plan_machete(&options(dir.path())).unwrap();
    assert!(plan.changes.is_empty());
    apply_machete_plan(&plan).unwrap();
    assert_eq!(files(dir.path()), before);
}
