---
# Template: Unit Test Coverage Enhancement
# Default variables

project_name: '${repository_name}'
repository: '${repository}'
package_path: '.'
package_name: '${derive_package_name(package_path)}'
target_path: "${project_type == 'rust' ? 'src/**/*.rs' : 'src/**/*.ts'}"
branch_name: 'test/coverage-${package_name}-${run_id}'
custom_guidelines: ''
commit_message: 'test(${package_name}): add unit tests to increase coverage'
---

You are helping increase unit test coverage for ${project_name}.

IMPORTANT: Follow the repository's AGENTS.md for guidance on code standards and test conventions.

Context:

- REPO: ${repository}
- PACKAGE: ${package_name}
- TARGET: ${target_path}
- BRANCH: ${branch_name}

## Task

Add meaningful unit tests to ${package_name} to increase test coverage for untested or undertested code.

${project_type == 'rust' ? include('rust/test-selection-criteria') : include('node/test-selection-criteria')}

${project_type == 'rust' ? include('rust/test-conventions', { package_name: package_name }) : include('node/test-conventions', { package_name: package_name })}

${project_type == 'rust' ? include('rust/dependency-management') : include('node/dependency-management')}

${project_type == 'rust' ? include('rust/verification-checklist', { package_name: package_name, run_tests: true }) : include('node/verification-checklist', { package_name: package_name, run_tests: true })}

${include('commit-message-instructions', { commit_type: 'tests', example_bullets: '- Added tests for connection pool error handling to verify proper cleanup on connection failures\\n- Added edge case tests for empty input validation in parse_config function\\n- Added concurrent access tests for cache operations to verify thread safety', no_changes_message: 'No tests added - existing coverage is adequate or no clear test opportunities identified' })}

${include('response-guidelines')}

${custom_guidelines}
