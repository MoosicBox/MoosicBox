# Clippier

Rust workspace analysis and automation tool for managing multi-package projects, with focus on CI/CD pipeline generation and dependency analysis.

## Overview

Clippier is a command-line utility designed to analyze Rust workspaces and automate various development tasks:

- **CI/CD Pipeline Generation**: Generate feature matrices for testing
- **Dependency Analysis**: Analyze workspace dependencies and relationships
- **Feature Management**: Generate feature combinations for comprehensive testing
- **Selective Package Processing**: Filter operations to specific packages for targeted analysis
- **Feature Propagation Validation**: Ensure features are correctly propagated across workspace dependencies
- **Parent Package Validation**: Validate that parent packages expose all features from their workspace dependencies
- **Unified Linting & Formatting**: Auto-detect and run multiple linters and formatters with `check` and `fmt` commands
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

- **`check`** (default): Enable the `check` command for running linters
- **`format`** (default): Enable the `fmt` command for running formatters
- **`git-diff`** (default): Enhanced change analysis using git diff to detect external dependency changes
- **`transforms-vendored`** (default): Enable Lua transform scripts with vendored Lua runtime
- **`transforms-system`**: Enable Lua transform scripts using system Lua installation
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

#### Wildcard Pattern Support in skip-features

The `--skip-features` flag supports powerful wildcard patterns and negation:

**Wildcard Patterns:**

```bash
# Skip all features ending with -default
clippier features Cargo.toml --skip-features "*-default" --output json

# Skip all features starting with test-
clippier features Cargo.toml --skip-features "test-*" --output json

# Skip features with single character after prefix (v1, v2, but not v10)
clippier features Cargo.toml --skip-features "v?" --output json

# Combine multiple patterns
clippier features Cargo.toml --skip-features "*-default,test-*,dev-*" --output json
```

**Negation Patterns (Skip All Except):**

```bash
# Skip everything except enable-bob
clippier features Cargo.toml --skip-features "*,!enable-bob" --output json

# Skip everything except features starting with enable-
clippier features Cargo.toml --skip-features "*,!enable-*" --output json

# Complex: skip *-default and test-* features, but keep test-utils
clippier features Cargo.toml \
  --skip-features "*-default,test-*,!test-utils" \
  --output json
```

**Pattern Syntax:**

| Pattern     | Matches                            | Example                                            |
| ----------- | ---------------------------------- | -------------------------------------------------- |
| `*`         | Zero or more characters            | `*-default` matches `bob-default`, `sally-default` |
| `?`         | Exactly one character              | `v?` matches `v1`, `v2` but not `v10`              |
| `!pattern`  | Negation (excludes matching items) | `*,!enable-bob` includes all except `enable-bob`   |
| Exact match | No wildcards                       | `default` matches only `default`                   |

**Pattern Evaluation:**

- Patterns are evaluated in order from left to right
- For `--skip-features`: The last matching pattern determines if a feature is skipped
- For `--features`, `--packages`, `--required-features`: Negations remove items from the result set
- Negation (`!`) works in all wildcard-supporting arguments

**Configuration File Usage:**

```toml
[[config]]
os = "ubuntu"
skip-features = ["*-default", "test-*", "!test-utils"]
```

#### Wildcard Pattern Support in --features

The `--features` flag supports wildcard patterns and negation for selecting features:

```bash
# Include all features starting with enable-
clippier features Cargo.toml --features "enable-*" --output json

# Include multiple wildcard patterns
clippier features Cargo.toml --features "enable-*,test-*" --output json

# Mix exact features with wildcards
clippier features Cargo.toml --features "enable-*,production,default" --output json

# Use negation to include all except specific features
clippier features Cargo.toml --features "*,!test-*" --output json

# Include enable-* features except enable-experimental
clippier features Cargo.toml --features "enable-*,!enable-experimental" --output json

# Combine with skip-features for powerful filtering
clippier features Cargo.toml \
  --features "enable-*,test-*" \
  --skip-features "test-integration" \
  --output json
```

**Note:** The `--features` flag uses **inclusion** (expand matching features), while `--skip-features` uses **exclusion** (remove matching features). Both support negation with `!` prefix. They can be combined for precise control.

#### Wildcard Pattern Support in --required-features

The `--required-features` flag supports wildcard patterns and negation for specifying required features. These patterns are **expanded to concrete feature names** in the JSON output, making them suitable for consumption by CI tools and scripts.

```bash
# Require all features starting with enable-
clippier features Cargo.toml --required-features "enable-*" --output json

# Require multiple wildcard patterns
clippier features Cargo.toml --required-features "enable-*,test-*" --output json

# Mix exact features with wildcards
clippier features Cargo.toml --required-features "enable-*,production,default" --output json

# Use negation to require enable-* except enable-experimental
clippier features Cargo.toml --required-features "enable-*,!enable-experimental" --output json

# Combine with other feature flags
clippier features Cargo.toml \
  --features "enable-*,production" \
  --skip-features "test-*,!test-utils" \
  --required-features "enable-*,!enable-experimental" \
  --output json
```

**Important:** Unlike `--skip-features` (which removes features) and `--features` (which selects features to process), `--required-features` is **metadata** that gets included in the JSON output. Wildcards and negations are expanded so downstream consumers receive concrete feature names, not glob patterns.

**Example output:**

```json
{
  "name": "my-package",
  "features": ["default", "production", "enable-bob"],
  "requiredFeatures": ["enable-bob", "enable-sally", "enable-feature"],
  "os": "ubuntu-latest",
  ...
}
```

**Configuration File Usage:**

```toml
[[config]]
os = "ubuntu"
required-features = ["enable-*", "production"]
```

The wildcards will be expanded when the configuration is processed, ensuring the JSON output contains concrete feature names.

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

**Wildcard Pattern Support:**

The `--packages` flag supports wildcard patterns for selecting packages:

```bash
# Process all packages starting with moosicbox_
clippier features . --packages "moosicbox_*" --output json

# Process all server packages except test servers
clippier features . --packages "*_server,!*_test_server" --output json

# Process specific API packages
clippier features . --packages "moosicbox_*_api" --output json

# Mix wildcards with exact names
clippier features . --packages "moosicbox_server,moosicbox_*_api,core" --output json
```

**Property-Based Filtering:**

```bash
# Filter by package properties (exclude unpublished and examples)
clippier features . \
  --skip-if "package.publish=false" \
  --skip-if "package.name$=_example" \
  --output json

# Include only packages with specific characteristics
clippier features . \
  --include-if "package.name^=moosicbox_" \
  --include-if "package.categories@=audio" \
  --output json
```

**Combined Filtering:**

```bash
# Combine wildcards with other filters
clippier features . \
  --packages "moosicbox_*_server" \
  --os ubuntu \
  --features "enable-*,production" \
  --skip-features "test-*" \
  --output json
```

This is particularly useful for:

- **Focused testing**: Test only specific packages during development
- **CI optimization**: Build matrix for selected components based on criteria
- **Monorepo management**: Process subsets of large workspaces with naming conventions
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

#### Overriding Validation Errors

Sometimes you need to suppress specific validation errors on a case-by-case basis. Clippier supports three methods for overriding validation failures, with clear precedence rules:

**Override Precedence (Highest to Lowest):**

1. CLI arguments (temporary overrides for testing)
2. Package-level `clippier.toml` (package-specific configuration)
3. Package-level `Cargo.toml` metadata (inline configuration)
4. Workspace-level `clippier.toml` (workspace-wide defaults)

**CLI Overrides (Quick Testing):**

```bash
# Allow a specific missing propagation
clippier validate-feature-propagation . \
  --features "fail-on-warnings" \
  --allow-missing "server:fail-on-warnings:legacy_dep"

# Allow missing propagation for all packages
clippier validate-feature-propagation . \
  --allow-missing "fail-on-warnings:legacy_dep"

# Ignore specific packages entirely
clippier validate-feature-propagation . \
  --ignore-package "experimental_*" \
  --ignore-package "wip_module"

# Ignore specific features globally
clippier validate-feature-propagation . \
  --ignore-feature "unstable-*"
```

**Package-Level clippier.toml (Recommended for Persistent Overrides):**

```toml
# packages/server/clippier.toml
[feature-validation]
[[feature-validation.override]]
feature = "fail-on-warnings"
dependency = "legacy_tcp"
type = "allow-missing"
reason = "Legacy dependency doesn't support fail-on-warnings - tracked in #123"
expires = "2025-12-31"

[[feature-validation.override]]
feature = "async-*"
dependency = "sync_util"
type = "allow-missing"
reason = "Utility crate is intentionally synchronous"
```

**Workspace-Level clippier.toml (Workspace-Wide Policies):**

```toml
# {workspace_root}/clippier.toml
[feature-validation]
[[feature-validation.override]]
feature = "*"
dependency = "external_vendor_*"
type = "allow-missing"
reason = "Vendor dependencies don't follow our feature conventions"
```

**Package Cargo.toml Metadata (Inline Configuration):**

```toml
# packages/server/Cargo.toml
[package.metadata.clippier.feature-validation]
[[package.metadata.clippier.feature-validation.override]]
feature = "fail-on-warnings"
dependency = "some_dep"
type = "allow-missing"
reason = "Dependency issue tracked in #456"
```

**Override Types:**

- `allow-missing` - Allow a specific missing propagation
- `allow-incorrect` - Allow a specific incorrect propagation
- `suppress` - Suppress all validation for matching cases

**Wildcard Support:**

Overrides support wildcard patterns for flexible matching:

```toml
[[feature-validation.override]]
feature = "async-*"        # Matches async-io, async-runtime, etc.
dependency = "*_sync"      # Matches util_sync, core_sync, etc.
type = "allow-missing"
reason = "Sync dependencies don't support async features"
```

**Expiration Dates:**

Add expiration dates to temporary overrides to ensure they're revisited:

```toml
[[feature-validation.override]]
feature = "fail-on-warnings"
dependency = "legacy_dep"
type = "allow-missing"
reason = "Migration in progress"
expires = "2025-12-31"  # RFC 3339 or YYYY-MM-DD format
```

**Output with Overrides:**

When overrides are applied, the validation output includes a summary:

```
🔍 Feature Propagation Validation Results
=========================================
Total packages checked: 147
Valid packages: 147

📋 Override Summary:
  Applied: 3 overrides
    - cli: 1
    - package-clippier-toml: 2

🔕 Overridden Errors (3):
  📦 server:fail-on-warnings:legacy_tcp
    Reason: Legacy dependency migration in progress
    Source: PackageClippierToml
    Expires: 2025-12-31

✅ All packages correctly propagate features (with 3 overrides)!
```

**Advanced Override Options:**

```bash
# Fail if any overrides have expired
clippier validate-feature-propagation . \
  --fail-on-expired

# Show detailed override information
clippier validate-feature-propagation . \
  --verbose-overrides
```

#### Parent Package Validation

Parent packages are packages that aggregate and re-export features from their workspace dependencies. The validator can ensure that parent packages correctly expose all features from their dependencies with appropriate naming conventions.

**What is a Parent Package?**

A parent package typically:

- Depends on multiple workspace packages
- Re-exports features from those dependencies using a prefix pattern
- Acts as a facade for a subsystem of the workspace

For example, if `moosicbox_app` depends on `moosicbox_audio` which has features `mp3`, `flac`, and `aac`, the parent should expose them as `audio-mp3`, `audio-flac`, and `audio-aac`.

**CLI Usage:**

```bash
# Validate specific packages as parent packages
clippier validate-feature-propagation . \
  --parent-packages "moosicbox_app,moosicbox_server"

# Limit depth of dependency chain checking
clippier validate-feature-propagation . \
  --parent-packages "moosicbox_app" \
  --parent-depth 2

# Skip additional features during parent validation
clippier validate-feature-propagation . \
  --parent-packages "moosicbox_app" \
  --parent-skip-features "unstable,experimental"

# Override prefix for specific dependencies
clippier validate-feature-propagation . \
  --parent-packages "moosicbox_app" \
  --parent-prefix "moosicbox_audio:audio" \
  --parent-prefix "moosicbox_video:vid"

# Disable loading parent config from clippier.toml
clippier validate-feature-propagation . \
  --parent-packages "moosicbox_app" \
  --no-parent-config
```

**Package-Level Configuration (clippier.toml):**

```toml
# packages/moosicbox_app/clippier.toml
[feature-validation]

# Declare this package as a parent package
[feature-validation.parent]
enabled = true
depth = 2  # Only check direct deps and their deps (optional)
skip-features = ["internal-*", "test-*"]  # Additional features to skip

# Override prefixes for specific dependencies
[[feature-validation.parent.prefix]]
dependency = "moosicbox_audio"
prefix = "audio"

[[feature-validation.parent.prefix]]
dependency = "moosicbox_video"
prefix = "video"
```

**Workspace-Level Configuration (clippier.toml):**

```toml
# {workspace_root}/clippier.toml
[feature-validation]

# Declare parent packages at workspace level
[[feature-validation.parent-packages]]
package = "moosicbox_app"
depth = 2

[[feature-validation.parent-packages]]
package = "moosicbox_server"

# Global prefix overrides
[[feature-validation.parent-prefix]]
dependency = "moosicbox_audio"
prefix = "audio"
```

**Understanding Prefix Inference:**

By default, the validator infers the prefix from the dependency name:

- `moosicbox_audio` → `audio`
- `switchy_database` → `database`
- `my_lib` → `lib`

The prefix is derived by taking the last segment after underscores. You can override this with explicit prefix configuration.

**Validation Output:**

```
🔍 Feature Propagation Validation Results
=========================================
Total packages checked: 147
Valid packages: 147

📦 Parent Package Validation
=============================

📦 Package: moosicbox_app
  Dependencies checked: 5
  Features checked: 42
  Features correctly exposed: 40

  ❌ Missing Feature Exposures:
    Dependency: moosicbox_audio
      Feature: experimental-codec
      Expected parent feature: audio-experimental-codec
      Expected propagation: moosicbox_audio?/experimental-codec
      Depth: 1

    Dependency: moosicbox_video (via moosicbox_media)
      Feature: av1
      Expected parent feature: video-av1
      Expected propagation: moosicbox_video?/av1
      Depth: 2
      Chain: moosicbox_app -> moosicbox_media -> moosicbox_video
```

**Parent Validation Options:**

| Option                   | Description                                           | Default             |
| ------------------------ | ----------------------------------------------------- | ------------------- |
| `--parent-packages`      | Packages to validate as parent packages               | From config         |
| `--parent-depth`         | Max depth for nested dependency checking (None = all) | None (unlimited)    |
| `--parent-skip-features` | Additional features to skip                           | `["default", "_*"]` |
| `--parent-prefix`        | Override prefix for dependencies (`dep:prefix`)       | Auto-inferred       |
| `--no-parent-config`     | Disable loading parent config from clippier.toml      | false               |

### Check Command (Linting)

Run all available linters in your workspace with automatic tool detection:

```bash
# Run all detected linters
clippier check

# Run in a specific directory
clippier check --working-dir /path/to/project

# Run only specific tools
clippier check --tools "clippy,eslint"

# List available tools without running them
clippier check --list

# Require specific tools (fail if not installed)
clippier check --required "clippy,prettier"

# Skip specific tools
clippier check --skip "shellcheck"

# JSON output for CI integration
clippier check --output json
```

The `check` command automatically detects and runs:

- **Rust**: `cargo clippy` (with `-D warnings` for zero-warnings policy)
- **TOML**: `taplo fmt --check`
- **JavaScript/TypeScript**: `prettier --check`, `biome check`, `eslint`
- **Python**: `ruff check`, `black --check`
- **Go**: `gofmt -l`
- **Shell**: `shfmt -d`, `shellcheck`

Tools run in parallel by default for maximum performance.

### Fmt Command (Formatting)

Run all available formatters to fix formatting issues:

```bash
# Format all files
clippier fmt

# Check formatting without modifying files
clippier fmt --check

# Run in a specific directory
clippier fmt --working-dir /path/to/project

# Run only specific formatters
clippier fmt --tools "rustfmt,prettier"

# List available formatters without running them
clippier fmt --list

# Require specific formatters (fail if not installed)
clippier fmt --required "rustfmt"

# Skip specific formatters
clippier fmt --skip "gofmt"

# JSON output for CI integration
clippier fmt --output json
```

The `fmt` command automatically detects and runs:

- **Rust**: `cargo fmt`
- **TOML**: `taplo fmt`
- **JavaScript/TypeScript**: `prettier --write`, `biome format --write`
- **Python**: `ruff format`, `black`
- **Go**: `gofmt -w`
- **Shell**: `shfmt -w`

#### Supported Tools

| Tool         | Language/Format         | Capabilities | Detection           |
| ------------ | ----------------------- | ------------ | ------------------- |
| `rustfmt`    | Rust                    | Format       | `cargo` in PATH     |
| `clippy`     | Rust                    | Lint         | `cargo` in PATH     |
| `taplo`      | TOML                    | Format, Lint | `taplo` binary      |
| `prettier`   | JS/TS/JSON/MD/YAML/etc. | Format       | `prettier` binary   |
| `biome`      | JS/TS/JSON              | Format, Lint | `biome` binary      |
| `eslint`     | JS/TS                   | Lint         | `eslint` binary     |
| `ruff`       | Python                  | Format, Lint | `ruff` binary       |
| `black`      | Python                  | Format       | `black` binary      |
| `gofmt`      | Go                      | Format       | `gofmt` binary      |
| `shfmt`      | Shell                   | Format       | `shfmt` binary      |
| `shellcheck` | Shell                   | Lint         | `shellcheck` binary |

#### Output Format (JSON)

```json
{
    "success": false,
    "total": 4,
    "passed": 3,
    "failed": 1,
    "duration_ms": 2345,
    "results": [
        {
            "name": "rustfmt",
            "display_name": "Rust Formatter",
            "success": true,
            "exit_code": 0,
            "duration_ms": 1234
        },
        {
            "name": "clippy",
            "display_name": "Rust Linter",
            "success": false,
            "exit_code": 1,
            "duration_ms": 567,
            "stderr": "error: unused variable..."
        }
    ]
}
```

#### CI Integration Example

```yaml
name: Lint & Format Check
on: [push, pull_request]

jobs:
    check:
        runs-on: ubuntu-latest
        steps:
            - uses: actions/checkout@v4

            - name: Install tools
              run: |
                  cargo install clippier taplo-cli
                  npm install -g prettier

            - name: Check formatting
              run: clippier fmt --check

            - name: Run linters
              run: clippier check
```

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

| Option                | Description                                                             | Default      |
| --------------------- | ----------------------------------------------------------------------- | ------------ |
| `--os`                | Target operating system                                                 | -            |
| `--offset`            | Skip first N features                                                   | 0            |
| `--max`               | Maximum number of features                                              | All          |
| `--max-parallel`      | Maximum parallel jobs                                                   | -            |
| `--chunked`           | Group features into chunks                                              | -            |
| `--spread`            | Spread features across jobs                                             | false        |
| `--randomize`         | Randomize features before chunking/spreading                            | false        |
| `--seed`              | Seed for deterministic randomization                                    | -            |
| `--features`          | Features to include (supports wildcards `*`, `?` and negation `!`)      | -            |
| `--skip-features`     | Features to exclude (supports wildcards `*`, `?` and negation `!`)      | -            |
| `--required-features` | Always-required features (supports wildcards `*`, `?` and negation `!`) | -            |
| `--packages`          | Packages to process (supports wildcards `*`, `?` and negation `!`)      | All packages |
| `--changed-files`     | Filter by changed files                                                 | -            |
| `--git-base`          | Git base commit for external dep analysis                               | -            |
| `--git-head`          | Git head commit for external dep analysis                               | -            |
| `--skip-if`           | Skip packages matching Cargo.toml filter                                | -            |
| `--include-if`        | Include only packages matching filter                                   | -            |

### Packages Command Options

| Option                | Description                                                        | Default      |
| --------------------- | ------------------------------------------------------------------ | ------------ |
| `--os`                | Target operating system                                            | `ubuntu`     |
| `--packages`          | Packages to include (supports wildcards `*`, `?` and negation `!`) | All packages |
| `--changed-files`     | Filter by changed files                                            | -            |
| `--git-base`          | Git base commit for change detection                               | -            |
| `--git-head`          | Git head commit for change detection                               | -            |
| `--include-reasoning` | Include reasoning for affected packages                            | false        |
| `--max-parallel`      | Maximum number of packages to return                               | -            |
| `--skip-if`           | Skip packages matching Cargo.toml filter                           | -            |
| `--include-if`        | Include only packages matching filter                              | -            |
| `--output`            | Output format: `json`, `raw`                                       | `json`       |

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

| Option                           | Description                                             | Default             |
| -------------------------------- | ------------------------------------------------------- | ------------------- |
| `--features`                     | Comma-separated list of features to validate            | Auto-detect         |
| `--skip-features`                | Features to skip during validation (supports wildcards) | `["default", "_*"]` |
| `--workspace-only`               | Only validate workspace packages                        | true                |
| `--output`                       | Output format: `json`, `raw`                            | `raw`               |
| `--path`                         | Workspace root path                                     | Current directory   |
| `--fail-on-error`                | Exit with error code if validation fails                | true                |
| `--strict-optional`              | Require `dep?/feature` syntax for optional deps         | false               |
| `--allow-missing`                | Allow specific missing propagations                     | -                   |
| `--allow-incorrect`              | Allow specific incorrect propagations                   | -                   |
| `--ignore-package`               | Suppress validation for specific packages               | -                   |
| `--ignore-feature`               | Suppress validation for specific features               | -                   |
| `--use-config-overrides`         | Load overrides from clippier.toml files                 | true                |
| `--use-cargo-metadata-overrides` | Load overrides from Cargo.toml metadata                 | true                |
| `--warn-expired`                 | Warn about expired overrides                            | true                |
| `--fail-on-expired`              | Fail validation if expired overrides exist              | false               |
| `--verbose-overrides`            | Show detailed override information                      | false               |
| `--parent-packages`              | Packages to validate as parent packages                 | From config         |
| `--parent-depth`                 | Max depth for nested dependency checking                | None (unlimited)    |
| `--parent-skip-features`         | Additional features to skip for parent validation       | -                   |
| `--parent-prefix`                | Override prefix for dependencies (`dep:prefix`)         | Auto-inferred       |
| `--no-parent-config`             | Disable loading parent config from clippier.toml        | false               |

### Check Command Options

| Option          | Description                                     | Default           |
| --------------- | ----------------------------------------------- | ----------------- |
| `--working-dir` | Working directory to run in                     | Current directory |
| `--tools`       | Specific tools to run (comma-separated)         | All detected      |
| `--list`        | List available tools instead of running them    | false             |
| `--required`    | Tools that MUST be installed (error if missing) | -                 |
| `--skip`        | Tools to skip even if detected                  | -                 |
| `--output`      | Output format: `json`, `raw`                    | `raw`             |

### Fmt Command Options

| Option          | Description                                     | Default           |
| --------------- | ----------------------------------------------- | ----------------- |
| `--working-dir` | Working directory to run in                     | Current directory |
| `--check`       | Only check formatting without modifying files   | false             |
| `--tools`       | Specific tools to run (comma-separated)         | All detected      |
| `--list`        | List available tools instead of running them    | false             |
| `--required`    | Tools that MUST be installed (error if missing) | -                 |
| `--skip`        | Tools to skip even if detected                  | -                 |
| `--output`      | Output format: `json`, `raw`                    | `raw`             |

## Configuration

Clippier can be configured using `clippier.toml` files at two levels:

### Workspace-Level Configuration

Place a `clippier.toml` at the workspace root to define defaults for all packages:

```toml
# {workspace_root}/clippier.toml
# Workspace-level defaults apply to all packages unless overridden

nightly = false

[env]
RUST_BACKTRACE = "1"
CARGO_TERM_COLOR = "always"

[[ci-steps]]
command = "cargo fmt --check"

[[dependencies]]
command = "sudo apt-get update"

[[dependencies]]
command = "sudo apt-get install -y pkg-config libssl-dev"
```

### Package-Level Configuration

Place a `clippier.toml` in individual package directories to override or extend workspace defaults:

```toml
# packages/{package}/clippier.toml
# Package-level config merges with workspace defaults

[env]
PACKAGE_SPECIFIC_VAR = "custom_value"

[[config]]
os = "ubuntu-latest"
nightly = false
cargo = ["build", "test", "clippy"]

[config.env]
RUST_BACKTRACE = "1"
CARGO_TERM_COLOR = "always"

# System dependencies for Docker generation
[[config.dependencies]]
command = "sudo apt-get update"

[[config.dependencies]]
command = "sudo apt-get install -y pkg-config libssl-dev libasound2-dev"

# Feature-specific dependencies
[[config.dependencies]]
command = "sudo apt-get install -y libsqlite3-dev"
features = ["database"]

[parallelization]
chunked = 4
```

### Configuration Precedence

Configuration values are resolved in the following order (highest to lowest priority):

1. **Config-specific** - `[[config]]` section in package's `clippier.toml`
2. **Package-level** - Top-level values in package's `clippier.toml`
3. **Workspace defaults** - Values in workspace root `clippier.toml`
4. **Built-in defaults** - Hardcoded fallback values

### Configuration Features

- **Workspace-level defaults**: Set organization-wide configuration once
- **Package-level overrides**: Customize specific packages as needed
- **Feature-specific dependencies**: Dependencies can be conditionally included based on enabled features
- **Multiple OS configurations**: Support for different operating systems
- **Environment variable management**: Configurable environment variables at workspace and package levels
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

| Operator | Description        | Example                               |
| -------- | ------------------ | ------------------------------------- |
| `=`      | Exact match        | `package.publish=false`               |
| `!=`     | Not equal          | `package.version!=0.1.0`              |
| `^=`     | Starts with        | `package.name^=moosicbox_`            |
| `$=`     | Ends with          | `package.name$=_example`              |
| `*=`     | Contains substring | `package.description*=audio`          |
| `~=`     | Regex match        | `package.name~=^moosicbox_.*_server$` |

#### Array Operators

Match against array properties (keywords, categories, authors, etc.):

| Operator | Description                           | Example                            |
| -------- | ------------------------------------- | ---------------------------------- |
| `@=`     | Array contains exact element          | `package.categories@=audio`        |
| `@*=`    | Array contains element with substring | `package.keywords@*=music`         |
| `@^=`    | Array contains element starting with  | `package.keywords@^=api-`          |
| `@~=`    | Array contains element matching regex | `package.categories@~=^multimedia` |
| `@!`     | Array is empty                        | `package.keywords@!`               |
| `@#=`    | Array length equals                   | `package.keywords@#=3`             |
| `@#>`    | Array length greater than             | `package.authors@#>1`              |
| `@#<`    | Array length less than                | `package.categories@#<5`           |
| `!@=`    | Array does NOT contain                | `package.keywords!@=deprecated`    |

#### Existence Operators

Check if properties exist:

| Operator | Description             | Example              |
| -------- | ----------------------- | -------------------- |
| `?`      | Property exists         | `package.readme?`    |
| `!?`     | Property does NOT exist | `package.homepage!?` |

### Logical Operators and Expressions

You can combine multiple filter conditions using logical operators to create complex expressions:

| Operator | Description                      | Example                                                                   |
| -------- | -------------------------------- | ------------------------------------------------------------------------- |
| `AND`    | Both conditions must be true     | `package.publish=false AND version^=0.1`                                  |
| `OR`     | At least one condition is true   | `package.publish=false OR name$=_example`                                 |
| `NOT`    | Inverts the condition            | `NOT package.publish=false`                                               |
| `( )`    | Groups conditions for precedence | `(package.publish=false OR package.name$=_test) AND package.version^=0.1` |

**Operator Precedence** (highest to lowest):

1. `NOT`
2. `AND`
3. `OR`

**Case Insensitive**: Keywords can be written as `AND`, `and`, `And`, etc.

**Quoted Values**: Use double quotes for values containing spaces or special characters:

- `package.name="my package"` - Matches packages with spaces in name
- `package.description="This AND that"` - Quotes prevent "AND" from being treated as operator

**Escape Sequences** in quoted strings:

- `\"` - Double quote
- `\\` - Backslash
- `\n` - Newline
- `\t` - Tab

### Usage Examples

#### Skip Unpublished Packages

```bash
# Exclude packages with publish = false
clippier features . --skip-if "package.publish=false" --output json
```

#### Include Only Specific Package Prefixes

```bash
# Only process moosicbox packages
clippier features . --include-if "package.name^=moosicbox_" --output json

# Exclude example packages
clippier packages . --skip-if "package.name$=_example" --output json
```

#### Filter by Categories or Keywords

```bash
# Only packages with audio category
clippier features . --include-if "package.categories@=audio" --output json

# Packages containing "api" in keywords
clippier features . --include-if "package.keywords@*=api" --output json

# Skip packages with empty keywords
clippier features . --skip-if "package.keywords@!" --output json
```

#### Array Length Filtering

```bash
# Only packages with 3+ keywords (well-documented)
clippier features . --include-if "package.keywords@#>2" --output json

# Packages with exactly 2 categories
clippier features . --include-if "package.categories@#=2" --output json
```

#### Nested Metadata Access

```bash
# Only independent workspace packages
clippier features . \
  --include-if "package.metadata.workspaces.independent=true" \
  --output json

# Skip packages with custom CI configuration
clippier packages . --skip-if "package.metadata.ci.skip-tests=true"
```

#### Combining Multiple Filters

**Using separate filter arguments (OR logic for skip, AND logic for include):**

```bash
# Include moosicbox packages, exclude examples and unpublished
clippier features . \
  --include-if "package.name^=moosicbox_" \
  --skip-if "package.name$=_example" \
  --skip-if "package.publish=false" \
  --output json

# Audio packages with sufficient documentation
clippier features . \
  --include-if "package.categories@=audio" \
  --include-if "package.keywords@#>2" \
  --include-if "package.readme?" \
  --output json
```

**Using logical expressions within a single filter:**

```bash
# Exclude examples OR unpublished packages
clippier features . \
  --skip-if "package.name$=_example OR publish=false" \
  --output json

# Include published moosicbox packages that are NOT examples
clippier features . \
  --include-if "package.name^=moosicbox_ AND publish=true AND NOT name$=_example" \
  --output json

# Complex: audio/video packages with good docs
clippier features . \
  --include-if "(package.categories@=audio OR package.categories@=video) AND package.keywords@#>2 AND package.readme?" \
  --output json

# Skip test packages or packages with specific metadata
clippier features . \
  --skip-if "package.name$=_test OR (metadata.ci? AND metadata.ci.skip=true)" \
  --output json
```

### Filter Logic

**Skip Filters (`--skip-if`):**

- Multiple skip filter arguments use **OR** logic
- Each filter argument can be a complex expression with `AND`, `OR`, `NOT`
- If **ANY** skip filter matches, the package is excluded
- Processed after include filters

**Include Filters (`--include-if`):**

- Multiple include filter arguments use **AND** logic
- Each filter argument can be a complex expression with `AND`, `OR`, `NOT`
- **ALL** include filters must match for a package to be included

**Expression Evaluation:**

Within each filter argument:

- `AND` requires both sides to be true
- `OR` requires at least one side to be true
- `NOT` inverts the result
- Parentheses `()` control precedence

**Examples:**

```bash
# Skip: Exclude if name ends with _example OR publish is false
--skip-if "package.name$=_example OR publish=false"
# Equivalent to separate args: --skip-if "package.name$=_example" --skip-if "package.publish=false"

# Include: Must match name prefix AND (one of the categories)
--include-if "package.name^=moosicbox_ AND (categories@=audio OR categories@=video)"

# Complex: Skip unpublished non-library packages
--skip-if "package.publish=false AND NOT package.name$=_lib"

# Multiple arguments with AND logic between them
--include-if "package.name^=moosicbox_" --include-if "package.categories@=audio"
# Both filters must match: name must start with moosicbox_ AND have audio category
```

### Property Paths

Access any Cargo.toml property using dot notation with the full path:

- `package.name`, `package.version`, `package.edition` - Standard package properties
- `package.publish`, `package.categories`, `package.keywords` - Package metadata
- `package.metadata.custom.field` - Custom nested metadata
- `dependencies.serde.version` - Dependency information
- `features.default` - Feature configuration
- `workspace.members` - Workspace configuration

### Practical Use Cases

#### CI/CD Optimization

```bash
# Test only published, non-example packages
clippier features . \
  --skip-if "package.publish=false" \
  --skip-if "package.name$=_example" \
  --max 20 \
  --output json
```

#### Monorepo Component Isolation

```bash
# Test only frontend packages (by naming convention)
clippier features . \
  --include-if "package.name*=_ui" \
  --include-if "package.name*=_web" \
  --output json

# Backend services only
clippier features . \
  --include-if "package.name*=_server" \
  --include-if "package.name*=_service" \
  --output json
```

#### Documentation Quality Checks

```bash
# Find packages missing documentation
clippier packages . --skip-if "package.readme?" --output raw

# Well-documented packages only
clippier features . \
  --include-if "package.readme?" \
  --include-if "package.keywords@#>2" \
  --include-if "package.categories@#>0" \
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
  --required-features "production" \
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
  --required-features "production" \
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
      --skip-features "dev,test" \
      --required-features "production"
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

### Feature Skip Patterns (Wildcards & Negation)

```bash
# Wildcard patterns
--skip-features "*-default"                    # Skip all *-default features
--skip-features "test-*"                       # Skip all test-* features
--skip-features "v?"                           # Skip v1, v2, v3 (not v10)
--skip-features "*internal*"                   # Skip features containing "internal"

# Negation patterns (skip all except)
--skip-features "*,!enable-bob"                # Skip all except enable-bob
--skip-features "*,!enable-*"                  # Skip all except enable-*
--skip-features "*,!production,!default"       # Skip all except production and default

# Complex combinations
--skip-features "*-default,test-*,!test-utils" # Skip defaults and tests, keep test-utils
--skip-features "v?,!v2"                       # Skip v1, v3 but keep v2
--skip-features "*,!enable-*,!production"      # Only keep enable-* and production features

# In configuration files
[[config]]
os = "ubuntu"
skip-features = ["*-default", "test-*", "!test-utils"]
```

**Pattern Syntax:**

- `*` - Matches zero or more characters
- `?` - Matches exactly one character
- `!pattern` - Negation (excludes matching items from the result set)
- Supported in: `--features`, `--skip-features`, `--required-features`, `--packages`
- Patterns evaluated left-to-right

### Feature Inclusion Patterns (`--features`)

```bash
# Include all features starting with enable-
--features "enable-*"

# Include multiple wildcard patterns
--features "enable-*,test-*"

# Mix wildcards with exact names
--features "enable-*,production,default"

# Use negation to include all except specific features
--features "*,!test-*"

# Include enable-* except enable-experimental
--features "enable-*,!enable-experimental"

# Combine with skip-features for precise control
--features "enable-*,test-*" --skip-features "test-integration"
```

**Note:** `--features` expands wildcards to **include** matching features. Supports negation with `!` prefix to exclude specific patterns.

### Required Features Patterns (`--required-features`)

```bash
# Require all features starting with enable-
--required-features "enable-*"

# Require multiple wildcard patterns
--required-features "enable-*,test-*"

# Mix wildcards with exact names
--required-features "enable-*,production,default"

# Use negation to require enable-* except enable-experimental
--required-features "enable-*,!enable-experimental"
```

**Note:** Wildcards and negations are **expanded to concrete feature names** in the JSON output. This is metadata that gets included for downstream consumers.

**Output example:**

```json
{
    "requiredFeatures": ["enable-bob", "enable-sally", "enable-feature", "production"]
}
```

### Package Selection Patterns (`--packages`)

```bash
# Include all packages starting with moosicbox_
--packages "moosicbox_*"

# Include all server packages
--packages "*_server"

# Mix wildcards with exact names
--packages "moosicbox_server,moosicbox_*_api,core"

# Use negation to include all except test packages
--packages "*,!*_test"

# Include all server packages except test servers
--packages "*_server,!*_test_server"

# Select specific API packages
--packages "moosicbox_*_api"
```

**Note:** `--packages` supports negation with `!` prefix to exclude specific packages from the selection.

### Property-Based Filter Syntax

```bash
# Scalar operators
--skip-if "package.publish=false"              # Exact match
--include-if "package.name^=moosicbox_"        # Starts with
--skip-if "package.name$=_example"             # Ends with
--include-if "package.description*=audio"      # Contains
--include-if "package.name~=^test_.*"          # Regex match

# Array operators
--include-if "package.categories@=audio"       # Array contains
--include-if "package.keywords@*=api"          # Array element contains substring
--include-if "package.keywords@^=music"        # Array element starts with
--include-if "package.categories@~=^multi"     # Array element matches regex
--skip-if "package.keywords@!"                 # Array is empty
--include-if "package.keywords@#=3"            # Array length equals
--include-if "package.authors@#>1"             # Array length greater than
--include-if "package.categories@#<5"          # Array length less than
--skip-if "package.keywords!@=deprecated"      # Array does NOT contain

# Existence operators
--include-if "package.readme?"                 # Property exists
--skip-if "package.homepage!?"                 # Property does NOT exist

# Nested properties
--include-if "package.metadata.workspaces.independent=true"
--skip-if "package.metadata.ci.skip-tests=true"

# Combining filters
clippier features . \
  --include-if "package.name^=moosicbox_" \
  --skip-if "package.name$=_example" \
  --skip-if "package.publish=false"
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
clippier packages . --skip-if "package.publish=false" --output json
clippier packages . --include-if "package.categories@=audio" --output json

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

# Multiple packages (exact names)
clippier features . --packages pkg1,pkg2,pkg3

# Wildcard patterns - all packages with prefix
clippier features . --packages "moosicbox_*"

# Wildcard patterns - all server packages
clippier features . --packages "*_server"

# Wildcard patterns - all API packages
clippier features . --packages "moosicbox_*_api"

# Mix wildcards with exact names
clippier features . --packages "moosicbox_server,moosicbox_*_api,core"

# Combined with OS filter
clippier features . --packages "moosicbox_*_server" --os ubuntu

# Combined with feature wildcards
clippier features . --packages "*_server" --features "enable-*,production"

# Combined with all wildcard features (comprehensive example)
clippier features . \
  --packages "moosicbox_*_server" \
  --features "enable-*,production" \
  --skip-features "test-*,!test-utils" \
  --required-features "enable-*"

# Combined with change detection
clippier features . --packages "moosicbox_*" --changed-files "src/main.rs"

# With chunking for CI
clippier features . --packages "*_server,*_api" --chunked 5 --max-parallel 10

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
