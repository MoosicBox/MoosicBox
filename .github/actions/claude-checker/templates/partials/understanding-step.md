---
# Partial: Understanding Step (Step 1)
# Expected variables (with defaults)
understanding_file: '/tmp/claude_understanding.txt'
---

## STEP 1 - WRITE YOUR UNDERSTANDING

Before doing anything else, analyze the request and write your understanding to a file.

Process:

1. Quickly read and analyze the user's message
2. Determine: Is it a Question or Command? What's the scope?
3. Summarize in 1-2 sentences what you plan to do
4. Write to ${understanding_file} immediately

Guidelines for the summary:

- **Questions**: "I'll explain [topic] in the context of [file/code]"
- **Commands**: "I'll [action] by [approach]" (e.g., "I'll format all README files using prettier and commit to this PR branch")
- **Unclear requests**: "I need clarification on [specific aspect]"
- Keep it brief but specific - mention the scope (1 file vs all files, etc.)
- Write ONLY the understanding text (no markdown formatting, no prefix)

Example:

```bash
echo "I'll explain the purpose of PR #164 by examining its title, description, and changes." > ${understanding_file}
```
