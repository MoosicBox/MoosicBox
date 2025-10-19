#!/bin/bash
set -euo pipefail

# Configuration
COMMENT_ID_FILE="/tmp/claude_ack_comment_id.txt"
STREAM_FILE="$1"
REPO="$2"
export GH_TOKEN="$3"

UPDATE_INTERVAL=3  # seconds between comment updates
MAX_WAIT_TIME=300  # max 5 minutes to wait for comment ID
IDLE_THRESHOLD=10  # Consider stream complete after 10s idle
PROCESSED_LINES=0
PROGRESS_FILE="/tmp/progress_content.md"

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

# Wait for comment ID file using inotify
wait_for_comment_id() {
    local watch_dir=$(dirname "$COMMENT_ID_FILE")
    local watch_file=$(basename "$COMMENT_ID_FILE")

    log "Using inotify to watch for: $COMMENT_ID_FILE"

    mkdir -p "$watch_dir"

    # If file already exists, return immediately
    if [ -f "$COMMENT_ID_FILE" ]; then
        local content=$(cat "$COMMENT_ID_FILE" | tr -d '[:space:]')
        log "File already exists with content: '$content'"
        echo "$content"
        return 0
    fi

    # Start inotifywait in background (without subshell)
    # Redirect BOTH stdout and stderr to prevent output pollution
    log "Starting inotifywait in background..."
    timeout $MAX_WAIT_TIME inotifywait -q -m -e create,modify,close_write "$watch_dir" >/dev/null 2>&1 &
    local inotify_pid=$!
    log "inotifywait PID: $inotify_pid"

    # Poll for file existence AND content (file is created empty, then written to)
    local elapsed=0
    while [ $elapsed -lt $((MAX_WAIT_TIME * 10)) ]; do
        if [ -f "$COMMENT_ID_FILE" ] && [ -s "$COMMENT_ID_FILE" ]; then
            log "✅ Detected $watch_file with content"
            kill $inotify_pid 2>/dev/null || true
            sleep 0.1
            break
        fi

        # Check if inotifywait is still running
        if ! kill -0 $inotify_pid 2>/dev/null; then
            log "inotifywait process died unexpectedly"
            break
        fi

        sleep 0.1
        elapsed=$((elapsed + 1))
    done

    # Clean up inotifywait if still running
    if kill -0 $inotify_pid 2>/dev/null; then
        log "Killing inotifywait process $inotify_pid"
        kill $inotify_pid 2>/dev/null || true
        wait $inotify_pid 2>/dev/null || true
    fi

    # Read and return the file content
    if [ -f "$COMMENT_ID_FILE" ]; then
        log "Comment ID file exists, reading content..."
        local content=$(cat "$COMMENT_ID_FILE" | tr -d '[:space:]')
        log "File content length: ${#content}"
        log "File content (raw): '$content'"
        if [ ${#content} -gt 0 ]; then
            log "File content (hex): $(cat "$COMMENT_ID_FILE" | xxd -p 2>/dev/null | head -c 100 || echo 'xxd not available')"
        else
            log "⚠️ File is EMPTY!"
            log "File size: $(wc -c < "$COMMENT_ID_FILE" 2>/dev/null || echo 'unknown') bytes"
        fi
        echo "$content"
        return 0
    else
        log "Timeout waiting for comment ID file after ${MAX_WAIT_TIME}s"
        return 1
    fi
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
    local api_endpoint="/repos/${REPO}/issues/comments/${comment_id}"

    local original_body=$(gh api "$api_endpoint" --jq '.body' 2>/dev/null || echo "")

    if [ -z "$original_body" ]; then
        log "⚠️ Failed to fetch original comment body"
        return 1
    fi

    log "Original body length: ${#original_body} chars"

    # Remove existing progress section (everything from first --- onwards)
    # Use simpler pattern: any line with only dashes (and optional whitespace)
    local base_body=$(echo "$original_body" | awk '/^[[:space:]]*-+[[:space:]]*$/ {exit} {print}')

    log "Base body length after stripping: ${#base_body} chars"
    # Strip trailing blank lines
    base_body=$(echo "$base_body" | awk '{lines[++n]=$0} END {for(i=1;i<=n;i++){if(i==n){gsub(/^[[:space:]]*$/,"",lines[i])} if(lines[i]!="")print lines[i]; else if(i<n)print ""}}')

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

    log "Using inotify to watch stream file: $STREAM_FILE"

    mkdir -p "$stream_dir"

    # Backfill any existing content
    backfill_existing_events
    update_comment "$comment_id"

    local last_update=$(date +%s)
    local last_event=$(date +%s)
    local last_modification=$(stat -c %Y "$STREAM_FILE" 2>/dev/null || echo "0")

    # Start inotifywait in background (without subshell pipe)
    log "Starting inotifywait for stream monitoring..."
    inotifywait -q -m -e modify,close_write "$stream_dir" >/dev/null 2>&1 &
    local inotify_pid=$!
    log "Stream inotifywait PID: $inotify_pid"

    # Poll for changes
    while kill -0 $inotify_pid 2>/dev/null; do
        # Check if stream file was modified
        local current_modification=$(stat -c %Y "$STREAM_FILE" 2>/dev/null || echo "0")

        if [ "$current_modification" != "$last_modification" ]; then
            local now=$(date +%s)
            last_event=$now
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

        # Check for idle timeout (no modifications for IDLE_THRESHOLD seconds)
        local now=$(date +%s)
        local idle_time=$((now - last_event))

        if [ $idle_time -ge $IDLE_THRESHOLD ]; then
            log "Stream idle for ${idle_time}s, checking if complete..."

            # Final processing
            process_new_events

            # If no new events for IDLE_THRESHOLD, consider stream complete
            if [ $((now - last_event)) -ge $IDLE_THRESHOLD ]; then
                log "Stream complete (idle for ${idle_time}s)"

                # Final update (don't finalize - let workflow fallback enhance with details)
                process_new_events
                update_comment "$comment_id"

                kill $inotify_pid 2>/dev/null || true
                return 0
            fi
        fi

        sleep 1
    done

    log "inotifywait process ended, finishing up..."

    # Final update (don't finalize - let workflow fallback enhance with details)
    process_new_events
    update_comment "$comment_id"
}

# Main execution
main() {
    log "Starting stream watcher"
    log "Stream file: $STREAM_FILE"
    log "Repository: $REPO"

    # Wait for comment ID
    log "Waiting for comment ID file..."
    local comment_id=""
    comment_id=$(wait_for_comment_id)
    local wait_result=$?

    log "wait_for_comment_id returned with code: $wait_result"
    log "Received comment_id: '$comment_id'"
    log "Comment ID length: ${#comment_id}"

    if [ $wait_result -ne 0 ]; then
        log "⚠️ wait_for_comment_id failed with exit code $wait_result"
        exit 0
    fi

    if [ -z "$comment_id" ]; then
        log "⚠️ No comment ID found (empty string), exiting gracefully"
        exit 0
    fi

    log "✅ Watching comment ID: $comment_id"

    # Initialize progress section
    init_progress_section

    # Watch stream file
    watch_stream "$comment_id"

    log "✅ Stream watching complete"
    exit 0
}

# Trap signals
trap 'log "Received signal, exiting..."; exit 0' SIGTERM SIGINT

# Run
main
