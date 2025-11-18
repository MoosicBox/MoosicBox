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

## 🔍 First: Check if README Exists

Before reviewing, check if `${readme_path}` exists at `${package_path}/${readme_path}`:

**If README does NOT exist:**

- Create a new README.md from scratch
- Base it on the actual code in `${package_path}/src/`
- Check `${package_path}/Cargo.toml` for package metadata (name, description, dependencies)
- Include standard sections: Description, Features (if applicable), Installation, Usage, License
- Follow the public API documentation rules below
- Keep it concise but complete for fundamental usage
- Focus only on what users of this package need to know

**If README exists:**

- Review it for fundamental errors and omissions only (see constraints below)

## Task

${readme_path == 'README.md' || readme_path == './README.md' ? 'Review or create the root README for ' + project_name : 'Review or create the README for the ' + package_name + ' package'}

## ⚠️ CRITICAL CONSTRAINT: FUNDAMENTAL ERRORS ONLY

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

**FORBIDDEN Changes (even if you think they would be "better"):**

- ❌ Rewording for clarity, style, or tone
- ❌ Reorganizing sections or structure
- ❌ Formatting/markdown improvements
- ❌ Adding more examples when basics are already covered
- ❌ Expanding descriptions that are already accurate
- ❌ Minor completeness improvements
- ❌ Changing future tense to present or vice versa (if already marked correctly)
- ❌ Removing features that are configured/enabled even if not fully implemented
- ❌ Nitpicking wording differences when the meaning is substantially the same
- ❌ Including specific line numbers in code references (e.g., `src/file.rs:123`) - line numbers change frequently and should be omitted
- ❌ Including specific counts of tests/test cases (e.g., "5 test cases", "10 tests") - test counts change frequently and should be omitted

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

## 📖 PUBLIC API FOCUS - Do Not Document Internals

READMEs are for **users of the package**, not maintainers. Only document the public-facing API.

**DO Document:**

- ✅ Public functions, structs, traits (items with `pub` visibility)
- ✅ Cargo features users can enable (e.g., `--features async`)
- ✅ Main entry points and usage patterns
- ✅ Public configuration options
- ✅ Integration examples for users

**DO NOT Document:**

- ❌ Internal macros (`macro_rules!` not in public API)
- ❌ Private or crate-private items (`pub(crate)`, `pub(super)`, or non-pub items)
- ❌ Implementation details (caches, thread pools, internal state)
- ❌ Test utilities or `#[cfg(test)]` code
- ❌ Build scripts or internal feature implementations
- ❌ Helper functions only used within the crate

**How to identify internal items when reviewing code:**

1. Check visibility: `pub` without qualifiers = Public ✅ | `pub(crate)` or no `pub` = Internal ❌
2. Check if exported in `lib.rs` or module root = Public ✅
3. Internal naming patterns (`_helper`, `internal_*`) = Internal ❌
4. Only called within same crate = Internal ❌

**Decision Rule for Documentation:**
When considering whether to document something, ask: "Would a user of this library as a dependency need to know this?"

- YES (it's a public API they'll call) → Document it
- NO (it's internal implementation) → Leave it out or remove it

## 📝 Commit Message Instructions

If you make changes to the README, you MUST provide a commit message description.

At the END of your response, include a section formatted EXACTLY as follows:

```
COMMIT_MESSAGE_START
- Brief description of what changed and why (1-2 sentences)
- Additional changes if applicable
COMMIT_MESSAGE_END
```

Example:

```
COMMIT_MESSAGE_START
- Removed claim about WebSocket support as the feature is not implemented in the codebase
- Added documentation for the new `connect_async` method which is exported in lib.rs but was missing from README
COMMIT_MESSAGE_END
```

Requirements:

- Keep each bullet point concise (1-2 sentences max)
- Focus on WHAT changed and WHY (the fundamental issue)
- Use bullet points with dashes (-)
- Do not include code snippets or line numbers
- If no changes needed, output "No changes required - documentation is accurate"

## Verification Process

1. **🚫 MANDATORY Git History Regression Check**

    Before making ANY change to the README, you MUST check git history for the SPECIFIC LINES you want to modify.

    **Required Commands:**
    For each line or section you want to change, run:

    ```bash
    # Replace LINE_NUMBER with the actual line number you want to modify
    # Add +/- 5 lines of context to catch related changes
    git log -L LINE_NUMBER,+1:${readme_path} --format="%H %s%n%b"

    # For a section spanning multiple lines (e.g., lines 240-250):
    git log -L 240,250:${readme_path} --format="%H %s%n%b"
    ```

    **What to Look For:**

    - **Revert commits**: Messages containing "revert", "revert back to", "prefer X format", "changed back to"
    - **PR discussion references**: Look for "Triggered-by:" URLs in commit messages
    - **Explicit reasoning**: Commit messages explaining "prefer X because Y" or "more concise", "better for readability"
    - **Multiple attempts**: If the same change appears 2+ times in history, this is a STRONG SIGNAL

    **Blocking Rules (NON-NEGOTIABLE):**

    - ❌ **If you find a revert commit for the exact change you want to make**: STOP. DO NOT make that change.
    - ❌ **If the same change was reverted 2+ times**: HARD STOP. The maintainers have decided against it.
    - ❌ **If a commit contains "Triggered-by:" with a GitHub PR/discussion URL**: You MUST read that discussion to understand the reasoning before proceeding.
    - ❌ **If unclear after reading history**: Leave it alone. Document the uncertainty in your output for human review.

    **Decision Process:**

    1. Run `git log -L` for the specific line(s) you want to change
    2. Read ALL commit messages in the history (no arbitrary limits - check FULL history)
    3. If you see a revert or "Triggered-by:" reference:
       a. Extract the discussion URL if present
       b. Understand why the change was reverted
       c. If your proposed change matches what was reverted: BLOCK IT
    4. Document your findings in a `REGRESSION_CHECK` block (format below)

    **Required Output Format:**
    For EVERY change you consider making, include this in your response:

    ```
    REGRESSION_CHECK_START
    File: ${readme_path}
    Line(s): [specific line numbers]
    Proposed change: [brief description of what you wanted to do]
    Git history checked: YES
    Revert found: [YES/NO]
    Revert details: [commit hash and reasoning if YES, otherwise "None found"]
    Decision: [PROCEED/BLOCKED - explanation]
    REGRESSION_CHECK_END
    ```

    **Example of BLOCKED change:**

    ```
    REGRESSION_CHECK_START
    File: ${readme_path}
    Line(s): 244
    Proposed change: Change --features "feature1,feature2" to --features feature1 --features feature2
    Git history checked: YES
    Revert found: YES
    Revert details: Commit 27bb0ed83 reverted this exact change with reasoning "prefer comma-separated for conciseness" and references PR discussion at https://github.com/${repository}/pull/175#discussion_r2462901200
    Decision: BLOCKED - This change was already discussed and reverted. Will not make it again.
    REGRESSION_CHECK_END
    ```

2. **Check Claims Against Code**

    - Read the code at `${package_path}/src/` to verify README claims
    - Compare API examples with actual function signatures
    - Check `${package_path}/Cargo.toml` for dependency accuracy

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
