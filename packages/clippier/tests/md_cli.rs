#![cfg(feature = "md")]

use std::process::Command;

#[test]
fn bundled_md_cli_checks_and_writes_without_cargo() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("guide.md");
    std::fs::write(&source, "#   Guide\n").unwrap();
    let binary = env!("CARGO_BIN_EXE_clippier");
    let check = Command::new(binary)
        .current_dir(root.path())
        .args(["md", "fmt", "--check", "--output", "json", "guide.md"])
        .output()
        .unwrap();
    assert_eq!(check.status.code(), Some(1), "{check:?}");
    let summary: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    assert_eq!(summary["changed_count"], 1);
    assert_eq!(std::fs::read_to_string(&source).unwrap(), "#   Guide\n");
    let write = Command::new(binary)
        .current_dir(root.path())
        .args(["md", "fmt", "guide.md"])
        .output()
        .unwrap();
    assert!(write.status.success(), "{write:?}");
    assert_eq!(std::fs::read_to_string(source).unwrap(), "# Guide\n");
}
