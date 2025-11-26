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

## Dependency Audit

Before scanning source code, run:

```bash
# Install cargo-audit if not present
cargo install cargo-audit

# Run audit
cargo audit
```

Document any findings from `cargo audit` in your report.

## Security Audit Checklist

Scan for vulnerabilities at or above **${severity_threshold}** severity level.

### Severity Levels

- **CRITICAL**: Immediate exploitation risk, data breach, RCE
- **HIGH**: Significant security impact, requires prompt attention
- **MEDIUM**: Moderate risk, should be addressed in normal development
- **LOW**: Minor issues, defense-in-depth improvements

---

### 1. Input Validation & Injection (HIGH-CRITICAL)

**Path Traversal**

- Look for file operations accepting user-controlled paths
- Check for `..` sequences not being sanitized
- Verify paths are canonicalized and confined to expected directories

```rust
// VULNERABLE: User can access arbitrary files
let path = format!("/data/{}", user_input);
std::fs::read_to_string(path)?;

// SECURE: Validate and canonicalize
let base = Path::new("/data").canonicalize()?;
let requested = base.join(user_input).canonicalize()?;
if !requested.starts_with(&base) {
    return Err(Error::PathTraversal);
}
```

**Command Injection**

- Check `std::process::Command` usage with user input
- Look for shell=true or command string interpolation
- Verify arguments are passed as separate args, not concatenated

```rust
// VULNERABLE: Command injection possible
Command::new("sh").arg("-c").arg(format!("echo {}", user_input));

// SECURE: Pass as separate argument
Command::new("echo").arg(user_input);
```

**SQL Injection**

- Check for raw SQL with string interpolation
- Verify parameterized queries are used
- Look for dynamic table/column names from user input

**Format String Issues**

- Check for user input in format strings
- Verify `format!`, `println!`, `log::*` macros don't include raw user data in format position

---

### 2. Integer & Memory Safety (MEDIUM-HIGH)

**Integer Overflow/Underflow**

- Look for arithmetic on untrusted integers without checked operations
- Check array/buffer size calculations
- Verify length/size values from untrusted sources use `checked_*` or `saturating_*`

```rust
// VULNERABLE: Can overflow
let size = width * height;
let mut buffer = Vec::with_capacity(size);

// SECURE: Use checked arithmetic
let size = width.checked_mul(height).ok_or(Error::Overflow)?;
```

**Unbounded Allocations**

- Check `Vec::with_capacity()`, `String::with_capacity()` with untrusted sizes
- Look for loops that grow collections based on untrusted input
- Verify size limits on user-provided data

**Panic-Inducing Operations on Untrusted Input**

- Check for `unwrap()`, `expect()` on user-controlled Results/Options
- Look for array indexing without bounds checks on untrusted indices
- Verify slice operations validate bounds

```rust
// VULNERABLE: Panic on invalid input
let value = user_data.parse::<i32>().unwrap();
let item = items[user_index];

// SECURE: Handle errors gracefully
let value = user_data.parse::<i32>().map_err(|_| Error::InvalidInput)?;
let item = items.get(user_index).ok_or(Error::IndexOutOfBounds)?;
```

---

### 3. Cryptography (HIGH-CRITICAL)

**Weak/Insecure Random**

- Check if `rand` crate is used for security-sensitive operations
- Verify `getrandom` or `rand::rngs::OsRng` for cryptographic use
- Look for seeded RNGs with predictable seeds

**Hardcoded Secrets**

- Search for hardcoded API keys, passwords, tokens
- Check for secrets in configuration files committed to repo
- Look for patterns: `password = "..."`, `api_key = "..."`, `secret = "..."`
- Verify secrets come from environment variables or secure vaults

**Weak Hashing**

- Check for MD5, SHA1 used for security purposes (passwords, signatures)
- Verify password hashing uses bcrypt, argon2, or scrypt
- Look for custom crypto implementations

**Missing/Weak Encryption**

- Check sensitive data at rest and in transit
- Verify TLS configuration is secure
- Look for deprecated cipher suites

---

### 4. Error Handling & Information Disclosure (MEDIUM-HIGH)

**Sensitive Data in Errors**

- Check error messages for passwords, tokens, internal paths
- Verify stack traces aren't exposed to end users
- Look for debug info in production error responses

**Logging Sensitive Data**

- Check `log::*`, `tracing::*` calls for sensitive data
- Verify credentials aren't logged
- Look for request/response logging that might include secrets

```rust
// VULNERABLE: Logs password
log::debug!("Login attempt for {} with password {}", user, password);

// SECURE: Redact sensitive data
log::debug!("Login attempt for {}", user);
```

**Silent Error Swallowing**

- Check for `let _ = ...` on security-relevant operations
- Verify errors in auth/authz paths are properly handled
- Look for empty catch blocks or ignored Results

---

### 5. Unsafe Code (MEDIUM-HIGH)

**Unnecessary Unsafe Blocks**

- Check if `unsafe` can be replaced with safe alternatives
- Verify unsafe is minimal and well-documented
- Look for unsafe used for convenience rather than necessity

**Missing Safety Documentation**

- Verify all `unsafe fn` have `# Safety` documentation
- Check that safety invariants are clearly stated
- Look for unsafe blocks without comments explaining why they're safe

**Undefined Behavior Patterns**

- Check for null pointer dereferences
- Look for uninitialized memory reads
- Verify pointer arithmetic stays in bounds
- Check for aliasing violations (`&mut` and `&` to same data)

---

### 6. Authentication & Authorization (HIGH-CRITICAL)

**Missing Auth Checks**

- Verify all sensitive endpoints require authentication
- Check for authorization bypass paths
- Look for admin functionality accessible without proper checks

**Credential Handling**

- Check password comparison uses constant-time comparison
- Verify credentials are cleared from memory after use
- Look for credentials stored in plain text

**Session Management**

- Check session token generation uses secure random
- Verify session expiration is enforced
- Look for session fixation vulnerabilities

---

### 7. Concurrency (MEDIUM-HIGH)

**Data Races in Unsafe Code**

- Check `unsafe` blocks for proper synchronization
- Verify `Sync`/`Send` bounds are correct
- Look for raw pointer sharing across threads

**TOCTOU (Time-of-Check to Time-of-Use)**

- Check for file existence checks followed by file operations
- Look for permission checks before sensitive operations
- Verify atomic operations where needed

```rust
// VULNERABLE: TOCTOU race
if path.exists() {
    std::fs::remove_file(path)?; // File might have changed
}

// SECURE: Handle error directly
match std::fs::remove_file(path) {
    Ok(_) => Ok(()),
    Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
    Err(e) => Err(e),
}
```

---

### 8. Dependencies (MEDIUM-CRITICAL)

**Known Vulnerabilities**

- Run `cargo audit` to check for known CVEs
- Review advisory database matches
- Prioritize by severity and exploitability

**Unmaintained Dependencies**

- Check for crates with no recent updates
- Look for deprecated crates
- Consider alternatives for abandoned dependencies

---

## Scope Exclusions

Skip the following when scanning:

- `packages/clippier/` - Build tooling
- `packages/bloaty/` - Build tooling
- `packages/gpipe/` - Build tooling
- Test code (`#[cfg(test)]` modules) - unless testing security features
- Example code (`examples/` directories) - document but don't fix

## What NOT to Flag

1. **Test code** - Security issues in `#[cfg(test)]` modules (unless they test security features)
2. **Example code** - Code in `examples/` directory (document but don't fix)
3. **Build scripts** - `build.rs` files (lower priority unless critical)
4. **False positives** - Patterns that look vulnerable but are actually safe due to context
5. **Already-mitigated** - Issues with existing mitigations in place

## Verification (MANDATORY)

Before creating ANY commit, you MUST run:

1. Run `cargo fmt`
2. Run `cargo test` to verify all tests pass
3. Run `cargo clippy --all-targets -- -D warnings`
4. Run `~/.cargo/bin/cargo-machete --with-metadata` from workspace root
5. Run `npx prettier --write "**/*.{md,yaml,yml}"` from workspace root
6. Run `~/.cargo/bin/taplo format` from workspace root

If ANY check fails, fix the issues before committing.
NEVER commit code that doesn't pass all checks.

## Commit Message Instructions

If you make security fixes, you MUST provide a commit message description.

At the END of your response, include a section formatted EXACTLY as follows:

```
COMMIT_MESSAGE_START
- Brief description of security fixes (1-2 sentences per major item)
- Focus on what vulnerability was fixed and how
COMMIT_MESSAGE_END
```

Example:

```
COMMIT_MESSAGE_START
- Fixed path traversal vulnerability in file handler by validating and canonicalizing paths
- Replaced unwrap() with proper error handling for user input parsing in config module
- Removed hardcoded API key from constants and moved to environment variable
- Updated vulnerable dependency xyz from 1.2.3 to 1.2.5 (fixes CVE-2024-XXXXX)
COMMIT_MESSAGE_END
```

Requirements:

- Keep each bullet point concise (1-2 sentences max)
- Focus on WHAT was fixed and WHY (the security impact)
- Use bullet points with dashes (-)
- Do not include code snippets or line numbers
- If no issues found, output "No security issues found at or above severity threshold"
- DO NOT push

## Response Guidelines

When responding to users:

- NEVER reference files in /tmp or other temporary directories - users cannot access these
- Always include plans, summaries, and important information directly in your comment response
- If you create a plan or analysis, paste the full content in your response, not just a file path
- Remember: you run on an ephemeral server - any files you create are only accessible during your execution

## Output Format

Structure your response as:

1. **Executive Summary**: Brief overview of findings
2. **Dependency Audit Results**: Output from `cargo audit`
3. **Source Code Findings**: Detailed findings by severity
4. **Fixes Applied**: Summary of remediation changes made
5. **Recommendations**: Issues requiring manual review or architectural changes

### Finding Report Format

For each finding, document:

1. **Location**: File path and line number(s)
2. **Severity**: CRITICAL / HIGH / MEDIUM / LOW
3. **Category**: Which checklist category
4. **Description**: What the vulnerability is
5. **Fix**: How to remediate (with code example if applicable)

Example:

```
### Finding: Path Traversal in file_handler.rs

- **Location**: `packages/server/src/file_handler.rs:45-52`
- **Severity**: HIGH
- **Category**: Input Validation
- **Description**: User-provided filename is directly used in file path without sanitization
- **Fix**: Canonicalize path and verify it's within allowed directory
```

${custom_guidelines}
