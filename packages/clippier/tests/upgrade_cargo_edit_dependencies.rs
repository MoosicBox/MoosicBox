#![cfg(feature = "upgrade")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Native editing equivalents of cargo-edit dependency serialization scenarios.
//! Attribution and original tests: `fixtures/cargo_edit_upgrade/dependency-original.rs.txt`.

use clippier::upgrade::{
    LockfilePolicy, RegistryKey, UpgradeOptions, load_upgrade_input, plan_upgrade_with_releases,
    registry::Release,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

#[test]
fn upstream_dependency_shapes_preserve_every_nonversion_field() {
    // Clippier edits existing declarations instead of constructing new ones.
    // This covers simple/no-source, versioned, optional, no-default-features,
    // path, Git, renamed, alternate-registry and complex dependency cases.
    for (alias, declaration, registry, skipped) in [
        ("dep", "{}", None, true),
        ("dep", "'1.0'", None, false),
        ("dep", "{version='1.0', optional=true}", None, false),
        (
            "dep",
            "{version='1.0', default-features=false}",
            None,
            false,
        ),
        ("dep", "{path='sibling/crate', version='1.0'}", None, true),
        (
            "dep",
            "{git='https://example.invalid/dep', branch='main', version='1.0'}",
            None,
            true,
        ),
        ("alias", "{package='dep', version='1.0'}", None, false),
        (
            "dep",
            "{version='1.0', registry='alternative'}",
            Some("alternative"),
            false,
        ),
        (
            "alias",
            "{package='dep', version='1.0', features=['feat'], optional=true, default-features=false, registry='alternative'}",
            Some("alternative"),
            false,
        ),
        ("dep", "{path='sibling/crate'}", None, true),
        ("dep", "{path='sibling\\crate'}", None, true),
    ] {
        let root = tempfile::tempdir().unwrap();
        let before = format!(
            "[package]\nname='dependency_fixture'\nversion='0.1.0'\n[dependencies]\n{alias}={declaration} # retained comment\n"
        );
        fs::write(root.path().join("Cargo.toml"), &before).unwrap();
        let input = load_upgrade_input(&UpgradeOptions {
            manifest_path: Some(root.path().into()),
            lockfile: Some(LockfilePolicy::None),
            ..UpgradeOptions::default()
        })
        .unwrap();
        let releases = if skipped {
            BTreeMap::new()
        } else {
            BTreeMap::from([(
                RegistryKey {
                    name: "dep".into(),
                    registry: registry.map(str::to_owned),
                },
                vec![Release {
                    version: "1.2.0".parse().unwrap(),
                    yanked: false,
                    rust_version: None,
                    features: BTreeSet::from(["feat".into()]),
                }],
            )])
        };
        let plan = plan_upgrade_with_releases(&input, &releases).unwrap();
        if skipped {
            assert!(plan.changes().is_empty(), "{declaration}");
        } else {
            let change = &plan.changes()[0];
            assert!(change.after.contains("# retained comment"));
            let mut expected: toml::Value = toml::from_str(&before).unwrap();
            let actual: toml::Value = toml::from_str(&change.after).unwrap();
            let value = &mut expected["dependencies"][alias];
            if value.is_str() {
                *value = toml::Value::String("1.2".into());
            } else {
                value["version"] = toml::Value::String("1.2".into());
            }
            assert_eq!(actual, expected, "{declaration}");
        }
        assert_eq!(
            fs::read_to_string(root.path().join("Cargo.toml")).unwrap(),
            before,
            "planning is read-only"
        );
    }
}
