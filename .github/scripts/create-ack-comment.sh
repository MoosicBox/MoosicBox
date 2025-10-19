#!/bin/bash
set -euo pipefail

COMMENT_TYPE="$1"
REPO="$2"
RUN_ID="$3"

case "$COMMENT_TYPE" in
    issue)
        ISSUE_NUMBER="$4"
        API_ENDPOINT="/repos/${REPO}/issues/${ISSUE_NUMBER}/comments"
        ;;
    pr_issue_comment)
        ISSUE_NUMBER="$4"
        API_ENDPOINT="/repos/${REPO}/issues/${ISSUE_NUMBER}/comments"
        ;;
    pr_review_comment)
        PR_NUMBER="$4"
        ROOT_COMMENT_ID="$5"
        API_ENDPOINT="/repos/${REPO}/pulls/${PR_NUMBER}/comments/${ROOT_COMMENT_ID}/replies"
        ;;
    *)
        echo "Error: Unknown comment type '$COMMENT_TYPE'" >&2
        exit 1
        ;;
esac

WORKFLOW_URL="https://github.com/${REPO}/actions/runs/${RUN_ID}"

cat > /tmp/ack_comment_body.md << EOF
👀 Looking into this...

[View workflow run](${WORKFLOW_URL})
EOF

if [ "$COMMENT_TYPE" = "pr_review_comment" ]; then
    COMMENT_ID=$(gh api -X POST "$API_ENDPOINT" -F body=@/tmp/ack_comment_body.md --jq '.id' 2>&1)
else
    COMMENT_ID=$(gh api -X POST "$API_ENDPOINT" -F body=@/tmp/ack_comment_body.md --jq '.id' 2>&1)
fi

if [ $? -eq 0 ] && [ -n "$COMMENT_ID" ]; then
    echo "$COMMENT_ID"
    exit 0
else
    echo "Error: Failed to create acknowledgment comment" >&2
    echo "Response: $COMMENT_ID" >&2
    exit 1
fi
