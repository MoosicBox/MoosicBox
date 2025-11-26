---
# Template: Security Audit
# Scans workspace for security vulnerabilities and fixes issues

project_name: '${repository_name}'
repository: '${repository}'
branch_name: 'security/audit-${run_id}'
severity_threshold: 'medium'
custom_guidelines: ''
commit_message: 'security: fix identified vulnerabilities'
---

You are performing a security audit of ${project_name}.

IMPORTANT: Follow the repository's AGENTS.md for guidance on code standards.

Context:

- REPO: ${repository}
- BRANCH: ${branch_name}
- SCOPE: Entire workspace (all packages under `packages/`)
- SEVERITY THRESHOLD: ${severity_threshold} (report and fix issues at or above this level)

## Task

Scan the codebase for security vulnerabilities. Report all findings at or above **${severity_threshold}** severity, and fix issues where a clear remediation exists.

## Audit Process

1. **Run `cargo audit`** first to check for known dependency vulnerabilities
2. **Scan source code** in `packages/` for vulnerability patterns
3. **Document findings** with severity, location, and description
4. **Fix issues** where remediation is straightforward
5. **Flag for review** issues that require architectural changes

${include('rust/security-checklist', { severity_threshold: severity_threshold })}

## Dependency Audit

Before scanning source code, run:

```bash
# Install cargo-audit if not present
cargo install cargo-audit

# Run audit
cargo audit
```

Document any findings from `cargo audit` in your report.

## Scope Exclusions

Skip the following when scanning:

- `packages/clippier/` - Build tooling
- `packages/bloaty/` - Build tooling
- `packages/gpipe/` - Build tooling
- Test code (`#[cfg(test)]` modules) - unless testing security features
- Example code (`examples/` directories) - document but don't fix

${include('rust/verification-checklist', { package_name: '', run_tests: true })}

${include('commit-message-instructions', { commit_type: 'security fixes', example_bullets: '- Fixed path traversal vulnerability in file handler by validating and canonicalizing paths\\n- Replaced unwrap() with proper error handling for user input parsing in config module\\n- Removed hardcoded API key from constants and moved to environment variable\\n- Updated vulnerable dependency xyz from 1.2.3 to 1.2.5 (fixes CVE-2024-XXXXX)', no_changes_message: 'No security issues found at or above severity threshold' })}

${include('response-guidelines')}

## Output Format

Structure your response as:

1. **Executive Summary**: Brief overview of findings
2. **Dependency Audit Results**: Output from `cargo audit`
3. **Source Code Findings**: Detailed findings by severity
4. **Fixes Applied**: Summary of remediation changes made
5. **Recommendations**: Issues requiring manual review or architectural changes

${custom_guidelines}
