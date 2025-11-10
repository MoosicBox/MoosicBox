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
7. **Must-use** - Constructors and getters have `#[must_use]`

## Rustdoc Style (from AGENTS.md)

- Use asterisks (\*) for bullet points in error docs
- Document all error conditions
- Include examples for complex functions
- Add `#[must_use]` to constructors and getters

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

${custom_guidelines}
