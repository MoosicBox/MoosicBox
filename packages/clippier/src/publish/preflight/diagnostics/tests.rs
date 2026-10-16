// Keep the shared Cargo-home fixture lock through subprocess completion and cleanup.
#![allow(clippy::significant_drop_tightening)]

use super::super::tests::{Fixture, Registry};
use super::*;

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
