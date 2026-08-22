use std::{fs, path::Path, process::Command};

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use tempfile::TempDir;

fn workspace(files: &[(&str, &str)]) -> TempDir {
    let directory = tempfile::tempdir().unwrap();
    for (path, contents) in files {
        let path = directory.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
    directory
}

fn run(directory: &Path, args: &[&str]) -> std::process::Output {
    cargo_bin_cmd!("bloaty")
        .current_dir(directory)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn analyzes_binary_features_combinations_and_custom_profiles() {
    let workspace = workspace(&[
        (
            "Cargo.toml",
            r#"[workspace]
members = ["app"]
resolver = "2"

[profile.small]
inherits = "release"
opt-level = "z"
"#,
        ),
        (
            "app/Cargo.toml",
            r#"[package]
name = "fixture-app"
version = "0.1.0"
edition = "2024"

[[bin]]
name = "different-bin-name"
path = "src/main.rs"

[features]
default = ["alpha"]
alpha = []
beta = []
"#,
        ),
        (
            "app/src/main.rs",
            r#"fn main() {
    #[cfg(feature = "alpha")]
    println!("alpha {}", include_str!("alpha.txt"));
    #[cfg(feature = "beta")]
    println!("beta {}", include_str!("beta.txt"));
}
"#,
        ),
        ("app/src/alpha.txt", &"a".repeat(2_000)),
        ("app/src/beta.txt", &"b".repeat(3_000)),
    ]);
    let report = workspace.path().join("report");
    let output = run(
        workspace.path(),
        &[
            "--package",
            "fixture-app",
            "--target",
            "different-bin-name",
            "--profile",
            "small",
            "--baseline",
            "none",
            "--feature",
            "alpha",
            "--scenario",
            "both=alpha,beta",
            "--output-format",
            "json",
            "--report-file",
            report.to_str().unwrap(),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: Value =
        serde_json::from_slice(&fs::read(report.with_extension("json")).unwrap()).unwrap();
    assert_eq!(report["profile"], "small");
    assert_eq!(report["target_name"], "different-bin-name");
    assert_eq!(report["baseline"]["status"], "success");
    assert_eq!(report["comparisons"][0]["status"], "success");
    assert_eq!(report["comparisons"][1]["status"], "success");
    assert!(
        report["comparisons"][1]["scenario"]["config"]["features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|feature| feature == "beta")
    );
}

#[test]
fn rejects_packages_without_supported_final_artifacts() {
    let workspace = workspace(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"library\"]\nresolver = \"2\"\n",
        ),
        (
            "library/Cargo.toml",
            "[package]\nname = \"library-only\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[features]\nalpha = []\n",
        ),
        ("library/src/lib.rs", "pub fn value() -> u8 { 1 }"),
    ]);
    let output = run(
        workspace.path(),
        &["--package", "library-only", "--feature", "alpha"],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("supported target"));
}

#[test]
fn reports_ambiguous_targets() {
    let workspace = workspace(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"app\"]\nresolver = \"2\"\n",
        ),
        (
            "app/Cargo.toml",
            r#"[package]
name = "multi-bin"
version = "0.1.0"
edition = "2024"

[[bin]]
name = "one"
path = "src/one.rs"

[[bin]]
name = "two"
path = "src/two.rs"

[features]
alpha = []
"#,
        ),
        ("app/src/one.rs", "fn main() {}"),
        ("app/src/two.rs", "fn main() {}"),
    ]);
    let output = run(
        workspace.path(),
        &["--package", "multi-bin", "--feature", "alpha"],
    );
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("multiple supported targets"));
    assert!(error.contains("one, two"));
}

#[test]
fn preserves_build_failures_without_stale_measurements() {
    let workspace = workspace(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"app\"]\nresolver = \"2\"\n",
        ),
        (
            "app/Cargo.toml",
            r#"[package]
name = "failing-app"
version = "0.1.0"
edition = "2024"

[features]
broken = []
"#,
        ),
        (
            "app/src/main.rs",
            r#"fn main() {
    #[cfg(feature = "broken")]
    compile_error!("requested failure");
}
"#,
        ),
    ]);
    let report = workspace.path().join("failure");
    let output = run(
        workspace.path(),
        &[
            "--package",
            "failing-app",
            "--baseline",
            "none",
            "--feature",
            "broken",
            "--output-format",
            "json",
            "--report-file",
            report.to_str().unwrap(),
        ],
    );
    assert!(output.status.success());
    let report: Value =
        serde_json::from_slice(&fs::read(report.with_extension("json")).unwrap()).unwrap();
    assert_eq!(report["baseline"]["status"], "success");
    assert_eq!(report["comparisons"][0]["status"], "failed");
    assert!(report["comparisons"][0].get("measurement").is_none());
    assert!(
        report["comparisons"][0]["error"]
            .as_str()
            .unwrap()
            .contains("Cargo build failed")
    );
}

#[test]
fn honors_target_required_features_through_cargo() {
    let workspace = workspace(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"app\"]\nresolver = \"2\"\n",
        ),
        (
            "app/Cargo.toml",
            r#"[package]
name = "required-app"
version = "0.1.0"
edition = "2024"

[[bin]]
name = "required-app"
path = "src/main.rs"
required-features = ["runtime"]

[features]
runtime = []
extra = []
"#,
        ),
        ("app/src/main.rs", "fn main() {}"),
    ]);
    let report = workspace.path().join("required");
    let output = run(
        workspace.path(),
        &[
            "--package",
            "required-app",
            "--baseline",
            "runtime",
            "--feature",
            "extra",
            "--output-format",
            "json",
            "--report-file",
            report.to_str().unwrap(),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value =
        serde_json::from_slice(&fs::read(report.with_extension("json")).unwrap()).unwrap();
    assert_eq!(report["baseline"]["status"], "success");
    assert_eq!(report["comparisons"][0]["status"], "success");
}

#[test]
fn rejects_incompatible_saved_reports() {
    let workspace = workspace(&[]);
    let release = workspace.path().join("release.json");
    let dev = workspace.path().join("dev.json");
    let report = |profile: &str| {
        serde_json::json!({
            "schema_version": 1,
            "started_at": 0,
            "package": "app",
            "target_name": "app",
            "target_kind": "binary",
            "profile": profile,
            "compilation_target": null,
            "environment": {
                "rustc": "rustc 1",
                "cargo": "cargo 1",
                "host_os": "linux",
                "host_arch": "x86_64",
                "git_revision": null,
                "git_dirty": null
            },
            "baseline": {
                "scenario": {"name": "baseline", "config": {"default_features": false, "features": []}},
                "status": "success",
                "measurement": {"artifact_path": "app", "size_bytes": 1, "delta_bytes": null, "delta_percent": null, "fresh": false}
            },
            "comparisons": []
        })
    };
    fs::write(&release, serde_json::to_vec(&report("release")).unwrap()).unwrap();
    fs::write(&dev, serde_json::to_vec(&report("dev")).unwrap()).unwrap();

    let output = run(
        workspace.path(),
        &[
            "--compare-reports",
            release.to_str().unwrap(),
            dev.to_str().unwrap(),
        ],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("reports are incompatible"));
}

#[test]
fn cargo_is_available_for_fixture_tests() {
    assert!(
        Command::new("cargo")
            .arg("--version")
            .status()
            .unwrap()
            .success()
    );
}
