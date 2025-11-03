# Feature Validation Example

This example demonstrates how to use the clippier library programmatically to validate feature propagation across workspace dependencies.

## Summary

This example shows how to use clippier's `FeatureValidator` API to ensure that Cargo features are correctly propagated across workspace dependencies. Feature validation helps prevent build failures caused by inconsistent feature configurations.

## What This Example Demonstrates

- Creating a `FeatureValidator` with custom configuration
- Validating specific features (e.g., `fail-on-warnings`)
- Auto-detecting features that need validation across the workspace
- Validating multiple features simultaneously
- Using different output formats (Raw text vs JSON)
- Interpreting validation results programmatically
- Integrating feature validation into custom tooling

## Prerequisites

- Basic understanding of Cargo workspaces and features
- Familiarity with Rust dependency management
- Knowledge of how features propagate in Cargo.toml files

## Running the Example

From the repository root, run:

```bash
cargo run --manifest-path packages/clippier/examples/feature_validation/Cargo.toml
```

Or with the fail-on-warnings feature enabled:

```bash
cargo run --manifest-path packages/clippier/examples/feature_validation/Cargo.toml --features fail-on-warnings
```

## Expected Output

The example runs four validation scenarios and displays results for each:

```
=== Clippier Feature Validation Example ===

Example 1: Validating 'fail-on-warnings' feature propagation
-----------------------------------------------------------
Total packages checked: 147
Valid packages: 147
Packages with errors: 0

Example 2: Auto-detecting features across workspace
----------------------------------------------------
Auto-detected validation for 147 packages
Valid: 147

Example 3: Validating multiple features
----------------------------------------
Total packages: 147
Valid packages: 147
Errors: 0

Example 4: JSON output format
------------------------------
JSON output (truncated):
{
  "total_packages": 147,
  "valid_packages": 147,
  "errors": [],
  "warnings": []
}

=== Summary ===
Successfully demonstrated feature validation using clippier library
Key capabilities shown:
  ✓ Single feature validation
  ✓ Auto-detection of features
  ✓ Multiple feature validation
  ✓ JSON output format for CI/CD
```

## Code Walkthrough

### 1. Single Feature Validation

```rust
let config = ValidatorConfig {
    features: Some(vec!["fail-on-warnings".to_string()]),
    workspace_only: true,
    output_format: OutputType::Raw,
};

let validator = FeatureValidator::new(None, config)?;
let result = validator.validate()?;
```

This validates that the `fail-on-warnings` feature is correctly propagated across all workspace dependencies. The `workspace_only: true` setting focuses validation on packages within your workspace, excluding external dependencies.

### 2. Auto-Detection Mode

```rust
let auto_config = ValidatorConfig {
    features: None, // Auto-detect features
    workspace_only: true,
    output_format: OutputType::Raw,
};

let auto_validator = FeatureValidator::new(None, auto_config)?;
let auto_result = auto_validator.validate()?;
```

When `features` is `None`, the validator automatically detects features that exist across multiple packages and validates their propagation. This is useful for comprehensive workspace validation.

### 3. Multiple Features

```rust
let multi_config = ValidatorConfig {
    features: Some(vec![
        "fail-on-warnings".to_string(),
        "git-diff".to_string(),
    ]),
    workspace_only: true,
    output_format: OutputType::Raw,
};
```

Validates multiple features in a single pass, efficient for CI/CD pipelines that need to verify several feature configurations.

### 4. JSON Output for CI/CD

```rust
let json_config = ValidatorConfig {
    features: Some(vec!["fail-on-warnings".to_string()]),
    workspace_only: true,
    output_format: OutputType::Json,
};

let json_validator = FeatureValidator::new(None, json_config)?;
let json_result = json_validator.validate()?;
```

JSON output is ideal for CI/CD integration, allowing automated systems to parse and act on validation results.

## Key Concepts

### Feature Propagation

In Cargo workspaces, when package A depends on package B, and both have a `fail-on-warnings` feature, package A should typically propagate this feature to package B:

```toml
[features]
fail-on-warnings = [
    "package_b/fail-on-warnings",  # Propagate to dependency
]
```

Without proper propagation, enabling `fail-on-warnings` on package A won't enable it on package B, leading to inconsistent builds.

### Workspace-Only Validation

Setting `workspace_only: true` restricts validation to packages within your workspace. This avoids false positives from external dependencies that you don't control.

### Validation Results

The `ValidationResult` struct provides:

- `total_packages`: Number of packages validated
- `valid_packages`: Number passing validation
- `errors`: Detailed errors for failing packages
- `warnings`: Non-critical issues detected

### Optional Dependencies

The validator correctly handles optional dependencies using the `?` syntax:

```toml
[features]
my-feature = [
    "required_dep/my-feature",   # Always propagate
    "optional_dep?/my-feature",  # Only when optional_dep is enabled
]
```

## Testing the Example

Run the example and observe the output for each validation scenario. The example validates the actual workspace it's running in, so results will reflect the real state of feature propagation.

To see validation errors, you could temporarily modify a package's Cargo.toml to remove feature propagation and re-run the example.

## Troubleshooting

### "Feature not found" errors

Ensure the feature exists in at least one workspace package. Use auto-detection mode to see available features:

```rust
let config = ValidatorConfig {
    features: None,
    workspace_only: true,
    output_format: OutputType::Raw,
};
```

### Empty validation results

Verify you're running the example from a Cargo workspace directory. The validator needs a valid workspace structure to analyze.

### Path-related errors

The example uses `None` for the path parameter, which defaults to the current directory. If needed, specify an explicit workspace path:

```rust
let validator = FeatureValidator::new(
    Some("/path/to/workspace".into()),
    config
)?;
```

## Related Examples

This is currently the only example for clippier. Future examples may demonstrate:

- Dependency analysis workflows
- CI/CD pipeline generation
- Docker integration

## Integration Use Cases

### CI/CD Pipeline

```rust
// In your CI script
let config = ValidatorConfig {
    features: Some(vec!["fail-on-warnings".to_string()]),
    workspace_only: true,
    output_format: OutputType::Json,
};

let validator = FeatureValidator::new(None, config)?;
let result = validator.validate()?;

if !result.errors.is_empty() {
    eprintln!("Feature validation failed!");
    std::process::exit(1);
}
```

### Custom Linting Tool

```rust
// Build a custom workspace linter
let validator = FeatureValidator::new(None, config)?;
let result = validator.validate()?;

for error in result.errors {
    println!("ERROR in {}: feature propagation issues", error.package);
}
```

### Pre-commit Hook

```rust
// Validate features before allowing commits
let validator = FeatureValidator::new(None, config)?;
let result = validator.validate()?;

if result.errors.is_empty() {
    println!("✓ Feature validation passed");
    Ok(())
} else {
    Err("Feature validation failed - fix before committing".into())
}
```
