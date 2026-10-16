//! Offline behavioral and integrity checks for native dependency analysis.
#![cfg(feature = "machete")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use clippier::machete::{
    DependencyKind, MacheteMetadata, MacheteOptions, analyze_machete, plan_machete,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/machete")
}
fn inventory() -> Value {
    serde_json::from_slice(&fs::read(root().join("sources.json")).unwrap()).unwrap()
}
fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(base: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            assert!(
                !fs::symlink_metadata(&path)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
            if path.is_dir() {
                visit(base, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(base).unwrap().to_owned(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}
fn copy(input: &str, output: &Path) {
    for (relative, stored) in inventory()["trees"][input].as_object().unwrap() {
        let target = output.join(relative);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(root().join(stored.as_str().unwrap()), target).unwrap();
    }
}

fn names(value: &toml::Value) -> Vec<(String, String)> {
    let mut result = Vec::new();
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(deps) = value.get(section).and_then(toml::Value::as_table) {
            result.extend(deps.iter().map(|(key, spec)| {
                (
                    key.clone(),
                    spec.get("package")
                        .and_then(toml::Value::as_str)
                        .unwrap_or(key)
                        .to_owned(),
                )
            }));
        }
    }
    if let Some(targets) = value.get("target").and_then(toml::Value::as_table) {
        for target in targets.values() {
            result.extend(names(target));
        }
    }
    result
}
// Build a minimal resolved graph against manifests in the temporary tree. No
// subprocess, registry/cache lookup, environment override, or resolver is needed.
fn metadata(
    base: &Path,
    registry_names: bool,
    configured: &Value,
) -> (MacheteMetadata, tempfile::TempDir) {
    let resolved = tempfile::tempdir().unwrap();
    let mut packages = Vec::new();
    let mut nodes = Vec::new();
    for relative in files(base)
        .keys()
        .filter(|p| p.file_name().is_some_and(|n| n == "Cargo.toml"))
    {
        let path = base.join(relative);
        let text = fs::read_to_string(&path).unwrap();
        let doc: toml::Value = toml::from_str(&text).unwrap();
        let consumer = format!("consumer-{}", packages.len());
        let name = doc
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(toml::Value::as_str)
            .unwrap_or("workspace");
        packages.push(json!({"id":consumer,"name":name,"manifest_path":path,"targets":[]}));
        let mut deps = Vec::new();
        for (key, package) in names(&doc) {
            let library = configured.get(&key).and_then(Value::as_str).map_or_else(
                || {
                    if registry_names {
                        match package.as_str() {
                            "xml-rs" => "xml".to_owned(),
                            "rustls-webpki" => "webpki".to_owned(),
                            _ => key.replace('-', "_"),
                        }
                    } else {
                        key.replace('-', "_")
                    }
                },
                str::to_owned,
            );
            let id = format!("resolved-{}", packages.len());
            let directory = resolved.path().join(&id);
            fs::create_dir_all(directory.join("src")).unwrap();
            let manifest = directory.join("Cargo.toml");
            fs::write(&manifest, format!("[package]\nname = {package:?}\nversion = \"1.0.0\"\n[lib]\nname = {library:?}\n")).unwrap();
            fs::write(directory.join("src/lib.rs"), "").unwrap();
            packages.push(json!({"id":id,"name":package,"manifest_path":manifest,"targets":[{"name":library,"kind":["lib"]}]}));
            nodes.push(json!({"id":id,"deps":[]}));
            deps.push(json!({"name":key.replace('-', "_"),"pkg":id}));
        }
        nodes.push(json!({"id":consumer,"deps":deps}));
    }
    let graph = json!({"version":1,"packages":packages,"resolve":{"nodes":nodes}});
    (
        MacheteMetadata::from_cargo_json(&graph.to_string()).unwrap(),
        resolved,
    )
}
fn metadata_for_probe(base: &Path, case: &Value) -> (MacheteMetadata, tempfile::TempDir) {
    metadata(
        base,
        case["metadata"].as_bool().unwrap(),
        &case["configured_names"],
    )
}
fn normalize_rows(
    report: &clippier::machete::MacheteReport,
    base: &Path,
    manifest: Option<&Path>,
) -> Vec<Value> {
    let mut rows = report.dependencies.iter()
        .filter(|d| manifest.is_none_or(|m| d.manifest_path == m))
        .map(|d| json!({
            "manifest": d.manifest_path.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/"),
            "package": d.package, "dependency": d.dependency,
            "dependency_package": d.dependency_package, "kind": d.kind,
            "target": d.target, "workspace": d.workspace, "status": d.status,
        })).collect::<Vec<_>>();
    rows.sort_by_key(Value::to_string);
    rows
}
fn assert_used_ignores(case: &Value) {
    // Independently recover the used subset of ignored declarations; an ignored
    // status alone does not establish the original used-ignore assertion.
    let ignored_keys = case["used_ignore_scope"]
        .as_array()
        .unwrap()
        .iter()
        .map(|key| key.as_str().unwrap().to_owned())
        .collect::<BTreeSet<_>>();
    if !ignored_keys.is_empty() {
        let probe = tempfile::tempdir().unwrap();
        copy(case["input"].as_str().unwrap(), probe.path());
        for relative in files(probe.path())
            .keys()
            .filter(|p| p.file_name().is_some_and(|n| n == "clippier.toml"))
        {
            let path = probe.path().join(relative);
            let mut doc: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
            doc.get_mut("machete")
                .unwrap()
                .as_table_mut()
                .unwrap()
                .remove("ignored");
            fs::write(path, toml::to_string(&doc).unwrap()).unwrap();
        }
        let probe_base = fs::canonicalize(probe.path()).unwrap();
        let (probe_metadata, _probe_packages) = metadata_for_probe(&probe_base, case);
        let probe_options = MacheteOptions {
            manifest_path: Some(probe_base.join(case["manifest"].as_str().unwrap())),
            kinds: vec![
                DependencyKind::Normal,
                DependencyKind::Dev,
                DependencyKind::Build,
            ],
            metadata: Some(probe_metadata),
            ..Default::default()
        };
        let probe_before = files(&probe_base);
        let probe_report = analyze_machete(&probe_options).unwrap();
        assert_eq!(files(&probe_base), probe_before);
        let actual = probe_report
            .dependencies
            .iter()
            .filter(|d| {
                d.manifest_path == probe_base.join(case["manifest"].as_str().unwrap())
                    && ignored_keys.contains(&d.dependency)
                    && d.status == clippier::machete::DependencyStatus::Used
            })
            .map(|d| d.dependency.clone())
            .collect::<BTreeSet<_>>();
        let expected = case["ignored_used"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            actual, expected,
            "used ignored declarations: {}",
            case["id"]
        );
    }
}
fn apply_native_expectations(case: &Value, expected: &mut [Value]) {
    let native: Value =
        serde_json::from_slice(&fs::read(root().join("native_expectations.json")).unwrap())
            .unwrap();
    if let Some(adjustment) = native["cases"].get(case["id"].as_str().unwrap()) {
        assert_ne!(adjustment["reason"].as_str().unwrap().len(), 0);
        for change in adjustment["changes"].as_array().unwrap() {
            let row = expected
                .iter_mut()
                .find(|row| {
                    row["manifest"] == change["manifest"]
                        && row["dependency"] == change["dependency"]
                        && row["kind"] == change["kind"]
                        && row["target"] == change["target"]
                })
                .unwrap();
            assert_eq!(row["status"], change["from"]);
            row["status"] = change["to"].clone();
        }
    }
}

fn run_case(case: &Value) {
    let temp = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(temp.path()).unwrap();
    copy(case["input"].as_str().unwrap(), &base);
    let manifest = base.join(case["manifest"].as_str().unwrap());
    let (metadata, _resolved_packages) = metadata(
        &base,
        case["metadata"].as_bool().unwrap(),
        &case["configured_names"],
    );
    let input = if case["relative_input"].as_bool() == Some(true) {
        // Exercise a genuinely relative path without racing process-wide cwd.
        let mut relative = PathBuf::new();
        for part in std::env::current_dir().unwrap().components() {
            if matches!(part, std::path::Component::Normal(_)) {
                relative.push("..");
            }
        }
        relative
            .join(base.strip_prefix("/").unwrap())
            .join(case["manifest"].as_str().unwrap())
    } else {
        manifest.clone()
    };
    let options = MacheteOptions {
        manifest_path: Some(input),
        kinds: vec![
            DependencyKind::Normal,
            DependencyKind::Dev,
            DependencyKind::Build,
        ],
        metadata: (case["metadata"].as_bool().unwrap()
            || !case["configured_names"].as_object().unwrap().is_empty())
        .then_some(metadata),
        ..Default::default()
    };
    let before = files(&base);
    let report = analyze_machete(&options).unwrap();
    assert_eq!(
        files(&base),
        before,
        "analysis mutated input: {}",
        case["id"]
    );
    let plan = plan_machete(&options).unwrap();
    assert_eq!(
        files(&base),
        before,
        "planning mutated input: {}",
        case["id"]
    );
    assert_used_ignores(case);
    let mut expected = case["expected"].as_array().unwrap().clone();
    apply_native_expectations(case, &mut expected);
    expected.sort_by_key(Value::to_string);
    let actual = normalize_rows(&report, &base, Some(&manifest));
    assert_eq!(
        actual, expected,
        "source assertions {:?}; diagnostics {:?}",
        case["source_ids"], report.diagnostics
    );
    assert_eq!(
        normalize_rows(&plan.report, &base, Some(&manifest)),
        expected,
        "plan findings diverged: {}",
        case["id"]
    );
    if case["discovery"].as_bool() == Some(true) {
        assert_eq!(
            normalize_rows(&report, &base, None),
            expected,
            "nested manifest exclusion: {}",
            case["id"]
        );
    }
    if let Some(nested) = case["nested_manifest"].as_str() {
        let rows = normalize_rows(&report, &base, Some(&base.join(nested)));
        if case["nested_excluded"] == true {
            assert!(rows.is_empty(), "{}", case["id"]);
        } else {
            assert_eq!(
                rows,
                vec![
                    json!({"manifest":nested,"package":"nested","dependency":"marker","dependency_package":"marker","kind":"NORMAL","target":null,"workspace":false,"status":"USED"})
                ],
                "{}",
                case["id"]
            );
        }
    }
}
fn run_group(predicate: impl Fn(&str) -> bool) {
    let data = inventory();
    let cases = data["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| predicate(case["input"].as_str().unwrap()));
    let mut failures = Vec::new();
    for case in cases {
        if std::panic::catch_unwind(|| run_case(case)).is_err() {
            failures.push(case["source_ids"].clone());
        }
    }
    assert!(
        failures.is_empty(),
        "unmet source assertions: {failures:#?}"
    );
}
#[test]
fn fixture_dependency_findings_are_exact() {
    run_group(|path| !path.contains("/lexical-") && !path.contains("/parent-pattern-"));
}
#[test]
fn lexical_roots_and_negative_boundaries() {
    run_group(|path| path.contains("/lexical-"));
}
#[test]
fn directory_patterns_match_ancestors_not_basename() {
    run_group(|path| path.contains("/parent-pattern-"));
}
#[test]
fn target_directory_is_always_pruned() {
    for no_ignore in [None, Some(false), Some(true)] {
        let temp = tempfile::tempdir().unwrap();
        copy(
            inventory()["special_inputs"]["target"].as_str().unwrap(),
            temp.path(),
        );
        fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname='probe'\nversion='0.1.0'\n[workspace]\nmembers=[]\n",
        )
        .unwrap();
        fs::create_dir_all(temp.path().join("src")).unwrap();
        fs::write(temp.path().join("src/lib.rs"), "").unwrap();
        let before = files(temp.path());
        let options = MacheteOptions {
            manifest_path: Some(temp.path().to_owned()),
            no_ignore,
            ..Default::default()
        };
        let report = analyze_machete(&options).unwrap();
        assert!(report.dependencies.is_empty(), "{no_ignore:?}");
        let plan = plan_machete(&options).unwrap();
        assert!(plan.changes.is_empty());
        assert_eq!(files(temp.path()), before);
    }
}

#[test]
fn corpus_payloads_trees_and_executable_cases_are_unique() {
    let data = inventory();
    let mut payloads = BTreeMap::new();
    payloads.insert(
        fs::read(root().join("LICENSE")).unwrap(),
        PathBuf::from("LICENSE"),
    );
    for (relative, bytes) in files(&root().join("files")) {
        assert!(
            payloads.insert(bytes, relative.clone()).is_none(),
            "duplicate payload: {relative:?}"
        );
    }
    let mut trees = BTreeSet::new();
    for tree in data["trees"].as_object().unwrap().values() {
        assert!(trees.insert(tree.to_string()), "duplicate input tree");
        for stored in tree.as_object().unwrap().values() {
            assert!(root().join(stored.as_str().unwrap()).is_file());
        }
    }
    let mut cases = BTreeSet::new();
    let mut origins = BTreeSet::new();
    for case in data["cases"].as_array().unwrap() {
        let mut behavior = case.as_object().unwrap().clone();
        behavior.remove("id");
        behavior.remove("source_ids");
        assert!(
            cases.insert(serde_json::to_string(&behavior).unwrap()),
            "duplicate executable case"
        );
        assert!(data["trees"].get(case["input"].as_str().unwrap()).is_some());
        origins.insert(case["id"].as_str().unwrap());
    }
    assert_eq!(data["case_origins"].as_object().unwrap().len(), 187);
    for canonical in data["case_origins"].as_object().unwrap().values() {
        assert!(origins.contains(canonical.as_str().unwrap()));
    }
    for test in data["tests"].as_array().unwrap() {
        for assertion in test["assertions"].as_array().unwrap() {
            for id in assertion["case_ids"].as_array().unwrap() {
                if let Some(case) = data["cases"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|case| case["id"] == *id)
                {
                    assert!(
                        case["source_ids"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|source| source == &assertion["id"] || source == &test["id"])
                    );
                }
            }
        }
    }
}

#[test]
fn corpus_files_have_exact_digests_and_inventory() {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    let data = inventory();
    let hashes: BTreeMap<String, String> =
        serde_json::from_slice(&fs::read(root().join("sha256.json")).unwrap()).unwrap();
    let actual = files(&root());
    assert_eq!(actual.len(), hashes.len() + 1);
    for (relative, bytes) in &actual {
        let relative = relative.to_str().unwrap().replace('\\', "/");
        if relative == "sha256.json" {
            continue;
        }
        let digest = Sha256::digest(bytes)
            .iter()
            .fold(String::new(), |mut s, b| {
                write!(s, "{b:02x}").unwrap();
                s
            });
        assert_eq!(hashes.get(&relative), Some(&digest), "{relative}");
    }
    assert_eq!(data["fixture_union"].as_array().unwrap().len(), 21);
    let tests = data["tests"].as_array().unwrap();
    assert_eq!(tests.len(), 43);
    let harness = format!(
        "{}\n{}",
        include_str!("machete_analysis.rs"),
        include_str!("machete_removal.rs")
    );
    let case_ids = data["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    for (source, count) in [("upstream", 18), ("fork", 25)] {
        assert_eq!(
            tests
                .iter()
                .filter(|t| t["id"].as_str().unwrap().starts_with(source))
                .count(),
            count
        );
    }
    for test in tests {
        assert!(
            !test["assertions"].as_array().unwrap().is_empty(),
            "{}",
            test["id"]
        );
        for assertion in test["assertions"].as_array().unwrap() {
            for name in assertion["executable_tests"].as_array().unwrap() {
                assert!(
                    harness.contains(&format!("fn {}(", name.as_str().unwrap())),
                    "{}",
                    assertion["id"]
                );
            }
            assert_ne!(assertion["case_ids"].as_array().unwrap().len(), 0);
            for id in assertion["case_ids"].as_array().unwrap() {
                let id = id.as_str().unwrap();
                assert!(
                    case_ids.contains(id) || id.contains(":target:") || id.ends_with(":removal"),
                    "{id}"
                );
            }
        }
    }
    for source in data["sources"].as_array().unwrap() {
        for file in source["files"].as_array().unwrap() {
            let key = file["stored"].as_str().unwrap();
            assert_eq!(hashes.get(key).unwrap(), file["sha256"].as_str().unwrap());
        }
    }
}
