# Claude Checker Action

Generic Claude-based documentation and code checker with customizable prompt templates.

## Overview

This action provides a flexible, template-driven system for running Claude Code checks on your repository. It supports:

- **Built-in templates** for common checks (README, rustdoc, examples, issue handling, PR comments, code review)
- **Custom templates** via file path or inline text
- **Variable interpolation** with YAML frontmatter defaults
- **Priority-based variable resolution** (user vars > template defaults > auto-detected)
- **Verification profiles** for different project types (Rust, TypeScript, Python, etc.)

## Quick Start

### Basic Usage (README Checker)

```yaml
- uses: MoosicBox/MoosicBox/.github/actions/claude-checker@v1
  with:
      github_token: ${{ secrets.GITHUB_TOKEN }}
      claude_token: ${{ secrets.CLAUDE_CODE_OAUTH_TOKEN }}
      prompt_template: 'readme'
```

This will check the root README.md using all default settings.

### Package README

```yaml
- uses: MoosicBox/MoosicBox/.github/actions/claude-checker@v1
  with:
      github_token: ${{ secrets.GITHUB_TOKEN }}
      claude_token: ${{ secrets.CLAUDE_CODE_OAUTH_TOKEN }}
      prompt_template: 'readme'
      template_vars: |
          package_path: packages/audio
```

## Inputs

### Required

| Input          | Description                         |
| -------------- | ----------------------------------- |
| `github_token` | GitHub token with write permissions |
| `claude_token` | Claude Code OAuth token             |

### Prompt Template (one required)

| Input                  | Description                                                                           |
| ---------------------- | ------------------------------------------------------------------------------------- |
| `prompt_template`      | Built-in template name: `readme`, `rustdoc`, `examples`, `issue`, `pr`, `code-review` |
| `prompt_template_file` | Path to custom template file                                                          |
| `prompt_template_text` | Inline template text                                                                  |

### Optional

| Input                        | Description                                                              | Default                           |
| ---------------------------- | ------------------------------------------------------------------------ | --------------------------------- |
| `template_vars`              | YAML/JSON object of variables                                            | `{}`                              |
| `verification_profile`       | Built-in profile: `auto`, `rust`, `typescript`, `python`, `go`, `custom` | `auto`                            |
| `verification_config_file`   | Path to verification config YAML                                         | `.github/claude-verification.yml` |
| `verification_config_inline` | Inline verification config (YAML)                                        |                                   |
| `branch_name`                | Branch to commit to (overrides template default)                         |                                   |
| `commit_message`             | Commit message (supports template variables)                             |                                   |
| `auto_commit`                | Automatically commit changes                                             | `true`                            |
| `model`                      | Claude model                                                             |                                   |
| `max_tokens`                 | Max tokens                                                               |                                   |
| `max_turns`                  | Max turns                                                                |                                   |
| `claude_args`                | Additional Claude Code arguments                                         |                                   |
| `working_directory`          | Working directory                                                        | `.`                               |

## Outputs

| Output                   | Description                                                    |
| ------------------------ | -------------------------------------------------------------- |
| `has_changes`            | `true` if changes were made                                    |
| `files_changed`          | Space-separated list of changed files                          |
| `branch_name`            | Branch name used                                               |
| `execution_file`         | Path to execution details file                                 |
| `resolved_template_vars` | JSON object of all resolved template variables (for debugging) |

## Template System

### Variable Resolution Priority

1. **User-provided `template_vars`** (highest priority)
2. **Template frontmatter defaults**
3. **Auto-detected from GitHub context** (lowest priority)

### Frontmatter Format

Templates use YAML frontmatter to define default variable values:

```markdown
---
# Default variables
project_name: ${repository_name}
package_path: .
custom_variable: some_value
---

# Your prompt template starts here

Use ${project_name} and ${custom_variable} in your template.
```

### Variable Interpolation

Use `${var_name}` syntax in templates:

```markdown
Project: ${project_name}
Package: ${package_name}
```

Supports expressions:

```markdown
${condition ? 'true_value' : 'false_value'}
${var1 + '-' + var2}
```

### Auto-Detected Variables

Always available:

```yaml
repository: 'owner/repo'
repository_owner: 'owner'
repository_name: 'repo'
run_id: '123456'
sha: 'abc123...'
actor: 'username'

# Event-specific (when available)
github_event_issue_number: '123'
github_event_issue_title: 'Issue title'
github_event_pull_request_number: '456'
# ... and many more
```

## Built-in Templates

### `readme`

Validates README accuracy against codebase.

**Default Variables:**

```yaml
project_name: ${repository_name}
package_path: .
readme_path: README.md
is_root_readme: ${readme_path == 'README.md'}
branch_name: ${is_root_readme ? 'docs/root-readme-updates-' + run_id : 'docs/readme-updates-' + run_id}
```

**Usage:**

```yaml
# Root README
- uses: MoosicBox/MoosicBox/.github/actions/claude-checker@v1
  with:
      github_token: ${{ secrets.GITHUB_TOKEN }}
      claude_token: ${{ secrets.CLAUDE_CODE_OAUTH_TOKEN }}
      prompt_template: 'readme'

# Package README
- uses: MoosicBox/MoosicBox/.github/actions/claude-checker@v1
  with:
      github_token: ${{ secrets.GITHUB_TOKEN }}
      claude_token: ${{ secrets.CLAUDE_CODE_OAUTH_TOKEN }}
      prompt_template: 'readme'
      template_vars: |
          package_path: packages/my-package
```

### `rustdoc` (TODO)

Ensures complete rustdoc for public APIs.

### `examples` (TODO)

Validates and creates examples.

### `issue` (TODO)

Handles @claude mentions in GitHub issues.

### `pr` (TODO)

Handles @claude mentions in PR comments.

### `code-review` (TODO)

Automated code review on PR open/sync.

## Custom Templates

### From File

```yaml
- uses: MoosicBox/MoosicBox/.github/actions/claude-checker@v1
  with:
      github_token: ${{ secrets.GITHUB_TOKEN }}
      claude_token: ${{ secrets.CLAUDE_CODE_OAUTH_TOKEN }}
      prompt_template_file: '.github/prompts/security-audit.md'
      template_vars: |
          severity: high
          scope: auth/
```

### Inline

```yaml
- uses: MoosicBox/MoosicBox/.github/actions/claude-checker@v1
  with:
      github_token: ${{ secrets.GITHUB_TOKEN }}
      claude_token: ${{ secrets.CLAUDE_CODE_OAUTH_TOKEN }}
      prompt_template_text: |
          ---
          target: ${package_path}/README.md
          ---

          Check ${target} for accuracy.
      template_vars: |
          package_path: packages/core
```

## Verification System

### Built-in Profiles

- `auto`: Auto-detect from repo files
- `rust`: cargo fmt, clippy, machete, prettier, taplo
- `typescript`: prettier, eslint, tsc
- `python`: black, ruff, mypy
- `go`: gofmt, golangci-lint
- `custom`: Read from config file

### Verification Config File

Create `.github/claude-verification.yml`:

```yaml
# Setup steps (run once)
setup:
    - name: Install cargo-machete
      run: cargo install cargo-machete || true

# Verification steps (run before each commit)
verification:
    format:
        - name: Format Rust
          run: cargo fmt
          when: rust_files_changed

        - name: Format Markdown
          run: npx prettier --write "**/*.md"
          when: markdown_files_changed

    lint:
        - name: Clippy
          run: cargo clippy --all-targets -- -D warnings
          when: rust_files_changed
          required: true
# Conditions: always, never, *_files_changed, package_changed, files_match:"pattern"
```

## Examples

### README Checker in Matrix

```yaml
jobs:
    check-readmes:
        strategy:
            matrix:
                package: [audio, player, server]
        steps:
            - uses: actions/checkout@v4

            - uses: MoosicBox/MoosicBox/.github/actions/claude-checker@v1
              with:
                  github_token: ${{ secrets.GITHUB_TOKEN }}
                  claude_token: ${{ secrets.CLAUDE_CODE_OAUTH_TOKEN }}
                  prompt_template: 'readme'
                  template_vars: |
                      package_path: packages/${{ matrix.package }}
                  branch_name: readme-updates-${{ github.run_id }}
```

### TypeScript Project

```yaml
- uses: MoosicBox/MoosicBox/.github/actions/claude-checker@v1
  with:
      github_token: ${{ secrets.GITHUB_TOKEN }}
      claude_token: ${{ secrets.CLAUDE_CODE_OAUTH_TOKEN }}
      prompt_template: 'readme'
      verification_profile: 'typescript'
      template_vars: |
          package_path: packages/auth
```

### With Custom Guidelines

```yaml
- uses: MoosicBox/MoosicBox/.github/actions/claude-checker@v1
  with:
      github_token: ${{ secrets.GITHUB_TOKEN }}
      claude_token: ${{ secrets.CLAUDE_CODE_OAUTH_TOKEN }}
      prompt_template: 'readme'
      template_vars: |
          package_path: packages/audio
          custom_guidelines: |
            ## Audio Package Specifics
            - Document all supported codecs
            - Include performance benchmarks
```

## Development

### Testing Changes

```bash
cd .github/actions/claude-checker
npm install
npm test  # TODO: Add tests
```

### Adding New Built-in Templates

1. Create template file in `templates/` with frontmatter
2. Document in this README
3. Add tests

## License

Same as parent repository.
