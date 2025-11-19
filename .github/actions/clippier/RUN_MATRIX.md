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

## Configuration Options

```yaml
- name: Run tests with custom configuration
  uses: ./.github/actions/clippier
  with:
      command: run-matrix
      run-matrix-package-json: ${{ toJson(matrix.package) }}
      run-matrix-strategy: 'sequential' # sequential, parallel, combined, chunked-N
      run-matrix-continue-on-failure: 'true' # Continue on failure (default: true)
      run-matrix-fail-fast: 'false' # Stop on first failure (default: false)
      run-matrix-verbose: 'false' # Show all output (default: false)
      run-matrix-max-output-lines: '200' # Lines of error output to capture
      run-matrix-working-directory: './custom' # Custom working directory
      run-matrix-label: 'Custom Tests' # Custom label for summary
      run-matrix-skip-doctest-check: 'false' # Skip lib target check for doctests
      run-matrix-generate-summary: 'true' # Generate GitHub summary
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
