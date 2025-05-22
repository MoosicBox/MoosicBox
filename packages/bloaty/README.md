# Bloaty

A tool for analyzing binary size and feature impact across Rust workspace members. Bloaty helps you understand how different features affect your binary sizes and identify potential optimization opportunities.

## Features

- Analyze binary sizes across workspace members
- Compare feature impact on binary sizes
- Support for multiple output formats (text, JSON, JSONL)
- Integration with cargo-bloat, cargo-llvm-lines, and cargo-size
- Detailed reporting of size differences between features

## Installation

```bash
cargo install --path packages/bloaty
```

## Usage

Basic usage:
```bash
cargo run --bin bloaty
```

This will analyze all workspace members and generate reports in all supported formats.

### Command Line Options

- `-p, --package <PACKAGE>`: Specify packages to analyze (comma-separated)
- `--skip-packages <SKIP_PACKAGES>`: Packages to skip (comma-separated)
- `--skip-features <SKIP_FEATURES>`: Features to skip (comma-separated)
- `-t, --tool <TOOL>`: Tools to use (comma-separated, options: bloat, llvm-lines, size)
- `--report-file <REPORT_FILE>`: Custom report file name (without extension)
- `--output-format <FORMAT>`: Output format (text, json, jsonl, all)

### Examples

Analyze specific packages:
```bash
cargo run --bin bloaty -- -p package1,package2
```

Skip certain features:
```bash
cargo run --bin bloaty -- --skip-features test,bench
```

Use specific tools:
```bash
cargo run --bin bloaty -- -t bloat,size
```

Generate only JSON output:
```bash
cargo run --bin bloaty -- --output-format json
```

Custom report file:
```bash
cargo run --bin bloaty -- --report-file analysis
```

## Output Formats

### Text Format
Human-readable format showing package, target, and feature sizes with differences.

Example:
```
Package: my_crate
===================

Target: my_target
-------------------
Base size: 1.2 MB
Feature: default        | Size: 1.2 MB | Diff: +0 B
Feature: extra          | Size: 1.5 MB | Diff: +300 KB
Feature: minimal        | Size: 900 KB | Diff: -300 KB
```

### JSON Format
Complete analysis in a single JSON object, useful for programmatic processing.

### JSONL Format
Line-delimited JSON format, with each line representing a single event in the analysis process. Useful for streaming and real-time processing.

Example:
```jsonl
{"type":"package_start","name":"my_crate","timestamp":1234567890}
{"type":"target_start","package":"my_crate","target":"my_target","timestamp":1234567890}
{"type":"base_size","package":"my_crate","target":"my_target","size":1258291,"size_formatted":"1.2 MB","timestamp":1234567890}
{"type":"feature","package":"my_crate","target":"my_target","feature":"default","size":1258291,"diff":0,"diff_formatted":"+0 B","size_formatted":"1.2 MB","timestamp":1234567890}
```

## Dependencies

- cargo-bloat
- cargo-llvm-lines
- cargo-size

Make sure these tools are installed before running bloaty:
```bash
cargo install cargo-bloat cargo-llvm-lines cargo-size
```

## License

This project is licensed under the same terms as the Rust project.
