# Clippier

Rust workspace analysis and automation tool for managing multi-package projects, with focus on CI/CD pipeline generation and dependency analysis.

## Overview

Clippier is a command-line utility designed to analyze Rust workspaces and automate various development tasks:

- **CI/CD Pipeline Generation**: Generate feature matrices for testing
- **Dependency Analysis**: Analyze workspace dependencies and relationships
- **Feature Management**: Generate feature combinations for comprehensive testing
- **Selective Package Processing**: Filter operations to specific packages for targeted analysis
- **Feature Propagation Validation**: Ensure features are correctly propagated across workspace dependencies
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

#### Package Filtering

Filter feature matrix generation to specific packages by name or by Cargo.toml properties:

```bash
# Process only specific packages by name
clippier features . \
  --packages moosicbox_server,moosicbox_audio_decoder \
  --chunked 15 \
  --max-parallel 256 \
  --output json

# Filter by package properties (exclude unpublished and examples)
clippier features . \
  --skip-if "publish=false" \
  --skip-if "name$=_example" \
  --output json

# Include only packages with specific characteristics
clippier features . \
  --include-if "name^=moosicbox_" \
  --include-if "categories@=audio" \
  --output json

# Combine with other filters
clippier features . \
  --packages moosicbox_server \
  --os ubuntu \
  --features "default,postgres" \
  --skip-features "fail-on-warnings" \
  --output json
```

This is particularly useful for:

- **Focused testing**: Test only specific packages during development
- **CI optimization**: Build matrix for selected components based on criteria
- **Monorepo management**: Process subsets of large workspaces
- **Quality gates**: Filter by documentation completeness, categories, etc.

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

### Packages Command

Generate a list of workspace packages (useful for CI matrix generation with one job per package):

```bash
# List all packages in workspace
clippier packages . --output json

# List packages for specific OS
clippier packages . --os ubuntu --output json

# Filter to specific packages
clippier packages . --packages "server,auth,database" --output json

# Limit number of packages (useful for parallel job limits)
clippier packages . --max-parallel 10 --output json
```

#### With Change Detection (Requires git-diff feature)

List only packages affected by file changes:

```bash
# Using manual changed files
clippier packages . \
  --changed-files "src/lib.rs,packages/server/src/main.rs" \
  --output json

# Using git diff
clippier packages . \
  --git-base "origin/main" \
  --git-head "HEAD" \
  --output json

# Combined: manual files + git diff + external dependency tracking
clippier packages . \
  --changed-files "Cargo.lock" \
  --git-base "origin/main" \
  --git-head "HEAD" \
  --output json
```

The packages command provides:

- **Package enumeration**: List all workspace packages with metadata
- **Change-based filtering**: Only include packages affected by specific changes
- **Git integration**: Automatically detect changed files from git commits
- **External dependency tracking**: Detect packages affected by Cargo.lock changes
- **CI matrix optimization**: Generate one job per package instead of per feature

**Output format:**

```json
[
    {
        "name": "server",
        "path": "packages/server",
        "os": "ubuntu-latest"
    },
    {
        "name": "auth",
        "path": "packages/auth",
        "os": "ubuntu-latest"
    }
]
```

**Use cases:**

- **CI/CD**: Generate job matrices for parallel package testing
- **Change analysis**: Identify which packages need rebuilding
- **Monorepo management**: List subsets of packages for targeted operations
- **Documentation**: Generate package inventories for workspace documentation

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

### Feature Propagation Validation

Validate that features are correctly propagated across workspace dependencies to ensure consistent builds and prevent feature-related compilation failures:

```bash
# Quick validation of fail-on-warnings feature
clippier validate-feature-propagation . --features "fail-on-warnings"

# Auto-detect and validate all matching features
clippier validate-feature-propagation . --output json

# Validate specific packages only
clippier validate-feature-propagation . --workspace-only --features "fail-on-warnings"
```

The feature validator ensures that when a package depends on another workspace package that has a specific feature, that feature is correctly propagated. This prevents build failures where features are inconsistently enabled across the dependency graph.

#### Common Use Cases

**Validate fail-on-warnings propagation:**

```bash
clippier validate-feature-propagation . --features "fail-on-warnings"
```

**Auto-detect features that need propagation:**

```bash
# Automatically finds features that exist in multiple packages
clippier validate-feature-propagation . --workspace-only
```

**CI/CD Integration:**

```bash
clippier validate-feature-propagation . \
  --features "fail-on-warnings" \
  --output json
```

**Get detailed JSON report:**

```bash
clippier validate-feature-propagation . \
  --features "fail-on-warnings,std,async" \
  --workspace-only \
  --output json > validation-report.json
```

#### Understanding Validation Results

The validator provides clear feedback about feature propagation issues:

**Successful validation:**

```
✅ All packages correctly propagate features!
Total packages checked: 147
Valid packages: 147
```

**Validation with errors:**

```
❌ Found 2 packages with incorrect feature propagation:

📦 Package: moosicbox_server
  Feature: fail-on-warnings
    Missing propagations:
      - moosicbox_tcp/fail-on-warnings (Dependency 'moosicbox_tcp' has feature but it's not propagated)
```

#### Error Types and Meanings

The validator detects two types of issues:

**Missing Propagations:**

- A dependency has a feature but it's not propagated
- Example: `pkg_a` depends on `pkg_b` which has `fail-on-warnings`, but `pkg_a` doesn't propagate it
- Fix: Add `"pkg_b/fail-on-warnings"` to the feature definition in `pkg_a`

**Incorrect Propagations:**

- A feature is propagated to a non-existent dependency or feature
- Example: `pkg_a` propagates `pkg_b/feature` but `pkg_b` doesn't have `feature`
- Fix: Remove the incorrect propagation or add the missing feature to the dependency

**Optional Dependencies:**
The validator correctly handles optional dependencies using the `?` syntax:

- `dep?/feature` - Propagates feature only when the optional dependency is activated
- Required for dependencies marked with `optional = true` in Cargo.toml

### Docker Deployment

Generate production-ready Dockerfiles with comprehensive dependency analysis:

```bash
# Generate Dockerfile for server package with all potential dependencies
clippier generate-dockerfile . server \
  --features "production,postgres" \
  --output docker/Dockerfile.server \
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

| Option     | Description                  |
| ---------- | ---------------------------- |
| `--output` | Output format: `json`, `raw` |

### Features Command Options

| Option                | Description                                  | Default      |
| --------------------- | -------------------------------------------- | ------------ |
| `--os`                | Target operating system                      | -            |
| `--offset`            | Skip first N features                        | 0            |
| `--max`               | Maximum number of features                   | All          |
| `--max-parallel`      | Maximum parallel jobs                        | -            |
| `--chunked`           | Group features into chunks                   | -            |
| `--spread`            | Spread features across jobs                  | false        |
| `--randomize`         | Randomize features before chunking/spreading | false        |
| `--seed`              | Seed for deterministic randomization         | -            |
| `--features`          | Specific features to include                 | -            |
| `--skip-features`     | Features to exclude                          | -            |
| `--required-features` | Always-required features                     | -            |
| `--packages`          | Comma-separated list of packages to process  | All packages |
| `--changed-files`     | Filter by changed files                      | -            |
| `--git-base`          | Git base commit for external dep analysis    | -            |
| `--git-head`          | Git head commit for external dep analysis    | -            |
| `--skip-if`           | Skip packages matching Cargo.toml filter     | -            |
| `--include-if`        | Include only packages matching filter        | -            |

### Packages Command Options

| Option                | Description                                 | Default      |
| --------------------- | ------------------------------------------- | ------------ |
| `--os`                | Target operating system                     | `ubuntu`     |
| `--packages`          | Comma-separated list of packages to include | All packages |
| `--changed-files`     | Filter by changed files                     | -            |
| `--git-base`          | Git base commit for change detection        | -            |
| `--git-head`          | Git head commit for change detection        | -            |
| `--include-reasoning` | Include reasoning for affected packages     | false        |
| `--max-parallel`      | Maximum number of packages to return        | -            |
| `--skip-if`           | Skip packages matching Cargo.toml filter    | -            |
| `--include-if`        | Include only packages matching filter       | -            |
| `--output`            | Output format: `json`, `raw`                | `json`       |

### Workspace Dependencies Options

| Option                 | Description                        | Default |
| ---------------------- | ---------------------------------- | ------- |
| `--features`           | Features to enable                 | -       |
| `--format`             | Output format: `json`, `text`      | `text`  |
| `--all-potential-deps` | Include all potential dependencies | false   |

### Docker Generation Options

| Option                    | Description                      | Default                |
| ------------------------- | -------------------------------- | ---------------------- |
| `--base-image`            | Docker builder image             | `rust:1-bookworm`      |
| `--final-image`           | Docker runtime image             | `debian:bookworm-slim` |
| `--build-args`            | Cargo build arguments            | -                      |
| `--generate-dockerignore` | Generate .dockerignore           | true                   |
| `--env`                   | Runtime environment variables    | -                      |
| `--build-env`             | Build-time environment variables | -                      |
| `--arg`                   | Arguments to pass to binary      | -                      |
| `--bin`                   | Specify binary name              | Auto-detect            |

### Affected Packages Options

| Option             | Description                               | Default  |
| ------------------ | ----------------------------------------- | -------- |
| `--changed-files`  | List of changed files                     | Required |
| `--target-package` | Specific package to check                 | -        |
| `--git-base`       | Git base commit for external dep analysis | -        |
| `--git-head`       | Git head commit for external dep analysis | -        |
| `--output`         | Output format: `json`, `raw`              | `json`   |

### Feature Validation Options

| Option             | Description                                  | Default           |
| ------------------ | -------------------------------------------- | ----------------- |
| `--features`       | Comma-separated list of features to validate | Auto-detect       |
| `--workspace-only` | Only validate workspace packages             | true              |
| `--output`         | Output format: `json`, `raw`                 | `raw`             |
| `--path`           | Workspace root path                          | Current directory |
| `--fail-on-error`  | Exit with error code if validation fails     | true              |

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

## Advanced Package Filtering

Clippier supports powerful property-based filtering to include or exclude packages based on their Cargo.toml properties using `--skip-if` and `--include-if` flags.

### Filter Syntax

**Format:** `property[.nested]<operator>value`

Filters can access any property in a package's Cargo.toml, including nested metadata.

### Available Operators

#### Scalar Operators

Match against string, boolean, or integer values:

| Operator | Description        | Example                       |
| -------- | ------------------ | ----------------------------- |
| `=`      | Exact match        | `publish=false`               |
| `!=`     | Not equal          | `version!=0.1.0`              |
| `^=`     | Starts with        | `name^=moosicbox_`            |
| `$=`     | Ends with          | `name$=_example`              |
| `*=`     | Contains substring | `description*=audio`          |
| `~=`     | Regex match        | `name~=^moosicbox_.*_server$` |

#### Array Operators

Match against array properties (keywords, categories, authors, etc.):

| Operator | Description                           | Example                    |
| -------- | ------------------------------------- | -------------------------- |
| `@=`     | Array contains exact element          | `categories@=audio`        |
| `@*=`    | Array contains element with substring | `keywords@*=music`         |
| `@^=`    | Array contains element starting with  | `keywords@^=api-`          |
| `@~=`    | Array contains element matching regex | `categories@~=^multimedia` |
| `@!`     | Array is empty                        | `keywords@!`               |
| `@#=`    | Array length equals                   | `keywords@#=3`             |
| `@#>`    | Array length greater than             | `authors@#>1`              |
| `@#<`    | Array length less than                | `categories@#<5`           |
| `!@=`    | Array does NOT contain                | `keywords!@=deprecated`    |

#### Existence Operators

Check if properties exist:

| Operator | Description             | Example      |
| -------- | ----------------------- | ------------ |
| `?`      | Property exists         | `readme?`    |
| `!?`     | Property does NOT exist | `homepage!?` |

### Usage Examples

#### Skip Unpublished Packages

```bash
# Exclude packages with publish = false
clippier features . --skip-if "publish=false" --output json
```

#### Include Only Specific Package Prefixes

```bash
# Only process moosicbox packages
clippier features . --include-if "name^=moosicbox_" --output json

# Exclude example packages
clippier packages . --skip-if "name$=_example" --output json
```

#### Filter by Categories or Keywords

```bash
# Only packages with audio category
clippier features . --include-if "categories@=audio" --output json

# Packages containing "api" in keywords
clippier features . --include-if "keywords@*=api" --output json

# Skip packages with empty keywords
clippier features . --skip-if "keywords@!" --output json
```

#### Array Length Filtering

```bash
# Only packages with 3+ keywords (well-documented)
clippier features . --include-if "keywords@#>2" --output json

# Packages with exactly 2 categories
clippier features . --include-if "categories@#=2" --output json
```

#### Nested Metadata Access

```bash
# Only independent workspace packages
clippier features . \
  --include-if "metadata.workspaces.independent=true" \
  --output json

# Skip packages with custom CI configuration
clippier packages . --skip-if "metadata.ci.skip-tests=true"
```

#### Combining Multiple Filters

```bash
# Include moosicbox packages, exclude examples and unpublished
clippier features . \
  --include-if "name^=moosicbox_" \
  --skip-if "name$=_example" \
  --skip-if "publish=false" \
  --output json

# Audio packages with sufficient documentation
clippier features . \
  --include-if "categories@=audio" \
  --include-if "keywords@#>2" \
  --include-if "readme?" \
  --output json
```

### Filter Logic

**Skip Filters (`--skip-if`):**

- Multiple skip filters use **OR** logic
- If **ANY** skip filter matches, the package is excluded
- Processed after include filters

**Include Filters (`--include-if`):**

- Multiple filters for the **same property** use **OR** logic
- Filters for **different properties** use **AND** logic
- All property groups must have at least one match

**Example:**

```bash
# Exclude if (name ends with _example) OR (publish is false)
--skip-if "name$=_example" --skip-if "publish=false"

# Include if (name starts with moosicbox_) AND (has audio category OR video category)
--include-if "name^=moosicbox_" --include-if "categories@=audio" --include-if "categories@=video"
```

### Backward Compatibility

Unprefixed property names automatically check the `[package]` section:

```bash
# These are equivalent:
--include-if "name=my_package"
--include-if "package.name=my_package"

# Explicit package prefix for clarity:
--include-if "package.categories@=audio"

# Access nested metadata:
--include-if "package.metadata.custom=value"
```

### Property Paths

Access any Cargo.toml property using dot notation:

- `name`, `version`, `edition` - Standard package properties
- `publish`, `categories`, `keywords` - Package metadata
- `metadata.custom.field` - Custom nested metadata
- `dependencies.serde.version` - Dependency information (if needed)

### Practical Use Cases

#### CI/CD Optimization

```bash
# Test only published, non-example packages
clippier features . \
  --skip-if "publish=false" \
  --skip-if "name$=_example" \
  --max 20 \
  --output json
```

#### Monorepo Component Isolation

```bash
# Test only frontend packages (by naming convention)
clippier features . \
  --include-if "name*=_ui" \
  --include-if "name*=_web" \
  --output json

# Backend services only
clippier features . \
  --include-if "name*=_server" \
  --include-if "name*=_service" \
  --output json
```

#### Documentation Quality Checks

```bash
# Find packages missing documentation
clippier packages . --skip-if "readme?" --output raw

# Well-documented packages only
clippier features . \
  --include-if "readme?" \
  --include-if "keywords@#>2" \
  --include-if "categories@#>0" \
  --output json
```

#### Dependency Auditing

```bash
# Packages with specific metadata flags
clippier packages . \
  --include-if "metadata.security.audited=true" \
  --output json
```

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

### Selective Package Processing

Process specific packages in large workspaces:

```bash
# Generate feature matrix for specific packages only
clippier features . \
  --packages server,auth,database \
  --max 10 \
  --chunked 3 \
  --output json

# Useful for:
# - Development: Focus on packages you're actively working on
# - CI/CD: Create targeted test matrices
# - Performance: Reduce processing time for large workspaces
```

#### Combining with Change Detection

```bash
# Process specific packages AND filter by changes
clippier features . \
  --packages server,auth \
  --changed-files "src/lib.rs,Cargo.toml" \
  --max 10 \
  --output json
```

When both `--packages` and `--changed-files` are specified:

- First filters to specified packages
- Then applies change detection within those packages
- Results in highly targeted feature matrices

### Docker Deployment

Generate production-ready Dockerfiles with comprehensive dependency analysis:

```bash
# Generate Dockerfile for server package with all potential dependencies
clippier generate-dockerfile . server \
  --features "production,postgres" \
  --output docker/Dockerfile.server \
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
- **Feature Propagation Validation**: Ensures features are correctly propagated across workspace dependencies
- **Workspace Dependency Analysis**: Maps dependencies between workspace packages
- **CI Configuration Generation**: Produces CI/CD pipeline configurations
- **Docker File Generation**: Creates optimized multi-stage Dockerfiles
- **Impact Analysis**: Determines which packages are affected by code changes
- **External Dependency Tracking**: Monitors changes in external dependencies via git analysis
- **Smart Filtering**: Reduces test scope by analyzing only affected packages
- **Selective Package Processing**: Filter operations to specific packages for improved performance

## Advanced Features

### Selective Package Processing

The `--packages` flag enables targeted processing of specific workspace packages:

- **Performance optimization**: Process only relevant packages instead of entire workspace
- **Development workflow**: Focus on packages under active development
- **CI/CD flexibility**: Create custom test matrices for different components
- **Monorepo management**: Handle subsets of large multi-package repositories

Example workflow for package groups:

```bash
# Frontend packages
clippier features . --packages ui,web,app --output json

# Backend services
clippier features . --packages server,auth,database --output json

# Core libraries
clippier features . --packages core,utils,common --output json
```

Package names should match exactly as defined in Cargo.toml files. For packages with prefixes (e.g., `moosicbox_server`), use the full name.

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

For feature validation results:

```json
{
    "total_packages": 147,
    "valid_packages": 145,
    "errors": [
        {
            "package": "moosicbox_server",
            "errors": [
                {
                    "feature": "fail-on-warnings",
                    "missing_propagations": [
                        {
                            "dependency": "moosicbox_tcp",
                            "expected": "moosicbox_tcp/fail-on-warnings",
                            "reason": "Dependency 'moosicbox_tcp' has feature 'fail-on-warnings' but it's not propagated"
                        }
                    ],
                    "incorrect_propagations": []
                }
            ]
        }
    ],
    "warnings": []
}
```

### Raw/Text Output

Human-readable text format for direct use in scripts:

```
server
auth
database
```

## Package Selection Examples

### Development Workflow

Focus on packages you're actively developing:

```bash
# Working on authentication system
clippier features . \
  --packages auth,auth_middleware,auth_api \
  --max 5 \
  --output json

# Testing database layer changes
clippier features . \
  --packages database,migrations,models \
  --features "postgres,sqlite" \
  --output json
```

### CI/CD Pipeline with Package Groups

```yaml
name: Component Testing
on:
    workflow_dispatch:
        inputs:
            component:
                description: 'Component to test'
                required: true
                type: choice
                options:
                    - frontend
                    - backend
                    - core

jobs:
    test-component:
        runs-on: ubuntu-latest
        steps:
            - uses: actions/checkout@v4

            - name: Set package list
              id: packages
              run: |
                  case "${{ github.event.inputs.component }}" in
                    frontend)
                      echo "list=ui,web,app" >> $GITHUB_OUTPUT
                      ;;
                    backend)
                      echo "list=server,api,auth" >> $GITHUB_OUTPUT
                      ;;
                    core)
                      echo "list=core,utils,common" >> $GITHUB_OUTPUT
                      ;;
                  esac

            - name: Generate test matrix
              run: |
                  clippier features . \
                    --packages "${{ steps.packages.outputs.list }}" \
                    --max 10 \
                    --output json
```

### Performance Comparison

```bash
# Full workspace analysis (slow for large workspaces)
time clippier features . --output json > all.json

# Targeted package analysis (much faster)
time clippier features . \
  --packages critical_service \
  --output json > critical.json

# Measure the improvement
echo "Full workspace: $(wc -l < all.json) configurations"
echo "Single package: $(wc -l < critical.json) configurations"
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

       - name: Validate feature propagation
         run: |
           ./target/release/clippier validate-feature-propagation . \
             --features "fail-on-warnings"

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

# Validate feature propagation before testing
echo "🔧 Validating feature propagation for affected packages..."
if ! ./clippier validate-feature-propagation . --features "fail-on-warnings" --workspace-only; then
    echo "❌ Feature propagation validation failed - fix before continuing"
    exit 1
fi

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

### Feature Validation in CI Pipeline

```bash
#!/bin/bash
# Comprehensive feature validation script for CI/CD

echo "🔧 Validating feature propagation..."

# First, validate that fail-on-warnings is properly propagated
echo "  Checking fail-on-warnings propagation..."
if ! ./clippier validate-feature-propagation . --features "fail-on-warnings"; then
    echo "❌ fail-on-warnings validation failed"
    exit 1
fi

# Auto-detect and validate all features that might need propagation
echo "  Auto-detecting features for validation..."
VALIDATION_RESULT=$(./clippier validate-feature-propagation . --workspace-only --output json)
ERRORS=$(echo "$VALIDATION_RESULT" | jq '.errors | length')

if [ "$ERRORS" -gt 0 ]; then
    echo "❌ Found $ERRORS feature propagation errors"
    echo "$VALIDATION_RESULT" | jq -r '.errors[] | "Package: \(.package) - \(.errors | length) error(s)"'
    exit 1
else
    VALID_PACKAGES=$(echo "$VALIDATION_RESULT" | jq '.valid_packages')
    echo "✅ All $VALID_PACKAGES packages have correct feature propagation"
fi

echo "🎯 Feature validation completed successfully!"
```

## Best Practices

### Feature Propagation Validation

1. **Run in CI/CD pipelines**: Catch propagation issues early before they cause build failures

    ```bash
    # In your CI pipeline
    clippier validate-feature-propagation . --features "fail-on-warnings"
    ```

2. **Use auto-detection mode**: Finds all features that might need propagation across your workspace

    ```bash
    # Discovers features that exist in multiple packages
    clippier validate-feature-propagation . --workspace-only
    ```

3. **Validate after adding dependencies**: Ensure new workspace dependencies don't break feature propagation

    ```bash
    # Quick check after adding a new workspace dependency
    clippier validate-feature-propagation . --features "fail-on-warnings,your-feature"
    ```

4. **Use workspace-only mode**: Focus validation on packages you control, excluding external dependencies

    ```bash
    # Avoids false positives from external dependencies
    clippier validate-feature-propagation . --workspace-only
    ```

5. **Document required features**: Make feature propagation requirements explicit in your workspace
    ```toml
    # In Cargo.toml, document why features are propagated
    [features]
    fail-on-warnings = [
        "dep_a/fail-on-warnings",  # Required for consistent builds
        "dep_b?/fail-on-warnings", # Optional dep - only when enabled
    ]
    ```

### Package Selection Best Practices

1. **Use exact package names**: Specify packages exactly as they appear in Cargo.toml

    ```bash
    # Correct
    clippier features . --packages moosicbox_server,moosicbox_auth

    # Incorrect (unless packages are actually named this way)
    clippier features . --packages server,auth
    ```

2. **Combine with other filters for precision**: Layer multiple filters for targeted analysis

    ```bash
    clippier features . \
      --packages server,database \
      --os ubuntu \
      --features "production" \
      --skip-features "dev,test"
    ```

3. **Use in CI/CD for component testing**: Create separate workflows for different components

    ```bash
    # Test only affected services
    clippier features . \
      --packages $AFFECTED_SERVICES \
      --changed-files "$CHANGED_FILES"
    ```

4. **Performance optimization for large workspaces**: Process packages in groups

    ```bash
    # Process in batches for very large workspaces
    BATCH1="pkg1,pkg2,pkg3,pkg4,pkg5"
    BATCH2="pkg6,pkg7,pkg8,pkg9,pkg10"

    clippier features . --packages $BATCH1 --output json > batch1.json
    clippier features . --packages $BATCH2 --output json > batch2.json
    ```

### General Workspace Management

1. **Regular dependency analysis**: Run `workspace-deps` to understand your dependency graph
2. **Impact-driven testing**: Use `affected-packages` to optimize CI runs
3. **Feature matrix optimization**: Use `--changed-files` to test only what matters
4. **Docker build optimization**: Use `--all-potential-deps` for complete build contexts

## Troubleshooting

### Feature Validation Issues

**"Feature not found" errors:**

- Ensure the feature exists in at least one workspace package
- Check spelling and case sensitivity in feature names
- Use auto-detection mode to see which features are available

**Too many false positives:**

- Use `--workspace-only` to exclude external dependencies
- Specify exact features with `--features` instead of auto-detection
- Check that external dependencies actually need the features

**Missing optional dependency syntax:**

- Use `dep?/feature` syntax for optional dependencies
- Ensure the dependency is marked as `optional = true` in Cargo.toml
- Verify the optional dependency actually has the feature you're propagating

**CI/CD integration issues:**

- Use `--output json` for programmatic output parsing
- Use `--fail-on-error` to control exit codes in CI pipelines
- Verify the clippier binary is built and available in the workflow

**Performance issues with large workspaces:**

- Use `--features` to validate specific features instead of auto-detection
- Consider `--workspace-only` to reduce scope
- Run validation in parallel with other CI steps

### Package Selection Issues

**"Package not found" warnings:**

- Verify package names match exactly as in Cargo.toml
- Check for typos or incorrect prefixes
- Use `cargo metadata` to list all workspace packages
- Remember packages are case-sensitive

**Empty results with --packages:**

- Ensure packages exist in the workspace
- Check that packages have Cargo.toml files
- Verify workspace members list includes the packages
- Try without --packages flag to see all available packages

**Combining --packages with --changed-files:**

- Both filters are applied (packages AND changes)
- May result in empty set if no changes affect selected packages
- Use one or the other for broader results
- Check package paths match changed file paths

### General Troubleshooting

**Command not found errors:**

- Ensure clippier is built: `cargo build --release --package clippier`
- Check the binary path: `./target/release/clippier`
- Verify you're in the workspace root directory

**Git-related errors (external dependency analysis):**

- Ensure git history is available (use `fetch-depth: 0` in GitHub Actions)
- Check that base and head commits exist
- Verify you have the `git-diff` feature enabled (default)

**JSON parsing errors:**

- Verify JSON output with `jq` or similar tools
- Check for proper shell escaping in CI environments
- Ensure output is captured correctly in scripts

## Quick Reference

### Property-Based Filter Syntax

```bash
# Scalar operators
--skip-if "publish=false"              # Exact match
--include-if "name^=moosicbox_"        # Starts with
--skip-if "name$=_example"             # Ends with
--include-if "description*=audio"      # Contains
--include-if "name~=^test_.*"          # Regex match

# Array operators
--include-if "categories@=audio"       # Array contains
--include-if "keywords@*=api"          # Array element contains substring
--include-if "keywords@^=music"        # Array element starts with
--include-if "categories@~=^multi"     # Array element matches regex
--skip-if "keywords@!"                 # Array is empty
--include-if "keywords@#=3"            # Array length equals
--include-if "authors@#>1"             # Array length greater than
--include-if "categories@#<5"          # Array length less than
--skip-if "keywords!@=deprecated"      # Array does NOT contain

# Existence operators
--include-if "readme?"                 # Property exists
--skip-if "homepage!?"                 # Property does NOT exist

# Nested properties
--include-if "metadata.workspaces.independent=true"
--skip-if "metadata.ci.skip-tests=true"

# Combining filters
clippier features . \
  --include-if "name^=moosicbox_" \
  --skip-if "name$=_example" \
  --skip-if "publish=false"
```

### Packages Command Patterns

```bash
# List all workspace packages
clippier packages . --output json

# List packages for specific OS
clippier packages . --os ubuntu --output json

# Filter to specific packages
clippier packages . --packages server,auth

# Property-based filtering
clippier packages . --skip-if "publish=false" --output json
clippier packages . --include-if "categories@=audio" --output json

# Only packages affected by changes
clippier packages . --changed-files "src/lib.rs,Cargo.toml"

# Only packages affected by git changes
clippier packages . --git-base origin/main --git-head HEAD

# Combined with max parallel limit
clippier packages . --max-parallel 20 --output json

# Raw output (package names only)
clippier packages . --output raw
```

### Common Package Selection Patterns (Features Command)

```bash
# Single package
clippier features . --packages my_package

# Multiple packages
clippier features . --packages pkg1,pkg2,pkg3

# With full names (including prefixes)
clippier features . --packages moosicbox_server,moosicbox_auth

# Combined with OS filter
clippier features . --packages server --os ubuntu

# Combined with feature filter
clippier features . --packages server --features "default,tls"

# Combined with change detection
clippier features . --packages server --changed-files "src/main.rs"

# With chunking for CI
clippier features . --packages server,auth --chunked 5 --max-parallel 10

# For Docker generation
clippier workspace-deps . my_package --all-potential-deps

# For affected packages
clippier affected-packages . --changed-files "*.rs" --target-package server
```

## See Also

- [MoosicBox Server](../server/README.md) - Example of complex workspace package
- [Cargo Workspaces](https://doc.rust-lang.org/book/ch14-03-cargo-workspaces.html) - Rust workspace documentation
- [GitHub Actions](https://docs.github.com/en/actions) - CI/CD platform integration
- [Docker Multi-stage Builds](https://docs.docker.com/develop/dev-best-practices/dockerfile_best-practices/#use-multi-stage-builds) - Docker optimization techniques
