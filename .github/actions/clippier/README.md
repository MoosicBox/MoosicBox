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
  if: steps.matrix.outputs.app-affected == 'true'
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

## Enhanced Features

### Smart Git Detection

The action automatically detects git ranges and handles complex scenarios:

| Input                | Description                                       | Default                    |
| -------------------- | ------------------------------------------------- | -------------------------- |
| `git-strategy`       | Strategy: `auto`, `workflow-history`, or `manual` | `auto`                     |
| `git-base`           | Git base commit (only for manual strategy)        | -                          |
| `git-head`           | Git head commit (only for manual strategy)        | -                          |
| `git-workflow-name`  | Workflow name for API lookups                     | Current workflow           |
| `git-default-branch` | Default branch name                               | `master`                   |
| `github-token`       | GitHub token for API access                       | `${{ github.token }}`      |
| `github-repository`  | Repository for API lookups                        | `${{ github.repository }}` |
| `github-ref-name`    | Current ref name                                  | `${{ github.ref_name }}`   |

**Strategies:**

- **`auto`** - Detects based on event type (PR, push, workflow_dispatch, etc.)
- **`workflow-history`** - Uses GitHub API to find valid commits after force pushes
- **`manual`** - Uses provided `git-base` and `git-head`

**Example:**

```yaml
- uses: ./.github/actions/clippier
  with:
      command: features
      git-strategy: workflow-history # Handles force pushes automatically
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

Creates outputs:

- `tauri-affected` (boolean)
- `tauri-reasoning` (JSON)

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

| Input                        | Description                         | Default   |
| ---------------------------- | ----------------------------------- | --------- |
| `transform-name-regex`       | Regex substitution for package name | -         |
| `transform-name-replacement` | Replacement string                  | `''`      |
| `matrix-properties`          | Properties to include in matrix     | All       |
| `os-suffix`                  | Suffix to add to OS field           | `-latest` |

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

**Dynamic Outputs:**
Additional package checks create dynamic outputs based on their `output-key`:

- `{output-key}-affected` - Boolean indicating if package is affected
- `{output-key}-reasoning` - JSON reasoning data

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
            app-affected: ${{ steps.analyze.outputs.app-affected }}
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
        if: needs.analyze.outputs.app-affected == 'true'
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

## Clippier Build Configuration

| Input               | Description                      | Default      |
| ------------------- | -------------------------------- | ------------ |
| `clippier-version`  | Git ref/tag for clippier         | Current repo |
| `clippier-features` | Features to enable when building | `git-diff`   |
| `cache-key-prefix`  | Prefix for cache key             | `clippier`   |
| `skip-cache`        | Skip caching clippier binary     | `false`      |

## License

Same as parent project.
