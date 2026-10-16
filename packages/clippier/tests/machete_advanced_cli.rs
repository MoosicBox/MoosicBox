//! End-to-end CLI contracts for explicit precision and evidence-driven workflows.
#![cfg(feature = "machete")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use serde_json::Value;
use std::{
    fs,
    process::{Command, Output},
};

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("Cargo.toml"), "[package]\nname='advanced'\nversion='0.1.0'\nedition='2021'\n[workspace]\n[dependencies]\nused='1.0.0'\nunused='1.0.0'\n").unwrap();
    fs::write(dir.path().join("src/lib.rs"), "pub use used::Thing;\n").unwrap();
    dir
}
fn run(dir: &tempfile::TempDir, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_clippier"))
        .current_dir(dir.path())
        .arg("machete")
        .args(args)
        .output()
        .unwrap()
}
fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "invalid JSON: {error}; stdout={}; stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}
fn declaration_status(value: &Value, alias: &str) -> String {
    value["report"]["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|dependency| dependency["dependency"] == alias)
        .unwrap()["status"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn default_command_retains_conditional_and_renamed_references_without_execution() {
    let dir = fixture();
    fs::write(dir.path().join("Cargo.toml"), "[package]\nname='advanced'\nversion='0.1.0'\nedition='2021'\n[workspace]\n[dependencies]\nrenamed={package='real-package',version='1.0.0'}\nconditional='1.0.0'\nunused='1.0.0'\n").unwrap();
    fs::write(dir.path().join("src/lib.rs"), "pub use renamed::Thing as PublicThing;\n#[cfg(target_os=\"windows\")] pub fn platform(){conditional::run();}\n").unwrap();
    // No subcommand, precision, profile, metadata, or execution options.
    let output = run(&dir, &["--output", "json"]);
    assert!(output.status.success());
    let value = json(&output);
    assert_eq!(declaration_status(&value, "renamed"), "USED");
    assert_eq!(declaration_status(&value, "conditional"), "USED");
    assert_eq!(declaration_status(&value, "unused"), "UNUSED");
    assert_eq!(value["report"]["coverage"]["precision"], false);
    assert_eq!(value["report"]["coverage"]["profile_limited"], false);
    assert!(!dir.path().join("target").exists());
    assert!(!dir.path().join("Cargo.lock").exists());
}

#[test]
fn default_command_protects_dynamic_generated_includes_instead_of_claiming_absence() {
    let dir = fixture();
    fs::write(
        dir.path().join("src/lib.rs"),
        "include!(concat!(env!(\"OUT_DIR\"),\"/generated.rs\"));\n",
    )
    .unwrap();
    let before = fs::read(dir.path().join("Cargo.toml")).unwrap();
    let output = run(&dir, &["--output", "json"]);
    assert_eq!(output.status.code(), Some(2));
    let value = json(&output);
    assert_eq!(declaration_status(&value, "used"), "INCOMPLETE");
    assert_eq!(declaration_status(&value, "unused"), "INCOMPLETE");
    assert_eq!(fs::read(dir.path().join("Cargo.toml")).unwrap(), before);
    assert!(!dir.path().join("target").exists());
}

#[test]
fn precision_is_opt_in_and_does_not_confuse_a_local_module_with_an_external_crate() {
    let dir = fixture();
    fs::write(
        dir.path().join("src/lib.rs"),
        "mod used {pub fn run(){}}\npub fn run(){used::run();}\n",
    )
    .unwrap();
    let baseline = json(&run(&dir, &["--output", "json"]));
    assert_eq!(declaration_status(&baseline, "used"), "USED");
    let precise = json(&run(&dir, &["--precision", "--output", "json"]));
    assert_eq!(declaration_status(&precise, "used"), "UNUSED");
}

#[test]
fn precision_explanation_has_source_positions_without_mutating_manifests() {
    let dir = fixture();
    let before = fs::read(dir.path().join("Cargo.toml")).unwrap();
    let output = run(
        &dir,
        &["analyze", "--precision", "--explain", "--output", "json"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value = json(&output);
    let evidence = value["report"]["evidence"].as_array().unwrap();
    assert!(evidence.iter().any(|entry| entry["alias"] == "used"
        && entry["line"] == 1
        && entry["column"].as_u64().unwrap() > 0));
    assert_eq!(fs::read(dir.path().join("Cargo.toml")).unwrap(), before);
    assert!(!dir.path().join("target").exists());
}
#[test]
fn unknown_profile_is_a_structured_error_not_a_default_analysis() {
    let dir = fixture();
    let output = run(&dir, &["plan", "--profile", "missing", "--output", "json"]);
    assert_eq!(output.status.code(), Some(2));
    let value = json(&output);
    assert_eq!(value["outcome"], "FAILED");
    assert!(value["error"].as_str().unwrap().contains("profile"));
}
#[test]
fn audit_is_read_only_and_has_structured_diagnostics() {
    let dir = fixture();
    fs::write(
        dir.path().join("clippier.toml"),
        "[machete]\nignored=['never_declared']\n",
    )
    .unwrap();
    let before = fs::read(dir.path().join("Cargo.toml")).unwrap();
    let output = run(&dir, &["audit", "--output", "json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value = json(&output);
    assert!(value["audit"].is_object(), "{value}");
    assert_eq!(fs::read(dir.path().join("Cargo.toml")).unwrap(), before);
    assert!(!dir.path().join("target").exists());
}
#[test]
fn finite_profile_reports_never_turn_into_global_removal_approval() {
    let dir = fixture();
    fs::write(
        dir.path().join("clippier.toml"),
        "[machete.profiles.minimal]\ndefault-features=false\n",
    )
    .unwrap();
    let before = fs::read(dir.path().join("Cargo.toml")).unwrap();
    let output = run(
        &dir,
        &["apply", "--profile", "minimal", "--yes", "--output", "json"],
    );
    assert_eq!(output.status.code(), Some(2));
    let value = json(&output);
    assert_eq!(value["report"]["coverage"]["profile_limited"], true);
    assert_eq!(fs::read(dir.path().join("Cargo.toml")).unwrap(), before);
}

#[test]
fn audit_preserves_exact_rule_scope_and_accepts_explicit_review_date() {
    let dir = fixture();
    fs::write(dir.path().join("clippier.toml"), "[[machete.rules]]\ndependencies=['unused']\nignored=['used']\nreason='reviewed'\nreview-after='2025-01-01'\n").unwrap();
    let output = run(
        &dir,
        &[
            "audit",
            "--precision",
            "--review-date",
            "2026-01-01",
            "--output",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let value = json(&output);
    let diagnostics = value["audit"]["diagnostics"].as_array().unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|finding| finding["kind"] == "UNMATCHED_IGNORE"),
        "{value}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|finding| finding["kind"] == "IGNORED_BUT_USED"),
        "{value}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|finding| finding["kind"] == "SUPPRESSION_REVIEW_OVERDUE"),
        "{value}"
    );
}

#[cfg(feature = "machete-verify")]
#[test]
fn expansion_also_requires_execution_consent_and_never_selects_a_toolchain() {
    let dir = fixture();
    let output = run(&dir, &["expand", "--output", "json"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        json(&output)["error"]
            .as_str()
            .unwrap()
            .contains("execution")
    );
    assert!(!dir.path().join("target").exists());
}

#[cfg(feature = "machete-verify")]
#[test]
fn verification_requires_independent_execution_approval() {
    let dir = fixture();
    let before = fs::read(dir.path().join("Cargo.toml")).unwrap();
    let output = run(&dir, &["verify", "--output", "json"]);
    assert_eq!(output.status.code(), Some(2));
    let value = json(&output);
    assert_eq!(value["outcome"], "FAILED");
    assert!(value["error"].as_str().unwrap().contains("execution"));
    assert_eq!(fs::read(dir.path().join("Cargo.toml")).unwrap(), before);
    assert!(!dir.path().join("target").exists());
}
