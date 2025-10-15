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
    local max_length=3000

    if [ ${#content} -gt $max_length ]; then
        echo "${content:0:2997}..."
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

format_result_content() {
    local content="$1"

    if [ -z "$content" ] || [ "$content" = "null" ]; then
        echo "*(No output)*"
        echo ""
        return
    fi

    content=$(echo "$content" | jq -r 'if type == "array" and length > 0 and .[0].type == "text" then .[0].text else . end' 2>/dev/null || echo "$content")

    content=$(truncate_content "$content")

    local content_type=$(detect_content_type "$content")

    if [ "$content_type" = "json" ]; then
        content=$(echo "$content" | jq . 2>/dev/null || echo "$content")
    fi

    if [ "$content_type" = "text" ] && [ ${#content} -lt 100 ] && ! echo "$content" | grep -q $'\n'; then
        echo "**→** $content"
        echo ""
    else
        echo "**Result:**"
        echo "\`\`\`$content_type"
        echo "$content"
        echo "\`\`\`"
        echo ""
    fi
}

cat > "$OUTPUT_FILE" << 'HEADER'
---

<details>
<summary>🔍 View Execution Details</summary>

## Claude's Reasoning & Actions

HEADER

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

        has_content=false

        echo "$content" | jq -c '.[]' | while IFS= read -r item; do
            item_type=$(echo "$item" | jq -r '.type // "unknown"')

            if [ "$item_type" = "text" ]; then
                text=$(echo "$item" | jq -r '.text // ""')
                if [ -n "$text" ]; then
                    has_content=true
                    echo "### 💭 Thinking" >> "$OUTPUT_FILE"
                    echo "" >> "$OUTPUT_FILE"
                    echo "$text" >> "$OUTPUT_FILE"
                    echo "" >> "$OUTPUT_FILE"
                fi
            elif [ "$item_type" = "tool_use" ]; then
                has_content=true
                tool_name=$(echo "$item" | jq -r '.name // "unknown_tool"')
                tool_input=$(echo "$item" | jq -r '.input // {}')
                tool_id=$(echo "$item" | jq -r '.id // ""')

                echo "### 🔧 \`$tool_name\`" >> "$OUTPUT_FILE"
                echo "" >> "$OUTPUT_FILE"

                if [ "$tool_input" != "{}" ]; then
                    echo "**Parameters:**" >> "$OUTPUT_FILE"
                    echo "\`\`\`json" >> "$OUTPUT_FILE"
                    echo "$tool_input" | jq . >> "$OUTPUT_FILE"
                    echo "\`\`\`" >> "$OUTPUT_FILE"
                    echo "" >> "$OUTPUT_FILE"
                fi

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
                            echo "❌ **Error:** \`$result_content\`" >> "$OUTPUT_FILE"
                            echo "" >> "$OUTPUT_FILE"
                        else
                            format_result_content "$result_content" >> "$OUTPUT_FILE"
                        fi
                    fi
                fi
            fi
        done

        if [ "$total_input" -gt 0 ] || [ "$output_tokens" -gt 0 ]; then
            echo "*Token usage: $total_input input, $output_tokens output*" >> "$OUTPUT_FILE"
            echo "" >> "$OUTPUT_FILE"
        fi

        if [ "$has_content" = true ]; then
            echo "---" >> "$OUTPUT_FILE"
            echo "" >> "$OUTPUT_FILE"
        fi
    elif [ "$turn_type" = "result" ]; then
        total_cost=$(echo "$turn" | jq -r '.total_cost_usd // .cost_usd // 0')
        total_duration=$(echo "$turn" | jq -r '.duration_ms // 0')
    fi
done

if [ "$(echo "$total_cost > 0" | bc -l 2>/dev/null || echo "0")" = "1" ] || [ "$total_duration" -gt 0 ]; then
    duration_sec=$(echo "scale=1; $total_duration / 1000" | bc -l 2>/dev/null || echo "0.0")

    echo "---" >> "$OUTPUT_FILE"
    echo "" >> "$OUTPUT_FILE"
    echo "**Total Cost:** \$$total_cost | **Duration:** ${duration_sec}s" >> "$OUTPUT_FILE"
    echo "" >> "$OUTPUT_FILE"
fi

echo "</details>" >> "$OUTPUT_FILE"

echo "✅ Generated execution details: $OUTPUT_FILE"
