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
