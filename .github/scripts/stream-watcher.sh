#!/usr/bin/env bash
set -euo pipefail

# Configuration
STREAM_FILE="$1"
REPO="$2"
export GH_TOKEN="$3"
COMMENT_ID="$4"
COMMENT_TYPE="${5:-auto}"  # Optional: issue, pr_issue_comment, pr_review_comment, or auto

UPDATE_INTERVAL=3  # seconds between comment updates
PROCESSED_LINES=0
PROGRESS_FILE="/tmp/progress_content.md"
UNDERSTANDING_FILE="/tmp/claude_understanding.txt"
UNDERSTANDING_INSERTED=false

log() {
    echo "[stream-watcher] $*" >&2
}

# Initialize progress section
init_progress_section() {
    cat > "$PROGRESS_FILE" << 'EOF'
<details open>
<summary>🔄 Live Progress</summary>

EOF
    log "Initialized progress section"
}

# Extract tool context for formatting
extract_tool_context() {
    local tool_name="$1"
    local tool_input="$2"

    case "$tool_name" in
        Read|read)
            filepath=$(echo "$tool_input" | jq -r '.file_path // .filePath // .file // .path // empty' 2>/dev/null || echo "")
            [ -n "$filepath" ] && echo " on \`$filepath\`" || echo ""
            ;;
        Write|write)
            filepath=$(echo "$tool_input" | jq -r '.file_path // .filePath // .file // empty' 2>/dev/null || echo "")
            [ -n "$filepath" ] && echo " to \`$filepath\`" || echo ""
            ;;
        Edit|edit)
            filepath=$(echo "$tool_input" | jq -r '.file_path // .filePath // .file // empty' 2>/dev/null || echo "")
            [ -n "$filepath" ] && echo " on \`$filepath\`" || echo ""
            ;;
        Bash|bash)
            command=$(echo "$tool_input" | jq -r '.command // empty' 2>/dev/null || echo "")
            if [ -n "$command" ]; then
                if echo "$command" | grep -q $'\n' || [ ${#command} -gt 200 ]; then
                    first_line=$(echo "$command" | head -n1)
                    echo ": \`${first_line:0:150}...\`"
                else
                    echo ": \`$command\`"
                fi
            else
                echo ""
            fi
            ;;
        Glob|glob|Grep|grep)
            pattern=$(echo "$tool_input" | jq -r '.pattern // empty' 2>/dev/null || echo "")
            [ -n "$pattern" ] && echo " for \`$pattern\`" || echo ""
            ;;
        List|list)
            path=$(echo "$tool_input" | jq -r '.path // empty' 2>/dev/null || echo "")
            [ -n "$path" ] && echo " in \`$path\`" || echo ""
            ;;
        *)
            echo ""
            ;;
    esac
}

# Process a single JSON line and append to progress file
process_json_line() {
    local line="$1"

    [ -z "$line" ] && return

    local turn_type=$(echo "$line" | jq -r '.type // "unknown"' 2>/dev/null || echo "unknown")

    if [ "$turn_type" != "assistant" ]; then
        return
    fi

    echo "$line" | jq -c '.message.content[]? // empty' 2>/dev/null | while IFS= read -r item; do
        local item_type=$(echo "$item" | jq -r '.type // "unknown"' 2>/dev/null || echo "unknown")

        if [ "$item_type" = "text" ]; then
            local text=$(echo "$item" | jq -r '.text // ""' 2>/dev/null || echo "")
            # Only show short thinking text
            if [ -n "$text" ] && [ ${#text} -lt 300 ]; then
                echo "" >> "$PROGRESS_FILE"
                echo "> ${text}" >> "$PROGRESS_FILE"
                echo "" >> "$PROGRESS_FILE"
            fi
        elif [ "$item_type" = "tool_use" ]; then
            local tool_name=$(echo "$item" | jq -r '.name // "unknown"' 2>/dev/null || echo "unknown")
            local tool_input=$(echo "$item" | jq -r '.input // {}' 2>/dev/null || echo "{}")

            local context=$(extract_tool_context "$tool_name" "$tool_input")

            echo "→ Used \`$tool_name\`$context" >> "$PROGRESS_FILE"
            echo "" >> "$PROGRESS_FILE"
        fi
    done
}

# Detect correct API endpoint for comment
detect_api_endpoint() {
    local comment_id="$1"
    local comment_type="$2"

    case "$comment_type" in
        pr_review_comment)
            echo "/repos/${REPO}/pulls/comments/${comment_id}"
            ;;
        issue|pr_issue_comment)
            echo "/repos/${REPO}/issues/comments/${comment_id}"
            ;;
        auto)
            # Try pulls endpoint first, fall back to issues
            if gh api "/repos/${REPO}/pulls/comments/${comment_id}" --jq '.id' >/dev/null 2>&1; then
                log "Auto-detected PR review comment"
                echo "/repos/${REPO}/pulls/comments/${comment_id}"
            else
                log "Auto-detected issue/PR issue comment"
                echo "/repos/${REPO}/issues/comments/${comment_id}"
            fi
            ;;
        *)
            log "⚠️ Unknown comment type '$comment_type', defaulting to issues endpoint"
            echo "/repos/${REPO}/issues/comments/${comment_id}"
            ;;
    esac
}

# Check for understanding file and insert if found
insert_understanding_if_available() {
    if [ "$UNDERSTANDING_INSERTED" = true ]; then
        return
    fi

    if [ ! -f "$UNDERSTANDING_FILE" ] || [ ! -s "$UNDERSTANDING_FILE" ]; then
        return
    fi

    log "📝 Found understanding file, inserting into comment..."

    local understanding_text=$(cat "$UNDERSTANDING_FILE")

    UNDERSTANDING_INSERTED=true
    log "✅ Understanding will be inserted in next comment update"
}

# Backfill existing events from stream file
backfill_existing_events() {
    if [ ! -f "$STREAM_FILE" ]; then
        log "No stream file yet, nothing to backfill"
        return
    fi

    local line_count=$(wc -l < "$STREAM_FILE" 2>/dev/null || echo "0")

    if [ "$line_count" -eq 0 ]; then
        log "Stream file empty, nothing to backfill"
        return
    fi

    log "Backfilling $line_count existing events..."

    while IFS= read -r line; do
        process_json_line "$line"
    done < "$STREAM_FILE"

    PROCESSED_LINES=$line_count
    log "✅ Backfilled $line_count lines"
}

# Process new events from stream file
process_new_events() {
    if [ ! -f "$STREAM_FILE" ]; then
        return
    fi

    local total_lines=$(wc -l < "$STREAM_FILE" 2>/dev/null || echo "0")

    if [ "$total_lines" -le "$PROCESSED_LINES" ]; then
        return
    fi

    local new_lines=$((total_lines - PROCESSED_LINES))

    tail -n "$new_lines" "$STREAM_FILE" | while IFS= read -r line; do
        process_json_line "$line"
    done

    PROCESSED_LINES=$total_lines
}

# Update GitHub comment with current progress
update_comment() {
    local comment_id="$1"
    local api_endpoint=$(detect_api_endpoint "$comment_id" "$COMMENT_TYPE")

    log "Using API endpoint: $api_endpoint"

    local original_body=$(gh api "$api_endpoint" --jq '.body' 2>/dev/null || echo "")

    if [ -z "$original_body" ]; then
        log "⚠️ Failed to fetch original comment body from $api_endpoint"
        return 1
    fi

    log "Original body length: ${#original_body} chars"

    # Remove existing progress section (everything from first --- onwards)
    # Use simpler pattern: any line with only dashes (and optional whitespace)
    local base_body=$(echo "$original_body" | awk '/^[[:space:]]*-+[[:space:]]*$/ {exit} {print}')

    log "Base body length after stripping: ${#base_body} chars"
    # Strip trailing blank lines
    base_body=$(echo "$base_body" | awk '{lines[++n]=$0} END {for(i=1;i<=n;i++){if(i==n){gsub(/^[[:space:]]*$/,"",lines[i])} if(lines[i]!="")print lines[i]; else if(i<n)print ""}}')

    # Insert understanding if available and not already in comment
    if [ "$UNDERSTANDING_INSERTED" = true ] && [ -f "$UNDERSTANDING_FILE" ]; then
        if ! echo "$base_body" | grep -q '\*\*My understanding:\*\*'; then
            local understanding_text=$(cat "$UNDERSTANDING_FILE")
            base_body=$(printf '%s\n\n**My understanding:** %s' "$base_body" "$understanding_text")
            log "📝 Inserted understanding into comment"
        fi
    fi

    # Build new body with progress section (add closing tag here, not to file)
    local progress_content=$(cat "$PROGRESS_FILE")
    local new_body
    new_body=$(printf '%s\n\n---\n\n%s\n</details>' "$base_body" "$progress_content")

    echo "$new_body" | gh api -X PATCH "$api_endpoint" -F body=@- > /dev/null 2>&1

    if [ $? -eq 0 ]; then
        log "✅ Updated comment"
        return 0
    else
        log "⚠️ Failed to update comment"
        return 1
    fi
}

# Finalize progress section
finalize_progress_section() {
    log "Finalizing progress section..."

    sed -i 's/🔄 Live Progress/💭 How I worked on this/' "$PROGRESS_FILE" 2>/dev/null || \
        sed -i '' 's/🔄 Live Progress/💭 How I worked on this/' "$PROGRESS_FILE" 2>/dev/null || true

    sed -i 's/<details open>/<details>/' "$PROGRESS_FILE" 2>/dev/null || \
        sed -i '' 's/<details open>/<details>/' "$PROGRESS_FILE" 2>/dev/null || true
}

# Watch stream file using inotify
watch_stream() {
    local comment_id="$1"
    local stream_dir=$(dirname "$STREAM_FILE")
    local stream_file=$(basename "$STREAM_FILE")

    log "Using file watcher to watch stream file: $STREAM_FILE"

    mkdir -p "$stream_dir"

    # Backfill any existing content
    backfill_existing_events
    update_comment "$comment_id"

    local last_update=$(date +%s)
    local last_modification=$(stat -c %Y "$STREAM_FILE" 2>/dev/null || echo "0")

    # Start file watcher in background (without subshell pipe)
    log "Starting file watcher for stream monitoring..."
    moosicbox-file-watcher -q -m -e modify,close_write "$stream_dir" >/dev/null 2>&1 &
    local watcher_pid=$!
    log "Stream file-watcher PID: $watcher_pid"

    # Poll for changes indefinitely until killed by workflow
    while kill -0 $watcher_pid 2>/dev/null; do
        # Check for understanding file
        insert_understanding_if_available

        # Check if stream file was modified
        local current_modification=$(stat -c %Y "$STREAM_FILE" 2>/dev/null || echo "0")

        if [ "$current_modification" != "$last_modification" ]; then
            local now=$(date +%s)
            last_modification=$current_modification

            log "Stream file modified, processing new events..."

            # Rate limiting
            if [ $((now - last_update)) -ge $UPDATE_INTERVAL ]; then
                process_new_events

                if [ "$PROCESSED_LINES" -gt 0 ]; then
                    update_comment "$comment_id"
                    last_update=$now
                fi
            fi
        fi

        sleep 1
    done

    log "file-watcher process ended, finishing up..."

    # Final update (don't finalize - let workflow fallback enhance with details)
    process_new_events
    update_comment "$comment_id"
}

# Main execution
main() {
    log "Starting stream watcher"
    log "Stream file: $STREAM_FILE"
    log "Repository: $REPO"
    log "Comment ID: $COMMENT_ID"

    if [ -z "$COMMENT_ID" ]; then
        log "⚠️ No comment ID provided, exiting"
        exit 1
    fi

    # Initialize progress section
    init_progress_section

    # Watch stream file
    watch_stream "$COMMENT_ID"

    log "✅ Stream watching complete"
    exit 0
}

# Trap signals
trap 'log "Received signal, exiting..."; exit 0' SIGTERM SIGINT

# Run
main
