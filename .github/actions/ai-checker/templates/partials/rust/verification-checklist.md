---
# Partial: Rust Verification Checklist
# Expected variables (with defaults)
package_name: ''
run_tests: true
run_doc_check: false
---

## Verification (MANDATORY)

Before creating ANY commit, you MUST run:

1. Run `cargo fmt`
2. Run `cargo clippy --all-targets -- -D warnings`
3. Run `~/.cargo/bin/cargo-machete --with-metadata` from workspace root
4. Run `npx prettier --write "**/*.{md,yaml,yml}"` from workspace root
5. Run `~/.cargo/bin/taplo format` from workspace root
   ${run_tests && package_name ? '6. Run `cargo test -p ' + package_name + '` to verify tests pass' : ''}
${run_doc_check && package_name ? (run_tests && package_name ? '7' : '6') + '. Run `cargo doc -p ' + package_name + ' --no-deps` to verify docs build' : ''}

If ANY check fails, fix the issues before committing.
NEVER commit code that doesn't pass all checks.
