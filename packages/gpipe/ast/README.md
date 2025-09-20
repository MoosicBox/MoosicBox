# gpipe_ast

Abstract syntax tree types for representing workflow definitions in a unified format.

This crate provides the core data structures for workflows, jobs, steps, and expressions that can be parsed from various CI/CD formats and executed locally or translated to different backend formats.

## Features

* Complete AST for generic workflow definitions
* GitHub Actions compatible expression language
* Serde serialization/deserialization support
* BTreeMap collections for deterministic ordering

## Usage

```rust
use gpipe_ast::*;

// Create a workflow
let workflow = Workflow {
    version: "1.0".to_string(),
    name: "My Workflow".to_string(),
    triggers: vec![],
    actions: BTreeMap::new(),
    jobs: BTreeMap::new(),
};
```