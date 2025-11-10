---
# Template: Examples Validator/Creator
# Default variables

project_name: '${repository_name}'
repository: '${repository}'
package_name: '${derive_package_name(package_path)}'
package_path: '.'
examples_path: 'examples/'
branch_name: 'docs/examples-updates-${run_id}'
custom_guidelines: ''
commit_message: 'docs(${package_name}): add/update examples'
---

You are helping validate and create examples for ${project_name}.

IMPORTANT: Follow the repository's AGENTS.md for guidance.

Context:

- REPO: ${repository}
- PACKAGE: ${package_name}
- EXAMPLES PATH: ${examples_path}
- BRANCH: ${branch_name}

## Task

Ensure ${package_name} has working examples demonstrating:

1. Core functionality
2. Common use cases
3. Integration patterns

## Requirements

- Examples must compile and run
- Each example should be self-contained
- Include comments explaining the code
- Examples should demonstrate best practices

## Verification (MANDATORY)

Before creating ANY commit, you MUST run:

1. Run `cargo fmt`
2. Run `cargo clippy --all-targets -- -D warnings`
3. Run `~/.cargo/bin/cargo-machete --with-metadata` from workspace root
4. Run `npx prettier --write "**/*.{md,yaml,yml}"` from workspace root
5. Run `~/.cargo/bin/taplo format` from workspace root
6. Run `cargo run --example <name>` for each example to verify it works

If ANY check fails, fix the issues before committing.

## Commit

If changes made:

- Commit message: "${commit_message}"
- DO NOT push

${custom_guidelines}
