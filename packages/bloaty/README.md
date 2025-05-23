# Bloaty

A tool for analyzing binary size and feature impact across Rust workspace members. Bloaty helps you understand how different features affect your binary sizes and identify potential optimization opportunities.

## Features

- Analyze binary sizes across workspace members
- Compare feature impact on binary sizes
- Support for multiple output formats (text, JSON, JSONL)
- Integration with cargo-bloat, cargo-llvm-lines, and cargo-size
- Detailed reporting of size differences between features
- Regex pattern matching for packages and features
- Analysis of both library (rlib) and binary sizes
- Tracking of statically linked dependencies

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

All list arguments (packages, skip-packages, skip-features, tool, output-format) support both comma-separated lists and multiple arguments.

- `-p, --package <PACKAGE>`: Specify packages to analyze
  ```bash
  # Using comma-separated list
  --package pkg1,pkg2,pkg3
  # Using multiple arguments
  --package pkg1 --package pkg2 --package pkg3
  ```

- `--package-pattern <PACKAGE_PATTERN>`: Regex pattern for packages to analyze
  ```bash
  # Analyze all packages starting with 'core-'
  --package-pattern "^core-.*"
  # Analyze packages containing 'test' or 'bench'
  --package-pattern ".*(test|bench).*"
  ```

- `--skip-packages <SKIP_PACKAGES>`: Packages to skip
  ```bash
  # Using comma-separated list
  --skip-packages pkg1,pkg2
  # Using multiple arguments
  --skip-packages pkg1 --skip-packages pkg2
  ```

- `--skip-package-pattern <SKIP_PACKAGE_PATTERN>`: Regex pattern for packages to skip
  ```bash
  # Skip all test packages
  --skip-package-pattern ".*-test$"
  # Skip packages starting with 'bench' or 'dev'
  --skip-package-pattern "^(bench|dev).*"
  ```

- `--skip-features <SKIP_FEATURES>`: Features to skip
  ```bash
  # Using comma-separated list
  --skip-features test,bench,dev
  # Using multiple arguments
  --skip-features test --skip-features bench
  ```

- `--skip-feature-pattern <SKIP_FEATURE_PATTERN>`: Regex pattern for features to skip
  ```bash
  # Skip all test-related features
  --skip-feature-pattern "test.*"
  # Skip features starting with 'bench' or 'dev'
  --skip-feature-pattern "^(bench|dev).*"
  ```

- `-t, --tool <TOOL>`: Tools to use (options: bloat, llvm-lines, size)
  ```bash
  # Using comma-separated list
  --tool bloat,size
  # Using multiple arguments
  --tool bloat --tool size
  ```

- `--report-file <REPORT_FILE>`: Custom report file name (without extension)

- `--output-format <FORMAT>`: Output format (text, json, jsonl, all)
  ```bash
  # Using comma-separated list
  --output-format text,json
  # Using multiple arguments
  --output-format text --output-format json
  # Using 'all' for all formats
  --output-format all
  ```

### Examples

Analyze specific packages:
```bash
# Using exact package names
cargo run --bin bloaty -- -p package1,package2

# Using package pattern
cargo run --bin bloaty -- --package-pattern "^core-.*"

# Combine both approaches
cargo run --bin bloaty -- -p package1 --package-pattern "^core-.*"
```

Skip certain packages:
```bash
# Skip specific packages
cargo run --bin bloaty -- --skip-packages pkg1,pkg2

# Skip packages matching a pattern
cargo run --bin bloaty -- --skip-package-pattern ".*-test$"

# Combine both approaches
cargo run --bin bloaty -- --skip-packages pkg1 --skip-package-pattern ".*-test$"
```

Skip certain features:
```bash
# Skip specific features
cargo run --bin bloaty -- --skip-features test,bench,dev

# Skip features matching a pattern
cargo run --bin bloaty -- --skip-feature-pattern "test.*"

# Combine both approaches
cargo run --bin bloaty -- --skip-features test,bench --skip-feature-pattern "dev.*"
```

Use specific tools:
```bash
# Using comma-separated list
cargo run --bin bloaty -- -t bloat,size

# Using multiple arguments
cargo run --bin bloaty -- -t bloat -t size
```

Generate specific output formats:
```bash
# Using comma-separated list
cargo run --bin bloaty -- --output-format text,json

# Using multiple arguments
cargo run --bin bloaty -- --output-format text --output-format json

# Generate all formats
cargo run --bin bloaty -- --output-format all
```

Custom report file:
```bash
cargo run --bin bloaty -- --report-file analysis
```

## Output Formats

### Text Format
Human-readable format showing package, target, and feature sizes with differences. For binary targets, both library (rlib) and binary sizes are reported.

Example:
```
Package: my_crate
===================

Target: my_target
-------------------
Base rlib size: 1.2 MB
Base binary size: 2.5 MB
Feature: default        | Rlib Size: 1.2 MB | Rlib Diff: +0 B
Feature: default        | Binary Size: 2.5 MB | Binary Diff: +0 B
Feature: extra          | Rlib Size: 1.5 MB | Rlib Diff: +300 KB
Feature: extra          | Binary Size: 3.0 MB | Binary Diff: +500 KB
Feature: minimal        | Rlib Size: 900 KB | Rlib Diff: -300 KB
Feature: minimal        | Binary Size: 2.0 MB | Binary Diff: -500 KB
```

### JSON Format
Complete analysis in a single JSON object, useful for programmatic processing. Includes both library and binary sizes for each feature.

### JSONL Format
Line-delimited JSON format, with each line representing a single event in the analysis process. Useful for streaming and real-time processing.

Example:
```jsonl
{"type":"package_start","name":"my_crate","timestamp":1234567890}
{"type":"target_start","package":"my_crate","target":"my_target","timestamp":1234567890}
{"type":"base_size","package":"my_crate","target":"my_target","size":1258291,"size_formatted":"1.2 MB","timestamp":1234567890}
{"type":"base_binary_size","package":"my_crate","target":"my_target","size":2621440,"size_formatted":"2.5 MB","timestamp":1234567890}
{"type":"feature","package":"my_crate","target":"my_target","feature":"default","size":1258291,"diff":0,"diff_formatted":"+0 B","size_formatted":"1.2 MB","timestamp":1234567890}
{"type":"binary_feature","package":"my_crate","target":"my_target","feature":"default","size":2621440,"diff":0,"diff_formatted":"+0 B","size_formatted":"2.5 MB","timestamp":1234567890}
```

## Dependencies

- cargo-bloat
- cargo-llvm-lines
- cargo-size
- regex

Make sure these tools are installed before running bloaty:
```bash
cargo install cargo-bloat cargo-llvm-lines cargo-size
```
