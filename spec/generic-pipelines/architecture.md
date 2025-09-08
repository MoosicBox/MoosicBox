# Generic Pipelines Architecture

## System Overview

Generic Pipelines is a universal CI/CD workflow orchestration tool that enables:

1. Writing platform-agnostic workflows in a generic format
2. Executing any workflow format locally without containers
3. Translating between different CI/CD platforms

## Architecture Principles

### 1. Generic Format as First-Class Citizen

The generic workflow format is not a special case - it's treated as just another backend alongside GitHub Actions, GitLab CI, etc. This ensures consistent parsing, translation, and execution logic.

### 2. AST as Universal Intermediate Representation

All workflow formats compile to the same Abstract Syntax Tree (AST), which serves as the common language between different CI systems:

```
[Generic YAML]  ─┐
[GitHub Actions] ├→ [Parser] → [AST] → [Backend] → [Execution/Output]
[GitLab CI]     ─┘
```

### 3. No Built-in Magic

No special built-in actions. Everything is explicitly defined through generic action YAML definitions that users can inspect, modify, and extend.

## Core Components

### Generic Workflow Format

Platform-agnostic YAML format with:

- Standard workflow structure (jobs, steps)
- Backend conditional execution
- GitHub Actions-compatible expression syntax
- Extensible through custom actions

Example:

```yaml
name: Build and Test
on: [push, pull_request]

jobs:
    build:
        steps:
            - uses: checkout
              with:
                  depth: 1

            - name: GitHub-specific step
              if: ${{ backend == 'github' }}
              uses: actions/setup-node@v3

            - name: Local-specific step
              if: ${{ backend == 'local' }}
              run: nvm use 18

            - run: npm test
```

### Generic Actions

User-defined YAML files that describe how to execute an action across different backends:

```yaml
# checkout.action.yml
name: checkout
description: Check out repository code
inputs:
    depth:
        description: Number of commits to fetch
        default: 1

translations:
    github:
        uses: actions/checkout@v3
        with:
            fetch-depth: ${{ inputs.depth }}

    gitlab:
        run: |
            git clone --depth ${{ inputs.depth }} $CI_REPOSITORY_URL .

    local:
        run: |
            git fetch --depth ${{ inputs.depth }}
            git checkout ${{ env.GIT_BRANCH }}
```

### Backend Conditionals

Steps can be conditionally executed based on target backend:

- Skip incompatible steps gracefully
- Clear error messages when no backend translation exists
- Validation mode to check backend compatibility

### Execution Modes

1. **Local Execution**: Run workflows directly without containers
2. **Translation**: Convert between workflow formats
3. **Validation**: Check workflow compatibility with backends
4. **Dry Run**: Preview what would be executed

## Implementation Architecture

### Package Structure

```
packages/
├── pipeline_ast/          # Core AST definitions
├── pipeline_parser/       # Format parsers
│   ├── src/
│   │   ├── generic.rs    # Generic format parser
│   │   ├── github.rs     # GitHub Actions parser
│   │   └── gitlab.rs     # GitLab CI parser
├── pipeline_runner/       # Local execution engine
├── pipeline_actions/      # Action resolution and translation
└── pipeline_backends/     # Output generators
```

### Data Flow

1. **Input**: Workflow file (any supported format)
2. **Parse**: Convert to AST representation
3. **Resolve**: Load generic action definitions
4. **Transform**: Apply backend-specific translations
5. **Execute/Output**: Run locally or generate target format

### Context System

GitHub Actions-compatible context variables:

- `${{ backend }}` - Current backend identifier
- `${{ env.* }}` - Environment variables
- `${{ secrets.* }}` - Secret values
- `${{ inputs.* }}` - Action/workflow inputs
- `${{ steps.*.outputs.* }}` - Step outputs

## Error Handling Strategy

### Clear Error Messages

- Backend incompatibility: "Step 'xyz' requires GitHub backend but running on 'local'"
- Missing action: "Cannot find action 'custom-action' in: ./actions, ~/.ci-runner/actions"
- Invalid expression: "Unknown context 'foo' in expression '${{ foo.bar }}'"

### Graceful Degradation

- Skip backend-specific steps when running on other backends
- Warn about skipped steps in output
- Continue execution unless critical step fails

## Future Extensibility

### Planned Extensions

- Action registry/marketplace for sharing generic actions
- Additional backend support (Jenkins, CircleCI, etc.)
- Hot-reload for development workflows
- Workflow composition and includes

### Plugin Architecture

- Custom parsers for new workflow formats
- Custom backends for new CI systems
- Custom action resolvers for enterprise systems
