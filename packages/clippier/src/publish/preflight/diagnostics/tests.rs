// Keep the shared Cargo-home fixture lock through subprocess completion and cleanup.
#![allow(clippy::significant_drop_tightening)]

use super::super::tests::{Fixture, Registry};
use super::*;

#[test]
fn concurrent_pending_probes_have_independent_staging_and_cleanup() {
    let fixture = Fixture::new(&[("clippier-fixture-core", "")]);
    let root = fixture.directory.path();
    fs::write(root.join("README.md"), "workspace readme").unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\n[workspace.package]\nreadme = 'README.md'\nedition = '2021'\n",
    )
    .unwrap();
    let path = fixture.packages["clippier-fixture-core"]
        .package_path
        .as_ref()
        .unwrap()
        .join("Cargo.toml");
    let original = fs::read_to_string(&path).unwrap().replace(
        "edition = \"2021\"",
        "edition = { workspace = true }\nreadme = { workspace = true }",
    );
    fs::write(&path, &original).unwrap();
    let manifest: toml::Value = toml::from_str("[package]\nname = 'app'\nversion = '0.1.0'\n[dependencies]\nclippier-fixture-core = '0.1.0'\n").unwrap();
    let dependency = &dependencies(&manifest)[0];
    let paths = std::sync::Mutex::new(Vec::new());
    let results = parallel_ordered(&(0..16).collect::<Vec<_>>(), 4, |_| {
        run_probe_with(
            root,
            &fixture.packages,
            &["clippier-fixture-core".into()],
            dependency,
            probe_manifest(&manifest, dependency),
            &PublishConfig::default(),
            |command| {
                paths
                    .lock()
                    .unwrap()
                    .push(command.get_current_dir().unwrap().to_path_buf());
                command.arg("--offline");
            },
        )
        .map_err(|error| error.to_string())
    })
    .unwrap();
    for result in results {
        result.unwrap();
    }
    let paths = paths.into_inner().unwrap();
    assert_eq!(paths.iter().collect::<BTreeSet<_>>().len(), 16);
    assert!(paths.iter().all(|path| !path.exists()));
    assert_eq!(fs::read_to_string(path).unwrap(), original);
}

#[test]
fn index_checks_require_all_matching_versions_to_be_yanked() {
    let versions = vec![
        IndexVersion {
            vers: "6.0.0".into(),
            yanked: true,
        },
        IndexVersion {
            vers: "6.0.1".into(),
            yanked: false,
        },
        IndexVersion {
            vers: "9.0.0".into(),
            yanked: false,
        },
    ];
    assert!(
        audit_requirement("=6.0.0", &versions)
            .unwrap()
            .unwrap()
            .contains("yanked")
    );
    assert!(audit_requirement("^6", &versions).unwrap().is_none());
    assert!(
        audit_requirement("^3", &versions)
            .unwrap()
            .unwrap()
            .contains("no published version")
    );
    assert!(audit_requirement("^1", &[]).unwrap().is_some());
    assert!(audit_requirement("invalid", &versions).is_err());
    for (name, path) in [
        ("a", "1/a"),
        ("ab", "2/ab"),
        ("ABC", "3/a/abc"),
        ("serde", "se/rd/serde"),
    ] {
        assert_eq!(index_path(name).unwrap(), path);
    }
    assert!(index_path("../secret").is_err());
}

#[test]
fn projection_preserves_alias_features_and_dependency_kinds() {
    let manifest: toml::Value = toml::from_str(r#"
[dependencies]
renamed = { package = "actual", version = "1.0.0", optional = true, default-features = false, features = ["explicit"] }
other = "2"
[target.'cfg(windows)'.build-dependencies]
platform = { version = "3", registry = "private" }
[features]
default = ["local", "other"]
local = ["dep:renamed", "renamed?/weak", "renamed/strong", "other/hidden"]
"#).unwrap();
    let deps = dependencies(&manifest);
    assert_eq!(deps.len(), 3);
    let renamed = deps.iter().find(|dep| dep.alias == "renamed").unwrap();
    let probe = probe_manifest(&manifest, renamed);
    assert_eq!(
        probe["dependencies"]["renamed"],
        manifest["dependencies"]["renamed"]
    );
    assert!(probe["dependencies"].get("other").is_none());
    assert_eq!(
        probe["features"]["default"].as_array().unwrap(),
        &[toml::Value::String("local".into())]
    );
    assert_eq!(probe["features"]["local"].as_array().unwrap().len(), 3);
    let platform = deps.iter().find(|dep| dep.alias == "platform").unwrap();
    assert!(!platform.crates_io());
    assert_eq!(
        probe_manifest(&manifest, platform)["target"]["cfg(windows)"]["build-dependencies"]["platform"],
        platform.specification
    );
}

#[test]
fn probes_expose_two_transitive_errors_hidden_in_one_package() {
    let fixture = Fixture::new(&[(
        "clippier-fixture-app",
        "[dependencies]\nfirst = { package = \"clippier-fixture-first\", version = \"1\", optional = true }\nsecond = { package = \"clippier-fixture-second\", version = \"1\" }\n",
    )]);
    let registry = Registry::new();
    registry.add("clippier-fixture-zipold", "6.0.0", None, true);
    registry.add("clippier-fixture-zipother", "3.0.0", None, true);
    registry.add(
        "clippier-fixture-first",
        "1.0.0",
        Some(("clippier-fixture-zipold", "6")),
        false,
    );
    registry.add(
        "clippier-fixture-second",
        "1.0.0",
        Some(("clippier-fixture-zipother", "3")),
        false,
    );
    let manifest = read_toml(
        &fixture.packages["clippier-fixture-app"]
            .package_path
            .as_ref()
            .unwrap()
            .join("Cargo.toml"),
    )
    .unwrap();
    let staged = super::super::prepare_workspace(
        fixture.directory.path(),
        &fixture.packages,
        &["clippier-fixture-app".into()],
    )
    .unwrap();
    let mut command = super::super::package_command(&staged, &PublishConfig::default());
    registry.configure(&mut command);
    let full = command.output().unwrap();
    assert!(!full.status.success());
    let full_error = String::from_utf8_lossy(&full.stderr);
    assert_eq!(full_error.matches("is yanked").count(), 1, "{full_error}");
    let mut failures = Vec::new();
    for dependency in dependencies(&manifest) {
        let error = run_probe_with(
            fixture.directory.path(),
            &fixture.packages,
            &[],
            &dependency,
            probe_manifest(&manifest, &dependency),
            &PublishConfig::default(),
            |command| registry.configure(command),
        )
        .unwrap_err()
        .to_string();
        failures.push(error);
    }
    assert_eq!(failures.len(), 2);
    assert!(failures[0].contains("6.0.0 is yanked"), "{}", failures[0]);
    assert!(failures[1].contains("3.0.0 is yanked"), "{}", failures[1]);
    assert!(!fixture.directory.path().join("Cargo.lock").exists());
}

#[test]
fn probes_preserve_requested_features_and_expose_missing_versions() {
    let fixture = Fixture::new(&[(
        "clippier-fixture-app",
        "[dependencies]\nfeatureful = { package = \"clippier-fixture-featureful\", version = \"1\", optional = true }\nmissing = { package = \"clippier-fixture-missing\", version = \"99\" }\n[features]\ndefault = [\"enable\"]\nenable = [\"featureful/not-present\"]\n",
    )]);
    let registry = Registry::new();
    registry.add("clippier-fixture-featureful", "1.0.0", None, false);
    registry.add("clippier-fixture-missing", "1.0.0", None, false);
    let manifest = read_toml(
        &fixture.packages["clippier-fixture-app"]
            .package_path
            .as_ref()
            .unwrap()
            .join("Cargo.toml"),
    )
    .unwrap();
    for dependency in dependencies(&manifest) {
        let error = run_probe_with(
            fixture.directory.path(),
            &fixture.packages,
            &[],
            &dependency,
            probe_manifest(&manifest, &dependency),
            &PublishConfig::default(),
            |command| registry.configure(command),
        )
        .unwrap_err()
        .to_string();
        let expected = if dependency.alias == "featureful" {
            "not-present"
        } else {
            "99"
        };
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn cached_probes_and_index_errors_are_emitted_once() {
    let fixture = Fixture::new(&[
        ("first", "[dependencies]\nexternal = \"1\"\n"),
        ("second", "[dependencies]\nexternal = \"1\"\n"),
    ]);
    let mut diagnostics = Diagnostics::default();
    let manifest = read_toml(
        &fixture.packages["first"]
            .package_path
            .as_ref()
            .unwrap()
            .join("Cargo.toml"),
    )
    .unwrap();
    let dependency = &dependencies(&manifest)[0];
    let key = format!(
        "{}\n{:?}",
        toml::to_string(&probe_manifest(&manifest, dependency)).unwrap(),
        BTreeSet::<String>::new()
    );
    diagnostics
        .probes
        .insert(key, Err("cached probe failure".into()));
    diagnostics
        .indexes
        .insert("external".into(), Err("registry unavailable".into()));
    let findings = diagnostics.investigate(
        fixture.directory.path(),
        &fixture.packages,
        &[],
        &["first".into(), "second".into()],
        &PublishConfig::default(),
    );
    assert_eq!(findings.len(), 2);
    assert!(findings[0].contains("Additional dependency-probe failure"));
    assert!(findings[1].contains("Unresolved registry-index check"));
    assert_eq!(diagnostics.probes.len(), 1);
    assert_eq!(diagnostics.indexes.len(), 1);
}

#[test]
fn unpublished_pending_dependencies_resolve_without_registry_publication() {
    let fixture = Fixture::new(&[
        ("clippier-fixture-core", ""),
        (
            "clippier-fixture-app",
            "[dependencies]\nrenamed = { package = \"clippier-fixture-core\", version = \"0.1\" }\n",
        ),
    ]);
    let manifest = read_toml(
        &fixture.packages["clippier-fixture-app"]
            .package_path
            .as_ref()
            .unwrap()
            .join("Cargo.toml"),
    )
    .unwrap();
    let dependency = &dependencies(&manifest)[0];
    run_probe_with(
        fixture.directory.path(),
        &fixture.packages,
        &["clippier-fixture-core".into()],
        dependency,
        probe_manifest(&manifest, dependency),
        &PublishConfig::default(),
        |command| {
            command.arg("--offline");
        },
    )
    .unwrap();
}

#[test]
fn workers_overlap_but_preserve_input_order_and_bound_concurrency() {
    use std::sync::{
        Barrier,
        atomic::{AtomicUsize, Ordering},
    };
    let barrier = Barrier::new(4);
    let active = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let visits = (0..12).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>();
    let results = parallel_ordered(&(0..12).collect::<Vec<_>>(), 4, |&index| {
        visits[index].fetch_add(1, Ordering::SeqCst);
        let count = active.fetch_add(1, Ordering::SeqCst) + 1;
        peak.fetch_max(count, Ordering::SeqCst);
        if index < 4 {
            barrier.wait();
        }
        std::thread::sleep(Duration::from_millis(if index == 0 { 20 } else { 1 }));
        active.fetch_sub(1, Ordering::SeqCst);
        index
    })
    .unwrap();
    assert_eq!(results, (0..12).collect::<Vec<_>>());
    assert_eq!(peak.load(Ordering::SeqCst), 4);
    assert!(visits.iter().all(|count| count.load(Ordering::SeqCst) == 1));
    assert_eq!(
        parallel_ordered::<u8, u8>(&[], 4, |&n| n).unwrap(),
        [] as [u8; 0]
    );
    assert_eq!(parallel_ordered(&[1, 2], 1, |&n| n).unwrap(), [1, 2]);
}

#[test]
fn worker_panic_is_an_unresolved_error_not_a_partial_success() {
    let result = parallel_ordered(&[0, 1, 2], 2, |&n| {
        assert_ne!(n, 1, "injected panic");
        n
    });
    assert!(result.unwrap_err().to_string().contains("worker panicked"));
}

#[test]
fn repeated_failures_are_emitted_once_in_input_order() {
    let fixture = Fixture::new(&[(
        "app",
        "[dependencies]\nmissing = { version = \"1\", registry = \"unconfigured-fixture\" }\n",
    )]);
    let manifest = read_toml(
        &fixture.packages["app"]
            .package_path
            .as_ref()
            .unwrap()
            .join("Cargo.toml"),
    )
    .unwrap();
    let mut diagnostics = Diagnostics::default();
    let entries = dependencies(&manifest);
    let dep = entries[0].clone();
    let key = format!(
        "{}\n{:?}",
        toml::to_string(&probe_manifest(&manifest, &dep)).unwrap(),
        BTreeSet::<String>::new()
    );
    diagnostics
        .probes
        .insert(key.clone(), Err("cached failure".into()));
    let findings = diagnostics.collect_findings(
        "app",
        vec![(key.clone(), dep.clone(), false), (key, dep, false)],
    );
    assert_eq!(findings.len(), 1);
}

#[test]
#[ignore = "manual Cargo contention benchmark"]
fn benchmark_serial_and_parallel_cargo_probes() {
    let fixture = Fixture::new(&[(
        "app",
        "[dependencies]\none = { package = \"clippier-fixture-one\", version = \"1\" }\ntwo = { package = \"clippier-fixture-two\", version = \"1\" }\nthree = { package = \"clippier-fixture-three\", version = \"1\" }\nfour = { package = \"clippier-fixture-four\", version = \"1\" }\n",
    )]);
    let registry = Registry::new();
    for name in ["one", "two", "three", "four"] {
        registry.add(
            &format!("clippier-fixture-{name}"),
            "1.0.0",
            Some(("clippier-fixture-yanked", "6")),
            false,
        );
    }
    registry.add("clippier-fixture-yanked", "6.0.0", None, true);
    let manifest = read_toml(
        &fixture.packages["app"]
            .package_path
            .as_ref()
            .unwrap()
            .join("Cargo.toml"),
    )
    .unwrap();
    let dependencies = dependencies(&manifest);
    for workers in [1, 4, 4, 1] {
        let started = Instant::now();
        let results = parallel_ordered(&dependencies, workers, |dependency| {
            run_probe_with(
                fixture.directory.path(),
                &fixture.packages,
                &[],
                dependency,
                probe_manifest(&manifest, dependency),
                &PublishConfig::default(),
                |command| registry.configure(command),
            )
            .map_err(|error| error.to_string())
        })
        .unwrap();
        for result in results {
            assert!(result.unwrap_err().contains("6.0.0 is yanked"));
        }
        eprintln!(
            "Cargo probe benchmark: {workers} workers: {:?}",
            started.elapsed()
        );
    }
}

#[test]
fn published_dependency_sources_preserve_registry_features_and_renames() {
    let mut manifest: toml::Value = toml::from_str(r#"
        [dependencies]
        renamed = { package = "external", version = "^0.4.0", git = "https://invalid.example/repo", branch = "master", features = ["required"], optional = true, default-features = false }
        local = { version = "1.0.0", path = "../local" }
        alternate = { version = "2.0.0", registry = "company" }
        missing = { git = "https://invalid.example/repo" }
        [target.'cfg(windows)'.build-dependencies]
        tool = { version = "3.0.0", git = "https://invalid.example/repo", rev = "deadbeef" }
    "#).unwrap();
    registry_manifest(&mut manifest);
    let renamed = &manifest["dependencies"]["renamed"];
    assert!(renamed.get("git").is_none());
    assert!(renamed.get("branch").is_none());
    assert_eq!(renamed["package"].as_str(), Some("external"));
    assert_eq!(renamed["version"].as_str(), Some("^0.4.0"));
    assert_eq!(renamed["features"][0].as_str(), Some("required"));
    assert_eq!(renamed["optional"].as_bool(), Some(true));
    assert_eq!(renamed["default-features"].as_bool(), Some(false));
    assert!(manifest["dependencies"]["local"].get("path").is_none());
    assert_eq!(
        manifest["dependencies"]["alternate"]["registry"].as_str(),
        Some("company")
    );
    assert!(manifest["dependencies"]["missing"].get("git").is_some());
    assert!(
        manifest["target"]["cfg(windows)"]["build-dependencies"]["tool"]
            .get("rev")
            .is_none()
    );
}

#[test]
fn repeated_supplemental_errors_are_grouped_without_losing_dependency_labels() {
    let manifest: toml::Value =
        toml::from_str("[dependencies]\na = '1.0.0'\nb = '1.0.0'\n").unwrap();
    let mut diagnostics = Diagnostics::default();
    for key in ["a", "b"] {
        diagnostics.probes.insert(key.into(), Err(format!("Updating crates.io index ({key})\nerror: shared registry unavailable\nCaused by:\n  network failure")));
    }
    // Avoid real index reads; these results are already conclusive for this test.
    let entries = dependencies(&manifest)
        .into_iter()
        .map(|dep| (dep.alias.clone(), dep, true))
        .collect();
    let findings = diagnostics.collect_findings("fixture", entries).join("\n");
    assert_eq!(
        findings
            .matches("error: shared registry unavailable")
            .count(),
        1
    );
    assert!(findings.contains("fixture -> a [dependencies]"));
    assert!(findings.contains("fixture -> b [dependencies]"));
    assert!(findings.contains("Same diagnostic as dependency-probe group #1"));
    assert_eq!(diagnostics.error_groups.len(), 1);
}
