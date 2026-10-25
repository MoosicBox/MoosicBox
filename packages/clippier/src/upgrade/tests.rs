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
fn workspace_discovery_ignores_invalid_nonmembers_but_rejects_invalid_members() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::write(root.join("Cargo.toml"), "[workspace]\nmembers=['member']\n").unwrap();
    for name in ["member", "fixtures/invalid", "independent/member"] {
        fs::create_dir_all(root.join(name)).unwrap();
    }
    fs::write(root.join("member/Cargo.toml"), PACKAGE).unwrap();
    fs::write(
        root.join("fixtures/invalid/Cargo.toml"),
        "This is clearly not a valid Cargo.toml.",
    )
    .unwrap();
    fs::write(
        root.join("independent/Cargo.toml"),
        "[workspace]\nmembers=['member']\n",
    )
    .unwrap();
    fs::write(root.join("independent/member/Cargo.toml"), "also invalid").unwrap();
    let options = UpgradeOptions {
        manifest_path: Some(root.into()),
        lockfile: Some(LockfilePolicy::None),
        ..UpgradeOptions::default()
    };
    let input = load_upgrade_input(&options).unwrap();
    assert_eq!(input.manifests.len(), 2);
    assert!(plan_upgrade_with_releases(&input, &BTreeMap::new()).is_ok());

    // Being selected as a member must not suppress a malformed manifest.
    fs::write(root.join("member/Cargo.toml"), "also invalid").unwrap();
    assert!(load_upgrade_input(&options).is_err());
    fs::write(
        root.join("member/Cargo.toml"),
        format!("{PACKAGE}[dependencies]\nbroken={{path='../fixtures/invalid'}}\n"),
    )
    .unwrap();
    assert!(load_upgrade_input(&options).is_err());
}

#[test]
fn patched_sources_are_preserved_before_registry_discovery() {
    for (patch, lock_source, status) in [
        (
            "fork={package='foo',git='https://example.invalid/fork'}",
            Some("git+https://example.invalid/fork#abc"),
            UpgradeStatus::Skipped,
        ),
        ("foo={path='vendor/foo'}", Some(""), UpgradeStatus::Skipped),
        (
            "foo={git='https://example.invalid/fork'}",
            None,
            UpgradeStatus::Unavailable,
        ),
        (
            "foo={git='https://example.invalid/fork',version='9.0.0'}",
            Some("registry+https://github.com/rust-lang/crates.io-index"),
            UpgradeStatus::Unavailable,
        ),
    ] {
        let (dir, _) = fixture(
            &format!(
                "{PACKAGE}[dependencies]\nrenamed={{package='foo',version='1.0.0'}}\n[patch.crates-io]\n{patch}\n"
            ),
            |_| {},
        );
        if let Some(source) = lock_source {
            let source = if source.is_empty() {
                String::new()
            } else {
                format!("source='{source}'\n")
            };
            fs::write(
                dir.path().join("Cargo.lock"),
                format!("version=4\n[[package]]\nname='foo'\nversion='1.0.0'\n{source}"),
            )
            .unwrap();
        }
        let input = load_upgrade_input(&UpgradeOptions {
            manifest_path: Some(dir.path().into()),
            lockfile: Some(LockfilePolicy::None),
            source: Some(SourcePolicy::IncludePath),
            ..UpgradeOptions::default()
        })
        .unwrap();
        assert!(
            super::planner::registry_requests(&input)
                .unwrap()
                .is_empty()
        );
        let plan = plan_upgrade_with_releases(&input, &BTreeMap::new()).unwrap();
        assert!(plan.changes().is_empty());
        assert_eq!(plan.lock_targets.len(), 0);
        assert_eq!(plan.lock_versions.len(), 0);
        assert_eq!(plan.report.dependencies[0].status, status);
        assert!(plan.report.dependencies[0].reason.contains("patch"));
    }
}

#[test]
fn patch_scope_and_package_identity_do_not_skip_unrelated_dependencies() {
    for (scope, patch) in [
        (
            "crates-io",
            "foo={package='different',git='https://example.invalid/fork'}",
        ),
        (
            "https://example.invalid/other-index",
            "foo={git='https://example.invalid/fork'}",
        ),
    ] {
        let (_dir, input) = fixture(
            &format!("{PACKAGE}[dependencies]\nfoo='1.0.0'\n[patch.'{scope}']\n{patch}\n"),
            |_| {},
        );
        assert_eq!(super::planner::registry_requests(&input).unwrap().len(), 1);
        assert_eq!(
            plan(&input, &["1.2.0"]).report.dependencies[0].status,
            UpgradeStatus::Planned
        );
    }
}

#[test]
fn inherited_patch_with_multiple_locked_sources_is_not_guessed() {
    let (dir, _) = fixture(PACKAGE, |_| {});
    fs::write(dir.path().join("Cargo.toml"), "[workspace]\nmembers=['member']\n[workspace.dependencies]\nfoo='1.0.0'\n[patch.crates-io]\nfoo={git='https://example.invalid/fork'}\n").unwrap();
    fs::create_dir(dir.path().join("member")).unwrap();
    fs::write(
        dir.path().join("member/Cargo.toml"),
        format!("{PACKAGE}[dependencies]\nfoo.workspace=true\n"),
    )
    .unwrap();
    fs::write(dir.path().join("Cargo.lock"), "version=4\n[[package]]\nname='foo'\nversion='1.0.0'\nsource='git+https://example.invalid/fork#abc'\n[[package]]\nname='foo'\nversion='1.1.0'\nsource='registry+https://github.com/rust-lang/crates.io-index'\n").unwrap();
    let input = load_upgrade_input(&UpgradeOptions {
        manifest_path: Some(dir.path().into()),
        ..UpgradeOptions::default()
    })
    .unwrap();
    let plan = plan_upgrade_with_releases(&input, &BTreeMap::new()).unwrap();
    assert_eq!(
        plan.report.dependencies[0].status,
        UpgradeStatus::Unavailable
    );
    assert!(plan.changes.is_empty());
    assert_eq!(plan.lock_versions.len(), 0);
}

#[test]
fn alternate_registry_patch_and_lock_targets_are_source_qualified() {
    let (dir, _) = fixture(
        &format!(
            "{PACKAGE}[dependencies]\nfoo={{version='1.0.0',registry='private'}}\n[patch.'https://example.invalid/index']\nfoo={{git='https://example.invalid/fork'}}\n"
        ),
        |_| {},
    );
    fs::create_dir(dir.path().join(".cargo")).unwrap();
    fs::write(
        dir.path().join(".cargo/config.toml"),
        "[registries.private]\nindex='https://example.invalid/index'\n",
    )
    .unwrap();
    fs::write(dir.path().join("Cargo.lock"), "version=4\n[[package]]\nname='foo'\nversion='1.0.0'\nsource='git+https://example.invalid/fork#abc'\n").unwrap();
    let options = UpgradeOptions {
        manifest_path: Some(dir.path().into()),
        lockfile: Some(LockfilePolicy::None),
        ..UpgradeOptions::default()
    };
    let input = load_upgrade_input(&options).unwrap();
    assert_eq!(super::planner::registry_requests(&input).unwrap().len(), 0);
    let plan = plan_upgrade_with_releases(&input, &BTreeMap::new()).unwrap();
    assert_eq!(plan.report.dependencies[0].status, UpgradeStatus::Skipped);
    fs::write(
        dir.path().join("Cargo.toml"),
        format!("{PACKAGE}[dependencies]\nfoo={{version='1.0.0',registry='private'}}\n"),
    )
    .unwrap();
    // A newer same-named crate from another registry must not cause a downgrade
    // rejection or become the target of reconciliation.
    fs::write(dir.path().join("Cargo.lock"), "version=4\n[[package]]\nname='foo'\nversion='1.9.0'\nsource='registry+https://github.com/rust-lang/crates.io-index'\n").unwrap();
    let input = load_upgrade_input(&options).unwrap();
    let releases = BTreeMap::from([(
        RegistryKey {
            name: "foo".into(),
            registry: Some("private".into()),
        },
        vec![release("1.2.0")],
    )]);
    let plan = plan_upgrade_with_releases(&input, &releases).unwrap();
    assert_eq!(
        plan.lock_versions[0].registry_source.as_deref(),
        Some("https://example.invalid/index")
    );
}

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

#[switchy_async::test]
async fn transitive_exact_pin_holds_back_conflict_but_refreshes_independent_packages() {
    let (dir, _) = fixture(&format!("{PACKAGE}[dependencies]\nrootdep='1'\n"), |_| {});
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/lib.rs"), "").unwrap();
    fs::create_dir(dir.path().join(".cargo")).unwrap();
    fs::write(
        dir.path().join(".cargo/config.toml"),
        "[source.crates-io]\nreplace-with='vendor'\n[source.vendor]\ndirectory='vendor'\n",
    )
    .unwrap();
    let vendor = |name: &str, version: &str, dependencies: &str| {
        let path = dir.path().join("vendor").join(format!("{name}-{version}"));
        fs::create_dir_all(path.join("src")).unwrap();
        fs::write(path.join("src/lib.rs"), "").unwrap();
        fs::write(
            path.join("Cargo.toml"),
            format!(
                "[package]\nname='{name}'\nversion='{version}'\n[dependencies]\n{dependencies}"
            ),
        )
        .unwrap();
        fs::write(
            path.join(".cargo-checksum.json"),
            "{\"files\":{},\"package\":null}",
        )
        .unwrap();
    };
    vendor(
        "rootdep",
        "1.0.0",
        "parser='1'\nstack='1'\nindependent='1'\n",
    );
    vendor("parser", "1.0.0", "");
    vendor("stack", "1.0.0", "leaf='1'\n");
    vendor("leaf", "1.1.0", "");
    vendor("independent", "1.0.0", "");
    let output = std::process::Command::new("cargo")
        .args(["generate-lockfile", "--offline"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let original = fs::read_to_string(dir.path().join("Cargo.lock")).unwrap();
    // Models pest's new exact PSM pin, without network/cache dependencies.
    vendor("parser", "1.1.0", "leaf='=1.0.0'\n");
    vendor("leaf", "1.0.0", "");
    vendor("independent", "1.1.0", "");
    let input = load_upgrade_input(&UpgradeOptions {
        manifest_path: Some(dir.path().into()),
        lockfile: Some(LockfilePolicy::Refresh),
        registry_overrides: RegistryOverrides {
            offline: Some(true),
            ..RegistryOverrides::default()
        },
        ..UpgradeOptions::default()
    })
    .unwrap();
    let releases = BTreeMap::from([(
        RegistryKey {
            name: "rootdep".into(),
            registry: None,
        },
        vec![release("1.0.0")],
    )]);
    let mut plan = plan_upgrade_with_releases(&input, &releases).unwrap();
    let (resolved, warnings, diagnostics) =
        super::apply::resolve_lockfile_with_warnings(&plan).unwrap();
    let doc: toml::Value = toml::from_str(&resolved).unwrap();
    let packages = doc["package"].as_array().unwrap();
    for (name, version) in [
        ("parser", "1.0.0"),
        ("leaf", "1.1.0"),
        ("independent", "1.1.0"),
    ] {
        assert!(packages.iter().any(|p| p["name"].as_str() == Some(name) && p["version"].as_str() == Some(version)), "{name}: {resolved}");
    }
    // Cargo can retain the old parser while updating the independent package.
    // Failed parent batches are recovered attempts, not final warnings.
    assert_eq!(warnings, Vec::<String>::new());
    assert!(
        diagnostics.iter().any(|message| message.contains("leaf")),
        "{diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|message| message.starts_with("Retrying transitive refresh batch"))
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("Cargo.lock")).unwrap(),
        original
    );
    // Preview diagnostics survive cached resolution and apply publication.
    plan.resolved_lockfile = Some(resolved.clone());
    plan.report.warnings = warnings.clone();
    plan.report.diagnostics = diagnostics.clone();
    let report = apply_upgrade_plan(&plan).await.unwrap();
    assert_eq!(report.warnings, warnings);
    assert_eq!(report.diagnostics, diagnostics);
    assert_eq!(
        fs::read_to_string(dir.path().join("Cargo.lock")).unwrap(),
        resolved
    );
}

#[switchy_async::test]
async fn incompatible_resolution_splits_shared_versions_at_selected_declaration_edges() {
    for inherited in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join(".cargo")).unwrap();
        fs::write(
            dir.path().join(".cargo/config.toml"),
            "[source.crates-io]\nreplace-with='vendor'\n[source.vendor]\ndirectory='vendor'\n",
        )
        .unwrap();
        let vendor = |name: &str, version: &str, dependencies: &str| {
            let path = dir.path().join("vendor").join(format!("{name}-{version}"));
            fs::create_dir_all(path.join("src")).unwrap();
            fs::write(path.join("src/lib.rs"), "").unwrap();
            fs::write(
                path.join("Cargo.toml"),
                format!(
                    "[package]\nname='{name}'\nversion='{version}'\n[dependencies]\n{dependencies}"
                ),
            )
            .unwrap();
            fs::write(
                path.join(".cargo-checksum.json"),
                "{\"files\":{},\"package\":null}",
            )
            .unwrap();
        };
        vendor("codec", "0.22.1", "");
        vendor("codec", "0.23.1", "");
        vendor("protocol", "1.0.0", "codec='0.22'\n");
        fs::create_dir_all(dir.path().join("member/src")).unwrap();
        fs::write(dir.path().join("member/src/lib.rs"), "").unwrap();
        let owner = if inherited {
            "[workspace.dependencies]\nmodern={package='codec',version='0.22.1'}\n"
        } else {
            ""
        };
        fs::write(
            dir.path().join("Cargo.toml"),
            format!("[workspace]\nmembers=['member']\nresolver='2'\n{owner}"),
        )
        .unwrap();
        let modern = if inherited {
            "modern.workspace=true"
        } else {
            "modern={package='codec',version='0.22.1'}"
        };
        let manifest = dir.path().join("member/Cargo.toml");
        fs::write(&manifest, format!("{PACKAGE}[dependencies]\n{modern}\nlegacy={{package='codec',version='=0.22.1'}}\nprotocol='1'\n")).unwrap();
        let output = std::process::Command::new("cargo")
            .args(["generate-lockfile", "--offline"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let original_lock = fs::read_to_string(dir.path().join("Cargo.lock")).unwrap();
        // A newer eligible index release exists, but the injected selection
        // deliberately chooses 0.23.1 (e.g. a feature/MSRV ceiling).
        vendor("codec", "0.23.2", "");
        let input = load_upgrade_input(&UpgradeOptions {
            manifest_path: Some(dir.path().into()),
            policy: Some(UpgradePolicy::Incompatible),
            dependencies: vec!["modern".into()],
            lockfile: Some(LockfilePolicy::Refresh),
            registry_overrides: RegistryOverrides {
                offline: Some(true),
                ..RegistryOverrides::default()
            },
            ..UpgradeOptions::default()
        })
        .unwrap();
        let releases = BTreeMap::from([(
            RegistryKey {
                name: "codec".into(),
                registry: None,
            },
            vec![release("0.23.1")],
        )]);
        let plan = plan_upgrade_with_releases(&input, &releases).unwrap();
        let (resolved, _, _) = super::apply::resolve_lockfile_with_warnings(&plan).unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("Cargo.lock")).unwrap(),
            original_lock
        );
        let report = apply_upgrade_plan(&plan).await.unwrap();
        assert!(report.applied);
        assert_eq!(
            fs::read_to_string(dir.path().join("Cargo.lock")).unwrap(),
            resolved
        );
        let lock: toml::Value = toml::from_str(&resolved).unwrap();
        let packages = lock["package"].as_array().unwrap();
        let versions = packages
            .iter()
            .filter(|p| p["name"].as_str() == Some("codec"))
            .map(|p| p["version"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(versions, ["0.22.1", "0.23.1"]);
        let protocol = packages
            .iter()
            .find(|p| p["name"].as_str() == Some("protocol"))
            .unwrap();
        assert_eq!(protocol["dependencies"][0].as_str(), Some("codec 0.22.1"));
        let owner_path = if inherited {
            dir.path().join("Cargo.toml")
        } else {
            manifest.clone()
        };
        let published = fs::read_to_string(owner_path).unwrap();
        assert!(published.contains("version=\"0.23.1\""), "{published}");
        assert!(
            !published.contains("version='=0.23.1'"),
            "temporary resolver pins must not escape"
        );
        assert!(
            fs::read_to_string(&manifest)
                .unwrap()
                .contains("version='=0.22.1'")
        );
        let output = std::process::Command::new("cargo")
            .args(["metadata", "--offline", "--locked", "--format-version", "1"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let resolve = metadata["resolve"]["nodes"].as_array().unwrap();
        let member = metadata["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == "fixture")
            .unwrap();
        let node = resolve.iter().find(|n| n["id"] == member["id"]).unwrap();
        for (alias, version) in [("modern", "0.23.1"), ("legacy", "0.22.1")] {
            let edge = node["deps"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["name"] == alias)
                .unwrap();
            let package = metadata["packages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["id"] == edge["pkg"])
                .unwrap();
            assert_eq!(package["version"].as_str(), Some(version));
        }
    }
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
