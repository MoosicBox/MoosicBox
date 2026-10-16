//! Checked removal and preservation of dependency declarations.
#![cfg(feature = "machete")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use clippier::machete::{
    DependencyKind, DependencyStatus, MacheteOptions, analyze_machete, apply_machete_plan,
    plan_machete,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/machete")
}
fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out);
    out
}
fn inventory() -> serde_json::Value {
    serde_json::from_slice(&fs::read(fixtures().join("sources.json")).unwrap()).unwrap()
}
fn temporary_input(input: &str) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let inventory = inventory();
    for (relative, stored) in inventory["trees"][input].as_object().unwrap() {
        let path = temp.path().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::copy(fixtures().join(stored.as_str().unwrap()), path).unwrap();
    }
    temp
}
fn options(root: &Path) -> MacheteOptions {
    MacheteOptions {
        manifest_path: Some(root.to_owned()),
        kinds: vec![
            DependencyKind::Normal,
            DependencyKind::Dev,
            DependencyKind::Build,
        ],
        ..Default::default()
    }
}
#[test]
fn removes_every_dependency_section_without_reformatting() {
    for case in inventory()["special_inputs"]["removal"].as_array().unwrap() {
        let temp = temporary_input(case["input"].as_str().unwrap());
        // Adapt a raw table-edit assertion to checked removal: remove the use
        // first, not the safety check. The byte-exact output oracle is unchanged.
        fs::write(
            temp.path().join("src/main.rs"),
            "fn main() { log::info!(\"retained\"); }\n",
        )
        .unwrap();
        let options = options(temp.path());
        let before = files(temp.path());
        let report = analyze_machete(&options).unwrap();
        let mut actual = report
            .dependencies
            .iter()
            .map(|d| (d.dependency.as_str(), d.kind, d.target.as_deref(), d.status))
            .collect::<Vec<_>>();
        actual.sort_by_key(|row| (row.0, row.1, row.2));
        let mut expected = vec![
            ("log", DependencyKind::Normal, None, DependencyStatus::Used),
            (
                "cc",
                DependencyKind::Normal,
                Some("cfg(unix)"),
                DependencyStatus::Unused,
            ),
            (
                "log-once",
                DependencyKind::Normal,
                Some("cfg(unix)"),
                DependencyStatus::Unused,
            ),
            (
                "log-once",
                DependencyKind::Dev,
                None,
                DependencyStatus::Unused,
            ),
            (
                "rand",
                DependencyKind::Build,
                None,
                DependencyStatus::Unused,
            ),
        ];
        expected.sort_by_key(|row| (row.0, row.1, row.2));
        assert_eq!(actual, expected, "removal assertion: {case}");
        assert_eq!(files(temp.path()), before);
        let plan = plan_machete(&options).unwrap();
        assert_eq!(files(temp.path()), before);
        assert!(plan.report.complete, "{:?}", plan.report.diagnostics);
        assert_eq!(plan.changes.len(), 1);
        let expected =
            fs::read_to_string(fixtures().join(case["expected"].as_str().unwrap())).unwrap();
        assert_eq!(
            plan.changes[0].before.as_bytes(),
            before[Path::new("Cargo.toml")]
        );
        assert_eq!(
            plan.changes[0].after, expected,
            "exact removal bytes: {case}"
        );
        let applied = apply_machete_plan(&plan).unwrap();
        assert!(applied.applied);
        assert!(applied.lockfile_preserved);
        let mut expected_tree = before;
        expected_tree.insert(PathBuf::from("Cargo.toml"), expected.into_bytes());
        assert_eq!(
            files(temp.path()),
            expected_tree,
            "only planned manifest may change"
        );
        let repeated = plan_machete(&options).unwrap();
        assert!(repeated.changes.is_empty());
        assert!(!repeated.report.has_unused());
    }
}
#[test]
fn used_dependencies_cannot_be_forced_out() {
    for case in inventory()["special_inputs"]["removal"].as_array().unwrap() {
        let temp = temporary_input(case["input"].as_str().unwrap());
        let mut options = options(temp.path());
        options.dependencies = vec!["cc".into(), "log-once".into(), "rand".into()];
        let before = files(temp.path());
        let plan = plan_machete(&options).unwrap();
        assert_eq!(files(temp.path()), before);
        let actual = plan
            .report
            .dependencies
            .iter()
            .filter(|d| d.dependency == "log-once")
            .map(|d| (d.kind, d.target.as_deref(), d.status))
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            vec![
                (
                    DependencyKind::Normal,
                    Some("cfg(unix)"),
                    DependencyStatus::Used
                ),
                (DependencyKind::Dev, None, DependencyStatus::Used),
            ]
        );
        let unused = plan
            .report
            .dependencies
            .iter()
            .filter(|d| d.status == DependencyStatus::Unused)
            .map(|d| d.dependency.as_str())
            .collect::<Vec<_>>();
        assert_eq!(unused, vec!["cc", "rand"]);
        assert!(
            plan.changes
                .iter()
                .all(|change| change.after.contains("log-once"))
        );
    }
}
#[test]
fn changed_source_or_lockfile_rejects_apply_without_partial_writes() {
    for changed in ["src/main.rs", "Cargo.lock"] {
        let temp = temporary_input(
            inventory()["special_inputs"]["removal"][0]["input"]
                .as_str()
                .unwrap(),
        );
        let options = options(temp.path());
        let plan = plan_machete(&options).unwrap();
        assert!(!plan.changes.is_empty());
        let path = temp.path().join(changed);
        let mut bytes = fs::read(&path).unwrap();
        bytes.extend_from_slice(b"\n// changed evidence\n");
        fs::write(path, bytes).unwrap();
        let before = files(temp.path());
        assert!(apply_machete_plan(&plan).is_err());
        assert_eq!(files(temp.path()), before);
    }
}
#[test]
fn changed_plan_rejects_apply_without_writes() {
    let temp = temporary_input(
        inventory()["special_inputs"]["removal"][0]["input"]
            .as_str()
            .unwrap(),
    );
    let mut plan = plan_machete(&options(temp.path())).unwrap();
    assert!(!plan.changes.is_empty());
    plan.changes[0].after.push_str("\n# unsealed change\n");
    let before = files(temp.path());
    assert!(apply_machete_plan(&plan).is_err());
    assert_eq!(files(temp.path()), before);
}
