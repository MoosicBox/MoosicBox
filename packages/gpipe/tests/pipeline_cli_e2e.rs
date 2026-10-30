//! Product CLI boundaries for checked-in native pipeline definitions.
#![cfg(feature = "runtime")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::process::Command;

#[test]
fn pipeline_commands_are_public_cli_not_a_github_script() {
    for command in ["plan", "run", "submit", "verify"] {
        let output = Command::new(env!("CARGO_BIN_EXE_gpipe"))
            .args(["pipeline", command, "--help"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{command}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!String::from_utf8_lossy(&output.stdout).contains("gpipe-smoke.py"));
    }
}

#[test]
fn github_bootstrap_invokes_checked_in_pipeline_without_python_or_credentials() {
    let workflow = include_str!("../../../.github/workflows/gpipe-pipeline.yml");
    assert!(workflow.contains("workflow_dispatch:"));
    assert!(workflow.contains("contents: read"));
    assert!(workflow.contains("persist-credentials: false"));
    assert!(workflow.contains("pipeline plan .gpipe/moosicbox.json"));
    assert!(workflow.contains("pipeline run"));
    assert!(workflow.contains("always()"));
    assert!(!workflow.contains("python"));
    assert!(!workflow.contains("secrets."));
    let definition: serde_json::Value =
        serde_json::from_str(include_str!("../../../.gpipe/moosicbox.json")).unwrap();
    assert_eq!(
        definition["selection"]["packages"],
        serde_json::json!(["moosicbox_assert"])
    );
    assert_eq!(definition["validation"]["coverage"], true);
}

#[test]
fn malformed_definition_is_rejected_without_starting_a_run() {
    let temp = tempfile::tempdir().unwrap();
    let definition = temp.path().join("pipeline.json");
    std::fs::write(&definition, r#"{"version":1,"unknown":true}"#).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_gpipe"))
        .args(["pipeline", "plan"])
        .arg(&definition)
        .arg("--workspace")
        .arg(temp.path())
        .args([
            "--revision",
            &"a".repeat(40),
            "--base-revision",
            &"a".repeat(40),
        ])
        .arg("--clippier")
        .arg(temp.path().join("missing-clippier"))
        .arg("--output")
        .arg(temp.path().join("output"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!temp.path().join("output/verified.json").exists());
    assert!(!temp.path().join("state.db").exists());
}
