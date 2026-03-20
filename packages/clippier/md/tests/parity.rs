use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::{io::Write as IoWrite, str};

use clippier_md::{
    Config, FormatterEngine, HeadingIndentationMode, ListIndentationMode, ListStyle, ProseWrapMode,
    format_markdown,
};

#[test]
fn prettier_parity_commonmark_gfm_fixtures() {
    assert_prettier_version();

    let fixtures_base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("parity")
        .join("fixtures");
    let mut fixture_dirs = Vec::new();
    fixture_dirs.extend(collect_fixture_dirs(&fixtures_base.join("commonmark")));
    fixture_dirs.extend(collect_fixture_dirs(&fixtures_base.join("gfm")));
    fixture_dirs.sort();

    assert!(!fixture_dirs.is_empty(), "No parity fixtures found");

    let config = Config {
        engine: FormatterEngine::Ast,
        prose_wrap: ProseWrapMode::Always,
        heading_indentation: HeadingIndentationMode::Normalize,
        list_indentation: ListIndentationMode::Normalize,
        list_style: ListStyle::Dash,
        ..Config::default()
    };

    let mut failures = Vec::new();

    for dir in fixture_dirs {
        let input = read_fixture_file(&dir, "input").expect("missing input fixture");
        let prettier = run_prettier(&input.0, &input.1);
        let output = format_markdown(&input.1, &config);

        if output != prettier {
            failures.push(ParityFailure {
                scope: format!("fixture:{}", dir.display()),
                kind: "parity".to_string(),
                details: first_difference_summary(&prettier, &output),
            });
        }

        let idempotent = format_markdown(&output, &config);
        if idempotent != output {
            failures.push(ParityFailure {
                scope: format!("fixture:{}", dir.display()),
                kind: "idempotence".to_string(),
                details: first_difference_summary(&output, &idempotent),
            });
        }
    }

    let spec_path = fixtures_base
        .join("..")
        .join("..")
        .join("vendor")
        .join("commonmark-spec")
        .join("spec.txt");
    if !spec_path.exists() {
        eprintln!(
            "Skipping CommonMark spec parity checks because '{}' is missing. Run: git submodule update --init --recursive",
            spec_path.display()
        );
        return;
    }

    let spec_contents = std::fs::read_to_string(&spec_path).unwrap_or_else(|error| {
        panic!(
            "Failed to read CommonMark spec at '{}': {error}",
            spec_path.display()
        )
    });
    let examples = parse_commonmark_examples(&spec_contents);
    assert!(
        !examples.is_empty(),
        "No examples parsed from CommonMark spec at '{}'",
        spec_path.display()
    );

    for example in examples {
        let input = example.markdown.replace('→', "\t");
        let prettier = run_prettier(Path::new("commonmark-spec.md"), &input);
        let output = format_markdown(&input, &config);

        if output != prettier {
            failures.push(ParityFailure {
                scope: format!("commonmark-spec#{}", example.id),
                kind: "parity".to_string(),
                details: first_difference_summary(&prettier, &output),
            });
        }

        let idempotent = format_markdown(&output, &config);
        if idempotent != output {
            failures.push(ParityFailure {
                scope: format!("commonmark-spec#{}", example.id),
                kind: "idempotence".to_string(),
                details: first_difference_summary(&output, &idempotent),
            });
        }
    }

    if !failures.is_empty() {
        let mut report = format!("Found {} parity failure(s):\n", failures.len());
        for failure in &failures {
            report.push_str(&format!(
                "- [{}] {}\n  {}\n",
                failure.kind, failure.scope, failure.details
            ));
        }
        panic!("{report}");
    }
}

#[derive(Debug)]
struct ParityFailure {
    scope: String,
    kind: String,
    details: String,
}

fn first_difference_summary(expected: &str, actual: &str) -> String {
    let expected_lines = expected.lines().collect::<Vec<_>>();
    let actual_lines = actual.lines().collect::<Vec<_>>();
    let max = expected_lines.len().max(actual_lines.len());

    for index in 0..max {
        let left = expected_lines.get(index).copied();
        let right = actual_lines.get(index).copied();
        if left != right {
            return format!(
                "line {}: expected {:?}, actual {:?}",
                index + 1,
                left.unwrap_or("<missing>"),
                right.unwrap_or("<missing>"),
            );
        }
    }

    format!(
        "byte mismatch with equal line splits (expected {} bytes, actual {} bytes)",
        expected.len(),
        actual.len()
    )
}

#[derive(Debug)]
struct CommonmarkExample {
    id: usize,
    markdown: String,
}

fn parse_commonmark_examples(spec: &str) -> Vec<CommonmarkExample> {
    let mut examples = Vec::new();
    let mut lines = spec.lines().peekable();
    let mut id = 0usize;

    while let Some(line) = lines.next() {
        let trimmed = line.trim_end();
        if !trimmed.ends_with(" example") {
            continue;
        }
        let fence = trimmed.trim_end_matches(" example");
        if fence.is_empty() || !fence.chars().all(|char| char == '`') {
            continue;
        }

        let mut markdown_lines = Vec::new();
        let mut reading_markdown = true;
        for body_line in lines.by_ref() {
            let body_trimmed = body_line.trim_end();
            if body_trimmed == fence {
                break;
            }
            if reading_markdown && body_trimmed == "." {
                reading_markdown = false;
                continue;
            }
            if reading_markdown {
                markdown_lines.push(body_line.to_string());
            }
        }

        id += 1;
        examples.push(CommonmarkExample {
            id,
            markdown: markdown_lines.join("\n"),
        });
    }

    examples
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
