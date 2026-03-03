---
# Template: TSDoc Completeness Checker
# Default variables

project_name: '${repository_name}'
repository: '${repository}'
package_path: '.'
package_name: '${derive_package_name(package_path)}'
target_path: 'src/**/*.ts'
branch_name: 'docs/tsdoc-updates-${run_id}'
custom_guidelines: ''
is_refinement_pass: 'false'
refinement_context: ''
commit_message: 'docs(${package_name}): complete TSDoc for public APIs'
---

${is_refinement_pass == 'true' ? '# Additional TSDoc Refinement for ' + package_name + '\n\nThis is a refinement pass on an existing TSDoc update branch.\n\n## Previous Context\n\nThe TSDoc at `' + package_path + '/src/` has already been reviewed and potentially updated.\n\n## Requirements for Refinement\n\n- Review the current state of the documentation\n- Apply additional improvements based on any new guidance below\n- Only make changes that add value beyond previous updates\n- Preserve previous improvements unless they conflict with new guidance\n\nFocus on incremental improvements based on the additional guidance.\n\n---\n\n' : ''}You are helping ensure complete TSDoc documentation for ${project_name}.

IMPORTANT: Follow the repository's AGENTS.md for guidance on documentation standards.

Context:

- REPO: ${repository}
- PACKAGE: ${package_name}
- TARGET: ${target_path}
- BRANCH: ${branch_name}

## Task

Check that ALL public APIs in ${target_path} have complete TSDoc documentation:

1. **Module-level docs** - Each module/file has a top-level `/** ... */` doc comment describing its purpose
2. **Interface/Type docs** - Every exported interface, type alias, and class is documented
3. **Function docs** - All exported functions have doc comments with summary
4. **Parameter docs** - Use `@param name - Description` for each parameter
5. **Return docs** - Use `@returns Description` to document return values
6. **Error docs** - Use `@throws {ErrorType} Description` to document thrown errors
7. **Example docs** - Complex functions have `@example` blocks with code samples
8. **Deprecation** - Deprecated APIs use `@deprecated` with migration guidance

## TSDoc Style Guidelines

- Use `/** ... */` block comments (not `//` line comments)
- Start with a brief summary sentence (no tag needed for summary)
- Use `@param name - Description` format (note the dash separator)
- Use `@returns Description` (not `@return`)
- Use `@throws {ErrorType} Description` for errors that may be thrown
- Use `@example` followed by a fenced code block for usage examples
- Use `@see` to reference related APIs or external documentation
- Use `@deprecated Reason. Use {@link alternative} instead.` for deprecated APIs
- Use `@internal` for APIs that are exported but not part of public API

## What to Document

**DO document:**

- All `export`ed functions, classes, interfaces, types, and constants
- Constructor parameters and their purposes
- Complex type parameters with `@typeParam`
- Side effects and state changes
- Async behavior and Promise resolution
- Default values when not obvious

**DO NOT document:**

- Private/internal implementation details (unless exported)
- Self-evident parameters (e.g., `@param id - The ID` adds no value)
- Obvious return values (e.g., `@returns The result` on `getResult()`)
- Implementation details that may change

## Example TSDoc

````typescript
/**
 * Fetches user data from the API by user ID.
 *
 * Retrieves the complete user profile including preferences and settings.
 * Results are cached for 5 minutes to reduce API calls.
 *
 * @param userId - The unique identifier of the user to fetch
 * @param options - Optional configuration for the request
 * @returns The user data object with profile information
 * @throws {NotFoundError} When no user exists with the given ID
 * @throws {NetworkError} When the API is unreachable
 * @throws {ValidationError} When userId is empty or malformed
 *
 * @example
 * ```typescript
 * // Basic usage
 * const user = await fetchUser('user-123');
 * console.log(user.name);
 *
 * // With options
 * const user = await fetchUser('user-123', { includeSettings: true });
 * ```
 *
 * @see {@link updateUser} for modifying user data
 * @see {@link deleteUser} for removing users
 */
export async function fetchUser(userId: string, options?: FetchOptions): Promise<User> {
    // implementation
}

/**
 * Configuration options for user-related API requests.
 */
export interface FetchOptions {
    /**
     * Whether to include user settings in the response.
     * @defaultValue false
     */
    includeSettings?: boolean;

    /**
     * Request timeout in milliseconds.
     * @defaultValue 5000
     */
    timeout?: number;
}

/**
 * Represents a user in the system.
 */
export interface User {
    /** Unique identifier for the user. */
    id: string;

    /** Display name of the user. */
    name: string;

    /** Email address (may be undefined if not provided). */
    email?: string;
}
````

${include('node/verification-checklist', { package_name: package_name, run_tests: false, run_doc_check: true })}

${include('commit-message-instructions', { commit_type: 'changes to TSDoc', example_bullets: '- Added missing `@throws` documentation to `fetchUser` function for NotFoundError and NetworkError cases\\n- Fixed `parseConfig` function docs to correctly describe the return type\\n- Added `@example` blocks to complex utility functions in utils.ts', no_changes_message: 'No changes required - documentation is adequate' })}

${include('response-guidelines')}

${custom_guidelines}
