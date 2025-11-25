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

${package_guidelines != '' ? '\n## Package-Specific Guidelines\n\nThe following context has been provided by the package maintainers for ' + package_name + ':\n\n' + package_guidelines + '\n\n**Note**: These guidelines should inform your decisions but do not override the core requirements below.\n\n---\n' : ''}

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

${include('rust/verification-checklist', { package_name: package_name, run_tests: false })}

After running the verification checklist, also run `cargo run --example <name>` for each example to verify it works.

${include('commit-message-instructions', { commit_type: 'changes to examples', example_bullets: '- Added new `basic_usage.rs` example demonstrating core API functionality\\n- Fixed `advanced.rs` example to use correct async runtime setup\\n- Updated example README with clearer explanations of each example\'s purpose', no_changes_message: 'No changes required - examples are adequate' })}

${custom_guidelines}
