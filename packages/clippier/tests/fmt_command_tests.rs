#![cfg(feature = "format")]

use std::path::Path;
use std::process::Command;

use git2::{IndexAddOption, Repository, Signature};

fn commit_all(repo: &Repository, message: &str) {
    let mut index = repo.index().expect("failed to open index");
    index
        .add_all(["*"], IndexAddOption::DEFAULT, None)
        .expect("failed to add files");
    index.write().expect("failed to write index");
    let tree_id = index.write_tree().expect("failed to write tree");
    let tree = repo.find_tree(tree_id).expect("failed to find tree");
    let signature = Signature::now("Clippier Test", "clippier@example.com")
        .expect("failed to create signature");
    let parents = repo
        .head()
        .ok()
        .and_then(|head| head.peel_to_commit().ok())
        .into_iter()
        .collect::<Vec<_>>();
    let parent_refs = parents.iter().collect::<Vec<_>>();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &parent_refs,
    )
    .expect("failed to commit");
}

fn write_project(root: &Path) {
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fmt-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        "pub mod changed;\npub mod child;\npub mod control;\n",
    )
    .unwrap();
    std::fs::write(root.join("src/child.rs"), "pub fn child(){let value=1;}\n").unwrap();
    std::fs::write(root.join("src/changed.rs"), "pub fn changed() {}\n").unwrap();
    std::fs::write(
        root.join("src/control.rs"),
        "pub fn control(){let value=1;}\n",
    )
    .unwrap();
}

fn run_fmt(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_clippier"))
        .arg("fmt")
        .arg("--working-dir")
        .arg(root)
        .arg("--tools")
        .arg("rustfmt")
        .arg("--no-tui")
        .args(args)
        .output()
        .expect("failed to run clippier fmt")
}

#[test]
fn changed_check_and_all_scopes_complete_the_cli_product_path() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_project(root);
    let repo = Repository::init(root).unwrap();
    commit_all(&repo, "initial");

    std::fs::write(
        root.join("src/changed.rs"),
        "pub fn changed(){let value=1;}\n",
    )
    .unwrap();
    let control_before = std::fs::read(root.join("src/control.rs")).unwrap();
    let child_before = std::fs::read(root.join("src/child.rs")).unwrap();

    let check_before = std::fs::read(root.join("src/changed.rs")).unwrap();
    let output = run_fmt(root, &["--check", "--output", "json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["success"], false);
    assert_eq!(json["selection"]["mode"], "files");
    assert_eq!(json["selection"]["file_count"], 1);
    assert_eq!(
        std::fs::read(root.join("src/changed.rs")).unwrap(),
        check_before
    );

    let output = run_fmt(root, &["--output", "json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let write_json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        write_json["success"],
        true,
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/changed.rs")).unwrap(),
        "pub fn changed() {\n    let value = 1;\n}\n"
    );
    assert_eq!(
        std::fs::read(root.join("src/control.rs")).unwrap(),
        control_before
    );
    assert_eq!(
        std::fs::read(root.join("src/child.rs")).unwrap(),
        child_before
    );

    let output = run_fmt(root, &["--scope", "all", "--output", "json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/control.rs")).unwrap(),
        "pub fn control() {\n    let value = 1;\n}\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/child.rs")).unwrap(),
        "pub fn child() {\n    let value = 1;\n}\n"
    );
}

#[test]
fn clean_and_unsupported_changed_selections_are_successful_noops() {
    let temp = tempfile::tempdir().unwrap();
    write_project(temp.path());
    let repo = Repository::init(temp.path()).unwrap();
    commit_all(&repo, "initial");

    let clean = run_fmt(temp.path(), &["--output", "json"]);
    assert!(clean.status.success());
    let clean_json: serde_json::Value = serde_json::from_slice(&clean.stdout).unwrap();
    assert_eq!(clean_json["selection"]["file_count"], 0);
    assert_eq!(clean_json["total"], 0);

    std::fs::write(temp.path().join("notes.txt"), "changed\n").unwrap();
    let unsupported = run_fmt(temp.path(), &["--output", "json"]);
    assert!(unsupported.status.success());
    let unsupported_json: serde_json::Value = serde_json::from_slice(&unsupported.stdout).unwrap();
    assert_eq!(unsupported_json["selection"]["file_count"], 1);
    assert_eq!(unsupported_json["total"], 1);
    assert_eq!(unsupported_json["results"][0]["success"], true);
}

#[test]
fn default_scope_outside_git_warns_and_falls_back_to_all_files() {
    let temp = tempfile::tempdir().unwrap();
    write_project(temp.path());

    let output = run_fmt(temp.path(), &["--output", "json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["selection"]["mode"], "all");
    assert_eq!(json["selection"]["fallback"], true);
    assert!(String::from_utf8_lossy(&output.stderr).contains("falling back"));
    assert_eq!(
        std::fs::read_to_string(temp.path().join("src/control.rs")).unwrap(),
        "pub fn control() {\n    let value = 1;\n}\n"
    );
}
