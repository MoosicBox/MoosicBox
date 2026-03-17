use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::{io::Write as IoWrite, str};

use clippier_md::{
    Config, FormatterEngine, HeadingIndentationMode, ListIndentationMode, ProseWrapMode,
    format_markdown,
};

#[test]
fn prettier_parity_commonmark_gfm_fixtures() {
    assert_prettier_version();

    let fixtures_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("parity")
        .join("fixtures");
    let fixture_dirs = collect_fixture_dirs(&fixtures_root);

    assert!(!fixture_dirs.is_empty(), "No parity fixtures found");

    let config = Config {
        engine: FormatterEngine::Ast,
        prose_wrap: ProseWrapMode::Preserve,
        heading_indentation: HeadingIndentationMode::Normalize,
        list_indentation: ListIndentationMode::Preserve,
        ..Config::default()
    };

    for dir in fixture_dirs {
        if dir
            .components()
            .any(|part| part.as_os_str() == "frontmatter")
        {
            continue;
        }
        if dir
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "nested_checklist")
        {
            continue;
        }

        let input = read_fixture_file(&dir, "input").expect("missing input fixture");
        let prettier = run_prettier(&input.0, &input.1);
        let output = format_markdown(&input.1, &config);

        assert_eq!(
            output,
            prettier,
            "Parity mismatch for fixture '{}': input={:?}",
            dir.display(),
            input.0,
        );

        let idempotent = format_markdown(&output, &config);
        assert_eq!(
            idempotent,
            output,
            "Idempotence mismatch for fixture '{}'",
            dir.display()
        );
    }
}

#[test]
fn frontmatter_is_preserved_byte_for_byte() {
    assert_prettier_version();

    let fixtures_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("parity")
        .join("fixtures")
        .join("frontmatter");
    let fixture_dirs = collect_fixture_dirs(&fixtures_root);

    let config = Config {
        engine: FormatterEngine::Ast,
        prose_wrap: ProseWrapMode::Preserve,
        heading_indentation: HeadingIndentationMode::Normalize,
        list_indentation: ListIndentationMode::Preserve,
        ..Config::default()
    };

    for dir in fixture_dirs {
        let input = read_fixture_file(&dir, "input").expect("missing input fixture");
        let output = format_markdown(&input.1, &config);

        if let Some((frontmatter_input, _)) = split_frontmatter(&input.1)
            && let Some((frontmatter_output, _)) = split_frontmatter(&output)
        {
            assert_eq!(
                frontmatter_output,
                frontmatter_input,
                "Frontmatter changed for fixture '{}'",
                dir.display()
            );
        }
    }
}

#[test]
fn non_commonmark_regression_fixtures_are_stable() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("parity")
        .join("fixtures")
        .join("regressions")
        .join("nested_checklist");
    let input = read_fixture_file(&dir, "input").expect("missing input fixture");

    let config = Config {
        engine: FormatterEngine::Ast,
        prose_wrap: ProseWrapMode::Preserve,
        heading_indentation: HeadingIndentationMode::Preserve,
        list_indentation: ListIndentationMode::Preserve,
        ..Config::default()
    };

    let output = format_markdown(&input.1, &config);
    let second = format_markdown(&output, &config);
    assert_eq!(
        second, output,
        "non-commonmark fixture should be idempotent"
    );
}

fn collect_fixture_dirs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_fixture_dirs_at_path(root, &mut out);
    out.sort();
    out
}

fn collect_fixture_dirs_at_path(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let mut has_input = false;
    let mut directories = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            directories.push(path);
            continue;
        }

        if let Some(name) = path.file_name().and_then(|v| v.to_str())
            && name.starts_with("input.")
        {
            has_input = true;
        }
    }

    if has_input {
        out.push(root.to_path_buf());
    }

    for directory in directories {
        collect_fixture_dirs_at_path(&directory, out);
    }
}

fn assert_prettier_version() {
    let runner = prettier_runner();

    static CHECK: OnceLock<()> = OnceLock::new();
    CHECK.get_or_init(|| {
        let output = run_prettier_command(runner, &["--version"], None)
            .expect("Failed to execute prettier version check command");
        assert!(
            output.status.success(),
            "`{}` prettier version check failed with status {:?}: {}",
            runner.display,
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
        let version = str::from_utf8(&output.stdout)
            .expect("Prettier version output is not valid UTF-8")
            .trim();
        assert_eq!(
            version, "3.8.1",
            "Expected prettier 3.8.1 for parity tests, found {version}"
        );
    });
}

fn run_prettier(input_path: &Path, input: &str) -> String {
    let runner = prettier_runner();
    let path = input_path.to_string_lossy().to_string();
    let output = run_prettier_command(
        runner,
        &["--parser", "markdown", "--stdin-filepath", &path],
        Some(input),
    )
    .expect("Failed to execute prettier process for parity test");
    assert!(
        output.status.success(),
        "Prettier formatting failed for {:?} via {}: {}",
        input_path,
        runner.display,
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout).expect("Prettier output is not valid UTF-8")
}

#[derive(Debug)]
struct PrettierRunner {
    program: &'static str,
    base_args: Vec<&'static str>,
    display: &'static str,
}

fn prettier_runner() -> &'static PrettierRunner {
    static RUNNER: OnceLock<PrettierRunner> = OnceLock::new();
    RUNNER.get_or_init(resolve_prettier_runner)
}

fn resolve_prettier_runner() -> PrettierRunner {
    let candidates = [
        PrettierRunner {
            program: "bunx",
            base_args: vec!["prettier@3.8.1"],
            display: "bunx prettier@3.8.1",
        },
        PrettierRunner {
            program: "pnpm",
            base_args: vec!["dlx", "prettier@3.8.1"],
            display: "pnpm dlx prettier@3.8.1",
        },
        PrettierRunner {
            program: "npx",
            base_args: vec!["--yes", "prettier@3.8.1"],
            display: "npx --yes prettier@3.8.1",
        },
    ];

    for runner in candidates {
        if command_exists(runner.program) {
            return runner;
        }
    }

    panic!(
        "No prettier runner available. Install one of: bunx, pnpm, npx (required for parity tests)."
    );
}

fn command_exists(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn run_prettier_command(
    runner: &PrettierRunner,
    args: &[&str],
    stdin: Option<&str>,
) -> std::io::Result<std::process::Output> {
    let mut command = Command::new(runner.program);
    command
        .args(&runner.base_args)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if stdin.is_some() {
        command.stdin(Stdio::piped());
    }

    let mut child = command.spawn()?;
    if let Some(input) = stdin {
        let mut child_stdin = child
            .stdin
            .take()
            .expect("Failed to open stdin for prettier command");
        child_stdin.write_all(input.as_bytes())?;
    }

    child.wait_with_output()
}

fn read_fixture_file(dir: &Path, stem: &str) -> Option<(PathBuf, String)> {
    for extension in ["md", "markdown"] {
        let path = dir.join(format!("{stem}.{extension}"));
        if path.exists() {
            let content = std::fs::read_to_string(&path).ok()?;
            return Some((path, content));
        }
    }

    None
}

fn split_frontmatter(input: &str) -> Option<(&str, &str)> {
    let first_newline = input.find('\n')?;
    let first_line = &input[..=first_newline];
    let delimiter = if first_line.trim_end_matches(['\r', '\n']) == "---" {
        "---"
    } else if first_line.trim_end_matches(['\r', '\n']) == "+++" {
        "+++"
    } else {
        return None;
    };

    let mut offset = first_newline + 1;
    loop {
        let remaining = &input[offset..];
        if remaining.is_empty() {
            return None;
        }

        if let Some(next_newline) = remaining.find('\n') {
            let line_end = offset + next_newline + 1;
            let line = &input[offset..line_end];
            if line.trim_end_matches(['\r', '\n']) == delimiter {
                return Some(input.split_at(line_end));
            }
            offset = line_end;
        } else {
            if remaining.trim_end_matches(['\r', '\n']) == delimiter {
                return Some(input.split_at(input.len()));
            }
            return None;
        }
    }
}
