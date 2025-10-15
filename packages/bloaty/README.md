# MoosicBox Bloaty

A binary size analysis tool for Rust workspace packages that helps track and compare the size impact of features.

## Overview

The MoosicBox Bloaty package provides binary size analysis for Rust packages in a workspace. It measures the size impact of individual features by building packages with different feature combinations and comparing the resulting binary and library sizes.

## Features

- **Feature size analysis**: Measures the size impact of each feature on rlib targets and binary executables
- **Binary target analysis**: For binary targets, analyzes both the rlib and the final executable sizes
- **Multiple output formats**: Supports text, JSON, and JSONL report formats
- **Package filtering**: Select specific packages using patterns or explicit lists
- **Feature filtering**: Skip features using patterns or explicit lists
- **Workspace integration**: Analyzes all workspace members automatically
- **External tool integration**: Supports cargo-bloat, cargo-llvm-lines, and cargo-size

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/MoosicBox/MoosicBox.git
cd MoosicBox

# Build the tool
cargo build --package bloaty --release

# The binary will be at target/release/bloaty
```

## Usage

### Basic Usage

Analyze all workspace packages:

```bash
bloaty
```

### Package Selection

Analyze specific packages:

```bash
# Single package
bloaty --package moosicbox_core

# Multiple packages
bloaty --package moosicbox_core --package moosicbox_server

# Using patterns
bloaty --package-pattern "moosicbox_.*"
```

### Feature Filtering

Skip specific features:

```bash
# Skip specific features
bloaty --skip-features fail-on-warnings --skip-features openssl

# Skip features matching a pattern
bloaty --skip-feature-pattern ".*-static$"
```

### External Tools

Run external analysis tools (requires installation):

```bash
# Run cargo-bloat
bloaty --tool bloat

# Run multiple tools
bloaty --tool bloat --tool llvm-lines
```

### Output Formats

Control report output:

```bash
# Generate all formats (default)
bloaty --output-format all

# Only text report
bloaty --output-format text

# JSON and JSONL reports
bloaty --output-format json --output-format jsonl

# Custom report filename
bloaty --report-file my_analysis
```

This generates files like:

- `my_analysis.txt` (text report)
- `my_analysis.json` (complete JSON report)
- `my_analysis.jsonl` (streaming JSONL format)

### Skip Packages

Exclude packages from analysis:

```bash
# Skip specific packages
bloaty --skip-packages moosicbox_test --skip-packages moosicbox_dev

# Skip packages matching pattern
bloaty --skip-package-pattern ".*_test$"
```

## Report Format

### Text Report

The text report shows base sizes and feature impact:

```
Package: moosicbox_core
===================

Target: moosicbox_core
-------------------
Base size: 1.2 MB
Feature: async          | Size: 1.3 MB | Diff: +100 KB
Feature: db             | Size: 1.5 MB | Diff: +300 KB
```

### JSON Report

The JSON report provides structured data:

```json
{
    "timestamp": 1234567890,
    "packages": [
        {
            "name": "moosicbox_core",
            "targets": [
                {
                    "name": "moosicbox_core",
                    "base_size": 1258291,
                    "base_binary_size": 0,
                    "features": [
                        {
                            "name": "async",
                            "size": 1360384,
                            "diff": 102093,
                            "diff_formatted": "+102 KB",
                            "size_formatted": "1.3 MB",
                            "binary_size": 0,
                            "binary_diff": 0,
                            "binary_diff_formatted": "+0 B",
                            "binary_size_formatted": "0 B"
                        }
                    ]
                }
            ]
        }
    ]
}
```

### JSONL Report

The JSONL report provides streaming event data for real-time processing:

```jsonl
{"type":"package_start","name":"moosicbox_core","timestamp":1234567890}
{"type":"target_start","package":"moosicbox_core","target":"moosicbox_core","timestamp":1234567890}
{"type":"base_size","package":"moosicbox_core","target":"moosicbox_core","size":1258291,"timestamp":1234567890}
{"type":"feature","package":"moosicbox_core","target":"moosicbox_core","feature":"async","size":1360384,"diff":102093,"timestamp":1234567890}
{"type":"target_end","package":"moosicbox_core","target":"moosicbox_core","timestamp":1234567890}
{"type":"package_end","name":"moosicbox_core","timestamp":1234567890}
```

## Dependencies

Core dependencies (automatically managed by Cargo):

- `anyhow` - Error handling
- `bytesize` - Human-readable size formatting
- `cargo_metadata` - Workspace metadata access
- `clap` - Command-line argument parsing
- `glob` - File pattern matching
- `log` - Logging framework
- `pretty_env_logger` - Pretty logging output
- `regex` - Pattern matching for filters
- `serde_json` - JSON output formatting
- `switchy_time` - Cross-platform time utilities

Optional external tools:

- `cargo-bloat` - For detailed binary bloat analysis
- `cargo-llvm-lines` - For LLVM IR line count analysis
- `cargo-size` - For size profiling

## How It Works

1. **Package Discovery**: Uses `cargo_metadata` to find workspace members
2. **Feature Extraction**: Identifies available features from each package's `Cargo.toml`
3. **Baseline Build**: Builds each target with no features enabled and measures rlib size
4. **Baseline Binary Build**: For binary targets, also builds and measures the executable size
5. **Feature Analysis**: Builds each target with individual features and compares rlib sizes
6. **Binary Feature Analysis**: For binary targets, also builds executables with features and compares sizes
7. **Report Generation**: Outputs results in requested formats (text, JSON, JSONL)

## Contributing

Contributions are welcome! Areas for improvement:

1. Add support for feature combinations (currently only analyzes individual features)
2. Implement historical size tracking and regression detection
3. Add visualization of size trends over time
4. Optimize build performance with better caching
5. Add more detailed breakdown of size contributors
