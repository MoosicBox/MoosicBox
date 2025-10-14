# gpipe_ast

Abstract syntax tree types for representing workflow definitions in a unified format.

This crate provides the core data structures for workflows, jobs, steps, and expressions. These types are designed to support parsing from various CI/CD formats and translation to different backend formats (parsing and translation functionality provided by other crates in the gpipe ecosystem).

## Features

* Complete AST for generic workflow definitions
* GitHub Actions compatible expression language
* Serde serialization/deserialization support
* BTreeMap collections for deterministic ordering

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
```

## Module Structure

- `workflow` - Core workflow, job, and trigger definitions
- `step` - Step types (UseAction and RunScript variants)
- `expression` - GitHub Actions compatible expression AST