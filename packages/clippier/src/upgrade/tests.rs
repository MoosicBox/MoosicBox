//! Pure fixtures for native planner behavior; no registry/network or Cargo calls.
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

fn release(version: &str) -> registry::Release {
    registry::Release {
        version: version.parse().unwrap(),
        yanked: false,
        rust_version: None,
        features: BTreeSet::new(),
    }
}
fn fixture(
    text: &str,
    options: impl FnOnce(&mut UpgradeOptions),
) -> (tempfile::TempDir, UpgradeInput) {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("Cargo.toml"), text).unwrap();
    let mut opts = UpgradeOptions {
        manifest_path: Some(directory.path().into()),
        lockfile: Some(LockfilePolicy::None),
        ..UpgradeOptions::default()
    };
    options(&mut opts);
    let input = load_upgrade_input(&opts).unwrap();
    (directory, input)
}
fn plan(input: &UpgradeInput, versions: &[&str]) -> UpgradePlan {
    let releases = BTreeMap::from([(
        RegistryKey {
            name: "foo".into(),
            registry: None,
        },
        versions.iter().map(|v| release(v)).collect(),
    )]);
    plan_upgrade_with_releases(input, &releases).unwrap()
}
const PACKAGE: &str = "[package]\nname='fixture'\nversion='0.1.0'\n";

#[test]
fn locked_baseline_rejects_msrv_induced_downgrade() {
    let (dir, _) = fixture(&format!("{PACKAGE}[dependencies]\nfoo='1.0.0'\n"), |_| {});
    fs::write(dir.path().join("Cargo.lock"),"version=4\n[[package]]\nname='foo'\nversion='1.9.0'\nsource='registry+https://github.com/rust-lang/crates.io-index'\n").unwrap();
    let input = load_upgrade_input(&UpgradeOptions {
        manifest_path: Some(dir.path().into()),
        lockfile_only: true,
        msrv: Some("1.80".into()),
        ..UpgradeOptions::default()
    })
    .unwrap();
    let mut too_new = release("1.9.0");
    too_new.rust_version = Some("1.90".into());
    let releases = BTreeMap::from([(
        RegistryKey {
            name: "foo".into(),
            registry: None,
        },
        vec![release("1.5.0"), too_new],
    )]);
    assert!(
        plan_upgrade_with_releases(&input, &releases)
            .unwrap_err()
            .to_string()
            .contains("downgrade")
    );
}

#[test]
fn multiple_alias_lines_are_requirement_qualified() {
    let (_dir, input) = fixture(
        &format!(
            "{PACKAGE}[dependencies]\nold={{package='foo',version='0.2.0'}}\nnew={{package='foo',version='1.0.0'}}\ncustom={{package='foo',version='1.0.0',registry='custom'}}\n"
        ),
        |_| {},
    );
    let data = BTreeMap::from([
        (
            RegistryKey {
                name: "foo".into(),
                registry: None,
            },
            vec![release("0.2.8"), release("1.8.0")],
        ),
        (
            RegistryKey {
                name: "foo".into(),
                registry: Some("custom".into()),
            },
            vec![release("1.3.0")],
        ),
    ]);
    let planned = plan_upgrade_with_releases(&input, &data).unwrap();
    assert_eq!(planned.lock_versions.len(), 3);
    assert!(
        planned
            .lock_versions
            .iter()
            .any(|t| t.previous == "0.2.0" && t.selected == "0.2.8")
    );
    assert!(
        planned
            .lock_versions
            .iter()
            .any(|t| t.key.registry.as_deref() == Some("custom") && t.selected == "1.3.0")
    );
}

#[switchy_async::test]
async fn stale_config_is_rejected_without_writes() {
    let (dir, input) = fixture(&format!("{PACKAGE}[dependencies]\nfoo='1.0.0'\n"), |_| {});
    let planned = plan(&input, &["1.5.0"]);
    fs::write(
        dir.path().join("clippier.toml"),
        "[upgrade]\npolicy='incompatible'\n",
    )
    .unwrap();
    assert!(
        apply_upgrade_plan(&planned)
            .await
            .unwrap_err()
            .to_string()
            .contains("stale")
    );
    assert!(
        fs::read_to_string(dir.path().join("Cargo.toml"))
            .unwrap()
            .contains("1.0.0")
    );
}

#[switchy_async::test]
async fn native_apply_and_second_apply_stale_detection() {
    let (dir, input) = fixture(&format!("{PACKAGE}[dependencies]\nfoo='1.0.0'\n"), |_| {});
    let planned = plan(&input, &["1.5.0"]);
    let report = apply_upgrade_plan(&planned).await.unwrap();
    assert_eq!(report.status, UpgradeStatus::Applied);
    assert!(!report.pending);
    assert!(
        fs::read_to_string(dir.path().join("Cargo.toml"))
            .unwrap()
            .contains("1.5.0")
    );
    assert!(apply_upgrade_plan(&planned).await.is_err());
}

#[switchy_async::test]
async fn cargo_local_directory_source_lock_only_refresh_and_git_preservation() {
    // A real Cargo resolver fixture using vendored directory source metadata.
    // No HTTP requests, provider use or toolchain compilation is needed.
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".cargo")).unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/lib.rs"), "").unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        format!("{PACKAGE}[dependencies]\nfoo='1.0.0'\n"),
    )
    .unwrap();
    fs::write(dir.path().join(".cargo/config.toml"),"[alias]\nfixture-check='check --workspace'\n[build]\ntarget-dir='target'\n[env]\nRUST_LOG='info'\n[source.crates-io]\nreplace-with='vendored'\n[source.vendored]\ndirectory='vendor'\n").unwrap();
    for (name, version, dependency) in [
        ("foo", "1.0.0", true),
        ("foo", "1.2.0", true),
        ("bar", "1.0.0", false),
        ("bar", "1.3.0", false),
    ] {
        let path = dir.path().join("vendor").join(format!("{name}-{version}"));
        fs::create_dir_all(path.join("src")).unwrap();
        fs::write(path.join("src/lib.rs"), "").unwrap();
        fs::write(
            path.join("Cargo.toml"),
            format!(
                "[package]\nname='{name}'\nversion='{version}'\n{}",
                if dependency {
                    "[dependencies]\nbar='1'\n"
                } else {
                    ""
                }
            ),
        )
        .unwrap();
        fs::write(
            path.join(".cargo-checksum.json"),
            "{\"files\":{},\"package\":null}",
        )
        .unwrap();
    }
    let original = "version = 4\n\n[[package]]\nname = \"fixture\"\nversion = \"0.1.0\"\ndependencies = [\"foo\"]\n\n[[package]]\nname = \"foo\"\nversion = \"1.0.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\ndependencies = [\"bar\"]\n\n[[package]]\nname = \"bar\"\nversion = \"1.0.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n";
    fs::write(dir.path().join("Cargo.lock"), original).unwrap();
    let resolve_input = load_upgrade_input(&UpgradeOptions {
        manifest_path: Some(dir.path().into()),
        lockfile_only: true,
        lockfile: Some(LockfilePolicy::Resolve),
        registry_overrides: RegistryOverrides {
            offline: Some(true),
            ..RegistryOverrides::default()
        },
        ..UpgradeOptions::default()
    })
    .unwrap();
    let resolve_plan = plan(&resolve_input, &["1.0.0"]);
    let resolved = super::apply::resolve_lockfile(&resolve_plan).unwrap();
    assert!(
        !resolved.contains("1.3.0"),
        "resolve must not opportunistically refresh transitives"
    );
    assert!(
        fs::read_to_string(dir.path().join("Cargo.lock")).unwrap() == original,
        "preview must not write live lockfile"
    );
    let input = load_upgrade_input(&UpgradeOptions {
        manifest_path: Some(dir.path().into()),
        lockfile_only: true,
        registry_overrides: RegistryOverrides {
            offline: Some(true),
            ..RegistryOverrides::default()
        },
        ..UpgradeOptions::default()
    })
    .unwrap();
    let planned = plan(&input, &["1.0.0", "1.2.0"]);
    assert!(planned.changes().is_empty());
    let report = apply_upgrade_plan(&planned).await.unwrap();
    assert_eq!(report.lockfile, LockfileStatus::Applied);
    let lock = fs::read_to_string(dir.path().join("Cargo.lock")).unwrap();
    assert!(lock.contains("1.2.0"));
    assert!(lock.contains("1.3.0"));
    assert!(
        fs::read_to_string(dir.path().join("Cargo.toml"))
            .unwrap()
            .contains("1.0.0")
    );
}

#[test]
fn compatible_precision_and_operators() {
    for (old, expected) in [
        ("1.0.0", "1.9.3"),
        ("^1.0", "^1.9"),
        ("~1.0.0", "~1.0.8"),
        ("1", "1"),
        ("1.*", "1.*"),
        ("*", "*"),
        (">=1, <2", ">=1, <2"),
        ("0.2.0", "0.2.8"),
    ] {
        let (_dir, input) = fixture(&format!("{PACKAGE}[dependencies]\nfoo='{old}'\n"), |_| {});
        let planned = plan(&input, &["0.2.8", "0.3.0", "1.0.8", "1.9.3", "2.0.0"]);
        assert_eq!(
            planned.report().dependencies[0].requirement.as_deref(),
            Some(expected),
            "{old}"
        );
    }
}

#[test]
fn pins_require_explicit_opt_in() {
    let text = format!("{PACKAGE}[dependencies]\nfoo='=1.0.0'\n");
    let (_dir, input) = fixture(&text, |_| {});
    let planned = plan_upgrade_with_releases(&input, &BTreeMap::new()).unwrap();
    assert_eq!(
        planned.report().dependencies[0].status,
        UpgradeStatus::Skipped
    );
    let (_dir, input) = fixture(&text, |o| o.pinned = Some(PinnedPolicy::Upgrade));
    assert_eq!(
        plan(&input, &["1.0.0", "1.4.0", "2.0.0"])
            .report()
            .dependencies[0]
            .requirement
            .as_deref(),
        Some("=1.4.0")
    );
}

#[test]
fn prerelease_scope_and_yanked_filter() {
    let (_dir, input) = fixture(
        &format!("{PACKAGE}[dependencies]\nfoo='1.0.0-alpha.1'\n"),
        |o| o.policy = Some(UpgradePolicy::Incompatible),
    );
    let mut versions = vec![
        release("1.0.0-alpha.2"),
        release("2.0.0-alpha.1"),
        release("3.0.0"),
    ];
    versions[2].yanked = true;
    let planned = plan_upgrade_with_releases(
        &input,
        &BTreeMap::from([(
            RegistryKey {
                name: "foo".into(),
                registry: None,
            },
            versions,
        )]),
    )
    .unwrap();
    assert_eq!(
        planned.report().dependencies[0].selected.as_deref(),
        Some("1.0.0-alpha.2")
    );
}

#[test]
fn all_table_styles_alias_targets_and_comments() {
    let text = format!(
        "{PACKAGE}[dependencies]\na = {{ package='foo', version='1.0.0', optional=true }} # retained\n[target.'cfg(unix)'.build-dependencies.b]\npackage='foo'\nversion='1.0.0' # version comment\n[dev-dependencies]\nc.package='foo'\nc.version='1.0.0'\n"
    );
    let (_dir, input) = fixture(&text, |_| {});
    let planned = plan(&input, &["1.2.0"]);
    assert_eq!(planned.report().dependencies.len(), 3);
    let after = &planned.changes()[0].after;
    assert!(after.contains("# retained"));
    assert!(after.contains("# version comment"));
    assert!(after.contains("optional=true"));
    assert_eq!(
        planned
            .report()
            .dependencies
            .iter()
            .filter(|d| d.requirement.as_deref() == Some("1.2.0"))
            .count(),
        3
    );
}

#[test]
fn git_path_and_exclusions_do_not_query_registry() {
    let text = format!(
        "{PACKAGE}[dependencies]\ngit_dep={{git='https://example.invalid/repo',version='1'}}\npath_dep={{path='local',version='1'}}\nfoo='1.0.0'\n"
    );
    let (_dir, input) = fixture(&text, |o| o.exclude = vec!["foo".into()]);
    assert!(planner::registry_requests(&input).unwrap().is_empty());
    let planned = plan_upgrade_with_releases(&input, &BTreeMap::new()).unwrap();
    assert!(planned.changes().is_empty());
}

#[test]
fn msrv_features_and_scoped_cli_precedence() {
    let (_dir, mut input) = fixture(
        &format!("{PACKAGE}[dependencies]\nfoo={{version='1.0.0',features=['present']}}\n"),
        |_| {},
    );
    input.config.msrv = Some("1.80".into());
    input.config.rules.push(DependencyRule {
        dependency: "foo".into(),
        policy: Some(UpgradePolicy::Incompatible),
        ..DependencyRule::default()
    });
    let mut versions = vec![release("1.2.0"), release("2.0.0"), release("3.0.0")];
    versions[0].features.insert("present".into());
    versions[1].features.insert("present".into());
    versions[1].rust_version = Some("1.90".into());
    let releases = BTreeMap::from([(
        RegistryKey {
            name: "foo".into(),
            registry: None,
        },
        versions,
    )]);
    assert_eq!(
        plan_upgrade_with_releases(&input, &releases)
            .unwrap()
            .report()
            .dependencies[0]
            .selected
            .as_deref(),
        Some("1.2.0")
    );
    input.config.msrv = Some("1.95".into());
    assert_eq!(
        plan_upgrade_with_releases(&input, &releases)
            .unwrap()
            .report()
            .dependencies[0]
            .selected
            .as_deref(),
        Some("2.0.0")
    );
    input.overrides.policy = Some(UpgradePolicy::Compatible);
    assert_eq!(
        plan_upgrade_with_releases(&input, &releases)
            .unwrap()
            .report()
            .dependencies[0]
            .selected
            .as_deref(),
        Some("1.2.0")
    );
}

#[test]
fn inheritance_edits_owner_and_reports_all_consumers() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[workspace]\nmembers=['a','b']\n[workspace.dependencies]\nfoo='1.0.0'\n",
    )
    .unwrap();
    for name in ["a", "b"] {
        fs::create_dir(dir.path().join(name)).unwrap();
        fs::write(
            dir.path().join(name).join("Cargo.toml"),
            format!(
                "[package]\nname='{name}'\nversion='0.1.0'\n[dependencies]\nfoo.workspace=true\n"
            ),
        )
        .unwrap();
    }
    let input = load_upgrade_input(&UpgradeOptions {
        manifest_path: Some(dir.path().into()),
        packages: vec!["a".into()],
        lockfile: Some(LockfilePolicy::None),
        ..UpgradeOptions::default()
    })
    .unwrap();
    let planned = plan(&input, &["1.2.0"]);
    assert_eq!(planned.changes().len(), 1);
    assert_eq!(planned.report().dependencies.len(), 2);
    assert!(planned.changes()[0].path.ends_with("Cargo.toml"));
    assert!(
        planned
            .report()
            .dependencies
            .iter()
            .all(|d| d.status == UpgradeStatus::Planned)
    );
    assert!(
        fs::read_to_string(dir.path().join("Cargo.toml"))
            .unwrap()
            .contains("1.0.0")
    );
}

#[test]
fn config_registry_presence_and_default_lock_work() {
    let (dir, _) = fixture(&format!("{PACKAGE}[dependencies]\nfoo='1'\n"), |_| {});
    fs::write(
        dir.path().join("clippier.toml"),
        "[upgrade]\npolicy='incompatible'\n[upgrade.registry]\noffline=true\nmax-retries=0\n",
    )
    .unwrap();
    let mut options = UpgradeOptions {
        manifest_path: Some(dir.path().into()),
        ..UpgradeOptions::default()
    };
    let input = load_upgrade_input(&options).unwrap();
    assert!(input.registry.offline);
    assert_eq!(input.registry.max_retries, 0);
    let planned = plan(&input, &["1.0.0"]);
    assert!(planned.report().pending);
    assert_eq!(planned.report().lockfile, LockfileStatus::Unresolved);
    options.registry_overrides.offline = Some(false);
    assert!(!load_upgrade_input(&options).unwrap().registry.offline);
}

#[test]
fn lock_only_preserves_manifests_and_constraints() {
    let (_dir, input) = fixture(&format!("{PACKAGE}[dependencies]\nfoo='1.0.0'\n"), |o| {
        o.lockfile_only = true;
        o.policy = Some(UpgradePolicy::Incompatible);
    });
    let planned = plan(&input, &["1.8.0", "2.0.0"]);
    assert!(planned.changes().is_empty());
    assert_eq!(
        planned.report().dependencies[0].selected.as_deref(),
        Some("1.8.0")
    );
}

#[test]
fn serialized_plan_and_diff_are_deterministic() {
    let (_dir, input) = fixture(&format!("{PACKAGE}[dependencies]\nfoo='1.0.0'\n"), |_| {});
    let first = plan(&input, &["1.5.0", "1.2.0"]);
    let second = plan(&input, &["1.2.0", "1.5.0"]);
    assert_eq!(
        serde_json::to_string(&first).unwrap(),
        serde_json::to_string(&second).unwrap()
    );
    assert_eq!(first.diff(), second.diff());
    assert!(first.diff().contains("-foo='1.0.0'"));
}
