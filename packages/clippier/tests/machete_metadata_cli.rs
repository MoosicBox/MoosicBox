#![cfg(feature = "machete")]
//! Explicit metadata mode remains offline/locked and ordinary analysis stays Cargo-free.
use std::{fs, process::Command};

#[test]
fn explicit_metadata_and_no_ignore_preserve_project_inputs() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::create_dir_all(dir.path().join("library/src")).unwrap();
    let manifest = "[package]\nname='consumer'\nversion='0.1.0'\nedition='2021'\n[workspace]\nmembers=['library']\n[dependencies]\nlibrary-package={path='library'}\n";
    fs::write(dir.path().join("Cargo.toml"), manifest).unwrap();
    fs::write(dir.path().join("library/Cargo.toml"), "[package]\nname='library-package'\nversion='0.1.0'\nedition='2021'\n[lib]\nname='custom_library'\n").unwrap();
    fs::write(
        dir.path().join("src/lib.rs"),
        "pub use custom_library::Thing;\n",
    )
    .unwrap();
    fs::write(dir.path().join("library/src/lib.rs"), "pub struct Thing;\n").unwrap();
    let setup = Command::new("cargo")
        .current_dir(dir.path())
        .args(["generate-lockfile", "--offline"])
        .output()
        .unwrap();
    assert!(
        setup.status.success(),
        "{}",
        String::from_utf8_lossy(&setup.stderr)
    );
    let lock = fs::read(dir.path().join("Cargo.lock")).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_clippier"))
        .current_dir(dir.path())
        .args([
            "machete",
            "check",
            "--with-metadata",
            "--no-ignore",
            "--output",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["outcome"], "CLEAN");
    assert_eq!(
        fs::read_to_string(dir.path().join("Cargo.toml")).unwrap(),
        manifest
    );
    assert_eq!(fs::read(dir.path().join("Cargo.lock")).unwrap(), lock);
}
