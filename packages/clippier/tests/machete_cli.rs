#![cfg(feature = "machete")]
//! CLI contracts for read-only analysis and explicit dependency removal.
use serde_json::Value;
use std::{
    fs,
    process::{Command, Output},
};
use tempfile::TempDir;

struct Fixture {
    dir: TempDir,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("Cargo.toml"), "# preserve me\n[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2021'\n[workspace]\n[dependencies]\nused = '1.0.0'\nunused = '1.0.0'\n").unwrap();
        fs::write(dir.path().join("src/lib.rs"), "pub use used::Thing;\n").unwrap();
        fs::write(dir.path().join("Cargo.lock"), "sentinel lockfile\n").unwrap();
        Self { dir }
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_clippier"))
            .current_dir(self.dir.path())
            .arg("machete")
            .args(args)
            .output()
            .unwrap()
    }
    fn json(&self, args: &[&str]) -> (Output, Value) {
        let out = self.run(args);
        let value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "invalid JSON: {e}: stdout={} stderr={}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        });
        (out, value)
    }
}

#[test]
fn analysis_check_and_plan_are_read_only() {
    let f = Fixture::new();
    let before = fs::read(f.dir.path().join("Cargo.toml")).unwrap();
    for args in [
        vec!["--output", "json"],
        vec!["analyze", "--output", "json"],
        vec!["plan", "--diff", "--output", "json"],
    ] {
        let (out, value) = f.json(&args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["report"]["complete"], true);
        assert_eq!(fs::read(f.dir.path().join("Cargo.toml")).unwrap(), before);
        assert!(!f.dir.path().join("target").exists());
    }
    let (out, value) = f.json(&["check", "--output", "json"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(value["outcome"], "FINDINGS");
    assert_eq!(
        fs::read(f.dir.path().join("Cargo.lock")).unwrap(),
        b"sentinel lockfile\n"
    );
}

#[test]
fn apply_requires_acceptance_and_preserves_lockfile() {
    let f = Fixture::new();
    let before = fs::read(f.dir.path().join("Cargo.toml")).unwrap();
    let (out, value) = f.json(&["apply", "--output", "json"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(value["outcome"], "CANCELLED");
    assert_eq!(fs::read(f.dir.path().join("Cargo.toml")).unwrap(), before);
    let (out, value) = f.json(&["apply", "--yes", "--output", "json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(value["outcome"], "APPLIED");
    let after = fs::read_to_string(f.dir.path().join("Cargo.toml")).unwrap();
    assert!(!after.contains("unused ="));
    assert!(after.contains("used =") && after.starts_with("# preserve me"));
    assert_eq!(
        fs::read(f.dir.path().join("Cargo.lock")).unwrap(),
        b"sentinel lockfile\n"
    );
}

#[test]
fn configuration_and_global_selectors_work_after_subcommands() {
    let f = Fixture::new();
    fs::write(
        f.dir.path().join("custom.toml"),
        "[machete]\nignored = ['unused']\n",
    )
    .unwrap();
    let (out, value) = f.json(&[
        "check",
        "--config",
        "custom.toml",
        "--kinds",
        "normal",
        "--packages",
        "fixture",
        "--output",
        "json",
    ]);
    assert!(
        out.status.success(),
        "{} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(value["outcome"], "CLEAN");
    assert!(
        value["report"]["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["status"] == "IGNORED")
    );
}

#[test]
fn failures_emit_structured_json_and_do_not_mutate() {
    let f = Fixture::new();
    fs::write(
        f.dir.path().join("clippier.toml"),
        "[machete]\nunknown = true\n",
    )
    .unwrap();
    let (out, value) = f.json(&["plan", "--output", "json"]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(value["outcome"], "FAILED");
    assert!(value["error"].as_str().unwrap().contains("unknown"));
    assert!(!f.dir.path().join("target").exists());
}
