#![cfg(feature = "upgrade")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Requirement regression cases adapted from cargo-edit's MIT-licensed tests.
//! See `fixtures/cargo_edit_upgrade/{sources.json,LICENSE-MIT,version-original.rs.txt}`.

use clippier::upgrade::{
    LockfilePolicy, PinnedPolicy, RegistryKey, UpgradeOptions, UpgradePolicy, load_upgrade_input,
    plan_upgrade_with_releases, registry::Release,
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

#[derive(Deserialize)]
struct RequirementCase {
    version: String,
    requirement: String,
    expected: String,
}

fn planned_requirement(requirement: &str, version: &str, pinned: PinnedPolicy) -> String {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Cargo.toml"), format!(
        "[package]\nname='requirement_fixture'\nversion='0.1.0'\n[dependencies]\ndemo='{requirement}'\n"
    )).unwrap();
    let input = load_upgrade_input(&UpgradeOptions {
        manifest_path: Some(root.path().into()),
        policy: Some(UpgradePolicy::Incompatible),
        pinned: Some(pinned),
        lockfile: Some(LockfilePolicy::None),
        ..UpgradeOptions::default()
    })
    .unwrap();
    let releases = BTreeMap::from([(
        RegistryKey {
            name: "demo".into(),
            registry: None,
        },
        vec![Release {
            version: version.parse().unwrap(),
            yanked: false,
            rust_version: None,
            features: BTreeSet::new(),
        }],
    )]);
    let plan = plan_upgrade_with_releases(&input, &releases).unwrap();
    plan.report().dependencies[0].requirement.clone().unwrap()
}

#[test]
fn all_52_upstream_requirement_upgrade_assertions() {
    let cases: Vec<RequirementCase> = serde_json::from_str(include_str!(
        "fixtures/cargo_edit_upgrade/requirements.json"
    ))
    .unwrap();
    assert_eq!(cases.len(), 52, "upstream corpus must not silently shrink");
    for case in cases {
        assert_eq!(
            planned_requirement(&case.requirement, &case.version, PinnedPolicy::Upgrade),
            case.expected,
            "upstream upgrade_requirement({:?}, {:?})",
            case.requirement,
            case.version,
        );
    }
}

#[test]
fn registry_build_metadata_never_enters_a_requirement() {
    for (old, expected) in [
        ("0.2", "99999.0"),
        ("^0.2", "^99999.0"),
        ("~0.2.0", "~99999.0.0"),
        ("0.2.*", "99999.0.*"),
    ] {
        assert_eq!(
            planned_requirement(old, "99999.0.0+my-package", PinnedPolicy::Upgrade),
            expected
        );
    }
}

#[test]
fn upstream_pin_boundaries_and_explicit_clippier_differences() {
    // Preserve cargo-edit's exact, wildcard and upper-bound pins by default.
    // Unbounded comparators remain constraints rather than being broadened.
    for (requirement, expected) in [
        ("=3", "=3"),
        ("<3", "<3"),
        ("<=3", "<=3"),
        ("3.*", "3.*"),
        ("*", "*"),
        (">3", ">3"),
        (">=3", ">=3"),
        ("^3", "^4"),
        ("3", "4"),
    ] {
        assert_eq!(
            planned_requirement(requirement, "4.0.0", PinnedPolicy::Preserve),
            expected,
            "{requirement}"
        );
    }
}
