---
# Template: Automated Code Review
# Default variables

project_name: '${repository_name}'
repository: '${repository}'
pr_number: '${github_event_pull_request_number}'
review_focus: 'all'
custom_guidelines: ''
---

REPO: ${repository}
PR NUMBER: ${pr_number}

Please review this pull request and provide feedback on:

- Code quality and best practices
- Potential bugs or issues
- Performance considerations
- Security concerns
- Test coverage

Use the repository's AGENTS.md for guidance on style and conventions. Be constructive and helpful in your feedback.

Use `gh pr comment` with your Bash tool to leave your review as a comment on the PR.

${custom_guidelines}
