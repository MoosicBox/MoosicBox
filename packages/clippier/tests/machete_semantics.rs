//! Public-API safety regressions for syntax and module-context integration.
#![cfg(feature = "machete")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use clippier::machete::{
    DependencyKind, DependencyStatus, MacheteOptions, analyze_machete, apply_machete_plan,
    plan_machete,
};
use std::{fs, path::Path};

fn write(root: &Path, path: &str, source: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
}

fn options(root: &Path) -> MacheteOptions {
    MacheteOptions {
        manifest_path: Some(root.join("Cargo.toml")),
        ..MacheteOptions::default()
    }
}

#[test]
fn raw_absolute_and_namespace_references_retain_the_external_dependency() {
    for source in [
        "use r#dep as external; pub fn run() { external::run(); }",
        "extern crate r#dep as external; pub fn run() { external::run(); }",
        "mod dep {} use ::dep as external; pub fn run() { external::run(); }",
        "mod dep {} pub use {::dep as external}; pub fn run() { external::run(); }",
        "trait Local { type dep; fn run() { dep::run(); } }",
        "mod local { pub fn dep() {} } use local::dep; pub fn run() { dep::run(); }",
    ] {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "Cargo.toml",
            "[package]\nname='semantic_fixture'\nversion='0.1.0'\nedition='2021'\n[dependencies]\ndep='1.0.0'\n",
        );
        write(root.path(), "src/lib.rs", source);
        let report = analyze_machete(&options(root.path())).unwrap();
        assert_eq!(report.dependencies.len(), 1);
        assert_eq!(
            report.dependencies[0].status,
            DependencyStatus::Used,
            "{source}: {:?}",
            report.diagnostics
        );
    }
}

#[test]
fn compound_test_cfg_in_another_file_has_development_ownership() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "Cargo.toml",
        "[package]\nname='semantic_fixture'\nversion='0.1.0'\nedition='2021'\n[dev-dependencies]\ndep='1.0.0'\n[features]\nlogic=[]\n",
    );
    write(
        root.path(),
        "src/lib.rs",
        "#[cfg(all(test, feature=\"logic\"))] mod tests;\n",
    );
    write(
        root.path(),
        "src/tests.rs",
        "pub fn run() { dep::run(); }\n",
    );
    let mut options = options(root.path());
    options.kinds = vec![DependencyKind::Normal, DependencyKind::Dev];
    let report = analyze_machete(&options).unwrap();
    assert_eq!(report.dependencies[0].status, DependencyStatus::Used);
    assert!(report.evidence.iter().any(|entry| {
        entry.alias == "dep"
            && entry.source_target == DependencyKind::Dev
            && entry.file.ends_with("src/tests.rs")
            && entry
                .conditions
                .iter()
                .any(|condition| condition.contains("test"))
    }));
}

#[test]
fn external_modules_in_associated_bodies_cannot_make_dependencies_removable() {
    for source in [
        "pub struct Runner; impl Runner { pub fn run() { #[path=\"hidden.rs\"] mod hidden; hidden::run(); } }",
        "pub trait Runner { fn run() { #[path=\"hidden.rs\"] mod hidden; hidden::run(); } }",
        "const INITIALIZER: () = { #[path=\"hidden.rs\"] mod hidden; hidden::run(); };",
    ] {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "Cargo.toml",
            "[package]\nname='semantic_fixture'\nversion='0.1.0'\nedition='2021'\n[dependencies]\ndep='1.0.0'\n",
        );
        write(root.path(), "src/lib.rs", source);
        write(root.path(), "src/hidden.rs", "pub fn run() { dep::run(); }");
        let plan = plan_machete(&options(root.path())).unwrap();
        assert_ne!(
            plan.report.dependencies[0].status,
            DependencyStatus::Unused,
            "{source}: {:?}",
            plan.report.diagnostics
        );
        if plan.report.dependencies[0].status == DependencyStatus::Incomplete {
            assert!(!plan.report.complete);
            assert!(plan.changes.is_empty());
            assert!(apply_machete_plan(&plan).is_err());
        }
    }
}
