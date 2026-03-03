---
# Partial: Git History Regression Check
# Expected variables (with defaults)
file_path: 'README.md'
repository: ''
---

## MANDATORY Git History Regression Check

Before making ANY change to ${file_path}, you MUST check git history for the SPECIFIC LINES you want to modify.

**Required Commands:**
For each line or section you want to change, run:

```bash
# Replace LINE_NUMBER with the actual line number you want to modify
# Add +/- 5 lines of context to catch related changes
git log -L LINE_NUMBER,+1:${file_path} --format="%H %s%n%b"

# For a section spanning multiple lines (e.g., lines 240-250):
git log -L 240,250:${file_path} --format="%H %s%n%b"
```

**What to Look For:**

- **Revert commits**: Messages containing "revert", "revert back to", "prefer X format", "changed back to"
- **PR discussion references**: Look for "Triggered-by:" URLs in commit messages
- **Explicit reasoning**: Commit messages explaining "prefer X because Y" or "more concise", "better for readability"
- **Multiple attempts**: If the same change appears 2+ times in history, this is a STRONG SIGNAL

**Blocking Rules (NON-NEGOTIABLE):**

- If you find a revert commit for the exact change you want to make: STOP. DO NOT make that change.
- If the same change was reverted 2+ times: HARD STOP. The maintainers have decided against it.
- If a commit contains "Triggered-by:" with a GitHub PR/discussion URL: You MUST read that discussion to understand the reasoning before proceeding.
- If unclear after reading history: Leave it alone. Document the uncertainty in your output for human review.

**Decision Process:**

1. Run `git log -L` for the specific line(s) you want to change
2. Read ALL commit messages in the history (no arbitrary limits - check FULL history)
3. If you see a revert or "Triggered-by:" reference:
   a. Extract the discussion URL if present
   b. Understand why the change was reverted
   c. If your proposed change matches what was reverted: BLOCK IT
4. Document your findings in a `REGRESSION_CHECK` block (format below)

**Required Output Format:**
For EVERY change you consider making, include this in your response:

```
REGRESSION_CHECK_START
File: ${file_path}
Line(s): [specific line numbers]
Proposed change: [brief description of what you wanted to do]
Git history checked: YES
Revert found: [YES/NO]
Revert details: [commit hash and reasoning if YES, otherwise "None found"]
Decision: [PROCEED/BLOCKED - explanation]
REGRESSION_CHECK_END
```

**Example of BLOCKED change:**

```
REGRESSION_CHECK_START
File: ${file_path}
Line(s): 244
Proposed change: Change --features "feature1,feature2" to --features feature1 --features feature2
Git history checked: YES
Revert found: YES
Revert details: Commit 27bb0ed83 reverted this exact change with reasoning "prefer comma-separated for conciseness" and references PR discussion at https://github.com/${repository}/pull/175#discussion_r2462901200
Decision: BLOCKED - This change was already discussed and reverted. Will not make it again.
REGRESSION_CHECK_END
```
