# Clippier Action Library Files

This directory contains modular library files for the clippier GitHub Action.

## File Structure

```
lib/
├── README.md           # This file
├── template.sh         # Template evaluation system (356 lines)
└── run-matrix.sh       # run-matrix command implementation (393 lines)
```

## Library Files

### `template.sh`

Template evaluation system that provides:

- **Variable interpolation**: `{{matrix.package.name}}`, `{{clippier.features}}`
- **Conditional blocks**: `{{if condition}}...{{endif}}`
- **Strategy support**: sequential, parallel, combined, chunked-N
- **JSON-backed resolution**: Uses jq for robust path navigation

**Key Functions:**

- `render_template()` - Main public API for rendering templates
- `init_template_context()` - Creates context JSON with all variables
- `resolve_variable()` - Resolves variable paths using jq
- `evaluate_template()` - Evaluates templates with variable substitution
- `evaluate_conditionals()` - Handles if/endif blocks
- `parse_strategy()` - Parses strategy strings
- `generate_feature_combinations()` - Generates combinations based on strategy

**Available Template Variables:**

- `matrix.package.*` - All properties from package matrix
- `clippier.features` - Current feature combination
- `clippier.all-features` - All features including fail-on-warnings
- `clippier.feature-flags` - Full cargo feature flag string
- `clippier.iteration` - Current iteration number
- `clippier.total-iterations` - Total iterations

### `run-matrix.sh`

Implementation of the `run-matrix` command that:

- Executes test commands across feature combinations
- Tracks all failures comprehensively
- Generates rich GitHub Actions summaries
- Supports multiple execution strategies
- Provides detailed error reporting

**Key Functions:**

- `run_matrix_command()` - Main command implementation
- `generate_run_matrix_summary()` - Creates GitHub Actions markdown summaries

**Dependencies:**

- Requires `template.sh` to be sourced first
- Uses template rendering for command execution
- Uses strategy functions for feature combinations

## Usage

These files are automatically sourced by `action.sh`:

```bash
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib/template.sh"
source "${SCRIPT_DIR}/lib/run-matrix.sh"
```

## Testing

To test the libraries independently:

```bash
# Source the library
source lib/template.sh

# Test template rendering
matrix_json='{"name":"test","nightly":true}'
template='cargo{{if matrix.package.nightly}} +nightly{{endif}} test {{clippier.feature-flags}}'
result=$(render_template "$template" "$matrix_json" "f1,f2" "default" "0" "5")
echo "$result"
# Output: cargo +nightly test --features="fail-on-warnings,default,f1,f2"

# Test strategy
features='["f1","f2","f3"]'
generate_feature_combinations "$features" "chunked-2"
# Output (one per line):
# f1,f2
# f3
```

## Benefits of Modularization

1. **Maintainability**: Each file has a clear, focused purpose
2. **Testability**: Can test libraries independently
3. **Readability**: Smaller files are easier to navigate
4. **Reusability**: Template system could be used for other commands
5. **Separation of Concerns**: Core logic separate from features

## File Sizes

- **action.sh**: 1,036 lines (down from 1,619)
- **lib/template.sh**: 356 lines
- **lib/run-matrix.sh**: 393 lines
- **Total**: 1,785 lines (organized into logical modules)

## Known Limitations

- **Nested conditionals**: The current implementation doesn't support nested `{{if}}` blocks. Use separate conditional blocks instead.
    - ❌ Not supported: `{{if a}}{{if b}}...{{endif}}{{endif}}`
    - ✅ Use instead: `{{if a}}...{{endif}}{{if b}}...{{endif}}`

This limitation doesn't affect the default templates and can be addressed if needed in the future.
