---
# Template: Rustdoc Completeness Checker
# Default variables

project_name: '${repository_name}'
repository: '${repository}'
package_path: '.'
package_name: '${derive_package_name(package_path)}'
target_path: 'src/**/*.rs'
branch_name: 'docs/rustdoc-updates-${run_id}'
custom_guidelines: ''
is_refinement_pass: 'false'
refinement_context: ''
commit_message: 'docs(${package_name}): complete rustdoc for public APIs'
---

${is_refinement_pass == 'true' ? '# Additional Rustdoc Refinement for ' + package_name + '\n\nThis is a refinement pass on an existing rustdoc update branch.\n\n## Previous Context\n\nThe rustdoc at `' + package_path + '/src/` has already been reviewed and potentially updated.\n\n## Requirements for Refinement\n\n- Review the current state of the rustdoc\n- Apply additional improvements based on any new guidance below\n- Only make changes that add value beyond previous updates\n- Preserve previous improvements unless they conflict with new guidance\n\nFocus on incremental improvements based on the additional guidance.\n\n---\n\n' : ''}You are helping ensure complete rustdoc documentation for ${project_name}.

IMPORTANT: Follow the repository's AGENTS.md for guidance on rustdoc standards.

Context:

- REPO: ${repository}
- PACKAGE: ${package_name}
- TARGET: ${target_path}
- BRANCH: ${branch_name}

## Task

Check that ALL public APIs in ${target_path} have:

1. **Module-level docs** - Each module has `//!` documentation
2. **Struct/enum docs** - Every public type is documented
3. **Function docs** - All public functions have doc comments
4. **Error docs** - Error conditions documented with `# Errors`
5. **Example docs** - Complex functions have `# Examples`
6. **Panic docs** - Functions that panic have `# Panics`
7. **Must-use** - Constructors and getters that return non-Result/non-Option types should have `#[must_use]`. IMPORTANT: Do NOT add `#[must_use]` to functions returning Result or Option types, as these types are already marked `#[must_use]` and adding the attribute to the function is redundant and will trigger clippy warnings. Only add `#[must_use]` to functions that return other types where ignoring the return value would be a mistake.

## Rustdoc Style (from AGENTS.md)

- Use asterisks (\*) for bullet points in error docs
- Document all error conditions
- Include examples for complex functions
- Add `#[must_use]` to constructors and getters that return types OTHER THAN Result or Option
- **CRITICAL**: Do NOT add `#[must_use]` to functions returning Result or Option - these types are already marked `#[must_use]` and adding it to the function is redundant and will cause clippy warnings
- Clippy's `must_use_candidate` lint will suggest where to add `#[must_use]` - but only follow this suggestion for non-Result/non-Option return types

${include('rust/verification-checklist', { package_name: package_name, run_tests: false, run_doc_check: true })}

${include('commit-message-instructions', { commit_type: 'changes to rustdoc', example_bullets: '- Added missing `# Errors` section to `parse_config` function documenting ConfigError and IoError cases\\n- Fixed `connect` function docs to correctly state it returns ConnectionPool instead of Connection\\n- Added `#[must_use]` attribute to constructor methods per AGENTS.md guidelines', no_changes_message: 'No changes required - documentation is adequate' })}

${include('response-guidelines')}

${custom_guidelines}
