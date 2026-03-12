# gpipe_ast

Abstract syntax tree types for representing workflow definitions in a unified format.

This crate provides the core data structures for workflows, jobs, steps, and expressions. These types are designed to support parsing from various CI/CD formats and translation to different backend formats (parsing and translation functionality provided by other crates in the gpipe ecosystem).

## Features

- Complete AST for generic workflow definitions
- GitHub Actions compatible expression language
- Serde serialization/deserialization support
- BTreeMap collections for deterministic ordering

## Installation

Add this crate to your `Cargo.toml`:

```toml
[dependencies]
gpipe_ast = "0.1.0"
```

## Usage

```rust
use gpipe_ast::*;
use std::collections::BTreeMap;

// Create a workflow
let workflow = Workflow {
    version: "1.0".to_string(),
    name: "My Workflow".to_string(),
    triggers: vec![],
    actions: BTreeMap::new(),
    jobs: BTreeMap::new(),
};

// Create steps
let run_step = Step::RunScript {
    id: Some("test".to_string()),
    run: "cargo test".to_string(),
    env: BTreeMap::new(),
    if_condition: None,
    continue_on_error: false,
    working_directory: None,
};

let action_step = Step::UseAction {
    id: None,
    uses: "checkout".to_string(),
    with: BTreeMap::new(),
    env: BTreeMap::new(),
    if_condition: None,
    continue_on_error: false,
};

// Build expressions for `if` conditions
let condition = Expression::binary_op(
    Expression::variable(["github", "ref"]),
    BinaryOperator::Equal,
    Expression::string("refs/heads/main"),
);

// Serialize/deserialize workflows with the re-exported serde_yaml crate
let yaml = gpipe_ast::serde_yaml::to_string(&workflow).unwrap();
let parsed: Workflow = gpipe_ast::serde_yaml::from_str(&yaml).unwrap();
```

## Module Structure

- `workflow` - Core workflow, job, and trigger definitions
- `step` - Step types (UseAction and RunScript variants)
- `expression` - GitHub Actions compatible expression AST

## License

MPL-2.0
