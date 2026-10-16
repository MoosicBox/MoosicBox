#![cfg(all(unix, feature = "format", feature = "check"))]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn write(root: &Path, path: &str, contents: impl AsRef<[u8]>) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn tool(root: &Path, name: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = root.join(format!("fake-{name}"));
    std::fs::write(
        &path,
        format!("#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$(dirname \"$0\")/{name}-invocations\"\n"),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn run(root: &Path, command: &str, tools: &[&str], automatic: bool) -> Output {
    let mut process = Command::new(env!("CARGO_BIN_EXE_clippier"));
    process
        .arg(command)
        .arg("--working-dir")
        .arg(root)
        .args(["--no-tui", "--output", "json"]);
    if command == "fmt" {
        process.args(["--scope", "all", "--check"]);
    }
    if !automatic {
        process.arg("--tools").arg(tools.join(","));
    }
    for name in tools {
        process.arg("--tool-path").arg(format!(
            "{name}={}",
            root.join(format!("fake-{name}")).display()
        ));
    }
    process.output().unwrap()
}

fn success(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn invocations(root: &Path, name: &str) -> String {
    std::fs::read_to_string(root.join(format!("{name}-invocations"))).unwrap_or_default()
}

#[test]
fn prettier_native_ignores_filter_before_binary_probing_in_both_commands() {
    for command in ["fmt", "check"] {
        for automatic in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path();
            write(root, ".prettierrc", "{}\n");
            write(root, ".prettierignore", "generated/\n");
            write(root, "document.md", "# Document\n");
            write(root, "generated/secret.md", b"\0binary generated content");
            tool(root, "prettier");
            let json = success(&run(root, command, &["prettier"], automatic));
            let args = invocations(root, "prettier");
            assert!(args.lines().any(|arg| arg == "document.md"), "{args}");
            assert!(!args.contains("secret.md"), "{args}");
            assert_eq!(json["plan"]["inventory"]["binary_files_excluded"], 0);
        }
    }
}

#[test]
fn taplo_include_and_exclude_select_files_in_fmt_and_check() {
    for command in ["fmt", "check"] {
        for automatic in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path();
            write(
                root,
                ".taplo.toml",
                "include = ['config/**/*.toml']\nexclude = ['config/generated/**']\n",
            );
            write(root, "config/application.toml", "value = 1\n");
            write(root, "config/generated/secret.toml", b"\0generated");
            write(root, "outside.toml", "value = 2\n");
            tool(root, "taplo");
            let json = success(&run(root, command, &["taplo"], automatic));
            let args = invocations(root, "taplo");
            assert!(
                args.lines().any(|arg| arg == "config/application.toml"),
                "{args}"
            );
            assert!(!args.contains("secret.toml"), "{args}");
            assert!(!args.contains("outside.toml"), "{args}");
            assert_eq!(json["plan"]["inventory"]["binary_files_excluded"], 0);
        }
    }
}

#[test]
fn explicitly_empty_native_includes_do_not_launch_the_formatter() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(root, ".taplo.toml", "include = []\n");
    write(root, "application.toml", "value = 1\n");
    tool(root, "taplo");
    let json = success(&run(root, "fmt", &["taplo"], false));
    assert!(invocations(root, "taplo").is_empty());
    assert_eq!(json["plan"]["inventory"]["binary_files_probed"], 0);
}

#[test]
fn prettier_ignores_do_not_hide_taplo_work_in_the_same_directory() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(root, ".prettierignore", "generated/\n");
    write(root, "generated/reference.md", "# Reference\n");
    write(root, "generated/application.toml", "value = 1\n");
    tool(root, "prettier");
    tool(root, "taplo");
    success(&run(root, "fmt", &["prettier", "taplo"], false));
    assert!(!invocations(root, "prettier").contains("reference.md"));
    assert!(invocations(root, "taplo").contains("generated/application.toml"));
}

#[test]
fn native_negation_and_clippier_boundaries_are_both_respected() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(root, ".prettierignore", "*.md\n!keep.md\n!private.md\n");
    write(
        root,
        "clippier.toml",
        "[runner.scope]\nexclude = ['private.md']\n",
    );
    write(root, "keep.md", "# Keep\n");
    write(root, "skip.md", "# Skip\n");
    write(root, "private.md", "# Private\n");
    tool(root, "prettier");
    success(&run(root, "fmt", &["prettier"], false));
    let args = invocations(root, "prettier");
    assert!(args.lines().any(|arg| arg == "keep.md"), "{args}");
    assert!(!args.contains("skip.md"), "{args}");
    assert!(!args.contains("private.md"), "{args}");
}

#[test]
fn python_native_selectors_are_applied_before_explicit_file_invocations() {
    for command in ["fmt", "check"] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        write(
            root,
            "pyproject.toml",
            "[tool.ruff]\ninclude = ['src/*.py']\nexclude = ['src/generated/**']\n[tool.black]\ninclude = '\\.py$'\nextend-exclude = '/generated/'\n",
        );
        write(root, "src/application.py", "value = 1\n");
        write(root, "src/generated/secret.py", b"\0generated");
        write(root, "outside.py", "value = 2\n");
        tool(root, "ruff");
        tool(root, "black");
        let json = success(&run(root, command, &["ruff", "black"], false));
        let ruff = invocations(root, "ruff");
        assert!(ruff.contains("src/application.py"), "{ruff}");
        assert!(!ruff.contains("secret.py"), "{ruff}");
        assert!(!ruff.contains("outside.py"), "{ruff}");
        if command == "fmt" {
            assert!(ruff.lines().any(|arg| arg == "format"), "{ruff}");
            assert!(ruff.lines().any(|arg| arg == "--check"), "{ruff}");
        }
        let black = invocations(root, "black");
        assert!(black.contains("src/application.py"), "{black}");
        assert!(!black.contains("secret.py"), "{black}");
        assert!(black.contains("outside.py"), "{black}");
        assert_eq!(json["plan"]["inventory"]["binary_files_excluded"], 0);
    }
}

#[test]
fn biome_empty_native_includes_survive_default_registry_flags() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(root, "biome.json", "{\"files\":{\"includes\":[]}}\n");
    write(root, "application.js", b"\0generated");
    tool(root, "biome");
    let json = success(&run(root, "fmt", &["biome"], false));
    assert!(invocations(root, "biome").is_empty());
    assert_eq!(json["plan"]["inventory"]["binary_files_probed"], 0);
}

#[test]
fn invalid_known_native_configuration_fails_before_invocation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(root, ".taplo.toml", "include = [\n");
    write(root, "application.toml", "value = 1\n");
    tool(root, "taplo");
    let output = run(root, "fmt", &["taplo"], false);
    assert!(!output.status.success());
    assert!(invocations(root, "taplo").is_empty());
    let diagnostic = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(diagnostic.contains("taplo"), "{diagnostic}");
}
