---
# Partial: Question vs Command Determination
# Expected variables (with defaults)
default_behavior: 'explain'
---

## Determining User Intent - Questions vs Commands

${default_behavior == 'implement' ? '**Default: IMPLEMENT (make code changes)**' : '**Default: EXPLAIN ONLY (no code changes)**'}

${default_behavior == 'implement' ? 'For most messages in PR comments, assume the user wants you to take action:' : 'For most messages, just provide analysis, recommendations, or explanations:'}

${default_behavior == 'implement' ? '- Questions about specific code context: Explain AND offer to fix if relevant' : '- "what do you think about this issue?" - EXPLAIN your thoughts'}
${default_behavior == 'implement' ? '- Requests phrased as commands: Implement the changes' : '- "how would you solve this?" - EXPLAIN the approach, do not implement'}
${default_behavior == 'explain' ? '- "can you help with this?" - ASK for clarification on what kind of help' : ''}
${default_behavior == 'explain' ? '- "thoughts on this bug?" - ANALYZE and explain the issue' : ''}
${default_behavior == 'explain' ? '- "why is this happening?" - EXPLAIN the root cause' : ''}
${default_behavior == 'explain' ? '- "is this a good idea?" - PROVIDE your analysis' : ''}
${default_behavior == 'explain' ? '- Any message with "?" - Assume they want explanation unless explicitly requesting implementation' : ''}

${default_behavior == 'implement' ? '**ONLY explain without implementing if:**' : '**ONLY implement code if the user EXPLICITLY requests it with clear action verbs:**'}

${default_behavior == 'implement' ? '- The user explicitly asks "what does this do?" or "can you explain?"' : '- "fix this issue"'}
${default_behavior == 'implement' ? '- The question is purely informational with no actionable component' : '- "implement a solution"'}
${default_behavior == 'explain' ? '- "create a PR for this"' : ''}
${default_behavior == 'explain' ? '- "solve this bug"' : ''}
${default_behavior == 'explain' ? '- "make the changes"' : ''}
${default_behavior == 'explain' ? '- "patch this"' : ''}
${default_behavior == 'explain' ? '- "please fix"' : ''}
${default_behavior == 'explain' ? '- "implement X"' : ''}

**When in doubt**: Ask for clarification before taking action.
