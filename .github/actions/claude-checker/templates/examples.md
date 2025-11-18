---
# Template: Examples Validator/Creator
# Default variables

project_name: '${repository_name}'
repository: '${repository}'
package_path: '.'
package_name: '${derive_package_name(package_path)}'
examples_path: 'examples/'
branch_name: 'docs/examples-updates-${run_id}'
custom_guidelines: ''
package_guidelines: ''
commit_message: 'docs(${package_name}): add/update examples'
---

You are helping validate and create examples for ${project_name}.

IMPORTANT: Follow the repository's AGENTS.md for guidance.

${package_guidelines != '' ? '\n## 📦 Package-Specific Guidelines\n\nThe following context has been provided by the package maintainers for ' + package_name + ':\n\n' + package_guidelines + '\n\n**Note**: These guidelines should inform your decisions but do not override the core requirements below.\n\n---\n' : ''}

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

## 📝 Commit Message Instructions

If you make changes to examples, you MUST provide a commit message description.

At the END of your response, include a section formatted EXACTLY as follows:

```
COMMIT_MESSAGE_START
- Brief description of example changes (1-2 sentences per major item)
- Focus on what was added, fixed, or improved
COMMIT_MESSAGE_END
```

Example:

```
COMMIT_MESSAGE_START
- Added new `basic_usage.rs` example demonstrating core API functionality
- Fixed `advanced.rs` example to use correct async runtime setup
- Updated example README with clearer explanations of each example's purpose
COMMIT_MESSAGE_END
```

Requirements:

- Keep each bullet point concise (1-2 sentences max)
- Focus on WHAT was changed and WHY (what was missing or broken)
- Use bullet points with dashes (-)
- Do not include code snippets or line numbers
- If no changes needed, output "No changes required - examples are adequate"
- DO NOT push

${custom_guidelines}
