#!/bin/bash
set -euo pipefail

EXECUTION_FILE="${1:-}"
OUTPUT_FILE="${2:-}"

if [ -z "$EXECUTION_FILE" ] || [ -z "$OUTPUT_FILE" ]; then
    echo "Usage: $0 <execution-file.json> <output-file.md>" >&2
    exit 1
fi

if [ ! -f "$EXECUTION_FILE" ]; then
    echo "Error: Execution file '$EXECUTION_FILE' not found" >&2
    exit 1
fi

truncate_content() {
    local content="$1"
    local max_length=2000

    if [ ${#content} -gt $max_length ]; then
        echo "${content:0:1997}..."
    else
        echo "$content"
    fi
}

detect_content_type() {
    local content="$1"

    if echo "$content" | grep -qE '^\{.*\}$|^\[.*\]$'; then
        if echo "$content" | jq empty 2>/dev/null; then
            echo "json"
            return
        fi
    fi

    if echo "$content" | grep -qE 'def |class |import |from |function |const |let |var '; then
        if echo "$content" | grep -qE 'def |import |from '; then
            echo "python"
        elif echo "$content" | grep -qE 'function |const |let |var |=>'; then
            echo "javascript"
        else
            echo "python"
        fi
        return
    fi

    if echo "$content" | grep -qE '^/|Error:|^total |ls -|cd |mkdir |rm |\$ |# '; then
        echo "bash"
        return
    fi

    if echo "$content" | grep -qE '^@@|^\+\+\+ |^--- '; then
        echo "diff"
        return
    fi

    if echo "$content" | grep -qE '^<.*>$'; then
        echo "html"
        return
    fi

    if echo "$content" | grep -qE '^# |^## |^### |^- |^\* |```'; then
        echo "markdown"
        return
    fi

    echo "text"
}

count_lines() {
    local content="$1"
    echo "$content" | wc -l
}

should_show_output() {
    local tool_name="$1"

    case "$tool_name" in
        Read|Glob|Grep|List|grep|glob|list|read)
            echo "false"
            ;;
        *)
            echo "true"
            ;;
    esac
}

extract_tool_context() {
    local tool_name="$1"
    local tool_input="$2"

    case "$tool_name" in
        Read|read)
            filepath=$(echo "$tool_input" | jq -r '.file_path // .filePath // .file // .path // empty' 2>/dev/null)
            if [ -n "$filepath" ]; then
                echo " on \`$filepath\`"
            fi
            ;;
        Edit|edit)
            filepath=$(echo "$tool_input" | jq -r '.file_path // .filePath // .file // empty' 2>/dev/null)
            if [ -n "$filepath" ]; then
                echo " on \`$filepath\`"
            fi
            ;;
        Write|write)
            filepath=$(echo "$tool_input" | jq -r '.file_path // .filePath // .file // empty' 2>/dev/null)
            if [ -n "$filepath" ]; then
                echo " to \`$filepath\`"
            fi
            ;;
        Glob|glob)
            pattern=$(echo "$tool_input" | jq -r '.pattern // empty' 2>/dev/null)
            if [ -n "$pattern" ]; then
                echo " for \`$pattern\`"
            fi
            ;;
        Grep|grep)
            pattern=$(echo "$tool_input" | jq -r '.pattern // empty' 2>/dev/null)
            if [ -n "$pattern" ]; then
                echo " for \`$pattern\`"
            fi
            ;;
        List|list)
            path=$(echo "$tool_input" | jq -r '.path // empty' 2>/dev/null)
            if [ -n "$path" ]; then
                echo " in \`$path\`"
            fi
            ;;
        Bash|bash)
            command=$(echo "$tool_input" | jq -r '.command // empty' 2>/dev/null)
            if [ -n "$command" ]; then
                # Show command inline, truncate if too long
                if echo "$command" | grep -q $'\n'; then
                    # Multiline command - show first line with ellipsis
                    first_line=$(echo "$command" | head -n1)
                    if [ ${#first_line} -gt 200 ]; then
                        echo ": \`${first_line:0:197}...\`"
                    else
                        echo ": \`$first_line...\`"
                    fi
                elif [ ${#command} -gt 200 ]; then
                    # Single line but too long - truncate
                    echo ": \`${command:0:197}...\`"
                else
                    # Short single line - show full command
                    echo ": \`$command\`"
                fi
            fi
            ;;
    esac
}

cat > "$OUTPUT_FILE" << 'HEADER'
---

<details>
<summary>💭 How I worked on this</summary>

HEADER

total_actions=0
total_input_tokens=0
total_output_tokens=0
total_cost=0
total_duration=0

jq -c '.[]' "$EXECUTION_FILE" | while IFS= read -r turn; do
    turn_type=$(echo "$turn" | jq -r '.type // "unknown"')

    if [ "$turn_type" = "assistant" ]; then
        message=$(echo "$turn" | jq -r '.message // {}')
        content=$(echo "$message" | jq -c '.content // []')
        usage=$(echo "$message" | jq -r '.usage // {}')

        input_tokens=$(echo "$usage" | jq -r '.input_tokens // 0')
        cache_creation=$(echo "$usage" | jq -r '.cache_creation_input_tokens // 0')
        cache_read=$(echo "$usage" | jq -r '.cache_read_input_tokens // 0')
        output_tokens=$(echo "$usage" | jq -r '.output_tokens // 0')

        total_input=$((input_tokens + cache_creation + cache_read))

        echo "$content" | jq -c '.[]' | while IFS= read -r item; do
            item_type=$(echo "$item" | jq -r '.type // "unknown"')

            if [ "$item_type" = "text" ]; then
                text=$(echo "$item" | jq -r '.text // ""')
                if [ -n "$text" ]; then
                    # Format as blockquote to visually distinguish thinking/reasoning
                    echo "$text" | sed 's/^/> /' >> "$OUTPUT_FILE"
                    echo "" >> "$OUTPUT_FILE"
                fi
            elif [ "$item_type" = "tool_use" ]; then
                total_actions=$((total_actions + 1))
                tool_name=$(echo "$item" | jq -r '.name // "unknown_tool"')
                tool_input=$(echo "$item" | jq -r '.input // {}')
                tool_id=$(echo "$item" | jq -r '.id // ""')

                tool_context=$(extract_tool_context "$tool_name" "$tool_input")
                echo "→ Used \`$tool_name\`$tool_context" >> "$OUTPUT_FILE"

                show_output=$(should_show_output "$tool_name")

                if [ -n "$tool_id" ]; then
                    tool_result=$(jq -r --arg tool_id "$tool_id" '
                        .[] | select(.type == "user") |
                        .message.content[]? |
                        select(.type == "tool_result" and .tool_use_id == $tool_id)
                    ' "$EXECUTION_FILE" 2>/dev/null || echo "")

                    if [ -n "$tool_result" ] && [ "$tool_result" != "null" ]; then
                        is_error=$(echo "$tool_result" | jq -r '.is_error // false')
                        result_content=$(echo "$tool_result" | jq -r '.content // ""')

                        if [ "$is_error" = "true" ]; then
                            echo "" >> "$OUTPUT_FILE"
                            echo "⚠️ Error: \`$(truncate_content "$result_content")\`" >> "$OUTPUT_FILE"
                            echo "" >> "$OUTPUT_FILE"
                        elif [ "$show_output" = "true" ] && [ -n "$result_content" ] && [ "$result_content" != "null" ]; then
                            result_content=$(echo "$result_content" | jq -r 'if type == "array" and length > 0 and .[0].type == "text" then .[0].text else . end' 2>/dev/null || echo "$result_content")

                            result_content=$(truncate_content "$result_content")
                            line_count=$(count_lines "$result_content")
                            content_type=$(detect_content_type "$result_content")

                            if [ "$line_count" -le 10 ] && [ ${#result_content} -lt 200 ]; then
                                if [ "$content_type" = "text" ] && ! echo "$result_content" | grep -q $'\n'; then
                                    echo " • $result_content" >> "$OUTPUT_FILE"
                                else
                                    echo "" >> "$OUTPUT_FILE"
                                    echo "\`\`\`$content_type" >> "$OUTPUT_FILE"
                                    echo "$result_content" >> "$OUTPUT_FILE"
                                    echo "\`\`\`" >> "$OUTPUT_FILE"
                                fi
                            else
                                if [ "$content_type" = "json" ]; then
                                    result_content=$(echo "$result_content" | jq . 2>/dev/null || echo "$result_content")
                                fi

                                echo "" >> "$OUTPUT_FILE"
                                echo "<details><summary>View output ($line_count lines)</summary>" >> "$OUTPUT_FILE"
                                echo "" >> "$OUTPUT_FILE"
                                echo "\`\`\`$content_type" >> "$OUTPUT_FILE"
                                echo "$result_content" >> "$OUTPUT_FILE"
                                echo "\`\`\`" >> "$OUTPUT_FILE"
                                echo "</details>" >> "$OUTPUT_FILE"
                            fi
                        fi
                    fi
                fi

                echo "" >> "$OUTPUT_FILE"
            fi
        done
    elif [ "$turn_type" = "result" ]; then
        total_cost=$(echo "$turn" | jq -r '.total_cost_usd // .cost_usd // 0')
        total_duration=$(echo "$turn" | jq -r '.duration_ms // 0')
    fi
done

if [ "$(echo "$total_cost > 0" | bc -l 2>/dev/null || echo "0")" = "1" ] || [ "$total_duration" -gt 0 ]; then
    duration_sec=$(echo "scale=1; $total_duration / 1000" | bc -l 2>/dev/null || echo "0.0")

    total_input_k=$(echo "scale=1; $total_input_tokens / 1000" | bc -l 2>/dev/null || echo "0.0")
    total_output_k=$(echo "scale=1; $total_output_tokens / 1000" | bc -l 2>/dev/null || echo "0.0")

    if [ "$total_input_k" = "0.0" ]; then
        total_input_display="${total_input_tokens}"
    else
        total_input_display="${total_input_k}k"
    fi

    if [ "$total_output_k" = "0.0" ]; then
        total_output_display="${total_output_tokens}"
    else
        total_output_display="${total_output_k}k"
    fi

    echo "---" >> "$OUTPUT_FILE"
    echo "" >> "$OUTPUT_FILE"
    if [ "$total_actions" -gt 0 ]; then
        echo "**Summary:** $total_actions actions • ${total_input_display} tokens • \$$total_cost • ${duration_sec}s" >> "$OUTPUT_FILE"
    else
        echo "**Summary:** ${total_input_display} tokens • \$$total_cost • ${duration_sec}s" >> "$OUTPUT_FILE"
    fi
    echo "" >> "$OUTPUT_FILE"
fi

echo "</details>" >> "$OUTPUT_FILE"

echo "✅ Generated execution details: $OUTPUT_FILE"
