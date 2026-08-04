//! Criterion benchmarks for formatter latency, repository throughput, and instrumentation.

#![allow(clippy::missing_panics_doc)]

use std::fmt::Write as _;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::process::Command;

use clippier_md::{Config, benchmark_counters, format_markdown, reset_benchmark_counters, run_fmt};
use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use tempfile::TempDir;

const TINY_CANONICAL: &str = "# Canonical\n\nA short canonical paragraph.\n";
const MIXED_MARKDOWN: &str = r#"---
title: Benchmark
---

# Mixed Markdown

A paragraph with [a link](https://example.com), **strong text**, and `code`.

- First item
- Second item

| Name | Value |
| --- | ---: |
| alpha | 1 |

<div data-kind="raw">HTML</div>

```rust
fn main() {
    println!("benchmark");
}
```

export const value = 1;

<Component enabled={true}>MDX content</Component>
"#;

fn benchmark_config() -> Config {
    Config::default()
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("clippier-md must be nested three levels below the repository root")
        .to_path_buf()
}

fn tracked_markdown_files(root: &Path) -> Vec<PathBuf> {
    let output = Command::new("git")
        .args(["ls-files", "-z", "--", "*.md", "*.mdx", "*.markdown"])
        .current_dir(root)
        .output()
        .expect("failed to list tracked Markdown files");
    assert!(output.status.success(), "git ls-files failed");
    output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            root.join(std::str::from_utf8(path).expect("tracked Markdown path must be valid UTF-8"))
        })
        .collect()
}

fn corpus_bytes(paths: &[PathBuf]) -> u64 {
    paths
        .iter()
        .map(|path| {
            std::fs::metadata(path)
                .expect("failed to read corpus file metadata")
                .len()
        })
        .sum()
}

fn bench_format_markdown(c: &mut Criterion) {
    let root = repository_root();
    let cases = [
        ("tiny_canonical", TINY_CANONICAL.to_string()),
        ("mixed_markdown", MIXED_MARKDOWN.to_string()),
        (
            "opus_native_plan",
            std::fs::read_to_string(root.join("spec/opus-native/plan.md"))
                .expect("missing spec/opus-native/plan.md"),
        ),
        (
            "generic_schema_migrations_plan",
            std::fs::read_to_string(root.join("spec/generic-schema-migrations/plan.md"))
                .expect("missing spec/generic-schema-migrations/plan.md"),
        ),
    ];
    let config = benchmark_config();
    let mut group = c.benchmark_group("format_markdown");
    for (name, input) in &cases {
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_function(*name, |bencher| {
            bencher.iter(|| format_markdown(black_box(input), black_box(&config)));
        });
    }
    group.finish();
}

fn prepare_clean_corpus(root: &Path, paths: &[PathBuf], config: &Config) -> TempDir {
    let directory = tempfile::tempdir().expect("failed to create clean benchmark corpus");
    for (index, path) in paths.iter().enumerate() {
        let input = std::fs::read_to_string(path).expect("failed to read corpus file");
        let output = format_markdown(&input, config);
        std::fs::write(directory.path().join(format!("{index}.md")), output)
            .expect("failed to write clean benchmark corpus");
    }
    assert!(root.is_dir());
    directory
}

fn bench_run_fmt(c: &mut Criterion) {
    let root = repository_root();
    let tracked_files = tracked_markdown_files(&root);
    let config = benchmark_config();
    let clean_corpus = prepare_clean_corpus(&root, &tracked_files, &config);
    let clean_path = clean_corpus.path().to_path_buf();
    let bytes = corpus_bytes(&tracked_files);
    let mut group = c.benchmark_group("run_fmt");
    group.throughput(Throughput::Bytes(bytes));
    group.bench_function("clean_repository_check_no_diff", |bencher| {
        bencher.iter(|| {
            run_fmt(
                black_box(std::slice::from_ref(&clean_path)),
                true,
                false,
                black_box(&config),
            )
            .expect("clean repository benchmark failed")
        });
    });

    let diff_fixture = tempfile::tempdir().expect("failed to create diff benchmark fixture");
    let diff_path = diff_fixture.path().join("changed.md");
    let mut changed_input = String::new();
    for index in 0..500 {
        writeln!(changed_input, "#Heading {index}\n\n{}", "word ".repeat(30))
            .expect("failed to build diff fixture");
    }
    std::fs::write(&diff_path, &changed_input).expect("failed to write diff benchmark fixture");
    let diff_paths = vec![diff_path];
    group.bench_function("changed_check_no_diff", |bencher| {
        bencher.iter(|| {
            run_fmt(black_box(&diff_paths), true, false, black_box(&config))
                .expect("changed check benchmark failed")
        });
    });
    group.bench_function("changed_check_capped_diff", |bencher| {
        bencher.iter(|| {
            run_fmt(black_box(&diff_paths), true, true, black_box(&config))
                .expect("capped diff benchmark failed")
        });
    });
    group.bench_function("changed_file_write", |bencher| {
        bencher.iter_batched(
            || {
                let directory = tempfile::tempdir().expect("failed to create write fixture");
                let path = directory.path().join("changed.md");
                std::fs::write(&path, &changed_input).expect("failed to stage write fixture");
                (directory, vec![path])
            },
            |(_directory, paths)| {
                run_fmt(black_box(&paths), false, false, black_box(&config))
                    .expect("changed write benchmark failed")
            },
            BatchSize::PerIteration,
        );
    });
    group.finish();
}

fn bench_instrumentation(c: &mut Criterion) {
    let config = benchmark_config();
    c.bench_function("instrumentation/mixed_markdown", |bencher| {
        bencher.iter(|| {
            reset_benchmark_counters();
            let output = format_markdown(black_box(MIXED_MARKDOWN), black_box(&config));
            black_box((output, benchmark_counters()))
        });
    });
}

criterion_group!(
    formatter_benches,
    bench_format_markdown,
    bench_run_fmt,
    bench_instrumentation,
);
criterion_main!(formatter_benches);
