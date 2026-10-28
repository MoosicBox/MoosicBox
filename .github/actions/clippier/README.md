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
- 🧰 **Automatic Diagnostics** - `run-matrix` publishes failure JSON and reproduction scripts by default
- 🎲 **Deterministic Randomization** - Reproducible test matrices with seed support
- 🔄 **Flexible Transformation** - Package name transformation via regex
- 📦 **Stateless Cargo Releases** - Registry-backed status, semver preparation, verification, and dependency-ordered publishing
- ⚡ **Smart Caching** - Automatic clippier binary caching

## Lossless feature-case stacking

Callers can opt into `stack-cases: true` for analysis. With this
option, `features` first generates and transforms the complete case plan, then
`max-parallel` bounds **runner jobs** without unioning or dropping feature cases.
Every job carries `cases: [{id, package}]`, including singleton jobs. Pass the
whole job as `run-matrix-package-json`; execution detects its format automatically.

The allocator spreads cases deterministically across identical setup groups.
Custom setup/dependency scripts, submodules, and project
`[parallelization] fresh-runner = true` cases stay on separate runners. Stacked
cases run sequentially through the existing executor in separate processes and
clean disposable checkouts; per-case results, diagnostics, and coverage remain
separate. If incompatible setups cannot fit, analysis fails explicitly.

Stacking and matrix sharding are independent opt-ins. Build and Test enables
`shard-matrix: true` without stacking: every case gets its own runner job.
`clippier plan-matrix` constructs these singleton jobs without a scheduling budget.
Enable `stack-cases: true` to group compatible cases within `max-parallel` jobs,
with or without sharding. Neither option requires the other. With both off,
legacy plain-package matrices remain supported. Without sharding, a plan that
exceeds the single-matrix count or payload budget fails with a clear error.

When either option is enabled, feature generation preserves the original cases:
`max-parallel` is not forwarded to legacy feature-combining logic. Only stacking
uses it as an execution-job budget. `allow-isolation-overflow: true` applies only
to stacking, and raises that budget only to the minimum safe setup-group count;
it does not require or enable sharding. Arbitrary installers remain isolated.
`schedule-summary.json` and `.md` record allocation statistics and shard count.
For singleton plans, all budget/count fields describe the one-case-per-job
allocation rather than an optimization budget.

`run-matrix` detects the explicit `cases` job format and uses the receipt-aware
executor for singleton and stacked jobs alike, without a `stack-cases` flag.
Plain-package inputs continue using the legacy executor. Malformed job entries
are rejected rather than falling back to legacy execution.

### Matrix transport policy

```yaml
shard-matrix: true      # Explicit permission to split oversized plans
matrix-max-jobs: 256    # Default; choose any integer from 1 through 256
```

`matrix-max-jobs` limits jobs **per emitted matrix**, not total jobs, feature
configurations, or execution concurrency. It is independent of stacking's
`max-parallel` budget. With sharding disabled (the default), an oversized plan
fails explicitly; the action never enables sharding or stacking on your behalf.
With sharding enabled, a fitting plan runs directly; only oversized plans split.
The action emits data for the caller workflow to dispatch, not new jobs itself.

The conservative transport payload budget is **200,000 UTF-8 bytes** per matrix.
This is our safety budget, not GitHub's exact output limit (which is measured in
UTF-16 and covers the job's combined outputs). Payload size can force a split
earlier than `matrix-max-jobs`. A single job exceeding this budget fails rather
than being altered. GitHub's hard ceiling remains 256 entries, including the
outer shard-ID matrix; plans requiring more than 256 shards fail explicitly.

Logs and the GitHub step summary describe direct/empty/sharded routing, the
selected limit, shard sizes, and whether job count or payload size caused the
split. Planned runs also save `transport-summary.json` alongside allocation
statistics. For example: `265 jobs → sharded [256,9]; ... reason: matrix job-count limit`.

### Actual nested workflow groups

Use `shard-levels` to request explicit reusable-workflow grouping levels up to
the available GitHub workflow-call budget. Unlike flat sharding, nested mode preserves the requested hierarchy even
when the complete job plan would fit directly. Empty plans make no calls.

```yaml
shard-matrix: true
matrix-max-jobs: 256
shard-levels: |
  - group-by-jq: '.os'
    name-jq: |
      if .group == "ubuntu-latest" then "🐧 Linux builds"
      elif .group == "windows-latest" then "🪟 Windows builds"
      elif .group == "macos-latest" then "🍎 macOS builds"
      else (.group | tostring) + " builds"
      end
  - group-by-jq: '.name'
    name-jq: '.group'
    matrix-max-jobs: 256
```

Each grouping expression evaluates against an unchanged execution job and must
return exactly one JSON key. Keys are compared/ordered using jq; composite keys
such as `[.toolchain, .metadata.architecture]`, objects, null, booleans and numbers
are supported. Missing fields yield null: use `// "unknown"` or `error("missing")`
for an explicit policy. Original order is preserved within each group. To group
package families, use an expression such as
`if .name | startswith("hyperchad") then "HyperChad" else .name end`.

`name-jq` receives `{group, path, level, job_count, part_index, part_count}`.
`path` contains ancestor keys and the current key; levels and part indices are
one-based. `job_count` counts all jobs in the logical group before partitioning.
The default name is the key rendered as a string/JSON. Names must be nonempty
single-line strings. Overflow automatically adds ` · 1/2` suffixes. Names never
become filesystem paths or receipt identities; duplicate names are allowed.
Unknown options, invalid expressions, multiple/empty jq results and invalid
limits fail planning. Expressions have a 30-second evaluation timeout and errors
do not print job/environment data. Do not put secrets in grouping keys or names:
those values are intentionally present in plan artifacts and previews.

Per-level `matrix-max-jobs` overrides the global value for that level's outgoing
matrix: child calls for intermediate groups, execution jobs for leaf groups.
Count/payload overflow partitions a logical group into clearly named parts,
never mixes grouping keys, and never rewrites tests. Every matrix is checked
against the conservative 200,000 UTF-8 byte payload budget. A single oversized
job/descriptor fails. The root descriptor matrix must itself fit the global
limit; if there are too many root parts, planning fails with a clear message.
Root pagination would require another statically wired dispatcher level; this
implementation does not silently invent that extra depth or flatten groups.

Nested mode requires `shard-matrix: true` and cannot be combined with flat
`shard-group-by-jq`/`shard-name-jq`. Stacking remains independently optional.
Without `shard-levels`, all existing direct/flat behavior is preserved.

Forward the action's `hierarchy-matrix` through the analysis job's outputs and
wire actual group calls (Build and Test already does this):

```yaml
name: ${{ matrix.group.name }}
strategy:
  fail-fast: false
  matrix:
    group: ${{ fromJson(needs.analysis.outputs.hierarchy-matrix) }}
uses: ./.github/workflows/clippier-build-group.yml
with:
  node-id: ${{ matrix.group.id }}
  node-kind: ${{ matrix.group.kind }}
secrets: inherit
```

Skip that job when `hierarchy-matrix` is `[]`. Direct and flat-shard outputs are
`[]` in nested mode, preventing double dispatch. Each group dispatcher calls the next statically declared adapter for child
groups; leaf nodes call the existing shard workflow and unchanged build lifecycle. These are real workflow calls, not just labels,
but GitHub controls the exact UI presentation. All sibling matrices use
`fail-fast: false`; the final full-plan receipt reconciliation runs even when a
preparation job, build, or entire subtree fails. Node IDs namespace artifacts.

The versioned `hierarchy/tree.json` artifact is built and validated before
publication, including exact job-multiset equality, references, node types,
reachability, depth and all matrix bounds. Readers validate it again against
`jobs.json`, reject invalid IDs/path traversal and select only the expected node
type. Grouping never changes the complete job plan used for receipt verification.

#### Depth budget and static workflow entries

A top-level caller supports **eight grouping levels**: GitHub's ten workflow
levels minus the caller minus the final build executor. The limit follows the
static call graph, including conditional leaf branches, not just the runtime
path taken by a shallow plan. The adapters are an acyclic chain; self-recursion
and dynamic `uses:` expressions are not used.

The only additional setting is for callers embedded inside other reusable
workflows:

```yaml
shard-caller-depth: 2 # top-level workflow -> this caller
```

This leaves seven grouping levels. Use the corresponding **literal** entry in
that caller's dispatch job:

| Caller workflow depth | Static `uses:` filename      | Available grouping levels |
| --------------------- | ---------------------------- | ------------------------- |
| 1 (default)           | `clippier-build-group.yml`   | 8                         |
| 2                     | `clippier-build-group-2.yml` | 7                         |
| 3                     | `clippier-build-group-3.yml` | 6                         |
| 4                     | `clippier-build-group-4.yml` | 5                         |
| 5                     | `clippier-build-group-5.yml` | 4                         |
| 6                     | `clippier-build-group-6.yml` | 3                         |
| 7                     | `clippier-build-group-7.yml` | 2                         |
| 8                     | `clippier-build-group-8.yml` | 1                         |

`hierarchy-entry` reports the required filename for inspection, but cannot be
passed dynamically to GitHub's `uses:`. Local preview accepts `--caller-depth`.
Call-depth declarations cannot discover external callers automatically: the
caller must supply its real depth and wire the matching entry. Readers enforce
the remaining static capacity before dispatch. Too-deep requests fail before
publishing hierarchy outputs and report the budget calculation.

The shared budget lives in `lib/shard_capacity.py`. The YAML adapters are generated
by `generate-dispatchers.py`, committed before execution, and checked for drift
with `python3 .github/actions/clippier/generate-dispatchers.py --check`. Never edit
generated adapters directly. Graph tests count every static branch, detect cycles,
and check this repository's reachable workflows against GitHub's 50-unique-workflow
limit. Other callers must include their own additional workflows in that count.
Platform reference: https://docs.github.com/en/actions/reference/workflows-and-actions/reusing-workflow-configurations

This extends grouping depth, not root pagination: overflowing the root matrix
still fails explicitly instead of inserting an unrequested execution level.

#### Local preview

The same planner used by CI can run locally with Python 3 and jq (no provider
access). Convert configuration with the action's existing yq dependency:

```bash
yq -o=json '.' levels.yml > levels.json
python3 .github/actions/clippier/lib/shard_tree.py plan jobs.json levels.json preview-plan --max-jobs 256
cat preview-plan/preview.md
```

Use a fresh destination. Preview output shows the full grouping tree, part sizes,
effective limits, split reasons and preparation-job overhead (one per tree node plus one per leaf executor).
It does not set execution concurrency. Actual UI presentation, inherited
permissions/secrets and cancellation propagation still require a real GitHub
smoke run; local tests/actionlint are not remote execution proof.

### Generic shard grouping and names

```yaml
shard-matrix: true
shard-group-by-jq: '.os' # Any jq expression on a job, not a reserved field name
shard-name-jq: |
  if .group == "ubuntu-latest" then "🐧 Linux builds"
  elif .group == "windows-latest" then "🪟 Windows builds"
  elif .group == "macos-latest" then "🍎 macOS builds"
  else (.group | tostring) + " builds"
  end
```

Grouping only applies when the complete plan exceeds transport limits; it never
forces a fitting plan off the direct path. With no expression, existing sequential
partitioning is preserved. Names alone do not enable grouping or sharding.

`shard-group-by-jq` must emit exactly one JSON value for each job. Equal keys are
grouped using jq equality/order, with deterministic key ordering and original job
order within each group. Keys can be strings, numbers, booleans, null, arrays, or
objects. For example, `.metadata.platform`, `[.os, .toolchain]`, or
`{platform:.os, tier:.metadata.tier}` work without special property support.
Missing properties yield null; use `// "unknown"` or `error("missing platform")`
for an explicit alternative. Each group splits independently under count and
payload limits. Job contents, case identities, and receipts remain unchanged;
grouping may reorder jobs across groups but never within a group.

`shard-name-jq` receives `{id, group, group_index, group_count}`. The numeric `id`
is global and zero-based; group indices are one-based. Return exactly one
nonempty single-line string. A ` · 1/2` suffix is automatically appended when a
group needs multiple shards. Default names are `Build shard <id>`. Names need not
be unique: they never determine filenames, artifact names, or receipt identity.
Expressions that error, emit no values/multiple values, or return invalid names
fail planning rather than silently dropping jobs. Expressions are evaluated only
when sharding occurs, not on the direct path.

The existing `matrix` output remains numeric shard IDs for compatibility. The new
`shard-matrix` output contains descriptors; wire it into the caller like this:

```yaml
strategy:
  matrix:
    shard: ${{ fromJson(needs.analysis.outputs.shard-matrix) }}
name: ${{ matrix.shard.name }}
uses: ./.github/workflows/clippier-build-shard.yml
with:
  shard-id: ${{ matrix.shard.id }}
```

Forward the action's `shard-matrix` through the analysis job outputs first.
Both outputs are `[]` on the direct/empty path. Descriptors are also saved as
`shards/manifest.json`; transport summaries include the names and grouping keys.
The outer descriptor matrix is itself checked against GitHub's 256-entry ceiling
and the transport payload budget. Custom grouping may require more shards; plans
that exceed those bounds fail rather than mixing groups or discarding jobs.

The `direct-matrix` output contains the full plan when it fits within `matrix-max-jobs`
and the 200,000 UTF-8 byte transport budget; `matrix` is then `[]`. Build and Test
runs that plan as a top-level matrix, without shard preparation jobs. Empty
plans skip both build routes. Otherwise `direct-matrix` is `[]` and `matrix`
contains shard IDs. The complete unchanged plan remains in
`$RUNNER_TEMP/clippier-plan/jobs.json`, with bounded job arrays in
`shards/<id>.json` only when needed. Upload the entire plan directory and dispatch
each shard ID to `clippier-build-shard.yml`. Each shard contains at most `matrix-max-jobs` jobs
and 200,000 UTF-8 bytes, keeping job outputs below GitHub's UTF-16 size limit.
Receipt artifact names distinguish direct execution from shard IDs;
reconciliation still checks the complete plan. Both routes use identical build
steps, enforced by the workflow regression test.

The workflow keeps job naming in jq: `jq-filter` changes execution metadata
before scheduling, while `jq-post-filter` adds top-level naming fields afterward.
The latter may not change existing fields, cases, job order, or job count. The
saved plan and diagnostic identities exclude those added naming fields. Build
and Test uses this hook for its conditional OS emojis, per-package/OS job indexes,
and a ` · N cases` suffix only on stacked jobs.

Always upload `case-receipts-path`, including after test failure, and reconcile
those receipts against the saved plan with `clippier verify-matrix`. Missing or
failed cases must fail the workflow; summaries alone are not completion proof.
`case-coverage: true` supplies a `coverage-output` template variable and exposes
`coverage-path` for upload. See the
[CLI scheduling contract](../../../packages/clippier/README.md#lossless-ci-scheduling)
for compatibility rules and isolation requirements.

Local regression commands (also run by Build and Test):

```bash
cargo test -p clippier --no-default-features --test matrix_schedule_tests
bash .github/actions/clippier/tests/analysis.sh
bash .github/actions/clippier/tests/case-stack.sh
bash .github/actions/clippier/tests/large-output.sh
bash .github/actions/clippier/tests/validation.sh
bash .github/actions/clippier/tests/report.sh
bash .github/actions/clippier/tests/rust-validation.sh
bash .github/actions/clippier/tests/shard-plan.sh
PYTHONDONTWRITEBYTECODE=1 python3 .github/actions/clippier/tests/shard_tree.py
```

## Stateless Cargo release commands

Use `release-status`, `release-prepare`, `release-verify`, or `release-publish` to invoke Clippier's
stateless Cargo release pipeline. The Action emits deterministic `release-status`,
`release-has-changes`, `release-packages`, and `release-publish-order` outputs. No output or artifact
is required by later release invocations: each command reconstructs state from the repository and
crates.io.

```yaml
- uses: MoosicBox/MoosicBox/.github/actions/clippier@master
  id: release
  with:
      command: release-prepare
      workspace-path: .
      release-packages: crate_a,crate_b # optional
      release-dry-run: "false"
      clippier-features: release
```

Use the reusable `cargo-release-prepare.yml` and `cargo-release-publish.yml` workflows for managed
release PRs and protected publication. Preparation receives no publishing credentials. Publication
must run from the default branch, is serialized per repository, and supports crates.io trusted
publishing or `CARGO_REGISTRY_TOKEN` as a fallback.

## Standard run-matrix reporting

`run-matrix` defaults to `run-matrix-reporting: standard`. Each invocation writes
a human-readable job summary and automatically uploads structured diagnostics and
reproduction scripts when it fails. No caller-managed artifact step is needed.

Use `off`, `summary`, `standard`, or `always` to change that behavior. Add one
reusable workflow job when a workflow-wide summary and consolidated AI artifact
are desired:

```yaml
clippier-report:
    if: always()
    needs: [test]
    uses: ./.github/workflows/clippier-report.yml
```

The report job combines all `clippier-diagnostics-*` artifacts with GitHub job
metadata into `report.md`, `manifest.json`, raw diagnostics, and reproduction
scripts.

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
