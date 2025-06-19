# Clippier

Rust workspace analysis and automation tool for managing multi-package projects, with focus on CI/CD pipeline generation and dependency analysis.

## Overview

Clippier is a command-line utility designed to analyze Rust workspaces and automate various development tasks:

- **CI/CD Pipeline Generation**: Generate feature matrices for testing
- **Dependency Analysis**: Analyze workspace dependencies and relationships
- **Feature Management**: Generate feature combinations for comprehensive testing
- **Docker Integration**: Generate Dockerfiles for workspace packages
- **Change Impact Analysis**: Determine which packages are affected by file changes

## Installation

### From Source

```bash
cargo install --path packages/clippier
```

## Usage

Clippier provides several subcommands for different analysis tasks:

### Dependencies Analysis

Analyze workspace dependencies for a specific package:
```bash
clippier dependencies Cargo.toml --output json
```

With specific OS and feature filters:
```bash
clippier dependencies Cargo.toml --os linux --features "feature1,feature2"
```

### Environment Configuration

Generate environment configurations:
```bash
clippier environment Cargo.toml --os ubuntu-latest --output json
```

### CI Steps Generation

Generate CI pipeline steps for testing:
```bash
clippier ci-steps Cargo.toml --os ubuntu-latest --features "default,feature1"
```

### Feature Matrix Analysis

Analyze and generate feature combinations for testing:
```bash
clippier features Cargo.toml \
  --max 10 \
  --max-parallel 4 \
  --chunked 2 \
  --features "default,audio,video" \
  --skip-features "test,dev" \
  --output json
```

### Workspace Dependencies

Find all workspace dependencies for a package:
```bash
clippier workspace-deps /path/to/workspace package-name --format json
```

Include all potential dependencies:
```bash
clippier workspace-deps /path/to/workspace package-name \
  --features "feature1,feature2" \
  --all-potential-deps \
  --format text
```

### Generate Dockerfile

Automatically generate optimized Dockerfiles:
```bash
clippier generate-dockerfile /path/to/workspace target-package \
  --features "feature1,feature2" \
  --output ./Dockerfile \
  --base-image rust:1-bookworm \
  --final-image debian:bookworm-slim \
  --port 8080 \
  --build-args "--release" \
  --generate-dockerignore true
```

### Affected Packages Analysis

Determine which packages are affected by file changes:
```bash
clippier affected-packages /path/to/workspace \
  --changed-files "src/lib.rs,Cargo.toml,packages/server/src/main.rs" \
  --target-package server \
  --output json
```

## Command Line Options

### Global Options

| Option | Description |
|--------|-------------|
| `--output` | Output format: `json`, `raw` |

### Features Command Options

| Option | Description | Default |
|--------|-------------|---------|
| `--os` | Target operating system | - |
| `--offset` | Skip first N features | 0 |
| `--max` | Maximum number of features | All |
| `--max-parallel` | Maximum parallel jobs | - |
| `--chunked` | Group features into chunks | - |
| `--spread` | Spread features across jobs | false |
| `--features` | Specific features to include | - |
| `--skip-features` | Features to exclude | - |
| `--required-features` | Always-required features | - |
| `--changed-files` | Filter by changed files | - |

### Docker Generation Options

| Option | Description | Default |
|--------|-------------|---------|
| `--base-image` | Docker builder image | `rust:1-bookworm` |
| `--final-image` | Docker runtime image | `debian:bookworm-slim` |
| `--port` | Port to expose | - |
| `--build-args` | Cargo build arguments | - |
| `--generate-dockerignore` | Generate .dockerignore | true |

## Configuration

Clippier can be configured using a `clippier.toml` file in your workspace root:

```toml
[config.linux]
os = "ubuntu-latest"
nightly = false
cargo = ["build", "test", "clippy"]

[config.linux.env]
RUST_BACKTRACE = "1"
CARGO_TERM_COLOR = "always"

[[config.linux.dependencies]]
command = "sudo apt-get update"

[[config.linux.dependencies]]
command = "sudo apt-get install -y pkg-config libssl-dev"

[parallelization]
chunked = 4
```

## Use Cases

### CI/CD Pipeline Generation

Generate feature matrices for GitHub Actions:

```bash
# Generate feature combinations for testing
clippier features Cargo.toml \
  --max 20 \
  --max-parallel 6 \
  --chunked 3 \
  --spread \
  --skip-features "dev,test" \
  --output json > feature-matrix.json
```

### Docker Deployment

Generate production-ready Dockerfiles:

```bash
# Generate Dockerfile for server package
clippier generate-dockerfile . server \
  --features "production,postgres" \
  --output docker/Dockerfile.server \
  --port 8080 \
  --build-args "--release --locked"
```

### Change Impact Analysis

Determine test scope based on changed files:

```bash
# Find affected packages from git changes
CHANGED=$(git diff --name-only HEAD~1)
clippier affected-packages . \
  --changed-files "$CHANGED" \
  --output json
```

## Core Functionality

- **Feature Combination Generation**: Creates combinations of Cargo features for testing
- **Workspace Dependency Analysis**: Maps dependencies between workspace packages
- **CI Configuration Generation**: Produces CI/CD pipeline configurations
- **Docker File Generation**: Creates optimized multi-stage Dockerfiles
- **Impact Analysis**: Determines which packages are affected by code changes

This tool is particularly useful for large Rust workspaces with complex feature interactions and CI/CD requirements.

## Output Formats

### JSON Output
Structured data suitable for programmatic processing:
```json
{
  "packages": [
    {
      "name": "package-name",
      "features": ["feature1", "feature2"],
      "dependencies": ["dep1", "dep2"]
    }
  ]
}
```

### Raw Output
Human-readable text format for direct use in scripts.

## Integration Examples

### GitHub Actions Workflow

```yaml
name: CI
on: [push, pull_request]

jobs:
  generate-matrix:
    runs-on: ubuntu-latest
    outputs:
      matrix: ${{ steps.matrix.outputs.matrix }}
    steps:
      - uses: actions/checkout@v3
      - name: Generate test matrix
        id: matrix
        run: |
          matrix=$(clippier features Cargo.toml --max 10 --output json)
          echo "matrix=$matrix" >> $GITHUB_OUTPUT

  test:
    needs: generate-matrix
    strategy:
      matrix: ${{ fromJson(needs.generate-matrix.outputs.matrix) }}
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v3
      - name: Test with features
        run: cargo test --features "${{ matrix.features }}"
```

### Docker Build Pipeline

```bash
#!/bin/bash
# Generate Dockerfiles for all binary packages

for package in server tunnel-server load-balancer; do
  clippier generate-dockerfile . "$package" \
    --output "docker/Dockerfile.$package" \
    --port 8080 \
    --generate-dockerignore
done
```

## Advanced Features

### Feature Combination Analysis
- Intelligent feature dependency resolution
- Conflict detection between features
- Optimization suggestions for feature flags

### Workspace Relationship Mapping
- Dependency graph generation
- Circular dependency detection
- Build order optimization

### Multi-Platform Support
- Cross-platform configuration generation
- Platform-specific dependency handling
- Architecture-aware build optimization

## See Also

- [MoosicBox Server](../server/README.md) - Example of complex workspace package
- [Cargo Workspaces](https://doc.rust-lang.org/book/ch14-03-cargo-workspaces.html) - Rust workspace documentation
- [GitHub Actions](https://docs.github.com/en/actions) - CI/CD platform integration
