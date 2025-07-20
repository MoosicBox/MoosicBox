# Clippier

Rust workspace analysis and automation tool for managing multi-package projects, with focus on CI/CD pipeline generation and dependency analysis.

## Overview

Clippier is a command-line utility designed to analyze Rust workspaces and automate various development tasks:

- **CI/CD Pipeline Generation**: Generate feature matrices for testing
- **Dependency Analysis**: Analyze workspace dependencies and relationships
- **Feature Management**: Generate feature combinations for comprehensive testing
- **Docker Integration**: Generate optimized Dockerfiles for workspace packages
- **Change Impact Analysis**: Determine which packages are affected by file changes
- **External Dependency Tracking**: Detect changes in external dependencies via git diff analysis

## Installation

### From Source

```bash
cargo install --path packages/clippier
```

### Features

Clippier supports optional features:

- **`git-diff`** (default): Enhanced change analysis using git diff to detect external dependency changes
- **`fail-on-warnings`**: Fail build on warnings

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
  --spread \
  --randomize \
  --features "default,audio,video" \
  --skip-features "test,dev" \
  --output json
```

#### Deterministic Randomization with Seed

Use a specific seed for reproducible randomized feature combinations:
```bash
clippier features Cargo.toml \
  --chunked 3 \
  --randomize \
  --seed 12345 \
  --output json
```

When `--randomize` is used without `--seed`, a random seed is generated and printed to stderr (not affecting JSON output):
```bash
clippier features Cargo.toml \
  --chunked 3 \
  --randomize \
  --output json
# Outputs: Generated seed: 1234567890 (to stderr)
```

This enables replaying the same randomized distribution by using the printed seed value.

#### Enhanced Change Impact Analysis

Include only features for packages affected by specific file changes:
```bash
clippier features Cargo.toml \
  --changed-files "src/lib.rs,packages/server/src/main.rs" \
  --max 10 \
  --output json
```

#### Git-Based External Dependency Analysis (Requires git-diff feature)

Analyze feature matrices considering both file changes and external dependency changes:
```bash
clippier features Cargo.toml \
  --changed-files "Cargo.lock,src/lib.rs" \
  --git-base "origin/main" \
  --git-head "HEAD" \
  --max 10 \
  --output json
```

### Workspace Dependencies

Find all workspace dependencies for a package:
```bash
clippier workspace-deps /path/to/workspace package-name --format json
```

Include specific features:
```bash
clippier workspace-deps /path/to/workspace package-name \
  --features "feature1,feature2" \
  --format text
```

#### All Potential Dependencies Mode

Include all potential workspace dependencies (useful for Docker builds):
```bash
clippier workspace-deps /path/to/workspace package-name \
  --all-potential-deps \
  --format json
```

This mode includes all workspace dependencies regardless of feature activation, ensuring Docker builds have access to all required packages for build compatibility.

### Generate Dockerfile

Automatically generate optimized multi-stage Dockerfiles:
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

The generated Dockerfiles include:
- Multi-stage builds for optimized layer caching
- Automatic system dependency detection from `clippier.toml` files
- Workspace member optimization
- Build artifact caching
- Runtime dependency installation

### Affected Packages Analysis

Determine which packages are affected by file changes:
```bash
clippier affected-packages /path/to/workspace \
  --changed-files "src/lib.rs,Cargo.toml,packages/server/src/main.rs" \
  --target-package server \
  --output json
```

#### Enhanced Git-Based Analysis (Requires git-diff feature)

Analyze impact including external dependency changes from Cargo.lock:
```bash
clippier affected-packages /path/to/workspace \
  --changed-files "Cargo.lock,src/lib.rs,packages/server/src/main.rs" \
  --git-base "origin/main" \
  --git-head "HEAD" \
  --output json
```

This enhanced mode:
- Detects changes in external dependencies by analyzing Cargo.lock diff
- Maps external dependency changes to affected workspace packages
- Provides comprehensive impact analysis for both internal and external changes

### Docker Deployment

Generate production-ready Dockerfiles with comprehensive dependency analysis:

```bash
# Generate Dockerfile for server package with all potential dependencies
clippier generate-dockerfile . server \
  --features "production,postgres" \
  --output docker/Dockerfile.server \
  --port 8080 \
  --build-args "--release --locked"
```

### Change Impact Analysis

Determine test scope based on changed files and external dependencies:

```bash
# Find affected packages from git changes including external deps
CHANGED=$(git diff --name-only HEAD~1)
clippier affected-packages . \
  --changed-files "$CHANGED" \
  --git-base "HEAD~1" \
  --git-head "HEAD" \
  --output json
```

### Smart Workspace Dependency Management

Analyze workspace dependencies with different levels of detail:

```bash
# Get minimal dependencies for current features
clippier workspace-deps . my-package --features "default,feature1"

# Get all potential dependencies for Docker builds
clippier workspace-deps . my-package --all-potential-deps --format json
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
| `--randomize` | Randomize features before chunking/spreading | false |
| `--seed` | Seed for deterministic randomization | - |
| `--features` | Specific features to include | - |
| `--skip-features` | Features to exclude | - |
| `--required-features` | Always-required features | - |
| `--changed-files` | Filter by changed files | - |
| `--git-base` | Git base commit for external dep analysis | - |
| `--git-head` | Git head commit for external dep analysis | - |

### Workspace Dependencies Options

| Option | Description | Default |
|--------|-------------|---------|
| `--features` | Features to enable | - |
| `--format` | Output format: `json`, `text` | `text` |
| `--all-potential-deps` | Include all potential dependencies | false |

### Docker Generation Options

| Option | Description | Default |
|--------|-------------|---------|
| `--base-image` | Docker builder image | `rust:1-bookworm` |
| `--final-image` | Docker runtime image | `debian:bookworm-slim` |
| `--port` | Port to expose | - |
| `--build-args` | Cargo build arguments | - |
| `--generate-dockerignore` | Generate .dockerignore | true |

### Affected Packages Options

| Option | Description | Default |
|--------|-------------|---------|
| `--changed-files` | List of changed files | Required |
| `--target-package` | Specific package to check | - |
| `--git-base` | Git base commit for external dep analysis | - |
| `--git-head` | Git head commit for external dep analysis | - |
| `--output` | Output format: `json`, `raw` | `json` |

## Configuration

Clippier can be configured using a `clippier.toml` file in your workspace root or individual package directories:

```toml
# Global configuration
[config.linux]
os = "ubuntu-latest"
nightly = false
cargo = ["build", "test", "clippy"]

[config.linux.env]
RUST_BACKTRACE = "1"
CARGO_TERM_COLOR = "always"

# System dependencies for Docker generation
[[config.linux.dependencies]]
command = "sudo apt-get update"

[[config.linux.dependencies]]
command = "sudo apt-get install -y pkg-config libssl-dev libasound2-dev"

# Feature-specific dependencies
[[config.linux.dependencies]]
command = "sudo apt-get install -y libsqlite3-dev"
features = ["database"]

[parallelization]
chunked = 4
```

### Configuration Features

- **Feature-specific dependencies**: Dependencies can be conditionally included based on enabled features
- **Multiple OS configurations**: Support for different operating systems
- **Environment variable management**: Configurable environment variables
- **CI step customization**: Custom CI pipeline steps
- **Toolchain specification**: Custom Rust toolchains per configuration

## Use Cases

### CI/CD Pipeline Generation

Generate feature matrices for GitHub Actions with intelligent change detection:

```bash
# Generate feature combinations for testing only affected packages
clippier features Cargo.toml \
  --changed-files "$CHANGED_FILES" \
  --git-base "origin/main" \
  --git-head "HEAD" \
  --max 20 \
  --max-parallel 6 \
  --chunked 3 \
  --spread \
  --randomize \
  --skip-features "dev,test" \
  --output json > feature-matrix.json
```

The `--randomize` flag shuffles features before chunking, creating different feature combinations across CI runs. This helps catch issues with various feature groupings that might not be discovered with deterministic chunking.

#### Reproducible CI Builds

For reproducible builds (useful for debugging specific feature combinations), use the `--seed` parameter:

```bash
# Generate reproducible feature combinations using a specific seed
clippier features Cargo.toml \
  --changed-files "$CHANGED_FILES" \
  --max 20 \
  --max-parallel 6 \
  --chunked 3 \
  --spread \
  --randomize \
  --seed 1234567890 \
  --skip-features "dev,test" \
  --output json > feature-matrix.json
```

This ensures the same feature distribution is generated every time, enabling reproduction of specific CI failures.

### Docker Deployment

Generate production-ready Dockerfiles with comprehensive dependency analysis:

```bash
# Generate Dockerfile for server package with all potential dependencies
clippier generate-dockerfile . server \
  --features "production,postgres" \
  --output docker/Dockerfile.server \
  --port 8080 \
  --build-args "--release --locked"
```

### Smart Workspace Dependency Management

Analyze workspace dependencies with different levels of detail:

```bash
# Get minimal dependencies for current features
clippier workspace-deps . my-package --features "default,feature1"

# Get all potential dependencies for Docker builds
clippier workspace-deps . my-package --all-potential-deps --format json
```

## Core Functionality

- **Feature Combination Generation**: Creates combinations of Cargo features for testing
- **Workspace Dependency Analysis**: Maps dependencies between workspace packages
- **CI Configuration Generation**: Produces CI/CD pipeline configurations
- **Docker File Generation**: Creates optimized multi-stage Dockerfiles
- **Impact Analysis**: Determines which packages are affected by code changes
- **External Dependency Tracking**: Monitors changes in external dependencies via git analysis
- **Smart Filtering**: Reduces test scope by analyzing only affected packages

## Advanced Features

### External Dependency Analysis

When the `git-diff` feature is enabled (default), Clippier can:

- **Parse Cargo.lock changes**: Detect version updates in external dependencies
- **Map external to internal dependencies**: Understand which workspace packages use which external dependencies
- **Provide comprehensive impact analysis**: Include both file-based and dependency-based changes
- **Optimize CI/CD pipelines**: Test only packages actually affected by changes

### Intelligent Docker Generation

The Docker generation feature provides:

- **System dependency detection**: Automatically includes required system packages
- **Multi-stage optimization**: Separates build and runtime environments
- **Layer caching optimization**: Structures Dockerfiles for maximum cache efficiency
- **Feature-aware builds**: Includes only dependencies needed for specified features
- **Comprehensive dependency inclusion**: Uses `--all-potential-deps` mode for build compatibility

### Workspace Relationship Mapping

- **Dependency graph generation**: Complete workspace dependency mapping
- **Circular dependency detection**: identify problematic dependency cycles
- **Build order optimization**: Determine optimal build sequences
- **Feature dependency resolution**: Handle complex feature interactions

## Output Formats

### JSON Output
Structured data suitable for programmatic processing:

```json
{
  "packages": [
    {
      "name": "package-name",
      "features": ["feature1", "feature2"],
      "dependencies": ["dep1", "dep2"],
      "os": "ubuntu-latest",
      "path": "packages/package-name"
    }
  ]
}
```

For affected packages analysis:
```json
{
  "affected_packages": ["server", "auth", "database"],
  "package": "server",
  "affected": true,
  "all_affected": ["server", "auth", "database"]
}
```

### Raw/Text Output
Human-readable text format for direct use in scripts:
```
server
auth
database
```

## Integration Examples

### GitHub Actions Workflow with Smart Change Detection

```yaml
name: CI
on: [push, pull_request]

jobs:
  analyze-changes:
    runs-on: ubuntu-latest
    outputs:
      matrix: ${{ steps.matrix.outputs.matrix }}
      analysis-method: ${{ steps.analysis.outputs.method }}
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Build clippier
        run: cargo build --release --package clippier

      - name: Analyze changes and generate matrix
        id: analysis
        run: |
          # Get changed files
          CHANGED_FILES=$(git diff --name-only ${{ github.event.before }}..${{ github.sha }} | tr '\n' ',' | sed 's/,$//')

          # Use enhanced analysis if Cargo.lock changed
          if echo "$CHANGED_FILES" | grep -q "Cargo.lock"; then
            echo "method=hybrid" >> $GITHUB_OUTPUT
            matrix=$(./target/release/clippier features Cargo.toml \
              --changed-files "$CHANGED_FILES" \
              --git-base "${{ github.event.before }}" \
              --git-head "${{ github.sha }}" \
              --max 15 --output json)
          else
            echo "method=file-based" >> $GITHUB_OUTPUT
            matrix=$(./target/release/clippier features Cargo.toml \
              --changed-files "$CHANGED_FILES" \
              --max 15 --output json)
          fi

          echo "matrix=$matrix" >> $GITHUB_OUTPUT

  test:
    needs: analyze-changes
    if: needs.analyze-changes.outputs.matrix != '[]'
    strategy:
      matrix: ${{ fromJson(needs.analyze-changes.outputs.matrix) }}
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - name: Test with features (${{ needs.analyze-changes.outputs.analysis-method }})
        run: cargo test --package ${{ matrix.name }} --features "${{ matrix.features }}"
```

### Docker Build Pipeline with Dependency Optimization

```bash
#!/bin/bash
# Generate optimized Dockerfiles for all binary packages

PACKAGES=("server" "tunnel-server" "load-balancer")

for package in "${PACKAGES[@]}"; do
  echo "Generating Dockerfile for $package..."

  # Show dependency analysis
  echo "Dependencies analysis:"
  echo "  Normal: $(./clippier workspace-deps . "moosicbox_$package" | wc -l) packages"
  echo "  All potential: $(./clippier workspace-deps . "moosicbox_$package" --all-potential-deps | wc -l) packages"

  # Generate Dockerfile
  ./clippier generate-dockerfile . "moosicbox_$package" \
    --output "docker/Dockerfile.$package" \
    --port 8080 \
    --generate-dockerignore

  echo "Generated docker/Dockerfile.$package"
done
```

### Advanced Change Analysis Script

```bash
#!/bin/bash
# Comprehensive change impact analysis

BASE_COMMIT=${1:-"origin/main"}
HEAD_COMMIT=${2:-"HEAD"}

echo "🔍 Analyzing changes from $BASE_COMMIT to $HEAD_COMMIT"

# Get changed files
CHANGED_FILES=$(git diff --name-only "$BASE_COMMIT".."$HEAD_COMMIT" | tr '\n' ',' | sed 's/,$//')
echo "📝 Changed files: $CHANGED_FILES"

# Analyze affected packages
if echo "$CHANGED_FILES" | grep -q "Cargo.lock"; then
  echo "🧠 Using hybrid analysis (external deps + file changes)"
  AFFECTED=$(./clippier affected-packages . \
    --changed-files "$CHANGED_FILES" \
    --git-base "$BASE_COMMIT" \
    --git-head "$HEAD_COMMIT" \
    --output json)
else
  echo "📁 Using file-based analysis"
  AFFECTED=$(./clippier affected-packages . \
    --changed-files "$CHANGED_FILES" \
    --output json)
fi

echo "📦 Affected packages: $AFFECTED"

# Generate test matrix for affected packages only
echo "🎯 Generating targeted test matrix..."
MATRIX=$(./clippier features Cargo.toml \
  --changed-files "$CHANGED_FILES" \
  --git-base "$BASE_COMMIT" \
  --git-head "$HEAD_COMMIT" \
  --max 20 \
  --output json)

echo "🧪 Test matrix: $MATRIX"
```

## See Also

- [MoosicBox Server](../server/README.md) - Example of complex workspace package
- [Cargo Workspaces](https://doc.rust-lang.org/book/ch14-03-cargo-workspaces.html) - Rust workspace documentation
- [GitHub Actions](https://docs.github.com/en/actions) - CI/CD platform integration
- [Docker Multi-stage Builds](https://docs.docker.com/develop/dev-best-practices/dockerfile_best-practices/#use-multi-stage-builds) - Docker optimization techniques
