---
# Partial: Node/TypeScript Verification Checklist
# Expected variables (with defaults)
package_name: ''
run_tests: true
run_typecheck: true
run_doc_check: false
---

## Verification (MANDATORY)

Before creating ANY commit, you MUST run:

1. **Format**: ${has_format_script ? pm_run(format_script_name) : pm_exec('prettier --write .')}
2. **Lint**: ${has_lint_script ? pm_run('lint') + ' -- --fix' : (has_eslint ? pm_exec('eslint . --fix') : 'No ESLint configured - skip')}
${language == 'typescript' && run_typecheck ? '3. **Type Check**: ' + (has_typecheck_script ? pm_run(typecheck_script_name) : pm_exec('tsc --noEmit')) : ''}
   ${run_tests ? (language == 'typescript' && run_typecheck ? '4' : '3') + '. **Tests**: ' + pm_run('test') : ''}
${run_doc_check && has_typedoc ? (run_tests ? (language == 'typescript' && run_typecheck ? '5' : '4') : (language == 'typescript' && run_typecheck ? '4' : '3')) + '. **Docs**: ' + pm_exec('typedoc --emit none') : ''}

If ANY check fails, fix the issues before committing.
NEVER commit code that doesn't pass all checks.
