use std::path::PathBuf;
use std::process::Command;

fn temp_dir(prefix: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time before epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("{prefix}-{nanos}"));
    std::fs::create_dir_all(&dir).expect("failed to create temp directory");
    dir
}

#[test]
fn cli_json_absolute_directory_and_explicit_file_modes() {
    let dir = temp_dir("clippier-md-cli-modes");
    let nested = dir.join("nested");
    std::fs::create_dir_all(&nested).expect("failed to create nested fixture directory");
    let changed = nested.join("changed.md");
    let clean = nested.join("clean.md");
    let excluded = nested.join("excluded.md");
    std::fs::write(&changed, "one two three four\n").expect("failed to write changed fixture");
    std::fs::write(&clean, "short\n").expect("failed to write clean fixture");
    std::fs::write(&excluded, "one two three four\n").expect("failed to write excluded fixture");
    let config_path = dir.join("clippier-md.toml");
    std::fs::write(
        &config_path,
        "line-width = 10\n\n[files]\nexclude = [\"/nested/excluded.md\"]\n",
    )
    .expect("failed to write mode config");
    let binary = env!("CARGO_BIN_EXE_clippier-md");

    let canonical_nested = nested
        .canonicalize()
        .expect("failed to canonicalize directory");
    let directory_check = Command::new(binary)
        .args([
            "fmt",
            "--check",
            "--no-diff",
            "--output",
            "json",
            "--config",
        ])
        .arg(&config_path)
        .arg(&canonical_nested)
        .output()
        .expect("failed to run absolute directory check");
    assert_eq!(directory_check.status.code(), Some(1));
    let summary: serde_json::Value =
        serde_json::from_slice(&directory_check.stdout).expect("invalid JSON summary");
    assert_eq!(summary["checked"], 2);
    assert_eq!(summary["changed_count"], 1);
    let expected_changed = canonical_nested.join("changed.md");
    assert_eq!(
        summary["changed"][0],
        expected_changed.display().to_string()
    );

    let explicit_write = Command::new(binary)
        .args(["fmt", "--config"])
        .arg(&config_path)
        .arg(&changed)
        .output()
        .expect("failed to run explicit-file write");
    assert!(explicit_write.status.success());
    assert_eq!(
        std::fs::read_to_string(&changed).expect("failed to read written fixture"),
        "one two\nthree four\n"
    );
    assert_eq!(
        std::fs::read_to_string(&excluded).expect("failed to read excluded fixture"),
        "one two three four\n"
    );

    std::fs::remove_dir_all(dir).expect("failed to clean mode fixtures");
}

#[test]
fn cli_check_and_write_cover_exit_status_and_file_output() {
    let dir = temp_dir("clippier-md-cli");
    let path = dir.join("input.md");
    let input = format!("{} {}\n", "word".repeat(12), "tail".repeat(12));
    std::fs::write(&path, &input).expect("failed to write markdown fixture");
    let config_path = dir.join("clippier-md.toml");
    std::fs::write(
        &config_path,
        "line-width = 80\n\n[prose]\nwrap = \"always\"\n",
    )
    .expect("failed to write formatter config");
    let binary = env!("CARGO_BIN_EXE_clippier-md");

    let check = Command::new(binary)
        .args(["fmt", "--check", "--color", "never", "--config"])
        .arg(&config_path)
        .arg(&path)
        .output()
        .expect("failed to run clippier-md check");
    assert_eq!(check.status.code(), Some(1));
    let check_stdout = String::from_utf8(check.stdout).expect("check stdout was not UTF-8");
    assert!(check_stdout.contains("--- a/"));
    assert!(check_stdout.contains("+++ b/"));
    assert_eq!(
        std::fs::read_to_string(&path).expect("failed to read checked fixture"),
        input
    );

    let write = Command::new(binary)
        .args(["fmt", "--config"])
        .arg(&config_path)
        .arg(&path)
        .output()
        .expect("failed to run clippier-md write");
    assert!(write.status.success());
    let formatted = std::fs::read_to_string(&path).expect("failed to read formatted fixture");
    assert_ne!(formatted, input);

    let clean_check = Command::new(binary)
        .args(["fmt", "--check", "--no-diff", "--config"])
        .arg(&config_path)
        .arg(&path)
        .output()
        .expect("failed to rerun clippier-md check");
    assert!(clean_check.status.success());

    std::fs::remove_dir_all(dir).expect("failed to clean temp directory");
}
