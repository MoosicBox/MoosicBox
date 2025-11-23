#!/usr/bin/env bash

# =============================================================================
# run-matrix Command Implementation
# =============================================================================
#
# This module implements the run-matrix command for the clippier action.
# It provides comprehensive test execution across feature combinations with:
# - Template-based command execution
# - Multiple execution strategies (sequential, parallel, combined, chunked)
# - Full failure tracking and reporting
# - Rich GitHub Actions summary generation
# - Configurable summary modes (individual, combined, group)
#
# Dependencies:
#   - lib/template.sh: Template rendering and strategy functions
#
# Usage:
#   run_matrix_command  # Reads from INPUT_RUN_MATRIX_* environment variables
#
# =============================================================================

# Global variables for state management
SUMMARY_STATE_DIR="${RUNNER_TEMP:-/tmp}/run-matrix-summary-state"

# Strip ANSI color codes from text
#
# Removes terminal color escape sequences that appear as garbage in markdown.
# ANSI codes look like: ESC[XXm where XX are color/style codes.
# Also handles cases where ESC byte was replaced with � (Unicode replacement char).
#
# Arguments:
#   stdin: Text with ANSI codes
#
# Returns:
#   Clean text without ANSI codes on stdout
#
# Example:
#   echo -e "\e[1m\e[92mGreen Bold\e[0m" | strip_ansi_codes
#   # Output: Green Bold
strip_ansi_codes() {
    # Remove ANSI escape sequences in two passes:
    # 1. Standard ESC[<codes>m sequences (ESC = \x1b)
    # 2. Malformed sequences where ESC became � (Unicode replacement char U+FFFD)
    sed 's/\x1b\[[0-9;]*m//g' | sed 's/�\[[0-9;]*m//g'
}

# Generate debug reproduction command for a specific shell
#
# Creates a shell-specific command that can be run locally to reproduce
# the test execution. Each shell has a hardcoded template for navigating
# to the working directory and executing the command.
#
# For single-line commands, uses compact syntax:
#   - bash: (cd <dir>; <command>)
#   - fish: pushd <dir>; <command>; popd
#   - zsh: (cd <dir> && <command>)
#
# For multi-line commands, uses expanded subshell/block syntax:
#   - bash: (cd <dir>\n<command>\n)
#   - fish: begin\n    pushd <dir>\n    <indented-command>\n    popd\nend
#   - zsh: (cd <dir> && {\n<command>\n})
#
# Arguments:
#   $1 - shell_name: Name of the shell (bash, fish, zsh)
#   $2 - working_dir: Working directory path
#   $3 - command: Command to execute (may contain newlines)
#
# Returns:
#   Shell-specific reproduction command on stdout
#
# Example:
#   get_debug_command "bash" "packages/player" "cargo test"
#   # Output: (cd packages/player; cargo test)
#
#   get_debug_command "bash" "packages/player" "if [ -f test ]; then\n  cargo test\nfi"
#   # Output: (cd packages/player
#   #         if [ -f test ]; then
#   #           cargo test
#   #         fi
#   #         )
get_debug_command() {
    local shell_name="$1"
    local working_dir="$2"
    local command="$3"

    # Detect if command contains newlines (multi-line script)
    if [[ "$command" == *$'\n'* ]]; then
        # Multi-line format with preserved newlines
        case "$shell_name" in
            "bash")
                printf "(cd %s\n%s\n)" "$working_dir" "$command"
                ;;
            "fish")
                # Indent the command block for fish
                local indented_cmd=$(echo "$command" | sed 's/^/    /')
                printf "begin\n    pushd %s\n%s\n    popd\nend" "$working_dir" "$indented_cmd"
                ;;
            "zsh")
                printf "(cd %s && {\n%s\n})" "$working_dir" "$command"
                ;;
            *)
                echo "# Unsupported shell: $shell_name"
                ;;
        esac
    else
        # Simple single-line format
        case "$shell_name" in
            "bash")
                echo "(cd $working_dir; $command)"
                ;;
            "fish")
                echo "pushd $working_dir; $command; popd"
                ;;
            "zsh")
                echo "(cd $working_dir && $command)"
                ;;
            *)
                echo "# Unsupported shell: $shell_name"
                ;;
        esac
    fi
}

# Initialize summary state directory
#
# Creates the state directory for accumulating summary data in combined mode.
# Safe to call multiple times.
#
# Side effects:
#   Creates $SUMMARY_STATE_DIR directory
init_summary_state() {
    mkdir -p "$SUMMARY_STATE_DIR"
}

# Accumulate summary data to JSON state file
#
# Stores run results in a JSON file for later aggregation in combined mode.
# Each run's data is appended to an array in the state file.
#
# Arguments:
#   $1 - group_name: Name of the group (e.g., "default", "tests", "validation")
#   $2 - package_name: Name of the package being tested
#   $3 - working_dir: Working directory where commands were executed
#   $4 - label: Custom label for the test run
#   $5 - total_runs: Total number of command runs
#   $6 - total_passed: Number of passed runs
#   $7 - total_failed: Number of failed runs
#   $8 - failures_json: JSON array of failure objects
#
# Side effects:
#   Creates or appends to $SUMMARY_STATE_DIR/group_<name>.json
accumulate_summary_to_state() {
    local group_name="$1"
    local package_name="$2"
    local working_dir="$3"
    local label="$4"
    local total_runs="$5"
    local total_passed="$6"
    local total_failed="$7"
    local failures_json="$8"

    init_summary_state

    local state_file="$SUMMARY_STATE_DIR/group_${group_name}.json"

    # Create JSON object with this run's data
    # Use pipe approach to avoid "Argument list too long" errors with large failure data
    local run_data=$(echo "$failures_json" | jq -c \
        --arg pkg "$package_name" \
        --arg label "$label" \
        --arg working_dir "$working_dir" \
        --argjson total_runs "$total_runs" \
        --argjson total_passed "$total_passed" \
        --argjson total_failed "$total_failed" \
        '{
            package: $pkg,
            label: $label,
            working_dir: $working_dir,
            total_runs: $total_runs,
            total_passed: $total_passed,
            total_failed: $total_failed,
            failures: .
        }')

    # Append to state file (create array if doesn't exist)
    if [[ -f "$state_file" ]]; then
        local temp_file
        temp_file=$(mktemp)
        jq ". += [$run_data]" "$state_file" > "$temp_file"
        mv "$temp_file" "$state_file"
    else
        echo "[$run_data]" > "$state_file"
    fi

    echo "📝 Accumulated summary data for group: $group_name"
}

# Get status emoji and text based on results
#
# Arguments:
#   $1 - total_runs: Total number of runs
#   $2 - total_passed: Number of passed runs
#   $3 - total_failed: Number of failed runs
#
# Returns:
#   Formatted status string on stdout
get_status_emoji_and_text() {
    local total_runs="$1"
    local total_passed="$2"
    local total_failed="$3"

    if [[ "$total_failed" -gt 0 ]]; then
        echo "❌ $total_failed/$total_runs failed"
    else
        echo "✅ $total_passed/$total_runs passed"
    fi
}

# Write a single run subsection in combined summary
#
# Writes detailed test results for one run-matrix invocation as a subsection
# within a combined summary.
#
# Arguments:
#   $1 - run_json: JSON object containing run data
#
# Side effects:
#   Appends to $GITHUB_STEP_SUMMARY
write_run_subsection() {
    local run_json="$1"

    local label=$(echo "$run_json" | jq -r '.label')
    local package=$(echo "$run_json" | jq -r '.package')
    local working_dir=$(echo "$run_json" | jq -r '.working_dir')
    local total_runs=$(echo "$run_json" | jq -r '.total_runs')
    local total_passed=$(echo "$run_json" | jq -r '.total_passed')
    local total_failed=$(echo "$run_json" | jq -r '.total_failed')
    local failures=$(echo "$run_json" | jq -c '.failures')

    # Check if should only generate summary on failure
    local only_on_failure="${INPUT_RUN_MATRIX_SUMMARY_ONLY_ON_FAILURE:-false}"
    if [[ "$only_on_failure" == "true" && "$total_failed" -eq 0 ]]; then
        echo "ℹ️  All tests passed for $package - skipping subsection (only-on-failure mode)"
        return 0
    fi

    # Determine if should be open based on auto-expand setting AND failures
    local auto_expand="${INPUT_RUN_MATRIX_SUMMARY_AUTO_EXPAND:-failures}"
    local details_tag="<details>"
    local error_details_tag="<details>"

    case "$auto_expand" in
        "always")
            details_tag="<details open>"
            error_details_tag="<details open>"
            ;;
        "failures")
            if [[ "$total_failed" -gt 0 ]]; then
                details_tag="<details open>"
                error_details_tag="<details open>"
            fi
            ;;
        "never")
            details_tag="<details>"
            error_details_tag="<details>"
            ;;
        *)
            # Default to "failures" behavior
            if [[ "$total_failed" -gt 0 ]]; then
                details_tag="<details open>"
                error_details_tag="<details open>"
            fi
            ;;
    esac

    # Write collapsible section
    echo "" >> $GITHUB_STEP_SUMMARY
    echo "$details_tag" >> $GITHUB_STEP_SUMMARY
    echo "<summary><b>📦 $label: $package</b> - $(get_status_emoji_and_text "$total_runs" "$total_passed" "$total_failed")</summary>" >> $GITHUB_STEP_SUMMARY
    echo "" >> $GITHUB_STEP_SUMMARY
    echo "**Working Directory:** \`$working_dir\`" >> $GITHUB_STEP_SUMMARY
    echo "" >> $GITHUB_STEP_SUMMARY

    if [[ "$total_failed" -gt 0 ]]; then
        echo "#### ❌ Failures ($total_failed)" >> $GITHUB_STEP_SUMMARY
        echo "" >> $GITHUB_STEP_SUMMARY

        # Write failure details (similar to individual mode)
        echo "$failures" | jq -c '.[]' | while IFS= read -r failure_json; do
            local cmd=$(echo "$failure_json" | jq -r '.command')
            local features=$(echo "$failure_json" | jq -r '.feature_combo')
            local exit_code=$(echo "$failure_json" | jq -r '.exit_code')
            local duration=$(echo "$failure_json" | jq -r '.duration_secs')
            local output_file=$(echo "$failure_json" | jq -r '.output_file')

            echo "##### 🔴 Features: \`$features\`" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY
            echo "**Exit Code:** $exit_code  " >> $GITHUB_STEP_SUMMARY
            echo "**Duration:** ${duration}s" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY

            # Script (collapsible)
            echo "<details>" >> $GITHUB_STEP_SUMMARY
            echo "<summary><b>📋 Script</b></summary>" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY
            echo "\`\`\`bash" >> $GITHUB_STEP_SUMMARY
            echo "$cmd" >> $GITHUB_STEP_SUMMARY
            echo "\`\`\`" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY
            echo "</details>" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY

            # Debug reproduction commands (collapsible)
            local debug_shells="${INPUT_RUN_MATRIX_DEBUG_SHELLS:-bash}"
            echo "<details>" >> $GITHUB_STEP_SUMMARY
            echo "<summary><b>🔄 Reproduce Locally</b></summary>" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY

            # Parse comma-separated shell list and generate commands for each
            IFS=',' read -ra SHELLS <<< "$debug_shells"
            for shell in "${SHELLS[@]}"; do
                # Trim whitespace
                shell=$(echo "$shell" | xargs)

                # Capitalize shell name for display (bash 3.2 compatible)
                local first_char="$(echo "${shell:0:1}" | tr '[:lower:]' '[:upper:]')"
                local shell_display="${first_char}${shell:1}"

                # Generate debug command for this shell
                local debug_cmd=$(get_debug_command "$shell" "$working_dir" "$cmd")

                echo "**$shell_display:**" >> $GITHUB_STEP_SUMMARY
                echo "\`\`\`$shell" >> $GITHUB_STEP_SUMMARY
                echo "$debug_cmd" >> $GITHUB_STEP_SUMMARY
                echo "\`\`\`" >> $GITHUB_STEP_SUMMARY
                echo "" >> $GITHUB_STEP_SUMMARY
            done

            echo "</details>" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY

            # Error output
            echo "$error_details_tag" >> $GITHUB_STEP_SUMMARY
            echo "<summary><b>❌ Error Output</b></summary>" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY
            echo "\`\`\`" >> $GITHUB_STEP_SUMMARY
            if [[ -f "$output_file" ]]; then
                local max_lines="${INPUT_RUN_MATRIX_MAX_OUTPUT_LINES:-200}"
                if [[ "$max_lines" -gt 0 ]]; then
                    tail -"$max_lines" "$output_file" | strip_ansi_codes >> $GITHUB_STEP_SUMMARY
                else
                    cat "$output_file" | strip_ansi_codes >> $GITHUB_STEP_SUMMARY
                fi
                rm -f "$output_file"
            else
                echo "Error: Output file not found" >> $GITHUB_STEP_SUMMARY
            fi
            echo "\`\`\`" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY
            echo "</details>" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY
        done

    else
        echo "✅ All tests passed!" >> $GITHUB_STEP_SUMMARY
        echo "" >> $GITHUB_STEP_SUMMARY
    fi

    echo "</details>" >> $GITHUB_STEP_SUMMARY
    echo "" >> $GITHUB_STEP_SUMMARY
}

# Flush combined summary from accumulated state
#
# Writes all accumulated run results for a group into a single combined
# summary section in GitHub Actions.
#
# Arguments:
#   $1 - group_name: Name of the group to flush
#
# Side effects:
#   Appends to $GITHUB_STEP_SUMMARY
#   Removes state file after writing
flush_combined_summary() {
    local group_name="$1"
    local state_file="$SUMMARY_STATE_DIR/group_${group_name}.json"

    if [[ ! -f "$state_file" ]]; then
        echo "⚠️  No accumulated summary data found for group: $group_name"
        return
    fi

    local runs_data=$(cat "$state_file")

    # Calculate aggregate statistics
    local total_runs=$(echo "$runs_data" | jq '[.[].total_runs] | add // 0')
    local total_passed=$(echo "$runs_data" | jq '[.[].total_passed] | add // 0')
    local total_failed=$(echo "$runs_data" | jq '[.[].total_failed] | add // 0')

    # Check if should only generate summary on failure
    local only_on_failure="${INPUT_RUN_MATRIX_SUMMARY_ONLY_ON_FAILURE:-false}"
    if [[ "$only_on_failure" == "true" && "$total_failed" -eq 0 ]]; then
        echo "ℹ️  All tests passed in group '$group_name' - skipping summary (only-on-failure mode)"
        # Clean up state file
        rm -f "$state_file"
        return 0
    fi

    # Determine title based on group name
    local title="Combined Test Results"
    [[ "$group_name" != "default" ]] && title="$title: $group_name"

    # Determine if should be open based on auto-expand setting
    local auto_expand="${INPUT_RUN_MATRIX_SUMMARY_AUTO_EXPAND:-failures}"
    local details_tag="<details>"

    case "$auto_expand" in
        "always")
            details_tag="<details open>"
            ;;
        "failures")
            if [[ "$total_failed" -gt 0 ]]; then
                details_tag="<details open>"
            fi
            ;;
        "never")
            details_tag="<details>"
            ;;
        *)
            # Default to "failures" behavior
            if [[ "$total_failed" -gt 0 ]]; then
                details_tag="<details open>"
            fi
            ;;
    esac

    # Write header
    echo "$details_tag" >> $GITHUB_STEP_SUMMARY
    echo "<summary><b>🧪 $title</b> - $(get_status_emoji_and_text "$total_runs" "$total_passed" "$total_failed")</summary>" >> $GITHUB_STEP_SUMMARY
    echo "" >> $GITHUB_STEP_SUMMARY

    # Aggregate statistics table
    echo "| Metric | Count |" >> $GITHUB_STEP_SUMMARY
    echo "|--------|-------|" >> $GITHUB_STEP_SUMMARY
    echo "| Total Runs | $total_runs |" >> $GITHUB_STEP_SUMMARY
    echo "| ✅ Passed | $total_passed |" >> $GITHUB_STEP_SUMMARY
    echo "| ❌ Failed | $total_failed |" >> $GITHUB_STEP_SUMMARY
    echo "" >> $GITHUB_STEP_SUMMARY

    # Write each test category as subsection
    echo "$runs_data" | jq -c '.[]' | while IFS= read -r run_json; do
        write_run_subsection "$run_json"
    done

    echo "</details>" >> $GITHUB_STEP_SUMMARY
    echo "" >> $GITHUB_STEP_SUMMARY

    echo "✅ Flushed combined summary for group: $group_name"

    # Clean up state file
    rm -f "$state_file"
}

# Generate GitHub Actions summary for run-matrix results
#
# Creates a comprehensive markdown summary including:
# - Statistics table (total/passed/failed)
# - Detailed failure information for each failed run
# - Full error output (last N lines)
# - Command reproduction instructions
#
# Supports multiple summary modes:
# - individual: Write summary immediately (default)
# - combined: Accumulate to state file, flush later
# - group:<name>: Accumulate to named group, flush later
#
# Arguments:
#   $1 - package_name: Name of the package being tested
#   $2 - working_dir: Working directory where commands were executed
#   $3 - label: Custom label for the test run (optional)
#   $4 - total_runs: Total number of command runs
#   $5 - total_passed: Number of passed runs
#   $6 - total_failed: Number of failed runs
#   $7 - failures_json: JSON array of failure objects
#
# Environment:
#   GITHUB_STEP_SUMMARY: Path to GitHub Actions step summary file
#   INPUT_RUN_MATRIX_MAX_OUTPUT_LINES: Max lines of output to show
#   INPUT_RUN_MATRIX_SUMMARY_MODE: Summary mode (individual/combined/group:<name>)
#   INPUT_RUN_MATRIX_SUMMARY_FLUSH: Whether to flush accumulated summaries
#
# Side effects:
#   Appends to $GITHUB_STEP_SUMMARY file (individual mode)
#   OR creates/appends to state file (combined/group mode)
#   Removes temporary output files
generate_run_matrix_summary() {
    local package_name="$1"
    local working_dir="$2"
    local label="$3"
    local total_runs="$4"
    local total_passed="$5"
    local total_failed="$6"
    shift 6
    local failures_json="$1"

    # Parse summary mode
    local summary_mode="${INPUT_RUN_MATRIX_SUMMARY_MODE:-individual}"
    local group_name="default"

    if [[ "$summary_mode" =~ ^group:(.+)$ ]]; then
        group_name="${BASH_REMATCH[1]}"
        summary_mode="combined"
    fi

    # Handle different summary modes
    case "$summary_mode" in
        "individual")
            # Write summary immediately (current behavior)
            write_individual_summary "$package_name" "$working_dir" "$label" \
                "$total_runs" "$total_passed" "$total_failed" "$failures_json"
            ;;
        "combined")
            # Accumulate to state file
            accumulate_summary_to_state "$group_name" "$package_name" "$working_dir" "$label" \
                "$total_runs" "$total_passed" "$total_failed" "$failures_json"

            # Flush if requested
            if [[ "${INPUT_RUN_MATRIX_SUMMARY_FLUSH:-false}" == "true" ]]; then
                flush_combined_summary "$group_name"
            fi
            ;;
        *)
            echo "⚠️  Unknown summary mode: $summary_mode (falling back to individual)"
            write_individual_summary "$package_name" "$working_dir" "$label" \
                "$total_runs" "$total_passed" "$total_failed" "$failures_json"
            ;;
    esac
}

# Write individual summary (original behavior)
#
# Arguments: Same as generate_run_matrix_summary
#
# Side effects:
#   Appends to $GITHUB_STEP_SUMMARY file
#   Removes temporary output files
write_individual_summary() {
    local package_name="$1"
    local working_dir="$2"
    local label="$3"
    local total_runs="$4"
    local total_passed="$5"
    local total_failed="$6"
    shift 6
    local failures_json="$1"

    # Check if should only generate summary on failure
    local only_on_failure="${INPUT_RUN_MATRIX_SUMMARY_ONLY_ON_FAILURE:-false}"
    if [[ "$only_on_failure" == "true" && "$total_failed" -eq 0 ]]; then
        echo "ℹ️  All tests passed - skipping summary (only-on-failure mode)"
        return 0
    fi

    local title="Test Results: $package_name"
    [[ -n "$label" ]] && title="$label: $package_name"

    # Determine if should be open based on auto-expand setting
    local auto_expand="${INPUT_RUN_MATRIX_SUMMARY_AUTO_EXPAND:-failures}"
    local details_tag="<details>"
    local error_details_tag="<details>"

    case "$auto_expand" in
        "always")
            details_tag="<details open>"
            error_details_tag="<details open>"
            ;;
        "failures")
            if [[ "$total_failed" -gt 0 ]]; then
                details_tag="<details open>"
                error_details_tag="<details open>"
            fi
            ;;
        "never")
            details_tag="<details>"
            error_details_tag="<details>"
            ;;
        *)
            # Default to "failures" behavior
            if [[ "$total_failed" -gt 0 ]]; then
                details_tag="<details open>"
                error_details_tag="<details open>"
            fi
            ;;
    esac

    # Create summary line with stats
    local status_emoji="✅"
    local status_text="$total_passed/$total_runs passed"
    if [[ "$total_failed" -gt 0 ]]; then
        status_emoji="❌"
        status_text="$total_failed/$total_runs failed"
    fi

    # Open details section
    echo "$details_tag" >> $GITHUB_STEP_SUMMARY
    echo "<summary><b>🧪 $title</b> - $status_emoji $status_text</summary>" >> $GITHUB_STEP_SUMMARY
    echo "" >> $GITHUB_STEP_SUMMARY

    # Working directory
    echo "**Working Directory:** \`$working_dir\`" >> $GITHUB_STEP_SUMMARY
    echo "" >> $GITHUB_STEP_SUMMARY

    # Statistics table
    echo "| Metric | Count |" >> $GITHUB_STEP_SUMMARY
    echo "|--------|-------|" >> $GITHUB_STEP_SUMMARY
    echo "| Total Runs | $total_runs |" >> $GITHUB_STEP_SUMMARY
    echo "| ✅ Passed | $total_passed |" >> $GITHUB_STEP_SUMMARY
    echo "| ❌ Failed | $total_failed |" >> $GITHUB_STEP_SUMMARY
    echo "" >> $GITHUB_STEP_SUMMARY

    # Failure details
    if [[ "$total_failed" -gt 0 && -n "$failures_json" && "$failures_json" != "[]" ]]; then
        echo "---" >> $GITHUB_STEP_SUMMARY
        echo "" >> $GITHUB_STEP_SUMMARY
        echo "### ❌ Failed Runs" >> $GITHUB_STEP_SUMMARY
        echo "" >> $GITHUB_STEP_SUMMARY

        # Process each failure
        echo "$failures_json" | jq -c '.[]' | while IFS= read -r failure_json; do
            local cmd=$(echo "$failure_json" | jq -r '.command')
            local features=$(echo "$failure_json" | jq -r '.feature_combo')
            local exit_code=$(echo "$failure_json" | jq -r '.exit_code')
            local duration=$(echo "$failure_json" | jq -r '.duration_secs')
            local output_file=$(echo "$failure_json" | jq -r '.output_file')

            echo "#### 🔴 Features: \`$features\`" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY
            echo "**Exit Code:** $exit_code  " >> $GITHUB_STEP_SUMMARY
            echo "**Duration:** ${duration}s" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY

            # Script (collapsible)
            echo "<details>" >> $GITHUB_STEP_SUMMARY
            echo "<summary><b>📋 Script</b></summary>" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY
            echo "\`\`\`bash" >> $GITHUB_STEP_SUMMARY
            echo "$cmd" >> $GITHUB_STEP_SUMMARY
            echo "\`\`\`" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY
            echo "</details>" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY

            # Debug reproduction commands (collapsible)
            local debug_shells="${INPUT_RUN_MATRIX_DEBUG_SHELLS:-bash}"
            echo "<details>" >> $GITHUB_STEP_SUMMARY
            echo "<summary><b>🔄 Reproduce Locally</b></summary>" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY

            # Parse comma-separated shell list and generate commands for each
            IFS=',' read -ra SHELLS <<< "$debug_shells"
            for shell in "${SHELLS[@]}"; do
                # Trim whitespace
                shell=$(echo "$shell" | xargs)

                # Capitalize shell name for display (bash 3.2 compatible)
                local first_char="$(echo "${shell:0:1}" | tr '[:lower:]' '[:upper:]')"
                local shell_display="${first_char}${shell:1}"

                # Generate debug command for this shell
                local debug_cmd=$(get_debug_command "$shell" "$working_dir" "$cmd")

                echo "**$shell_display:**" >> $GITHUB_STEP_SUMMARY
                echo "\`\`\`$shell" >> $GITHUB_STEP_SUMMARY
                echo "$debug_cmd" >> $GITHUB_STEP_SUMMARY
                echo "\`\`\`" >> $GITHUB_STEP_SUMMARY
                echo "" >> $GITHUB_STEP_SUMMARY
            done

            echo "</details>" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY

            # Error output (expanded by default)
            echo "$error_details_tag" >> $GITHUB_STEP_SUMMARY
            echo "<summary><b>❌ Error Output</b></summary>" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY
            echo "\`\`\`" >> $GITHUB_STEP_SUMMARY
            if [[ -f "$output_file" ]]; then
                local max_lines="${INPUT_RUN_MATRIX_MAX_OUTPUT_LINES:-200}"
                if [[ "$max_lines" -gt 0 ]]; then
                    tail -"$max_lines" "$output_file" | strip_ansi_codes >> $GITHUB_STEP_SUMMARY
                else
                    cat "$output_file" | strip_ansi_codes >> $GITHUB_STEP_SUMMARY
                fi
                rm -f "$output_file"
            else
                echo "Error: Output file not found" >> $GITHUB_STEP_SUMMARY
            fi
            echo "\`\`\`" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY
            echo "</details>" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY
        done

        echo "---" >> $GITHUB_STEP_SUMMARY
        echo "" >> $GITHUB_STEP_SUMMARY
    elif [[ "$total_failed" -eq 0 ]]; then
        echo "### ✅ All Tests Passed!" >> $GITHUB_STEP_SUMMARY
        echo "" >> $GITHUB_STEP_SUMMARY
        echo "All $total_runs test runs passed successfully." >> $GITHUB_STEP_SUMMARY
    fi

    # Close details section
    echo "" >> $GITHUB_STEP_SUMMARY
    echo "</details>" >> $GITHUB_STEP_SUMMARY
    echo "" >> $GITHUB_STEP_SUMMARY
}

# Main run-matrix command function
#
# Executes test commands across feature combinations with comprehensive
# failure tracking and reporting.
#
# Algorithm:
# 1. Parse configuration from INPUT_RUN_MATRIX_* environment variables
# 2. Generate feature combinations based on strategy
# 3. For each feature combination:
#    a. Render command templates with current context
#    b. Execute commands and capture output
#    c. Track successes and failures
# 4. Generate summary and output results
#
# Environment Variables (inputs):
#   INPUT_RUN_MATRIX_PACKAGE_JSON: Package matrix JSON (required)
#   INPUT_RUN_MATRIX_COMMANDS: Command templates (newline or comma-separated)
#   INPUT_RUN_MATRIX_STRATEGY: Execution strategy (sequential, combined, etc.)
#   INPUT_RUN_MATRIX_CONTINUE_ON_FAILURE: Continue on failure (default: true)
#   INPUT_RUN_MATRIX_FAIL_FAST: Stop on first failure (default: false)
#   INPUT_RUN_MATRIX_VERBOSE: Show script content before execution (default: false)
#   INPUT_RUN_MATRIX_STREAM_OUTPUT: Stream output to console in real-time (default: true)
#   INPUT_RUN_MATRIX_MAX_OUTPUT_LINES: Lines of output to capture (default: 200)
#   INPUT_RUN_MATRIX_WORKING_DIRECTORY: Custom working directory
#   INPUT_RUN_MATRIX_LABEL: Custom label for summary
#   INPUT_RUN_MATRIX_SKIP_DOCTEST_CHECK: Skip lib target check
#   INPUT_RUN_MATRIX_GENERATE_SUMMARY: Generate summary (default: true)
#
# Environment Variables (outputs):
#   GITHUB_OUTPUT: Path to GitHub Actions output file
#   GITHUB_STEP_SUMMARY: Path to GitHub Actions summary file
#
# Outputs (written to GITHUB_OUTPUT):
#   run-success: Overall success (true/false)
#   run-total: Total number of runs
#   run-passed: Number of passed runs
#   run-failed: Number of failed runs
#   run-results: JSON array of failure details
#
# Exit codes:
#   0: All tests passed
#   1: One or more tests failed OR configuration error
run_matrix_command() {
    CONTEXT_PHASE="run-matrix"
    echo "🧪 Running matrix tests with template-based commands"

    # Disable ERR trap and errexit - we handle errors explicitly in this function
    # This prevents the generic error handler from interfering with our detailed failure tracking
    trap - ERR
    set +e  # Disable errexit so we can handle failures explicitly

    # Validate required inputs
    if [[ -z "$INPUT_RUN_MATRIX_PACKAGE_JSON" ]]; then
        echo "❌ ERROR: run-matrix-package-json is required"
        exit 1
    fi

    local package_json="$INPUT_RUN_MATRIX_PACKAGE_JSON"
    local commands="${INPUT_RUN_MATRIX_COMMANDS}"
    local strategy="${INPUT_RUN_MATRIX_STRATEGY:-sequential}"
    local continue_on_failure="${INPUT_RUN_MATRIX_CONTINUE_ON_FAILURE:-true}"
    local fail_fast="${INPUT_RUN_MATRIX_FAIL_FAST:-false}"
    local verbose="${INPUT_RUN_MATRIX_VERBOSE:-false}"
    local stream_output="${INPUT_RUN_MATRIX_STREAM_OUTPUT:-true}"
    local max_output_lines="${INPUT_RUN_MATRIX_MAX_OUTPUT_LINES:-200}"
    local working_dir="${INPUT_RUN_MATRIX_WORKING_DIRECTORY}"
    local label="${INPUT_RUN_MATRIX_LABEL}"
    local generate_summary="${INPUT_RUN_MATRIX_GENERATE_SUMMARY:-true}"

    # Parse package JSON
    local package_name=$(echo "$package_json" | jq -r '.name')
    local package_path=$(echo "$package_json" | jq -r '.path // "."')
    local features=$(echo "$package_json" | jq -c '.features // []')
    local required_features=$(echo "$package_json" | jq -r '.requiredFeatures // ""')

    # Set global context for error handling
    CONTEXT_PACKAGE_NAME="$package_name"
    CONTEXT_PACKAGE_PATH="$package_path"
    CONTEXT_LABEL="$label"

    # Use custom working directory or default to package path
    working_dir="${working_dir:-$package_path}"

    echo "📦 Package: $package_name"
    echo "📂 Working Directory: $working_dir"
    echo "🎯 Strategy: $strategy"
    [[ -n "$label" ]] && echo "🏷️  Label: $label"

    # Treat entire commands input as a single script template
    local script_template="$commands"

    echo "🔧 Script template to execute per iteration:"
    echo "$script_template" | sed 's/^/  /' # Indent for display

    # Generate feature combinations based on strategy
    # Use while-read loop for bash 3.2 compatibility (macOS)
    # Manually strip trailing whitespace (newlines and carriage returns)
    FEATURE_COMBINATIONS=()
    while IFS= read -r combo || [[ -n "$combo" ]]; do
        # Strip trailing and leading whitespace
        combo="${combo#"${combo%%[![:space:]]*}"}"  # Strip leading
        combo="${combo%"${combo##*[![:space:]]}"}"  # Strip trailing
        [[ -n "$combo" ]] && FEATURE_COMBINATIONS+=("$combo")
    done < <(generate_feature_combinations "$features" "$strategy")

    if [[ $? -ne 0 ]]; then
        echo "❌ Failed to generate feature combinations"
        exit 1
    fi

    local total_iterations=${#FEATURE_COMBINATIONS[@]}
    echo "📊 Total iterations: $total_iterations"

    # Initialize tracking
    declare -a ALL_FAILURES=()
    local total_runs=0
    local total_passed=0
    local total_failed=0

    # Iterate through feature combinations
    for iteration in "${!FEATURE_COMBINATIONS[@]}"; do
        local feature_combo="${FEATURE_COMBINATIONS[$iteration]}"

        echo ""
        echo "### Iteration $((iteration + 1))/$total_iterations: Features [$feature_combo]"

        # Render the entire script with template variables
        local rendered_script=$(render_template \
            "$script_template" \
            "$package_json" \
            "$feature_combo" \
            "$required_features" \
            "$iteration" \
            "$total_iterations")

        # Check for doctest and lib target
        if [[ "$rendered_script" =~ "test --doc" && "$INPUT_RUN_MATRIX_SKIP_DOCTEST_CHECK" != "true" ]]; then
            local package_path_clean="${package_path#./}"
            if ! cargo metadata --format-version=1 --no-deps 2>/dev/null | \
                jq -e ".packages[] | select(.manifest_path | contains(\"${package_path_clean}/Cargo.toml\")) | .targets[] | select(.kind[] | contains(\"lib\"))" > /dev/null 2>&1; then
                echo "⏭️  Skipping iteration - no library target found for doctest"
                continue
            fi
        fi

        total_runs=$((total_runs + 1))

        # Create temp file for output
        local output_file=$(mktemp)
        local start_time=$(date +%s.%N)

        echo "RUNNING script for features [$feature_combo]"
        if [[ "$verbose" == "true" ]]; then
            echo "Script content:"
            echo "$rendered_script" | sed 's/^/  /'
        fi

        # Execute the entire script as bash
        local exit_code=0

        # Determine whether to stream output based on stream_output setting
        if [[ "$stream_output" == "true" ]]; then
            # Stream output to console while capturing to file
            # Capture exit code inside subshell and return it
            (
                cd "$working_dir"
                bash -e -o pipefail <<< "$rendered_script" 2>&1 | tee "$output_file"
                exit ${PIPESTATUS[0]}
            )
            exit_code=$?
            if [[ $exit_code -eq 0 ]]; then
                echo "✅ SUCCESS for features [$feature_combo]"
            else
                echo "❌ FAILED for features [$feature_combo] (exit code: $exit_code)" >&2
            fi
        else
            # Silent mode - only capture to file
            (cd "$working_dir" && bash -e -o pipefail <<< "$rendered_script" > "$output_file" 2>&1)
            exit_code=$?
            if [[ $exit_code -eq 0 ]]; then
                echo "✅ SUCCESS for features [$feature_combo]"
            else
                echo "❌ FAILED for features [$feature_combo] (exit code: $exit_code)" >&2
            fi
        fi

        local end_time=$(date +%s.%N)
        local duration=$(echo "$end_time - $start_time" | bc -l 2>/dev/null || echo "0")

        # Track results
        if [[ "$exit_code" -eq 0 ]]; then
            total_passed=$((total_passed + 1))
            rm -f "$output_file"
        else
            total_failed=$((total_failed + 1))

            # Read and store error output directly in JSON for artifact upload
            local error_output=""
            if [[ -f "$output_file" ]]; then
                error_output=$(cat "$output_file" | strip_ansi_codes)
            fi

            # Create failure JSON with error output embedded
            local failure_json=$(jq -n \
                --arg pkg "$package_name" \
                --arg path "$package_path" \
                --arg features "$feature_combo" \
                --arg req_features "$required_features" \
                --arg script "$rendered_script" \
                --arg script_template "$script_template" \
                --argjson exit_code "$exit_code" \
                --arg output_file "$output_file" \
                --arg error_output "$error_output" \
                --arg duration "$duration" \
                --argjson iteration "$iteration" \
                '{
                    package: $pkg,
                    path: $path,
                    feature_combo: $features,
                    required_features: $req_features,
                    command: $script,
                    command_template: $script_template,
                    exit_code: $exit_code,
                    output_file: $output_file,
                    error_output: $error_output,
                    duration_secs: ($duration | tonumber),
                    iteration: $iteration
                }')

            ALL_FAILURES+=("$failure_json")

            # Show error details immediately
            echo "SCRIPT executed in: (cd $working_dir; bash -e -o pipefail <<< '\$SCRIPT')" >&2

            # Fail fast if requested
            if [[ "$fail_fast" == "true" ]]; then
                echo "🛑 Fail-fast mode enabled, stopping"

                if [[ "$generate_summary" == "true" ]]; then
                    local failures_array=$(printf '%s\n' "${ALL_FAILURES[@]}" | jq -s .)
                    generate_run_matrix_summary \
                        "$package_name" \
                        "$working_dir" \
                        "$label" \
                        "$total_runs" \
                        "$total_passed" \
                        "$total_failed" \
                        "$failures_array"
                fi

                exit 1
            fi
        fi
    done

    # Generate summary
    if [[ "$generate_summary" == "true" ]]; then
        local failures_array=$(printf '%s\n' "${ALL_FAILURES[@]}" | jq -s .)
        generate_run_matrix_summary \
            "$package_name" \
            "$working_dir" \
            "$label" \
            "$total_runs" \
            "$total_passed" \
            "$total_failed" \
            "$failures_array"
    fi

    # Output to GitHub Actions
    echo "run-success=$([ $total_failed -eq 0 ] && echo true || echo false)" >> $GITHUB_OUTPUT
    echo "run-total=$total_runs" >> $GITHUB_OUTPUT
    echo "run-passed=$total_passed" >> $GITHUB_OUTPUT
    echo "run-failed=$total_failed" >> $GITHUB_OUTPUT

    # Output results JSON
    if [[ ${#ALL_FAILURES[@]} -gt 0 ]]; then
        local failures_array=$(printf '%s\n' "${ALL_FAILURES[@]}" | jq -s .)
        echo "run-results<<EOF" >> $GITHUB_OUTPUT
        echo "$failures_array" >> $GITHUB_OUTPUT
        echo "EOF" >> $GITHUB_OUTPUT
    else
        echo "run-results=[]" >> $GITHUB_OUTPUT
    fi

    # Exit with appropriate code
    # Note: We don't need to re-enable 'set -e' here because we exit explicitly
    if [[ "$total_failed" -gt 0 ]]; then
        echo "❌ Tests failed: $total_failed/$total_runs"
        exit 1
    else
        echo "✅ All tests passed: $total_runs/$total_runs"
        exit 0
    fi
}

# Flush accumulated summaries command
#
# Flushes all accumulated summary data from combined/group modes.
# This command is designed to be called as a separate workflow step
# after all run-matrix steps have completed.
#
# Environment Variables (inputs):
#   None required - automatically discovers all accumulated groups
#
# Side effects:
#   Writes accumulated summaries to $GITHUB_STEP_SUMMARY
#   Removes all state files
#
# Exit codes:
#   0: Successfully flushed all summaries (or no summaries to flush)
run_matrix_flush_command() {
    CONTEXT_PHASE="run-matrix-flush"
    echo "🧹 Flushing accumulated run-matrix summaries"

    # Check if state directory exists
    if [[ ! -d "$SUMMARY_STATE_DIR" ]]; then
        echo "ℹ️  No accumulated summaries to flush (state directory doesn't exist)"
        return 0
    fi

    # Prepare upload artifact BEFORE flushing (which deletes the files)
    local prepare_upload="${INPUT_RUN_MATRIX_PREPARE_UPLOAD:-false}"
    if [[ "$prepare_upload" == "true" ]]; then
        local package_name="${INPUT_RUN_MATRIX_UPLOAD_PACKAGE_NAME}"
        local package_os="${INPUT_RUN_MATRIX_UPLOAD_PACKAGE_OS}"

        if [[ -z "$package_name" || -z "$package_os" ]]; then
            echo "⚠️  Warning: prepare-upload enabled but package-name or package-os not provided"
        else
            # Create upload directory
            local upload_dir="${RUNNER_TEMP:-/tmp}/failure-upload"
            mkdir -p "$upload_dir"

            # Copy and rename the group_default.json file if it exists
            local source_file="$SUMMARY_STATE_DIR/group_default.json"
            if [[ -f "$source_file" ]]; then
                local target_file="$upload_dir/group_default_${package_name}_${package_os}.json"
                cp "$source_file" "$target_file"
                echo "📦 Prepared failure artifact: $target_file"

                # Output the path for the workflow to use
                echo "failure-artifact-path=$target_file" >> $GITHUB_OUTPUT
            else
                echo "ℹ️  No failure data to prepare for upload (group_default.json not found)"
                echo "failure-artifact-path=" >> $GITHUB_OUTPUT
            fi
        fi
    fi

    # Find all group state files and flush them
    local found_groups=false
    for state_file in "$SUMMARY_STATE_DIR"/group_*.json; do
        if [[ -f "$state_file" ]]; then
            found_groups=true
            local group_name=$(basename "$state_file" .json | sed 's/^group_//')
            echo "📝 Flushing summary group: $group_name"
            flush_combined_summary "$group_name"
        fi
    done

    if [[ "$found_groups" == "false" ]]; then
        echo "ℹ️  No accumulated summaries to flush"
    else
        echo "✅ Successfully flushed all accumulated summaries"
    fi

    # Cleanup state directory
    rm -rf "$SUMMARY_STATE_DIR"

    return 0
}

# Aggregate failures from multiple matrix jobs into a single workflow-wide summary
#
# Discovers all downloaded test result JSON artifacts and combines them into
# a single failures-only summary. This is designed to run in a separate job
# after all matrix jobs complete, providing a centralized view of all failures.
#
# Algorithm:
# 1. Find all group_*.json files in current directory (downloaded artifacts)
# 2. Parse each JSON file and extract packages with failures
# 3. Aggregate by package → category → failures
# 4. Calculate workflow-wide statistics
# 5. Generate single markdown summary showing only failures
#
# Environment:
#   GITHUB_STEP_SUMMARY: Path to GitHub Actions step summary file
#   INPUT_RUN_MATRIX_MAX_OUTPUT_LINES: Max lines of error output to show
#
# Side effects:
#   Writes to $GITHUB_STEP_SUMMARY
#
# Exit codes:
#   0: Success (summary generated or no failures found)
#   1: Error discovering or parsing artifacts
run_matrix_aggregate_failures_command() {
    CONTEXT_PHASE="run-matrix-aggregate-failures"
    echo "🔥 Aggregating failures from all matrix jobs"

    # Change to working directory if specified
    if [[ -n "${INPUT_RUN_MATRIX_WORKING_DIRECTORY}" ]]; then
        if [[ ! -d "${INPUT_RUN_MATRIX_WORKING_DIRECTORY}" ]]; then
            echo "ℹ️  Working directory doesn't exist: ${INPUT_RUN_MATRIX_WORKING_DIRECTORY}"
            echo "ℹ️  No test failure artifacts were found - all tests may have passed!"
            echo "# ✅ Workflow-Wide Test Results" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY
            echo "**Status**: All tests passed! No failures to report." >> $GITHUB_STEP_SUMMARY
            return 0
        fi
        echo "📂 Changing to working directory: ${INPUT_RUN_MATRIX_WORKING_DIRECTORY}"
        cd "${INPUT_RUN_MATRIX_WORKING_DIRECTORY}"
    fi

    # Determine auto-expand behavior
    local auto_expand="${INPUT_RUN_MATRIX_SUMMARY_AUTO_EXPAND:-failures}"
    local package_details="<details>"
    local category_details="<details>"
    local error_details="<details>"

    case "$auto_expand" in
        "always")
            package_details="<details open>"
            category_details="<details open>"
            error_details="<details open>"
            ;;
        "failures")
            # Since this is failures-only summary, expand by default
            package_details="<details open>"
            category_details="<details open>"
            error_details="<details open>"
            ;;
        "never")
            # User wants everything collapsed
            package_details="<details>"
            category_details="<details>"
            error_details="<details>"
            ;;
        *)
            # Default to failures behavior
            package_details="<details open>"
            category_details="<details open>"
            error_details="<details open>"
            ;;
    esac

    # Find all group JSON files in current directory
    # Pattern: group_default_*.json (includes package name in filename)
    local json_files=()
    for file in group_default_*.json; do
        if [[ -f "$file" ]]; then
            json_files+=("$file")
        fi
    done

    if [[ ${#json_files[@]} -eq 0 ]]; then
        echo "ℹ️  No test result artifacts found - all tests may have passed!"
        echo "# ✅ Workflow-Wide Test Results" >> $GITHUB_STEP_SUMMARY
        echo "" >> $GITHUB_STEP_SUMMARY
        echo "**Status**: All tests passed! No failures to report." >> $GITHUB_STEP_SUMMARY
        return 0
    fi

    echo "📂 Found ${#json_files[@]} test result artifact(s)"

    # Parse all JSON files and collect packages with failures
    # Use temp files to avoid "Argument list too long" errors with large data
    local package_files=()
    local total_packages=0
    local packages_with_failures=0
    local total_failures=0

    for json_file in "${json_files[@]}"; do
        echo "  Processing: $json_file"

        # Read the JSON file (it's an array of run results)
        local file_content
        if ! file_content=$(cat "$json_file" 2>/dev/null); then
            echo "⚠️  Warning: Failed to read $json_file, skipping"
            continue
        fi

        # Validate JSON
        if ! echo "$file_content" | jq empty 2>/dev/null; then
            echo "⚠️  Warning: Invalid JSON in $json_file, skipping"
            continue
        fi

        # Extract packages with failures from this file
        local packages_in_file=$(echo "$file_content" | jq -c '[.[] | select(.total_failed > 0)]')
        local failure_count=$(echo "$packages_in_file" | jq 'length')

        if [[ "$failure_count" -gt 0 ]]; then
            echo "    Found $failure_count package(s) with failures"
            # Save to temp file instead of building large string
            local temp_file=$(mktemp)
            echo "$packages_in_file" > "$temp_file"
            package_files+=("$temp_file")
            packages_with_failures=$((packages_with_failures + failure_count))
        fi

        # Count all packages and failures
        local file_total=$(echo "$file_content" | jq 'length')
        local file_failures=$(echo "$file_content" | jq '[.[].total_failed] | add // 0')
        total_packages=$((total_packages + file_total))
        total_failures=$((total_failures + file_failures))
    done

    # Merge all failure files at once using jq
    local all_packages_json="[]"
    if [[ ${#package_files[@]} -gt 0 ]]; then
        all_packages_json=$(jq -c -s 'flatten' "${package_files[@]}")
        rm -f "${package_files[@]}"
    fi

    echo "📊 Summary: $total_failures total failures across $packages_with_failures packages"

    if [[ "$packages_with_failures" -eq 0 ]]; then
        echo "# ✅ Workflow-Wide Test Results" >> $GITHUB_STEP_SUMMARY
        echo "" >> $GITHUB_STEP_SUMMARY
        echo "**Status**: All tests passed across all $total_packages test categories!" >> $GITHUB_STEP_SUMMARY
        return 0
    fi

    # Generate summary header
    echo "# 🔥 Workflow-Wide Failures Summary" >> $GITHUB_STEP_SUMMARY
    echo "" >> $GITHUB_STEP_SUMMARY

    local packages_passed=$((total_packages - packages_with_failures))
    echo "**Total**: $total_failures failures across $packages_with_failures $(if [[ $packages_with_failures -eq 1 ]]; then echo "category"; else echo "categories"; fi)" >> $GITHUB_STEP_SUMMARY
    if [[ "$packages_passed" -gt 0 ]]; then
        echo "($packages_passed $(if [[ $packages_passed -eq 1 ]]; then echo "category"; else echo "categories"; fi) passed, not shown)" >> $GITHUB_STEP_SUMMARY
    fi
    echo "" >> $GITHUB_STEP_SUMMARY

    # Group packages by package name and write each as a section
    echo "$all_packages_json" | jq -c 'group_by(.package) | .[]' | while IFS= read -r package_group; do
        local package_name=$(echo "$package_group" | jq -r '.[0].package')
        local package_total_failures=$(echo "$package_group" | jq '[.[].total_failed] | add')

        echo "$package_details" >> $GITHUB_STEP_SUMMARY
        echo "<summary><b>📦 $package_name</b> - ❌ $package_total_failures $(if [[ $package_total_failures -eq 1 ]]; then echo "failure"; else echo "failures"; fi)</summary>" >> $GITHUB_STEP_SUMMARY
        echo "" >> $GITHUB_STEP_SUMMARY

        # Write each category (label) within this package
        echo "$package_group" | jq -c '.[]' | while IFS= read -r run_json; do
            local label=$(echo "$run_json" | jq -r '.label')
            local working_dir=$(echo "$run_json" | jq -r '.working_dir')
            local total_failed=$(echo "$run_json" | jq -r '.total_failed')
            local failures=$(echo "$run_json" | jq -c '.failures')

            echo "$category_details" >> $GITHUB_STEP_SUMMARY
            echo "<summary><b>🔴 $label</b> - $total_failed $(if [[ $total_failed -eq 1 ]]; then echo "failure"; else echo "failures"; fi)</summary>" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY
            echo "**Working Directory:** \`$working_dir\`" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY

            # Write each failure
            echo "$failures" | jq -c '.[]' | while IFS= read -r failure_json; do
                local cmd=$(echo "$failure_json" | jq -r '.command')
                local features=$(echo "$failure_json" | jq -r '.feature_combo')
                local exit_code=$(echo "$failure_json" | jq -r '.exit_code')
                local duration=$(echo "$failure_json" | jq -r '.duration_secs')
                local error_output=$(echo "$failure_json" | jq -r '.error_output // empty')

                echo "##### 🔴 Features: \`$features\`" >> $GITHUB_STEP_SUMMARY
                echo "" >> $GITHUB_STEP_SUMMARY
                echo "**Exit Code:** $exit_code  " >> $GITHUB_STEP_SUMMARY
                echo "**Duration:** ${duration}s" >> $GITHUB_STEP_SUMMARY
                echo "" >> $GITHUB_STEP_SUMMARY

                # Script (collapsible)
                echo "<details>" >> $GITHUB_STEP_SUMMARY
                echo "<summary><b>📋 Script</b></summary>" >> $GITHUB_STEP_SUMMARY
                echo "" >> $GITHUB_STEP_SUMMARY
                echo "\`\`\`bash" >> $GITHUB_STEP_SUMMARY
                echo "$cmd" >> $GITHUB_STEP_SUMMARY
                echo "\`\`\`" >> $GITHUB_STEP_SUMMARY
                echo "" >> $GITHUB_STEP_SUMMARY
                echo "</details>" >> $GITHUB_STEP_SUMMARY
                echo "" >> $GITHUB_STEP_SUMMARY

                # Debug reproduction commands (collapsible)
                local debug_shells="${INPUT_RUN_MATRIX_DEBUG_SHELLS:-bash}"
                echo "<details>" >> $GITHUB_STEP_SUMMARY
                echo "<summary><b>🔄 Reproduce Locally</b></summary>" >> $GITHUB_STEP_SUMMARY
                echo "" >> $GITHUB_STEP_SUMMARY

                # Parse comma-separated shell list and generate commands for each
                IFS=',' read -ra SHELLS <<< "$debug_shells"
                for shell in "${SHELLS[@]}"; do
                    # Trim whitespace
                    shell=$(echo "$shell" | xargs)

                    # Capitalize shell name for display (bash 3.2 compatible)
                    local first_char="$(echo "${shell:0:1}" | tr '[:lower:]' '[:upper:]')"
                    local shell_display="${first_char}${shell:1}"

                    # Generate debug command for this shell
                    local debug_cmd=$(get_debug_command "$shell" "$working_dir" "$cmd")

                    echo "**$shell_display:**" >> $GITHUB_STEP_SUMMARY
                    echo "\`\`\`$shell" >> $GITHUB_STEP_SUMMARY
                    echo "$debug_cmd" >> $GITHUB_STEP_SUMMARY
                    echo "\`\`\`" >> $GITHUB_STEP_SUMMARY
                    echo "" >> $GITHUB_STEP_SUMMARY
                done

                echo "</details>" >> $GITHUB_STEP_SUMMARY
                echo "" >> $GITHUB_STEP_SUMMARY

                # Error output
                echo "$error_details" >> $GITHUB_STEP_SUMMARY
                echo "<summary><b>❌ Error Output</b></summary>" >> $GITHUB_STEP_SUMMARY
                echo "" >> $GITHUB_STEP_SUMMARY
                echo "\`\`\`" >> $GITHUB_STEP_SUMMARY

                if [[ -n "$error_output" ]]; then
                    # Use embedded error output from JSON
                    local max_lines="${INPUT_RUN_MATRIX_MAX_OUTPUT_LINES:-200}"
                    if [[ "$max_lines" -gt 0 ]]; then
                        echo "$error_output" | tail -"$max_lines" >> $GITHUB_STEP_SUMMARY
                    else
                        echo "$error_output" >> $GITHUB_STEP_SUMMARY
                    fi
                else
                    echo "Error: Output not available" >> $GITHUB_STEP_SUMMARY
                fi

                echo "\`\`\`" >> $GITHUB_STEP_SUMMARY
                echo "" >> $GITHUB_STEP_SUMMARY
                echo "</details>" >> $GITHUB_STEP_SUMMARY
                echo "" >> $GITHUB_STEP_SUMMARY
            done

            echo "</details>" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY
        done

        echo "</details>" >> $GITHUB_STEP_SUMMARY
        echo "" >> $GITHUB_STEP_SUMMARY
    done

    echo "✅ Successfully generated workflow-wide failures summary"
    return 0
}

# =============================================================================
# End of run-matrix Command Implementation
# =============================================================================
