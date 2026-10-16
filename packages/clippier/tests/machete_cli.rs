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

#[cfg(unix)]
#[test]
fn ordinary_operations_never_launch_cargo() {
    use std::os::unix::fs::PermissionsExt;

    for args in [
        vec!["analyze"],
        vec!["check"],
        vec!["plan", "--diff"],
        vec!["apply", "--yes"],
    ] {
        let f = Fixture::new();
        let bin = f.dir.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let sentinel = bin.join("cargo");
        fs::write(
            &sentinel,
            "#!/bin/sh\nprintf invoked > cargo-was-invoked\nexit 99\n",
        )
        .unwrap();
        fs::set_permissions(&sentinel, fs::Permissions::from_mode(0o755)).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_clippier"))
            .current_dir(f.dir.path())
            .env("PATH", &bin)
            .arg("machete")
            .args(&args)
            .args(["--output", "json"])
            .output()
            .unwrap();
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_ne!(value["outcome"], "FAILED", "{args:?}: {value}");
        assert!(!f.dir.path().join("cargo-was-invoked").exists(), "{args:?}");
        assert!(!f.dir.path().join("target").exists());
    }
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

#[test]
fn text_output_prioritizes_findings_and_keeps_suppressed_rows_in_verbose_only() {
    let f = Fixture::new();
    fs::write(
        f.dir.path().join("clippier.toml"),
        "[machete]\nignored=['used']\n",
    )
    .unwrap();
    let output = f.run(&[]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.starts_with("Machete | FINDINGS\n"));
    assert!(text.contains("1 unused, 0 unverified, 0 used, 1 ignored"));
    assert!(text.contains("Unused dependencies:\n  fixture (Cargo.toml)\n    unused [normal]"));
    assert!(!text.contains("Ignored:"));
    assert!(!text.contains(&f.dir.path().display().to_string()));
    let verbose = String::from_utf8(f.run(&["--verbose"]).stdout).unwrap();
    assert!(verbose.contains("used (Normal): Ignored"));
    assert!(verbose.contains("unused (Normal): Unused"));
}

#[test]
fn text_output_groups_repeated_diagnostics_without_claiming_incomplete_is_clean() {
    let f = Fixture::new();
    let mut source = String::new();
    for i in 0..30 {
        source.push_str(&format!("mod m{i};\n"));
        fs::write(
            f.dir.path().join(format!("src/m{i}.rs")),
            "pub fn run(){ unknown::emit!(); }\n",
        )
        .unwrap();
    }
    fs::write(f.dir.path().join("src/lib.rs"), source).unwrap();
    let output = f.run(&[]);
    assert_eq!(output.status.code(), Some(2));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Machete | INCOMPLETE"));
    assert!(text.contains("Unverified dependencies (not removable)"));
    assert!(
        text.contains("30 limitations, 30 advisories")
            || text.contains("0 limitations, 30 advisories"),
        "{text}"
    );
    assert!(text.contains("Unexpanded macros/attributes"));
    assert!(text.contains("Apply is blocked"));
    assert!(text.contains("--verbose"));
    assert!(text.lines().count() < 30, "{text}");
    assert!(!text.contains("src/m0.rs"));
    let verbose = f.run(&["--verbose"]);
    assert_eq!(verbose.status.code(), Some(2));
    let verbose = String::from_utf8(verbose.stdout).unwrap();
    assert!(verbose.contains("src/m0.rs"));
    let (_, json) = f.json(&["--output", "json"]);
    assert_eq!(json["report"]["diagnostics"].as_array().unwrap().len(), 30);
    let (plain_output, plain_json) = f.json(&["--output", "json"]);
    let (verbose_output, verbose_json) = f.json(&["--verbose", "--output", "json"]);
    assert_eq!(plain_output.status.code(), verbose_output.status.code());
    assert_eq!(plain_json, verbose_json);
    assert_eq!(json["report"]["complete"], false);
}

#[test]
fn text_clean_plan_diff_and_explanation_remain_distinct() {
    let f = Fixture::new();
    fs::write(
        f.dir.path().join("src/lib.rs"),
        "pub fn run(){used::run(); unused::run();}\n",
    )
    .unwrap();
    let text = String::from_utf8(f.run(&[]).stdout).unwrap();
    assert!(text.starts_with("Machete | CLEAN\n"));
    assert!(text.contains("No unused dependencies identified."));
    assert!(!text.contains("Analysis is incomplete"));
    let explained = String::from_utf8(f.run(&["--explain"]).stdout).unwrap();
    assert!(explained.contains("Evidence:"));
    fs::write(
        f.dir.path().join("src/lib.rs"),
        "pub fn run(){used::run();}\n",
    )
    .unwrap();
    let plan = f.run(&["plan", "--diff"]);
    let text = String::from_utf8(plan.stdout).unwrap();
    assert!(text.starts_with("Machete | PLANNED\n"));
    assert!(text.contains("Unused dependencies:"));
    assert!(text.contains("--- "));
    assert!(text.contains("-unused"));
}

#[test]
fn bmux_report_colors_are_explicit_and_never_leak_into_json_or_plain_output() {
    let f = Fixture::new();
    let forced = f.run(&["--color", "always"]);
    assert!(forced.status.success());
    let forced = String::from_utf8(forced.stdout).unwrap();
    assert!(forced.contains("\x1b["));
    // BMUX emits styled inline rows, not an alternate screen or cursor control session.
    assert!(!forced.contains("\x1b[?1049"));
    assert!(!forced.contains("\x1b[?25"));
    let lines = bmux_tui::ansi::ansi_to_lines(&forced);
    let restored = lines
        .iter()
        .map(bmux_tui::text::Line::plain_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(restored.contains("Machete | FINDINGS"));
    assert!(restored.contains("unused [normal]"));
    for args in [vec!["--color", "never"], vec!["--color", "auto"]] {
        let output = f.run(&args);
        assert!(output.status.success());
        assert!(!output.stdout.contains(&0x1b));
    }
    let (_, plain) = f.json(&["--output", "json"]);
    let (forced, json) = f.json(&["--color", "always", "--output", "json"]);
    assert!(!forced.stdout.contains(&0x1b));
    assert_eq!(json, plain);
    // An explicit CLI color choice overrides NO_COLOR; auto still honors it.
    for (mode, expected) in [("always", true), ("never", false), ("auto", false)] {
        let output = Command::new(env!("CARGO_BIN_EXE_clippier"))
            .current_dir(f.dir.path())
            .env("NO_COLOR", "1")
            .args(["machete", "--color", mode])
            .output()
            .unwrap();
        assert_eq!(output.stdout.contains(&0x1b), expected);
    }
}

#[test]
fn progress_is_absent_from_redirected_text_and_json_even_with_forced_colors() {
    let f = Fixture::new();
    for args in [
        vec![],
        vec!["--color", "always"],
        vec!["--no-progress"],
        vec!["--output", "json"],
    ] {
        let output = f.run(&args);
        assert!(output.status.success());
        assert!(output.stderr.is_empty(), "{:?}", output.stderr);
        assert!(!String::from_utf8_lossy(&output.stdout).contains("Discovering Cargo"));
    }
}
