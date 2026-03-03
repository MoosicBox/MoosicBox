---
# Template: README Accuracy Checker
# Default variables (lowest priority - can be overridden)

project_name: '${repository_name}'
repository: '${repository}'
package_path: '.'
readme_path: 'README.md'
package_name: "${package_path != '.' ? derive_package_name(package_path) : ''}"
is_root_readme: "${readme_path == 'README.md' || readme_path == './README.md'}"
branch_name: "${is_root_readme ? 'docs/root-readme-updates-' + run_id : 'docs/readme-updates-' + run_id}"
custom_guidelines: ''
is_refinement_pass: 'false'
refinement_context: ''
commit_message: "${package_name ? 'docs(' + package_name + '): update README for accuracy' : 'docs(root): update README for accuracy'}"
---

${is_refinement_pass == 'true' ? '# Additional README Refinement' + (package_name ? ' for ' + package_name : '') + '\n\nThis is a refinement pass on an existing README update branch.\n\n## Previous Context\n\nThe README at `' + (package_path != '.' ? package_path + '/' : '') + 'README.md` has already been reviewed and potentially updated.\n\n## Requirements for Refinement\n\n- Review the current state of the README\n- Apply the additional guidance below\n- Only make changes that align with the new guidance\n- Preserve previous improvements unless they conflict with new guidance\n\nFocus on incremental improvements based on the additional guidance.\n\n---\n\n' : ''}# README Accuracy Review${package_name ? ' for ' + package_name : ''}

## First: Check if README Exists

Before reviewing, check if `${readme_path}` exists at `${package_path}/${readme_path}`:

**If README does NOT exist:**

- Create a new README.md from scratch
- Base it on the actual code in `${package_path}/src/`
  ${project_type == 'rust' ? '- Check `' + package_path + '/Cargo.toml` for package metadata (name, description, dependencies)' : ''}${project_type == 'node' ? '- Check `' + package_path + '/package.json` for package metadata (name, description, dependencies)' : ''}
- Include standard sections: Description, Features (if applicable), Installation, Usage, License
- Follow the public API documentation rules below
- Keep it concise but complete for fundamental usage
- Focus only on what users of this package need to know

**If README exists:**

- Review it for fundamental errors and omissions only (see constraints below)

## Task

${readme_path == 'README.md' || readme_path == './README.md' ? 'Review or create the root README for ' + project_name : 'Review or create the README for the ' + package_name + ' package'}

## CRITICAL CONSTRAINT: FUNDAMENTAL ERRORS ONLY

You may ONLY make changes for these reasons:

**Fundamentally Incorrect:**

- README claims a feature that doesn't exist in the code at `${package_path}/`
- Code examples show wrong function signatures (don't match actual code in `${package_path}/src/`)
- Dependencies listed don't match `${package_path}/Cargo.toml`
- Module/file references don't match actual structure
- Links are broken or point to wrong locations

**Fundamentally Incomplete:**

- A major implemented feature is completely missing from README
- Critical usage information is absent (e.g., how to use the main API)
- Critical public API items lack documentation (main entry points, primary functions, core types users interact with)
- CLI programs missing documentation for commands, subcommands, or important arguments

**FORBIDDEN Changes (even if you think they would be "better"):**

- Rewording for clarity, style, or tone
- Reorganizing sections or structure
- Formatting/markdown improvements
- Adding more examples when basics are already covered
- Expanding descriptions that are already accurate
- Minor completeness improvements
- Changing future tense to present or vice versa (if already marked correctly)
- Removing features that are configured/enabled even if not fully implemented
- Nitpicking wording differences when the meaning is substantially the same
- Including specific line numbers in code references (e.g., `src/file.rs:123`) - line numbers change frequently and should be omitted
- Including specific counts of tests/test cases (e.g., "5 test cases", "10 tests") - test counts change frequently and should be omitted

**Decision Rule:**
Before making ANY change, ask yourself:

1. "Would a user be MISLED or UNABLE TO USE this package because of this issue?"
2. "Am I removing information that is technically accurate based on configuration/capabilities?"

- If either is NO → Leave it alone
- If both are YES → Fix it (it's fundamental)

**Examples of FORBIDDEN changes:**

- Changing "System notifications, tray integration, and OS-specific features" to just "System notification support" (removing configured capabilities)
- Changing "Media keys, notifications, and system tray" to "Media keys and notifications" (removing tray mention when capability exists)
- Simplifying feature lists that accurately describe configured functionality

## PUBLIC API FOCUS - Do Not Document Internals

READMEs are for **users of the package**, not maintainers. Only document the public-facing API.

**DO Document:**

- Public functions, structs, traits (items with `pub` visibility)
- Cargo features users can enable (e.g., `--features async`)
- Main entry points and usage patterns
- Public configuration options
- Integration examples for users

**REQUIRED: Add Documentation for Critical Public APIs**

While preserving existing README content and style, you MUST add documentation for critical public API items that are missing:

- **Main entry points**: Primary functions/methods users call to use the library
- **Core types**: Key structs, traits, and enums that users interact with directly
- **CLI programs**: All commands, subcommands, and important arguments/flags with examples
- **Configuration**: Public options that affect behavior

This is NOT about documenting every `pub` item - focus on what users actually need to know to use the library effectively. If a user would be unable to use a key feature because it's undocumented, that's a fundamental omission that must be fixed.

**DO NOT Document:**

- Internal macros (`macro_rules!` not in public API)
- Private or crate-private items (`pub(crate)`, `pub(super)`, or non-pub items)
- Implementation details (caches, thread pools, internal state)
- Test utilities or `#[cfg(test)]` code
- Build scripts or internal feature implementations
- Helper functions only used within the crate

**How to identify internal items when reviewing code:**

1. Check visibility: `pub` without qualifiers = Public | `pub(crate)` or no `pub` = Internal
2. Check if exported in `lib.rs` or module root = Public
3. Internal naming patterns (`_helper`, `internal_*`) = Internal
4. Only called within same crate = Internal

**Decision Rule for Documentation:**
When considering whether to document something, ask: "Would a user of this library as a dependency need to know this to use the library effectively?"

- YES (it's a critical API they'll use) → **Add documentation if missing**
- NO (it's internal or rarely-used implementation detail) → Leave it out or remove it

Note: Adding missing documentation for critical APIs is a **required fix**, not a stylistic improvement.

${include('commit-message-instructions', { commit_type: 'changes to the README', example_bullets: '- Removed claim about WebSocket support as the feature is not implemented in the codebase\\n- Added documentation for the new `connect_async` method which is exported in lib.rs but was missing from README', no_changes_message: 'No changes required - documentation is accurate' })}

## Verification Process

1. ${include('regression-check', { file_path: readme_path, repository: repository })}

2. **Check Claims Against Code** - Read the code at `${package_path}/src/` to verify README claims - Compare API examples with actual function signatures
   ${project_type == 'rust' ? '    - Check `' + package_path + '/Cargo.toml` for dependency accuracy' : ''}${project_type == 'node' ? ' - Check `' + package_path + '/package.json` for dependency accuracy' : ''}

3. **Identify Only Fundamental Issues**
    - Focus on factual errors and critical omissions
    - Ignore style, wording, or organizational preferences

## Scope

Only modify `${readme_path}`. Do not change any code files.

## Output

- If the README is fundamentally accurate and complete: **Make NO changes**
- If you find fundamental errors: Fix them with minimal edits
- Do not "improve" things that are already correct

${custom_guidelines}
