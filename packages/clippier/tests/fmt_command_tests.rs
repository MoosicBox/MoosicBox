#![cfg(feature = "format")]

use std::path::{Path, PathBuf};
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

#[cfg(unix)]
fn write_successful_tool(root: &Path, name: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = root.join(name);
    std::fs::write(
        &path,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$(dirname \"$0\")/tool-invocation\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(unix)]
#[test]
fn explicit_tool_path_skip_check_json_and_raw_controls_complete_cli_paths() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join(".prettierrc"), "{}\n").unwrap();
    std::fs::write(temp.path().join("document.md"), "# test\n").unwrap();
    let prettier = write_successful_tool(temp.path(), "fake-prettier");
    let tool_path = format!("prettier={}", prettier.display());

    let checked = Command::new(env!("CARGO_BIN_EXE_clippier"))
        .arg("fmt")
        .arg("--working-dir")
        .arg(temp.path())
        .arg("--tools")
        .arg("prettier")
        .arg("--tool-path")
        .arg(&tool_path)
        .arg("--scope")
        .arg("all")
        .arg("--check")
        .arg("--no-tui")
        .arg("--output")
        .arg("json")
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    assert_eq!(json["total"], 1);
    assert_eq!(json["results"][0]["name"], "prettier");
    assert_eq!(json["results"][0]["success"], true);
    let args = std::fs::read_to_string(temp.path().join("tool-invocation")).unwrap();
    assert!(args.lines().any(|arg| arg == "--check"));
    assert!(args.lines().any(|arg| arg == "document.md"));

    std::fs::remove_file(temp.path().join("tool-invocation")).unwrap();
    let skipped = Command::new(env!("CARGO_BIN_EXE_clippier"))
        .arg("fmt")
        .arg("--working-dir")
        .arg(temp.path())
        .arg("--tool-path")
        .arg(&tool_path)
        .arg("--skip")
        .arg("prettier")
        .arg("--scope")
        .arg("all")
        .arg("--output")
        .arg("json")
        .output()
        .unwrap();
    assert!(skipped.status.success());
    let skipped_json: serde_json::Value = serde_json::from_slice(&skipped.stdout).unwrap();
    assert_eq!(skipped_json["total"], 0);
    assert!(!temp.path().join("tool-invocation").exists());

    let raw = Command::new(env!("CARGO_BIN_EXE_clippier"))
        .arg("fmt")
        .arg("--working-dir")
        .arg(temp.path())
        .arg("--tools")
        .arg("prettier")
        .arg("--tool-path")
        .arg(tool_path)
        .arg("--scope")
        .arg("all")
        .arg("--no-tui")
        .arg("--output")
        .arg("raw")
        .output()
        .unwrap();
    assert!(raw.status.success());
    assert!(String::from_utf8_lossy(&raw.stdout).contains("Prettier"));
    assert!(temp.path().join("tool-invocation").exists());
}

#[cfg(unix)]
#[test]
fn representative_ecosystem_fixtures_complete_check_and_fmt_product_paths() {
    struct Fixture {
        label: &'static str,
        tool: &'static str,
        signal: &'static str,
        source: &'static str,
    }

    for fixture in [
        Fixture {
            label: "rust",
            tool: "taplo",
            signal: "Cargo.toml",
            source: "project.toml",
        },
        Fixture {
            label: "node",
            tool: "prettier",
            signal: ".prettierrc",
            source: "application.js",
        },
        Fixture {
            label: "python",
            tool: "ruff",
            signal: "ruff.toml",
            source: "application.py",
        },
        Fixture {
            label: "go",
            tool: "gofmt",
            signal: "go.mod",
            source: "application.go",
        },
        Fixture {
            label: "lua",
            tool: "stylua",
            signal: "stylua.toml",
            source: "application.lua",
        },
        Fixture {
            label: "terraform",
            tool: "terraform",
            signal: ".terraform.lock.hcl",
            source: "main.tf",
        },
        Fixture {
            label: "markdown",
            tool: "mdformat",
            signal: ".mdformat.toml",
            source: "README.md",
        },
        Fixture {
            label: "clang",
            tool: "clang-format",
            signal: ".clang-format",
            source: "application.cpp",
        },
    ] {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join(fixture.signal), "{}\n").unwrap();
        std::fs::write(temp.path().join(fixture.source), "fixture\n").unwrap();
        let executable = write_successful_tool(temp.path(), fixture.tool);
        let tool_path = format!("{}={}", fixture.tool, executable.display());

        for (command, extra_args) in [("fmt", vec!["--scope", "all"]), ("check", vec![])] {
            let output = Command::new(env!("CARGO_BIN_EXE_clippier"))
                .arg(command)
                .arg("--working-dir")
                .arg(temp.path())
                .arg("--tool-path")
                .arg(&tool_path)
                .args(extra_args)
                .arg("--no-tui")
                .arg("--output")
                .arg("json")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{} {command}: {}",
                fixture.label,
                String::from_utf8_lossy(&output.stderr)
            );
            let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert!(
                json["results"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|result| result["name"] == fixture.tool && result["success"] == true),
                "{} {command} did not execute {}: {json}",
                fixture.label,
                fixture.tool
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn mixed_monorepo_resolves_nested_configs_ownership_linters_exclusions_and_overrides() {
    let temp = tempfile::tempdir().unwrap();
    let web = temp.path().join("packages/web");
    let python = temp.path().join("packages/python");
    std::fs::create_dir_all(web.join("node_modules")).unwrap();
    std::fs::create_dir_all(&python).unwrap();
    std::fs::write(web.join("package.json"), "{}\n").unwrap();
    std::fs::write(web.join("biome.json"), "{}\n").unwrap();
    std::fs::write(web.join(".prettierrc"), "{}\n").unwrap();
    std::fs::write(web.join("eslint.config.js"), "export default [];\n").unwrap();
    std::fs::write(web.join("application.js"), "const value=1;\n").unwrap();
    std::fs::write(web.join("README.md"), "# Web\n").unwrap();
    std::fs::write(web.join("node_modules/generated.js"), "generated\n").unwrap();
    std::fs::write(
        python.join("pyproject.toml"),
        "[project]\nname='fixture'\n[tool.ruff]\nline-length=100\n",
    )
    .unwrap();
    std::fs::write(python.join("application.py"), "value=1\n").unwrap();

    let mut tool_paths = Vec::new();
    for tool in ["biome", "prettier", "eslint", "ruff"] {
        let executable = write_successful_tool(temp.path(), &format!("fake-{tool}"));
        tool_paths.push(format!("{tool}={}", executable.display()));
    }
    let mut command = Command::new(env!("CARGO_BIN_EXE_clippier"));
    command
        .arg("check")
        .arg("--working-dir")
        .arg(temp.path())
        .arg("--skip")
        .arg("prettier")
        .arg("--no-tui")
        .arg("--output")
        .arg("json");
    for tool_path in &tool_paths {
        command.arg("--tool-path").arg(tool_path);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let names = json["results"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|result| result["name"].as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"biome"));
    assert!(names.contains(&"eslint"));
    assert!(names.contains(&"ruff"));
    assert!(!names.contains(&"prettier"));
    assert_eq!(
        json["plan"]["selection_evidence"]["biome"],
        "NativeConfig:packages/web/biome.json"
    );
    assert_eq!(
        json["plan"]["selection_evidence"]["ruff"],
        "NativeConfig:packages/python/pyproject.toml"
    );
    assert!(
        json["plan"]["formatter_ownership"]["biome"]
            .as_array()
            .unwrap()
            .iter()
            .any(|extension| extension == "js")
    );
    assert!(
        json["plan"]["automatic_exclusions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|pattern| pattern == "packages/web/node_modules/**")
    );
}

#[cfg(unix)]
fn write_failing_acquisition_runner(root: &Path, name: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = root.join(name);
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' '{name}' >> \"$(dirname \"$0\")/acquisition-invocations\"\nexit 99\n"
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(unix)]
#[test]
fn default_fmt_and_list_never_invoke_acquisition_runners() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join(".prettierrc"), "{}\n").unwrap();
    std::fs::write(temp.path().join("document.md"), "# test\n").unwrap();
    for runner in ["bunx", "pnpm", "npx", "uvx", "nix"] {
        write_failing_acquisition_runner(temp.path(), runner);
    }

    let output = Command::new(env!("CARGO_BIN_EXE_clippier"))
        .arg("fmt")
        .arg("--working-dir")
        .arg(temp.path())
        .arg("--output")
        .arg("json")
        .env("PATH", temp.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 0);
    let unavailable = json["plan"]["unavailable_tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect::<Vec<_>>();
    assert!(unavailable.contains(&"prettier"));

    let checked = Command::new(env!("CARGO_BIN_EXE_clippier"))
        .arg("check")
        .arg("--working-dir")
        .arg(temp.path())
        .arg("--output")
        .arg("json")
        .env("PATH", temp.path())
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let check_json: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    assert_eq!(check_json["total"], 0);
    assert!(
        check_json["plan"]["unavailable_tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool == "prettier")
    );

    let listed = Command::new(env!("CARGO_BIN_EXE_clippier"))
        .arg("fmt")
        .arg("--working-dir")
        .arg(temp.path())
        .arg("--list")
        .arg("--output")
        .arg("json")
        .env("PATH", temp.path())
        .output()
        .unwrap();
    assert!(
        listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let list: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    let prettier = list
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "prettier")
        .unwrap();
    assert_eq!(prettier["relevant"], true);
    assert_eq!(prettier["available"], false);
    assert_eq!(prettier["selected"], false);

    let required = Command::new(env!("CARGO_BIN_EXE_clippier"))
        .arg("fmt")
        .arg("--working-dir")
        .arg(temp.path())
        .arg("--required")
        .arg("prettier")
        .arg("--output")
        .arg("json")
        .env("PATH", temp.path())
        .output()
        .unwrap();
    assert!(!required.status.success());
    assert!(
        String::from_utf8_lossy(&required.stderr).contains("Required tool 'prettier' not found")
    );
    assert!(!temp.path().join("acquisition-invocations").exists());
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
