# run-matrix Command

The `run-matrix` command allows you to run tests across all feature combinations with comprehensive failure tracking and summary generation.

## Key Features

- **Template-based commands**: Use `{{matrix.package.*}}` and `{{clippier.*}}` variables in your commands
- **Multiple execution strategies**: sequential, parallel, combined, or chunked
- **Comprehensive failure tracking**: Continues running all tests even when some fail
- **Rich GitHub Actions summaries**: See all failures with full error output
- **Flexible and extensible**: Easy to customize for your needs

## Basic Usage

```yaml
- name: Run comprehensive tests
  uses: ./.github/actions/clippier
  with:
      command: run-matrix
      run-matrix-package-json: ${{ toJson(matrix.package) }}
```

This uses the default commands:

- `cargo clippy --all-targets --no-default-features {{clippier.feature-flags}}`
- `cargo llvm-cov test --no-report --no-default-features {{clippier.feature-flags}}`
- `cargo test --doc --no-default-features {{clippier.feature-flags}}`

## Custom Commands

```yaml
- name: Run custom tests
  uses: ./.github/actions/clippier
  with:
      command: run-matrix
      run-matrix-package-json: ${{ toJson(matrix.package) }}
      run-matrix-commands: |
          cargo check --no-default-features {{clippier.feature-flags}}
          cargo build --no-default-features {{clippier.feature-flags}}
          cargo test --no-default-features {{clippier.feature-flags}}
```

## Template Variables

### Matrix Properties (`matrix.package.*`)

Access any property from your package matrix:

- `{{matrix.package.name}}` - Package name
- `{{matrix.package.path}}` - Package path
- `{{matrix.package.nightly}}` - Nightly flag (true/false)
- `{{matrix.package.env}}` - Environment variables
- `{{matrix.package.cargo}}` - Additional cargo arguments
- `{{matrix.package.requiredFeatures}}` - Required features

### Generated Values (`clippier.*`)

- `{{clippier.features}}` - Current feature combination (e.g., "feature-1,feature-2")
- `{{clippier.all-features}}` - All features including fail-on-warnings and required features
- `{{clippier.feature-flags}}` - Full cargo feature flag: `--features="fail-on-warnings,feature-1"`
- `{{clippier.iteration}}` - Current iteration number (0-based)
- `{{clippier.total-iterations}}` - Total number of iterations

### Conditional Rendering

Use `{{if condition}}...{{endif}}` blocks:

```yaml
run-matrix-commands: |
    {{if matrix.package.env}}{{matrix.package.env}} {{endif}}cargo{{if matrix.package.nightly}} +nightly{{endif}} test {{clippier.feature-flags}}
```

This renders as:

- With env and nightly: `RUST_BACKTRACE=1 cargo +nightly test --features="..."`
- Without env: `cargo +nightly test --features="..."`
- Without nightly: `cargo test --features="..."`

## Execution Strategies

### Sequential (default)

Run one feature at a time:

```yaml
run-matrix-strategy: 'sequential'
```

### Combined

Test all features together in one run:

```yaml
run-matrix-strategy: 'combined'
```

### Chunked

Test N features at a time:

```yaml
run-matrix-strategy: 'chunked-3' # Test 3 features per run
```

## Reporting

`run-matrix` provides reporting with no additional workflow steps. The default
`standard` mode writes the job summary and uploads structured diagnostics and
reproduction scripts when a run fails.

```yaml
- uses: ./.github/actions/clippier
  with:
      command: run-matrix
      run-matrix-package-json: ${{ toJson(matrix.package) }}
      run-matrix-steps-file: .github/clippier/run-matrix/rust-validation.yml
```

Use `run-matrix-reporting` only to override the default:

- `off`: no summary or diagnostics
- `summary`: GitHub job summary only
- `standard`: summary plus failure-only diagnostics (default)
- `always`: summary plus diagnostics for successful and failed runs

For a workflow-wide summary and an AI-friendly consolidated artifact, add one
reusable workflow job:

```yaml
clippier-report:
    if: always()
    needs: [test]
    uses: ./.github/workflows/clippier-report.yml
```

## Outputs

The command provides these outputs:

- `run-success`: Overall success status (true/false)
- `run-total`: Total number of command runs
- `run-passed`: Number of passed runs
- `run-failed`: Number of failed runs
- `run-results`: Detailed results as JSON

Example usage:

```yaml
- name: Run tests
  id: tests
  uses: ./.github/actions/clippier
  with:
      command: run-matrix
      run-matrix-package-json: ${{ toJson(matrix.package) }}

- name: Check results
  if: steps.tests.outputs.run-success != 'true'
  run: |
      echo "Tests failed: ${{ steps.tests.outputs.run-failed }} out of ${{ steps.tests.outputs.run-total }}"
      exit 1
```

## File-backed step templates

Keep reusable command suites outside the action by passing a repository YAML or JSON file:

```yaml
- uses: ./.github/actions/clippier
  with:
      command: run-matrix
      run-matrix-package-json: ${{ toJson(matrix.package) }}
      run-matrix-steps-file: .github/clippier/run-matrix/rust-validation.yml
      run-matrix-template-vars: '{"test-command":"test"}'
```

`run-matrix-steps-file` uses the same schema and template syntax as
`run-matrix-steps`. Relative paths are resolved from `GITHUB_WORKSPACE`.
`run-matrix-template-vars` accepts a JSON object whose values are available as
`{{vars.*}}` during command rendering. Exactly one of `run-matrix-commands`, `run-matrix-steps`, or
`run-matrix-steps-file` must be provided.

## Complete Example

```yaml
build:
    runs-on: ${{ matrix.package.os }}
    needs: [determine-affected-packages]
    if: ${{ needs.determine-affected-packages.outputs.has-changes == 'true' }}

    strategy:
        fail-fast: false
        matrix:
            package: ${{ fromJson(needs.determine-affected-packages.outputs.matrix) }}

    steps:
        - uses: actions/checkout@v4
          with:
              fetch-depth: 0
              submodules: ${{ matrix.package.gitSubmodules == true }}

        - name: Setup CI environment
          uses: ./.github/actions/clippier
          with:
              command: setup
              package-json: ${{ toJson(matrix.package) }}
              skip-checkout: 'true'
              rust-components: 'rustfmt, clippy, llvm-tools-preview'

        - name: Install cargo-llvm-cov
          uses: taiki-e/install-action@cargo-llvm-cov

        - name: Run comprehensive tests
          id: run-tests
          uses: ./.github/actions/clippier
          with:
              command: run-matrix
              run-matrix-package-json: ${{ toJson(matrix.package) }}
              run-matrix-commands: |
                  cargo{{if matrix.package.nightly}} +nightly{{endif}} clippy --all-targets --no-default-features {{clippier.feature-flags}} {{matrix.package.cargo}}
                  cargo{{if matrix.package.nightly}} +nightly{{endif}} llvm-cov test --no-report --no-default-features {{clippier.feature-flags}} {{matrix.package.cargo}}
                  cargo{{if matrix.package.nightly}} +nightly{{endif}} test --doc --no-default-features {{clippier.feature-flags}} {{matrix.package.cargo}}
              run-matrix-strategy: 'sequential'
              run-matrix-continue-on-failure: 'true'

        # Other steps (coverage, format, etc.)...
```

## Benefits

1. **See All Failures**: No more "fix one, run CI, find another" cycles
2. **Better Debugging**: Full error output captured for each failure
3. **Flexible**: Customize commands and strategies for your needs
4. **Maintainable**: All test logic in one place
5. **Rich Reporting**: GitHub Actions summaries with collapsible error details

## Migration from Old Pattern

### Before (separate steps):

```yaml
- name: Clippy
  run: |
      while read -r feature; do
          cargo clippy --features="$feature" || exit 1
      done

- name: Tests
  run: |
      while read -r feature; do
          cargo test --features="$feature" || exit 1
      done

- name: Doctests
  run: |
      while read -r feature; do
          cargo test --doc --features="$feature" || exit 1
      done
```

### After (single step):

```yaml
- name: Run all tests
  uses: ./.github/actions/clippier
  with:
      command: run-matrix
      run-matrix-package-json: ${{ toJson(matrix.package) }}
```

The new approach runs ALL feature combinations for ALL commands before failing, giving you complete visibility into all issues at once.
