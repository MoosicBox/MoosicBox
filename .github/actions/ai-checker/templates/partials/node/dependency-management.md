---
# Partial: Node/TypeScript Dependency Management Guidelines
# Expected variables: none required
---

## Dependency Management

**CRITICAL**: Follow these dependency management rules when adding packages.

### Before Adding Dependencies

1. **Check existing dependencies first**: Before adding a new package, check if:
    - The functionality exists in an existing dependency
    - A lighter alternative is already installed
    - The built-in Node.js APIs can accomplish the task

2. **Evaluate the package**:
    - Check the package's maintenance status (recent updates, open issues)
    - Review the bundle size impact
    - Check for known vulnerabilities with `${pm_audit()}`

### Dependency Types

- **dependencies**: Runtime requirements (code that runs in production)
- **devDependencies**: Development-only tools (test frameworks, linters, build tools, type definitions)
- **peerDependencies**: Requirements that the consumer must provide (for libraries/plugins)

### Adding Dependencies

```bash
# Add production dependency
${pm_install('package-name')}

# Add dev dependency
${pm_install('package-name', true)}
```

### Version Specification

- **Exact version** (`"1.2.3"`): For critical dependencies where any change could break functionality
- **Caret** (`"^1.2.3"`): Allows minor and patch updates (most common, recommended default)
- **Tilde** (`"~1.2.3"`): Allows only patch updates (more conservative)

### Common Dependencies

Before adding, check if these are already installed:

**Testing:**

- `vitest` - Test runner
- `@testing-library/react` / `@testing-library/dom` - DOM testing utilities
- `msw` - API mocking
- `@faker-js/faker` - Test data generation

**Development:**

- `typescript` - TypeScript compiler
- `eslint` - Linter
- `prettier` - Code formatter

**Utilities:**

- `lodash` / `lodash-es` - Utility functions (consider if native JS can do it)
- `date-fns` / `dayjs` - Date manipulation
- `zod` - Schema validation

### Workspace/Monorepo Considerations

In monorepo setups:

- Shared dependencies should typically be in the root `package.json`
- Package-specific dependencies go in that package's `package.json`
- Use workspace protocol for internal packages: `"my-package": "workspace:*"`
