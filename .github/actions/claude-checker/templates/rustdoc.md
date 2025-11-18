---
# Template: Rustdoc Completeness Checker
# Default variables

project_name: '${repository_name}'
repository: '${repository}'
package_name: '${derive_package_name(package_path)}'
package_path: '.'
target_path: 'src/**/*.rs'
branch_name: 'docs/rustdoc-updates-${run_id}'
custom_guidelines: ''
commit_message: 'docs(${package_name}): complete rustdoc for public APIs'
---

You are helping ensure complete rustdoc documentation for ${project_name}.

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
7. **Must-use** - Constructors and getters that don't return Result or Option have `#[must_use]` (Result/Option are already must_use by default)

## Rustdoc Style (from AGENTS.md)

- Use asterisks (\*) for bullet points in error docs
- Document all error conditions
- Include examples for complex functions
- Add `#[must_use]` to constructors and getters that return direct types (Self, String, Vec, etc.)
- DO NOT add `#[must_use]` to functions returning Result or Option - these types already have the attribute

## Verification (MANDATORY)

Before creating ANY commit, you MUST run:

1. Run `cargo fmt`
2. Run `cargo clippy --all-targets -- -D warnings`
3. Run `~/.cargo/bin/cargo-machete --with-metadata` from workspace root
4. Run `npx prettier --write "**/*.{md,yaml,yml}"` from workspace root
5. Run `~/.cargo/bin/taplo format` from workspace root
6. Run `cargo doc -p ${package_name} --no-deps` to verify docs build

If ANY check fails, fix the issues before committing.
NEVER commit code that doesn't pass all checks.

## Commit

If changes made:

- Commit message: "${commit_message}"
- DO NOT push

## Response Guidelines

When responding to users:

- NEVER reference files in /tmp or other temporary directories - users cannot access these
- Always include plans, summaries, and important information directly in your comment response
- If you create a plan or analysis, paste the full content in your response, not just a file path
- Remember: you run on an ephemeral server - any files you create are only accessible during your execution

${custom_guidelines}
