---
# Partial: Commit Message Instructions
# Expected variables (with defaults)
commit_type: 'changes'
example_bullets: '- Brief description of what changed and why (1-2 sentences)\n- Additional changes if applicable'
no_changes_message: 'No changes required'
---

## Commit Message Instructions

If you make ${commit_type}, you MUST provide a commit message description.

At the END of your response, include a section formatted EXACTLY as follows:

```
COMMIT_MESSAGE_START
${example_bullets}
COMMIT_MESSAGE_END
```

Requirements:

- Keep each bullet point concise (1-2 sentences max)
- Focus on WHAT was changed and WHY
- Use bullet points with dashes (-)
- Do not include code snippets or line numbers
- If no changes needed, output "${no_changes_message}"
- DO NOT push
