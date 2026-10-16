//! Non-executing verification safety and typed CLI configuration contracts.
#![cfg(feature = "machete-verify")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
use clippier::machete::{
    MacheteOptions, plan_machete,
    verify::{
        ExpansionOptions, VerificationOptions, VerificationProfile, expand_machete_project,
        verify_machete_plan,
    },
};
use std::fs;

/// Deliberate coordinator-only execution: select a real installed Cargo/rustc pair
/// via `CLIPPIER_TEST_CARGO` and `CLIPPIER_TEST_RUSTC`, then run this ignored test.
#[cfg(unix)]
#[test]
#[ignore = "explicit compiler execution; coordinator supplies installed toolchain paths"]
fn real_path_dependency_removal_refreshes_only_scratch_lockfile() {
    use clippier::machete::verify::VerificationStatus;
    let cargo =
        std::env::var_os("CLIPPIER_TEST_CARGO").expect("select CLIPPIER_TEST_CARGO explicitly");
    let rustc =
        std::env::var_os("CLIPPIER_TEST_RUSTC").expect("select CLIPPIER_TEST_RUSTC explicitly");
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::create_dir_all(root.path().join("local-dep/src")).unwrap();
    let manifest = "[package]\nname='verification'\nversion='0.1.0'\nedition='2021'\n[workspace]\nexclude=['local-dep']\n[dependencies]\nunused={path='local-dep'}\n";
    let lockfile = "version = 4\n\n[[package]]\nname = \"verification\"\nversion = \"0.1.0\"\ndependencies = [\"unused\"]\n\n[[package]]\nname = \"unused\"\nversion = \"0.1.0\"\n";
    fs::write(root.path().join("Cargo.toml"), manifest).unwrap();
    fs::write(root.path().join("Cargo.lock"), lockfile).unwrap();
    fs::write(root.path().join("src/lib.rs"), "pub fn library() {}\n").unwrap();
    fs::write(
        root.path().join("local-dep/Cargo.toml"),
        "[package]\nname='unused'\nversion='0.1.0'\nedition='2021'\n",
    )
    .unwrap();
    fs::write(
        root.path().join("local-dep/src/lib.rs"),
        "pub fn unused() {}\n",
    )
    .unwrap();
    let plan = plan_machete(&MacheteOptions {
        manifest_path: Some(root.path().join("Cargo.toml")),
        ..MacheteOptions::default()
    })
    .unwrap();
    assert!(
        plan.changes
            .iter()
            .any(|change| change.before.contains("unused={path="))
    );
    let report = verify_machete_plan(
        &plan,
        &VerificationOptions {
            allow_execution: true,
            cargo_path: cargo.into(),
            rustc_path: rustc.into(),
            profiles: vec![VerificationProfile {
                name: "real-local-removal".into(),
                ..VerificationProfile::default()
            }],
            ..VerificationOptions::default()
        },
    )
    .unwrap();
    assert_eq!(report.profiles.len(), 1);
    let observation = &report.profiles[0];
    assert!(
        matches!(observation.baseline.status, VerificationStatus::Passed),
        "{:?}",
        observation.baseline
    );
    let preparation = observation.candidate_preparation.as_ref().unwrap();
    assert!(
        matches!(preparation.status, VerificationStatus::Passed),
        "{preparation:?}"
    );
    assert!(
        matches!(
            observation.candidate_result.status,
            VerificationStatus::Passed
        ),
        "{:?}",
        observation.candidate_result
    );
    assert_eq!(
        fs::read_to_string(root.path().join("Cargo.toml")).unwrap(),
        manifest
    );
    assert_eq!(
        fs::read_to_string(root.path().join("Cargo.lock")).unwrap(),
        lockfile
    );
    assert!(!root.path().join("target").exists());
}

#[test]
fn defaults_never_invoke_compiler_and_preserve_project_bytes() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    let manifest = "[package]\nname='verification'\nversion='0.1.0'\nedition='2021'\n[dependencies]\nunused='1.0.0'\n";
    fs::write(root.path().join("Cargo.toml"), manifest).unwrap();
    fs::write(root.path().join("src/lib.rs"), "pub fn library() {}\n").unwrap();
    let plan = plan_machete(&MacheteOptions {
        manifest_path: Some(root.path().join("Cargo.toml")),
        ..MacheteOptions::default()
    })
    .unwrap();
    let error = verify_machete_plan(&plan, &VerificationOptions::default()).unwrap_err();
    assert!(error.to_string().contains("allow_execution"));
    assert_eq!(
        fs::read_to_string(root.path().join("Cargo.toml")).unwrap(),
        manifest
    );
    assert!(!root.path().join("target").exists());
    assert!(!root.path().join("Cargo.lock").exists());
}

#[test]
fn expansion_defaults_require_separate_deliberate_execution_consent() {
    let root = tempfile::tempdir().unwrap();
    let error = expand_machete_project(
        root.path(),
        &ExpansionOptions {
            verification: VerificationOptions::default(),
            profile: VerificationProfile {
                name: "expanded".into(),
                ..VerificationProfile::default()
            },
            package: None,
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("allow_execution"));
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn verification_configuration_is_typed_and_rejects_arbitrary_command_fields() {
    let config: VerificationOptions = serde_json::from_str(
        r#"{"profiles":[{"name":"test","operation":"TEST_NO_RUN","features":["optional"]}]}"#,
    )
    .unwrap();
    assert!(!config.allow_execution);
    assert_eq!(config.profiles[0].features, ["optional"]);
    assert!(
        serde_json::from_str::<VerificationOptions>(r#"{"command":"sh -c something"}"#).is_err()
    );
    assert!(serde_json::from_str::<VerificationProfile>(r#"{"operation":"SHELL"}"#).is_err());
}
