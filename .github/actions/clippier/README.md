# Clippier GitHub Action

A powerful, flexible GitHub Action for generating CI matrices and analyzing package dependencies using [clippier](../../../packages/clippier).

## Features

- 🎯 **Smart Matrix Generation** - Generate feature matrices for comprehensive testing
- 🔍 **Change Detection** - Analyze affected packages based on file changes
- 🤖 **Intelligent Git Detection** - Automatically detects git ranges, handles force pushes, and finds valid commits via GitHub API
- 🔄 **Custom Reasoning** - Inject custom reasoning into package analysis
- ✅ **Additional Checks** - Run multiple package checks with custom summary sections
- 🐳 **Docker Integration** - Optional Docker package analysis and matrix generation
- 📊 **Rich Summaries** - Optional GitHub workflow summary generation with custom sections
- 🎲 **Deterministic Randomization** - Reproducible test matrices with seed support
- 🔄 **Flexible Transformation** - Package name transformation via regex
- ⚡ **Smart Caching** - Automatic clippier binary caching

## Quick Start

### Basic Usage

```yaml
- name: Generate test matrix
  id: matrix
  uses: ./.github/actions/clippier
  with:
      command: features
      workspace-path: .

- name: Run tests
  strategy:
      matrix: ${{ fromJson(steps.matrix.outputs.matrix) }}
  runs-on: ${{ matrix.os }}
  steps:
      - run: cargo test --package ${{ matrix.name }}
```

### With Auto Git Detection

```yaml
- name: Generate matrix with smart change detection
  id: matrix
  uses: ./.github/actions/clippier
  with:
      command: features
      workspace-path: .
      git-strategy: workflow-history # Automatically handles force pushes!
      changed-files: ${{ steps.files.outputs.all }}
```

### With Additional Package Checks

```yaml
- name: Generate matrix and check critical packages
  id: matrix
  uses: ./.github/actions/clippier
  with:
      command: features
      additional-package-checks: |
          [
            {
              "package": "my_app",
              "output-key": "app",
              "summary-section": {
                "title": "📱 App Deployment",
                "show-reasoning": true,
                "status-labels": {
                  "affected": "Ready to deploy",
                  "not-affected": "No deployment needed"
                }
              }
            }
          ]

# Use the outputs
- name: Deploy app
  if: ${{ fromJson(steps.matrix.outputs.additional-checks).app.affected }}
  run: ./deploy-app.sh
```

## Core Inputs

### Command Configuration

| Input            | Description                                                                                          | Required | Default |
| ---------------- | ---------------------------------------------------------------------------------------------------- | -------- | ------- |
| `command`        | Clippier command (`features`, `affected-packages`, `workspace-deps`, `validate-feature-propagation`) | Yes      | -       |
| `workspace-path` | Path to workspace/package                                                                            | No       | `.`     |

### Features Command Options

| Input               | Description                                    | Required | Default |
| ------------------- | ---------------------------------------------- | -------- | ------- |
| `os`                | Target operating system                        | No       | -       |
| `offset`            | Skip first N features                          | No       | -       |
| `max`               | Maximum number of features                     | No       | -       |
| `max-parallel`      | Maximum parallel jobs                          | No       | -       |
| `chunked`           | Group features into chunks                     | No       | -       |
| `spread`            | Spread features across jobs                    | No       | `false` |
| `randomize`         | Randomize features before chunking/spreading   | No       | `false` |
| `seed`              | Seed for deterministic randomization           | No       | -       |
| `features`          | Specific features to include (comma-separated) | No       | -       |
| `skip-features`     | Features to exclude (comma-separated)          | No       | -       |
| `required-features` | Always-required features (comma-separated)     | No       | -       |
| `packages`          | Specific packages to process (comma-separated) | No       | -       |
| `skip-if`           | Skip packages matching Cargo.toml filters      | No       | -       |
| `include-if`        | Include only packages matching filters         | No       | -       |

### Property-Based Package Filtering

Filter packages based on their Cargo.toml properties using `skip-if` and `include-if`:

**Format:** `property[.nested]<operator>value`

**Supported Operators:**

- **Scalar**: `=`, `!=`, `^=`, `$=`, `*=`, `~=` (equals, not equals, starts with, ends with, contains, regex)
- **Array**: `@=`, `@*=`, `@^=`, `@~=`, `@!`, `@#=`, `@#>`, `@#<`, `!@=` (contains, substring, starts with, regex, empty, length comparisons, not contains)
- **Existence**: `?`, `!?` (exists, not exists)

**Examples:**

```yaml
# Skip unpublished packages
- uses: ./.github/actions/clippier
  with:
      command: features
      skip-if: 'publish=false'

# Include only moosicbox packages, exclude examples
- uses: ./.github/actions/clippier
  with:
      command: features
      include-if: 'name^=moosicbox_'
      skip-if: 'name$=_example,publish=false'

# Multiple filters (newline-separated)
- uses: ./.github/actions/clippier
  with:
      command: packages
      include-if: |
          categories@=audio
          readme?
          keywords@#>2
      skip-if: |
          publish=false
          name$=_example

# Nested metadata access
- uses: ./.github/actions/clippier
  with:
      command: features
      include-if: 'metadata.workspaces.independent=true'
```

**Logical Operators:**

Combine conditions using `AND`, `OR`, `NOT`, and `()` for grouping:

```yaml
# Complex expression - exclude examples OR unpublished packages
- uses: ./.github/actions/clippier
  with:
      command: features
      skip-if: 'name$=_example OR publish=false'

# Include published moosicbox packages that are NOT examples
- uses: ./.github/actions/clippier
  with:
      command: features
      include-if: 'name^=moosicbox_ AND publish=true AND NOT name$=_example'

# Grouping with parentheses
- uses: ./.github/actions/clippier
  with:
      command: features
      include-if: '(categories@=audio OR categories@=video) AND keywords@#>2 AND readme?'

# Quoted values for spaces or keywords in values
- uses: ./.github/actions/clippier
  with:
      command: features
      skip-if: 'description="Internal AND Testing"'
```

**Operator Precedence**: `NOT` > `AND` > `OR`
**Case Insensitive**: `AND`, `and`, `And` all work

**Common Use Cases:**

- Skip unpublished/internal packages in CI
- Filter by categories or keywords
- Require documentation quality (README, keywords)
- Filter by custom metadata flags
- Component isolation by naming conventions
- Complex multi-condition filtering with boolean logic

## Enhanced Features

### Smart Git Detection

The action automatically detects git ranges and handles complex scenarios:

| Input                | Description                                                         | Default                    |
| -------------------- | ------------------------------------------------------------------- | -------------------------- |
| `git-strategy`       | Strategy: `auto`, `workflow-history`, `branch-comparison`, `manual` | `auto`                     |
| `git-base`           | Git base commit (only for manual strategy)                          | -                          |
| `git-head`           | Git head commit (only for manual strategy)                          | -                          |
| `git-compare-branch` | Branch to compare against (for branch-comparison strategy)          | `master`                   |
| `git-workflow-name`  | Workflow name for API lookups                                       | Current workflow           |
| `git-default-branch` | Default branch name                                                 | `master`                   |
| `github-token`       | GitHub token for API access                                         | `${{ github.token }}`      |
| `github-repository`  | Repository for API lookups                                          | `${{ github.repository }}` |
| `github-ref-name`    | Current ref name                                                    | `${{ github.ref_name }}`   |

**Strategies:**

- **`auto`** - Detects based on event type (PR, push, workflow_dispatch, etc.)
- **`workflow-history`** - Uses GitHub API to find valid commits after force pushes
- **`branch-comparison`** - Compare current branch against a target branch (see below)
- **`manual`** - Uses provided `git-base` and `git-head`

#### Branch Comparison Strategy

Perfect for feature branch workflows where you want to see "what changed since master":

```yaml
- uses: ./.github/actions/clippier
  with:
      command: features
      git-strategy: branch-comparison
      git-compare-branch: master # or 'main', 'develop', etc.
```

**How it works:**

1. Finds common ancestor (merge-base) between HEAD and target branch
2. Compares HEAD against that ancestor
3. Shows all changes introduced in the current branch

**Use cases:**

- Feature branch testing before merge
- Manual workflow dispatches on feature branches
- "What will change when I merge this?"
- Scheduled branch checks

**Example - Feature Branch Testing:**

```yaml
name: Test Feature Branch
on: workflow_dispatch

jobs:
    test:
        runs-on: ubuntu-latest
        steps:
            - uses: actions/checkout@v4
              with:
                  fetch-depth: 0 # Need full history for merge-base

            - name: Analyze changes vs master
              uses: ./.github/actions/clippier
              with:
                  command: features
                  git-strategy: branch-comparison
                  git-compare-branch: master
```

**Other Examples:**

```yaml
# Workflow History (handles force pushes)
- uses: ./.github/actions/clippier
  with:
      git-strategy: workflow-history
      changed-files: ${{ steps.files.outputs.all }}
```

### Custom Reasoning Injection

Inject custom reasoning into all packages (useful for manual/scheduled runs):

| Input                        | Description                   | Default |
| ---------------------------- | ----------------------------- | ------- |
| `inject-reasoning`           | Custom reasoning to inject    | -       |
| `inject-reasoning-condition` | Condition to inject reasoning | `true`  |

**Example:**

```yaml
- uses: ./.github/actions/clippier
  with:
      command: features
      inject-reasoning: |
          ${{ github.event_name == 'workflow_dispatch' && 'Manual workflow dispatch - all packages included' || '' }}
      inject-reasoning-condition: ${{ github.event_name == 'workflow_dispatch' }}
```

### Additional Package Checks

Run additional package checks with custom summary sections:

| Input                       | Description                                     |
| --------------------------- | ----------------------------------------------- |
| `additional-package-checks` | JSON array of package checks with configuration |

**Format:**

```json
[
    {
        "package": "package_name",
        "output-key": "custom-key",
        "summary-section": {
            "title": "📱 Custom Section",
            "show-reasoning": true,
            "show-all-affected": true,
            "status-labels": {
                "affected": "Will be triggered",
                "not-affected": "Will be skipped"
            }
        }
    }
]
```

**Example:**

```yaml
additional-package-checks: |
    [
      {
        "package": "moosicbox_app",
        "output-key": "tauri",
        "summary-section": {
          "title": "📱 Tauri Release",
          "show-reasoning": true,
          "show-all-affected": true,
          "status-labels": {
            "affected": "Will be triggered",
            "not-affected": "Will be skipped"
          }
        }
      }
    ]
```

**Accessing Results:**

All check results are returned in a single `additional-checks` JSON output:

```yaml
# Access check results using fromJson()
- if: ${{ fromJson(steps.analyze.outputs.additional-checks).tauri.affected }}
  run: echo "Tauri is affected!"

- if: ${{ fromJson(steps.analyze.outputs.additional-checks).server.affected }}
  run: echo "Server is affected!"
```

The `additional-checks` output structure:

```json
{
  "tauri": {
    "affected": true,
    "reasoning": [...]
  },
  "server": {
    "affected": false,
    "reasoning": [...]
  }
}
```

Each check result contains the full output from the `affected-packages` command, keyed by the `output-key` specified in the configuration.

### Conditional Matrix Generation

| Input                         | Description                               | Default |
| ----------------------------- | ----------------------------------------- | ------- |
| `force-full-matrix-condition` | Condition to force full matrix generation | -       |
| `skip-on-no-changes`          | Skip matrix if no changes detected        | `true`  |

**Example:**

```yaml
- uses: ./.github/actions/clippier
  with:
      command: features
      force-full-matrix-condition: ${{ github.event_name == 'workflow_dispatch' || github.event_name == 'schedule' }}
      skip-on-no-changes: true
```

### Enhanced Summaries

| Input                       | Description                           | Default                     |
| --------------------------- | ------------------------------------- | --------------------------- |
| `generate-summary`          | Generate GitHub workflow summary      | `false`                     |
| `summary-title`             | Title for the summary section         | `Smart CI Analysis Summary` |
| `summary-include-seed`      | Include randomization seed in summary | `true`                      |
| `summary-event-name`        | GitHub event name for context         | `${{ github.event_name }}`  |
| `summary-ref-name`          | GitHub ref name for context           | `${{ github.ref_name }}`    |
| `summary-trigger-input`     | Workflow input that triggered run     | -                           |
| `summary-show-trigger`      | Show trigger information              | `true`                      |
| `summary-show-jobs-details` | Show detailed job breakdown           | `false`                     |

**Example:**

```yaml
- uses: ./.github/actions/clippier
  with:
      command: features
      generate-summary: true
      summary-title: '🧠 Smart CI Analysis'
      summary-event-name: ${{ github.event_name }}
      summary-ref-name: ${{ github.ref_name }}
      summary-trigger-input: ${{ github.event.inputs.packages }}
      summary-show-jobs-details: true
```

### Docker Analysis

| Input                    | Description                             | Default |
| ------------------------ | --------------------------------------- | ------- |
| `enable-docker-analysis` | Enable Docker package analysis          | `false` |
| `docker-packages`        | JSON mapping of packages to Docker info | -       |
| `docker-name-prefix`     | Prefix when looking up Docker packages  | `''`    |

**Example:**

```yaml
- uses: ./.github/actions/clippier
  with:
      command: features
      enable-docker-analysis: true
      docker-packages: |
          {
            "moosicbox_server": {
              "name": "server",
              "dockerfile": "packages/server/Server.Dockerfile"
            }
          }
      docker-name-prefix: 'moosicbox_'
```

### Output Transformation

| Input                        | Description                         | Default |
| ---------------------------- | ----------------------------------- | ------- |
| `transform-name-regex`       | Regex substitution for package name | -       |
| `transform-name-replacement` | Replacement string                  | `''`    |
| `matrix-properties`          | Properties to include in matrix     | All     |

## Outputs

| Output                 | Description                                                    |
| ---------------------- | -------------------------------------------------------------- |
| `matrix`               | Generated matrix JSON for GitHub Actions                       |
| `raw-output`           | Raw output from clippier before transformation                 |
| `affected`             | Whether target package is affected (affected-packages command) |
| `reasoning`            | Reasoning output (when include-reasoning is enabled)           |
| `git-base`             | Detected or provided git base SHA                              |
| `git-head`             | Detected or provided git head SHA                              |
| `has-changes`          | Whether any changes were detected                              |
| `docker-matrix`        | Generated Docker build matrix                                  |
| `has-docker-changes`   | Whether any Docker packages are affected                       |
| `docker-count`         | Number of Docker images to build                               |
| `docker-packages-list` | Formatted list of Docker packages                              |
| `additional-checks`    | JSON object with all additional package check results          |

**Additional Checks Output:**

Results from `additional-package-checks` are returned as a JSON object keyed by `output-key`:

```json
{
  "tauri": {"affected": true, "reasoning": [...]},
  "server": {"affected": false, "reasoning": [...]}
}
```

**Access via:** `fromJson(steps.analyze.outputs.additional-checks).{key}.affected`

**Example:**

```yaml
- if: ${{ fromJson(needs.analyze.outputs.additional-checks).tauri.affected }}
  run: echo "Tauri package is affected"
```

## Complete Examples

### Simple Project

```yaml
jobs:
    test:
        runs-on: ubuntu-latest
        steps:
            - uses: actions/checkout@v4

            - name: Generate matrix
              id: matrix
              uses: ./.github/actions/clippier
              with:
                  command: features
                  transform-name-regex: '^myproject_'

            - name: Test
              run: |
                  for pkg in $(echo '${{ steps.matrix.outputs.matrix }}' | jq -r '.[].name'); do
                    cargo test --package myproject_$pkg
                  done
```

### Advanced Multi-Stage Workflow

```yaml
jobs:
    analyze:
        runs-on: ubuntu-latest
        outputs:
            matrix: ${{ steps.analyze.outputs.matrix }}
            app-affected: ${{ fromJson(steps.analyze.outputs.additional-checks).app.affected || false }}
            additional-checks: ${{ steps.analyze.outputs.additional-checks }}
            docker-matrix: ${{ steps.analyze.outputs.docker-matrix }}
            git-base: ${{ steps.analyze.outputs.git-base }}
            git-head: ${{ steps.analyze.outputs.git-head }}
        steps:
            - uses: actions/checkout@v4
              with:
                  fetch-depth: 0

            - name: Get changed files
              id: files
              uses: tj-actions/changed-files@v44

            - name: Comprehensive analysis
              id: analyze
              uses: ./.github/actions/clippier
              with:
                  command: features
                  workspace-path: .

                  # Basic configuration
                  chunked: 15
                  max-parallel: 256
                  spread: true
                  randomize: true
                  seed: ${{ github.event.inputs.seed || '' }}
                  skip-features: fail-on-warnings
                  include-reasoning: true
                  transform-name-regex: '^(moosicbox|switchy|hyperchad)_'

                  # Smart git detection
                  git-strategy: workflow-history
                  changed-files: ${{ steps.files.outputs.all }}

                  # Custom reasoning for manual runs
                  inject-reasoning: |
                      ${{ github.event_name == 'workflow_dispatch' && 'Manual workflow dispatch - all packages included' || '' }}
                  inject-reasoning-condition: ${{ github.event_name == 'workflow_dispatch' }}
                  # Force full matrix for manual/scheduled
                  force-full-matrix-condition: ${{ github.event_name == 'workflow_dispatch' || github.event_name == 'schedule' }}

                  # Additional checks
                  additional-package-checks: |
                      [
                        {
                          "package": "moosicbox_app",
                          "output-key": "app",
                          "summary-section": {
                            "title": "📱 App Deployment",
                            "show-reasoning": true,
                            "show-all-affected": true,
                            "status-labels": {
                              "affected": "Will be deployed",
                              "not-affected": "No deployment needed"
                            }
                          }
                        }
                      ]

                  # Docker analysis
                  enable-docker-analysis: true
                  docker-packages: |
                      {
                        "moosicbox_server": {"name": "server", "dockerfile": "packages/server/Server.Dockerfile"}
                      }
                  docker-name-prefix: 'moosicbox_'
                  # Enhanced summary
                  generate-summary: true
                  summary-title: '🧠 CI Analysis'
                  summary-event-name: ${{ github.event_name }}
                  summary-show-jobs-details: true

    test:
        needs: analyze
        if: needs.analyze.outputs.matrix != '[]'
        strategy:
            matrix: ${{ fromJson(needs.analyze.outputs.matrix) }}
        runs-on: ${{ matrix.os }}
        steps:
            - uses: actions/checkout@v4
            - run: cargo test --package moosicbox_${{ matrix.name }}

    deploy-app:
        needs: analyze
        if: ${{ needs.analyze.outputs.app-affected == true }}
        runs-on: ubuntu-latest
        steps:
            - uses: actions/checkout@v4
            - run: ./deploy-app.sh

    build-docker:
        needs: analyze
        if: needs.analyze.outputs.has-docker-changes == 'true'
        strategy:
            matrix: ${{ fromJson(needs.analyze.outputs.docker-matrix) }}
        runs-on: ubuntu-latest
        steps:
            - uses: actions/checkout@v4
            - run: docker build -f ${{ matrix.dockerfile }} .
```

## Migration Guide

### Before (Raw Clippier)

```yaml
- name: Build matrix
  id: matrix
  run: |
      # 100+ lines of git SHA detection
      # 50+ lines of clippier invocation
      # 30+ lines of jq transformation
      # 40+ lines of reasoning injection
      MATRIX=$(cargo run -p clippier features . -o json | jq -rc '[.[] | {...}]')
      echo 'matrix<<EOF' >> $GITHUB_OUTPUT
      echo $MATRIX >> $GITHUB_OUTPUT
      echo 'EOF' >> $GITHUB_OUTPUT
```

### After (Using Action)

```yaml
- name: Build matrix
  id: matrix
  uses: ./.github/actions/clippier
  with:
      command: features
      workspace-path: .
      git-strategy: workflow-history
      transform-name-regex: '^moosicbox_'
      generate-summary: true
```

**Result:** ~200 lines → ~8 lines

## Breaking Changes & Migration

### v2.0: Additional Package Checks Output Format

**Breaking Change:** Additional package checks now return results in a single JSON output instead of individual dynamic outputs.

**Before (v1.x):**

```yaml
- uses: ./.github/actions/clippier
  with:
      additional-package-checks: |
          [{"package": "my_app", "output-key": "app"}]

# Old access pattern (no longer works)
- if: steps.analyze.outputs.app-affected == 'true'
  run: ./deploy.sh
```

**After (v2.0):**

```yaml
- uses: ./.github/actions/clippier
  with:
      additional-package-checks: |
          [{"package": "my_app", "output-key": "app"}]

# New access pattern (required)
- if: ${{ fromJson(steps.analyze.outputs.additional-checks).app.affected }}
  run: ./deploy.sh
```

**Why?** GitHub Actions composite actions cannot have dynamic outputs. The new format:

- ✅ Works for unlimited additional checks
- ✅ No action.yml changes needed for new checks
- ✅ Structured, type-safe access to results
- ✅ Each check includes full reasoning data

**Migration Steps:**

1. Replace `steps.X.outputs.{key}-affected` with `fromJson(steps.X.outputs.additional-checks).{key}.affected`
2. Replace `steps.X.outputs.{key}-reasoning` with `fromJson(steps.X.outputs.additional-checks).{key}`
3. Change boolean comparisons from `== 'true'` to `== true`
4. Expose `additional-checks` output in job outputs if needed by downstream jobs

## Clippier Build Configuration

| Input               | Description                      | Default      |
| ------------------- | -------------------------------- | ------------ |
| `clippier-version`  | Git ref/tag for clippier         | Current repo |
| `clippier-features` | Features to enable when building | `git-diff`   |
| `cache-key-prefix`  | Prefix for cache key             | `clippier`   |
| `skip-cache`        | Skip caching clippier binary     | `false`      |

## License

Same as parent project.
