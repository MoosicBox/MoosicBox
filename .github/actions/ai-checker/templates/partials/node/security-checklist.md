---
# Partial: Node/TypeScript Security Audit Checklist
# Expected variables (with defaults)
severity_threshold: 'medium'
---

## Security Audit Checklist

Scan for vulnerabilities at or above **${severity_threshold}** severity level.

### Severity Levels

- **CRITICAL**: Immediate exploitation risk, data breach, RCE
- **HIGH**: Significant security impact, requires prompt attention
- **MEDIUM**: Moderate risk, should be addressed in normal development
- **LOW**: Minor issues, defense-in-depth improvements

---

### 1. Dependency Vulnerabilities (HIGH-CRITICAL)

**Run security audit:**

```bash
${pm_audit()}
```

Review and address all vulnerabilities at or above ${severity_threshold} severity.

For detailed JSON output:

```bash
${package_manager == 'npm' ? 'npm audit --json' : (package_manager == 'pnpm' ? 'pnpm audit --json' : (package_manager == 'yarn' ? 'yarn audit --json' : 'bun pm audit'))}
```

---

### 2. Prototype Pollution (HIGH-CRITICAL)

**Vulnerable patterns:**

```typescript
// VULNERABLE: Direct property assignment from user input
function merge(target: any, source: any) {
    for (const key in source) {
        target[key] = source[key]; // Can pollute Object.prototype
    }
}

// VULNERABLE: Recursive merge without prototype check
obj[userKey] = userValue; // If userKey is "__proto__"

// VULNERABLE: Using bracket notation with user input
const value = obj[userInput]; // Can access __proto__

// SECURE: Check for prototype properties
function safeMerge(target: Record<string, unknown>, source: Record<string, unknown>) {
    for (const key in source) {
        if (key === '__proto__' || key === 'constructor' || key === 'prototype') {
            continue;
        }
        if (Object.hasOwn(source, key)) {
            target[key] = source[key];
        }
    }
}

// SECURE: Use Object.create(null) for dictionaries
const safeDict = Object.create(null);
```

---

### 3. Path Traversal (HIGH)

**Vulnerable patterns:**

```typescript
// VULNERABLE: User input directly in path
import * as path from 'path';
import * as fs from 'fs';

const filePath = path.join(uploadDir, userFilename);
fs.readFileSync(filePath); // User can use "../" to escape

// VULNERABLE: URL-based path traversal
const url = new URL(userInput, 'file://');
fs.readFileSync(url.pathname);

// SECURE: Validate resolved path stays within allowed directory
function safeReadFile(uploadDir: string, userFilename: string): Buffer {
    const resolvedBase = path.resolve(uploadDir);
    const resolvedPath = path.resolve(uploadDir, userFilename);

    if (!resolvedPath.startsWith(resolvedBase + path.sep)) {
        throw new Error('Path traversal detected');
    }

    return fs.readFileSync(resolvedPath);
}
```

---

### 4. ReDoS - Regular Expression Denial of Service (MEDIUM-HIGH)

**Vulnerable patterns:**

```typescript
// VULNERABLE: Catastrophic backtracking with nested quantifiers
const badRegex = /^(a+)+$/;
const evilInput = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa!';
badRegex.test(evilInput); // Hangs

// VULNERABLE: Other dangerous patterns
/^([a-zA-Z0-9]+)*$/    // Nested quantifiers
/^(a|a)+$/             // Alternation with overlap
/(.*a){x}/             // Quantified groups with wildcards

// SECURE: Limit input length before regex matching
function safeMatch(input: string, pattern: RegExp): boolean {
    if (input.length > 1000) {
        throw new Error('Input too long');
    }
    return pattern.test(input);
}

// SECURE: Use atomic groups or possessive quantifiers (if supported)
// SECURE: Use linear-time regex engines like RE2
```

---

### 5. Injection Attacks (HIGH-CRITICAL)

**SQL Injection:**

```typescript
// VULNERABLE: String interpolation in queries
const query = `SELECT * FROM users WHERE id = ${userId}`;

// SECURE: Use parameterized queries
const query = 'SELECT * FROM users WHERE id = ?';
db.query(query, [userId]);

// SECURE: Use ORMs with proper escaping
const user = await prisma.user.findUnique({ where: { id: userId } });
```

**Command Injection:**

```typescript
import { exec, execFile } from 'child_process';

// VULNERABLE: String concatenation with exec
exec(`echo ${userInput}`); // Shell injection possible

// SECURE: Use execFile with arguments array
execFile('echo', [userInput]); // Arguments are escaped

// SECURE: Use spawn with shell: false (default)
import { spawn } from 'child_process';
spawn('echo', [userInput]); // Safe
```

**NoSQL Injection:**

```typescript
// VULNERABLE: Direct object from user input
const query = { username: req.body.username };
// If req.body.username = { "$gt": "" }, returns all users

// SECURE: Validate input types
if (typeof req.body.username !== 'string') {
    throw new Error('Invalid username');
}
```

---

### 6. Sensitive Data Exposure (MEDIUM-HIGH)

**Check for:**

- Hardcoded secrets, API keys, passwords in source code
- Secrets in error messages or logs
- Sensitive data in client-side bundles
- `.env` files committed to repository
- Secrets in stack traces sent to clients

**Patterns to search for:**

```bash
# Search for potential hardcoded secrets
rg -i "(api_key|apikey|secret|password|token|auth|credential)" --type ts --type js -g '!*.test.*' -g '!*.spec.*'

# Check for .env files in git
git ls-files | grep -i '\.env'
```

**Secure patterns:**

```typescript
// VULNERABLE: Hardcoded secret
const API_KEY = 'sk_live_abc123';

// SECURE: Environment variable
const API_KEY = process.env.API_KEY;
if (!API_KEY) {
    throw new Error('API_KEY environment variable is required');
}

// VULNERABLE: Logging sensitive data
console.log('Login attempt:', { username, password });

// SECURE: Redact sensitive fields
console.log('Login attempt:', { username, password: '[REDACTED]' });
```

---

### 7. Insecure Deserialization (MEDIUM-HIGH)

```typescript
// VULNERABLE: eval-based parsing
const data = eval('(' + userInput + ')');

// VULNERABLE: Function constructor
const fn = new Function(userInput);

// VULNERABLE: Deserializing untrusted YAML (with !!js/function)
import yaml from 'js-yaml';
yaml.load(userInput); // Can execute arbitrary code

// SECURE: Use JSON.parse with validation
const parsed = JSON.parse(userInput);
if (!isValidSchema(parsed)) {
    throw new Error('Invalid data structure');
}

// SECURE: Use safe YAML loading
yaml.load(userInput, { schema: yaml.JSON_SCHEMA });
```

---

### 8. Cross-Site Scripting (XSS) - For Web Applications (HIGH)

```typescript
// VULNERABLE: Direct HTML insertion
element.innerHTML = userInput;

// VULNERABLE: Template literal in HTML
const html = `<div>${userName}</div>`;

// SECURE: Use textContent for text
element.textContent = userInput;

// SECURE: Use framework escaping (React, Vue, etc.)
// React automatically escapes: <div>{userName}</div>

// SECURE: Sanitize HTML if needed
import DOMPurify from 'dompurify';
element.innerHTML = DOMPurify.sanitize(userInput);
```

---

### 9. Missing Security Headers - For Web Applications (MEDIUM)

For HTTP servers, verify these headers are set:

- `Content-Security-Policy` - Prevent XSS and data injection
- `X-Content-Type-Options: nosniff` - Prevent MIME sniffing
- `X-Frame-Options: DENY` or `SAMEORIGIN` - Prevent clickjacking
- `Strict-Transport-Security` - Enforce HTTPS
- `X-XSS-Protection: 0` - Disable legacy XSS filter (rely on CSP instead)

---

## Reporting Format

For each finding, document:

1. **Location**: File path and line number(s)
2. **Severity**: CRITICAL / HIGH / MEDIUM / LOW
3. **Category**: Which checklist category
4. **Description**: What the vulnerability is
5. **Fix**: How to remediate (with code example if applicable)

Example:

```
### Finding: Prototype Pollution in merge utility

- **Location**: `src/utils/merge.ts:15-22`
- **Severity**: HIGH
- **Category**: Prototype Pollution
- **Description**: The merge function iterates over all keys including __proto__
- **Fix**: Add prototype property check before assignment
```

---

## What NOT to Flag

1. **Test code** - Security issues in test files (unless they test security features)
2. **Development-only code** - Code that only runs in development mode
3. **Already-mitigated** - Issues with existing mitigations in place
4. **False positives** - Patterns that look vulnerable but are actually safe due to context
5. **Intentional behavior** - Documented security decisions (e.g., public endpoints)
