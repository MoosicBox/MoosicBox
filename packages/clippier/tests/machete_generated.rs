//! Supplied generated and expanded source freshness/scope contracts.
#![cfg(feature = "machete")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
use clippier::machete::generated::{
    GeneratedEvidence, GeneratedSource, GeneratedTargetRole, InputFingerprint,
    validate_generated_evidence,
};
use std::{fs, path::Path};

fn supplied(root: &Path) -> GeneratedEvidence {
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='generated'\nversion='0.1.0'\n",
    )
    .unwrap();
    fs::write(root.join("input.rs"), "generate!();\n").unwrap();
    fs::write(
        root.join("expanded.rs"),
        "pub fn generated() { dep_alias::call(); }\n",
    )
    .unwrap();
    GeneratedEvidence {
        sources: vec![GeneratedSource {
            owner_manifest: "Cargo.toml".into(),
            target_role: GeneratedTargetRole::Library,
            profile: "nightly-explicit/features=a".into(),
            output: InputFingerprint::capture(root, Path::new("expanded.rs")).unwrap(),
            inputs: ["Cargo.toml", "input.rs"]
                .map(|path| InputFingerprint::capture(root, Path::new(path)).unwrap())
                .to_vec(),
        }],
    }
}

#[test]
fn supplied_expansion_provides_scoped_positive_names_without_execution() {
    let directory = tempfile::tempdir().unwrap();
    let evidence = supplied(directory.path());
    let report = validate_generated_evidence(directory.path(), &evidence).unwrap();
    assert_eq!(report.sources.len(), 1);
    assert!(report.sources[0].names.contains("dep_alias"));
    assert_eq!(report.sources[0].profile, "nightly-explicit/features=a");
    assert_eq!(report.sources[0].target_role, GeneratedTargetRole::Library);
    assert!(!directory.path().join("target").exists());
    let serialized = serde_json::to_string(&evidence).unwrap();
    let decoded = serde_json::from_str(&serialized).unwrap();
    assert!(validate_generated_evidence(directory.path(), &decoded).is_ok());
}

#[test]
fn every_declared_input_and_output_must_match_generation_time_bytes() {
    for changed in ["Cargo.toml", "input.rs", "expanded.rs"] {
        let directory = tempfile::tempdir().unwrap();
        let evidence = supplied(directory.path());
        fs::write(directory.path().join(changed), "changed").unwrap();
        assert!(validate_generated_evidence(directory.path(), &evidence).is_err());
    }
}

#[test]
fn missing_owner_snapshot_empty_profiles_and_escapes_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let mut evidence = supplied(directory.path());
    evidence.sources[0].inputs.remove(0);
    assert!(validate_generated_evidence(directory.path(), &evidence).is_err());
    let mut evidence = supplied(directory.path());
    evidence.sources[0].profile.clear();
    assert!(validate_generated_evidence(directory.path(), &evidence).is_err());
    for path in ["../outside.rs", "/etc/passwd", "sub/../input.rs"] {
        assert!(InputFingerprint::capture(directory.path(), Path::new(path)).is_err());
    }
}

#[cfg(unix)]
#[test]
fn generated_input_symlinks_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let evidence = supplied(directory.path());
    fs::rename(
        directory.path().join("input.rs"),
        directory.path().join("real.rs"),
    )
    .unwrap();
    std::os::unix::fs::symlink("real.rs", directory.path().join("input.rs")).unwrap();
    assert!(validate_generated_evidence(directory.path(), &evidence).is_err());
}

#[test]
fn comments_and_strings_in_generated_output_are_not_dependency_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let mut evidence = supplied(directory.path());
    fs::write(
        directory.path().join("expanded.rs"),
        "// fake_dep::call();\nconst S: &str = \"string_dep::call()\";\n",
    )
    .unwrap();
    evidence.sources[0].output =
        InputFingerprint::capture(directory.path(), Path::new("expanded.rs")).unwrap();
    let report = validate_generated_evidence(directory.path(), &evidence).unwrap();
    assert!(!report.sources[0].names.contains("fake_dep"));
    assert!(!report.sources[0].names.contains("string_dep"));
}
