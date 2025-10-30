# Basic Usage Example

A comprehensive demonstration of the gpipe_ast crate showing how to programmatically build workflow definitions, serialize them to YAML, and work with all core AST types.

## Summary

This example demonstrates the complete API of gpipe_ast including workflow construction, triggers, jobs with matrix strategies, different step types (UseAction and RunScript), conditional expressions, and YAML serialization/deserialization.

## What This Example Demonstrates

- Building a complete workflow programmatically using AST types
- Creating different trigger types (push, pull_request, manual)
- Defining action references (GitHub actions and file-based actions)
- Building jobs with dependencies and matrix strategies
- Using both UseAction and RunScript step types
- Creating conditional expressions with the builder API
- Serializing workflows to YAML format
- Deserializing workflows from YAML strings
- Working with environment variables and step configurations

## Prerequisites

- Basic understanding of CI/CD concepts (workflows, jobs, steps)
- Familiarity with GitHub Actions or similar CI/CD systems is helpful
- Understanding of Rust's ownership and error handling

## Running the Example

```bash
cargo run --manifest-path packages/gpipe/ast/examples/basic_usage/Cargo.toml
```

## Expected Output

The example will print:

1. A complete workflow definition serialized to YAML format
2. Confirmation of successful deserialization with workflow metadata
3. Examples of different expression types (literals, variables, operators, functions)
4. Examples of step types with their properties

```
=== gpipe_ast Basic Usage Example ===

--- Serializing workflow to YAML ---
version: '1.0'
name: Example CI/CD Pipeline
triggers:
  - type: push
    branches:
      - main
      - develop
  - type: pull_request
    branches:
      - main
    types:
      - opened
      - synchronize
  - type: manual
actions:
  checkout:
    type: github
    repo: actions/checkout@v4
  custom-action:
    type: file
    path: .github/actions/custom/action.yml
jobs:
  build:
    needs: []
    env:
      RUST_BACKTRACE: '1'
    strategy:
      matrix:
        os:
          - ubuntu-latest
          - windows-latest
          - macos-latest
        rust:
          - stable
          - nightly
        exclude:
          - os: windows-latest
            rust: nightly
    steps:
      - id: checkout
        uses: checkout
      - id: setup
        run: rustup toolchain install ${{ matrix.rust }}
      - id: build
        run: cargo build --release
  deploy:
    needs:
      - build
      - test
    if: BinaryOp { left: Variable(["github", "ref"]), op: Equal, right: String("refs/heads/main") }
    steps:
      - uses: checkout
      - id: deploy
        run: cargo publish
        if: String("success()")
  test:
    needs:
      - build
    steps:
      - uses: checkout
      - id: test
        run: cargo test --all-features
      - id: clippy
        run: cargo clippy -- -D warnings
        continue-on-error: true

--- Deserializing workflow from YAML ---
Successfully deserialized workflow: Example CI/CD Pipeline
  Version: 1.0
  Triggers: 3
  Jobs: 3

--- Expression Builder Examples ---
String: String("hello")
Number: Number(42.0)
Boolean: Boolean(true)
Null: Null
Variable: Variable(["github", "actor"])
Equality: BinaryOp { left: Variable(["github", "event_name"]), op: Equal, right: String("push") }
Logical AND: BinaryOp { left: Boolean(true), op: And, right: Boolean(false) }
Negation: UnaryOp { op: Not, expr: Boolean(true) }
Function call: FunctionCall { name: "contains", args: [Variable(["github", "ref"]), String("release")] }
Index: Index { expr: Variable(["matrix", "os"]), index: Number(0.0) }

--- Step Type Examples ---
UseAction step ID: Some("checkout-step")
  Continue on error: false
RunScript step ID: Some("build-step")
  Has condition: true
  Environment vars: 1

=== Example completed successfully! ===
```

## Code Walkthrough

### Building a Complete Workflow

The example creates a workflow with multiple jobs demonstrating job dependencies and execution order:

```rust
let workflow = Workflow {
    version: "1.0".to_string(),
    name: "Example CI/CD Pipeline".to_string(),
    triggers,
    actions,
    jobs,
};
```

See `packages/gpipe/ast/examples/basic_usage/src/main.rs:39` for the complete implementation.

### Defining Triggers

Multiple trigger types are demonstrated to show different workflow activation scenarios:

```rust
let triggers = vec![
    Trigger {
        trigger_type: TriggerType::Push,
        config: TriggerConfig {
            branches: Some(vec!["main".to_string(), "develop".to_string()]),
            types: None,
            cron: None,
        },
    },
    // ... more triggers
];
```

See `packages/gpipe/ast/examples/basic_usage/src/main.rs:47` for trigger configuration.

### Creating Matrix Strategies

The build job demonstrates matrix strategies for parallel execution across multiple configurations:

```rust
let mut matrix_vars = BTreeMap::new();
matrix_vars.insert(
    "os".to_string(),
    vec!["ubuntu-latest".to_string(), "windows-latest".to_string(), "macos-latest".to_string()],
);
matrix_vars.insert(
    "rust".to_string(),
    vec!["stable".to_string(), "nightly".to_string()],
);

// Exclude specific combinations
let mut exclude = vec![];
let mut exclude_combo = BTreeMap::new();
exclude_combo.insert("os".to_string(), "windows-latest".to_string());
exclude_combo.insert("rust".to_string(), "nightly".to_string());
exclude.push(exclude_combo);
```

See `packages/gpipe/ast/examples/basic_usage/src/main.rs:105` for the complete matrix configuration.

### Working with Step Types

Two step variants are supported:

**UseAction** - References predefined actions:

```rust
Step::UseAction {
    id: Some("checkout".to_string()),
    uses: "checkout".to_string(),
    with: BTreeMap::new(),
    env: BTreeMap::new(),
    if_condition: None,
    continue_on_error: false,
}
```

**RunScript** - Executes shell commands:

```rust
Step::RunScript {
    id: Some("build".to_string()),
    run: "cargo build --release".to_string(),
    env: BTreeMap::new(),
    if_condition: None,
    continue_on_error: false,
    working_directory: None,
}
```

See `packages/gpipe/ast/examples/basic_usage/src/main.rs:140` for step examples.

### Building Expressions

The expression builder API provides a fluent interface for creating conditional expressions:

```rust
// Variable references
let var_expr = Expression::variable(["github", "actor"]);

// Binary operations
let equals_expr = Expression::binary_op(
    Expression::variable(["github", "event_name"]),
    BinaryOperator::Equal,
    Expression::string("push"),
);

// Function calls
let func_expr = Expression::function_call(
    "contains",
    vec![
        Expression::variable(["github", "ref"]),
        Expression::string("release"),
    ],
);
```

See `packages/gpipe/ast/examples/basic_usage/src/main.rs:230` for all expression examples.

### Serialization and Deserialization

The example shows YAML round-tripping:

```rust
// Serialize to YAML
let yaml = gpipe_ast::serde_yaml::to_string(&workflow)?;

// Deserialize from YAML
let deserialized: Workflow = gpipe_ast::serde_yaml::from_str(&yaml)?;
```

See `packages/gpipe/ast/examples/basic_usage/src/main.rs:19` for serialization usage.

## Key Concepts

### BTreeMap for Deterministic Ordering

All maps in the AST use `BTreeMap` instead of `HashMap` to ensure deterministic serialization order, which is important for:

- Reproducible YAML output
- Version control diffs
- Consistent workflow behavior

### Job Dependencies

Jobs specify dependencies using the `needs` field, creating a directed acyclic graph (DAG) of execution:

```rust
Job {
    needs: vec!["build".to_string(), "test".to_string()],
    // ... job runs after both build and test complete
}
```

### Conditional Execution

Both jobs and steps support conditional execution using the `if_condition` field with Expression types, allowing dynamic workflow behavior based on context.

### Matrix Strategies

Matrix strategies enable parallel execution across multiple configurations, with the ability to exclude specific combinations that shouldn't run.

### Step Variants

The `Step` enum uses untagged serde serialization to provide two distinct step types that serialize naturally to YAML, matching the familiar CI/CD syntax.

## Testing the Example

Run the example and verify:

1. The YAML output is well-formatted and contains all workflow elements
2. Deserialization succeeds without errors
3. All expression types are demonstrated correctly
4. Step properties can be accessed via the helper methods

You can modify the code to experiment with:

- Different trigger configurations
- Custom action definitions
- Additional matrix dimensions
- More complex expression trees
- Different job dependency graphs

## Troubleshooting

**YAML serialization fails**

Ensure all required fields are populated. The `Workflow` type requires `version`, `name`, `triggers`, `actions`, and `jobs`.

**Deserialization errors**

Check that the YAML format matches the expected schema. The `serde_yaml` error messages will indicate which field caused the issue.

**Matrix exclusions not working**

Ensure the exclusion map keys exactly match the matrix variable names (case-sensitive).

## Related Examples

This is currently the only example for gpipe_ast. Other packages in the gpipe ecosystem may provide examples for:

- Parsing GitHub Actions workflows to this AST format
- Translating this AST to other CI/CD formats
- Executing workflows locally
