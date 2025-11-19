#!/bin/bash

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
#
# Dependencies:
#   - lib/template.sh: Template rendering and strategy functions
#
# Usage:
#   run_matrix_command  # Reads from INPUT_RUN_MATRIX_* environment variables
#
# =============================================================================

# Generate GitHub Actions summary for run-matrix results
#
# Creates a comprehensive markdown summary including:
# - Statistics table (total/passed/failed)
# - Detailed failure information for each failed run
# - Full error output (last N lines)
# - Command reproduction instructions
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
#
# Side effects:
#   Appends to $GITHUB_STEP_SUMMARY file
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

    local title="Test Results: $package_name"
    [[ -n "$label" ]] && title="$label: $package_name"

    echo "## 🧪 $title" >> $GITHUB_STEP_SUMMARY
    echo "" >> $GITHUB_STEP_SUMMARY
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

            # Command (collapsible)
            echo "<details>" >> $GITHUB_STEP_SUMMARY
            echo "<summary><b>📋 Command</b></summary>" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY
            echo "\`\`\`bash" >> $GITHUB_STEP_SUMMARY
            echo "(cd $working_dir; $cmd)" >> $GITHUB_STEP_SUMMARY
            echo "\`\`\`" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY
            echo "</details>" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY

            # Error output (expanded by default)
            echo "<details open>" >> $GITHUB_STEP_SUMMARY
            echo "<summary><b>❌ Error Output</b></summary>" >> $GITHUB_STEP_SUMMARY
            echo "" >> $GITHUB_STEP_SUMMARY
            echo "\`\`\`" >> $GITHUB_STEP_SUMMARY
            if [[ -f "$output_file" ]]; then
                local max_lines="${INPUT_RUN_MATRIX_MAX_OUTPUT_LINES:-200}"
                if [[ "$max_lines" -gt 0 ]]; then
                    tail -"$max_lines" "$output_file" >> $GITHUB_STEP_SUMMARY
                else
                    cat "$output_file" >> $GITHUB_STEP_SUMMARY
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
#   INPUT_RUN_MATRIX_VERBOSE: Show all output (default: false)
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
    echo "🧪 Running matrix tests with template-based commands"

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
    local max_output_lines="${INPUT_RUN_MATRIX_MAX_OUTPUT_LINES:-200}"
    local working_dir="${INPUT_RUN_MATRIX_WORKING_DIRECTORY}"
    local label="${INPUT_RUN_MATRIX_LABEL}"
    local generate_summary="${INPUT_RUN_MATRIX_GENERATE_SUMMARY:-true}"

    # Parse package JSON
    local package_name=$(echo "$package_json" | jq -r '.name')
    local package_path=$(echo "$package_json" | jq -r '.path // "."')
    local features=$(echo "$package_json" | jq -c '.features // []')
    local required_features=$(echo "$package_json" | jq -r '.requiredFeatures // ""')

    # Use custom working directory or default to package path
    working_dir="${working_dir:-$package_path}"

    echo "📦 Package: $package_name"
    echo "📂 Working Directory: $working_dir"
    echo "🎯 Strategy: $strategy"
    [[ -n "$label" ]] && echo "🏷️  Label: $label"

    # Parse commands (newline or comma-separated)
    mapfile -t COMMAND_TEMPLATES < <(echo "$commands" | tr ',' '\n' | sed '/^[[:space:]]*$/d' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')

    echo "🔧 Commands to run:"
    for cmd_template in "${COMMAND_TEMPLATES[@]}"; do
        echo "  - $cmd_template"
    done

    # Generate feature combinations based on strategy
    mapfile -t FEATURE_COMBINATIONS < <(generate_feature_combinations "$features" "$strategy")

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

        # Run each command template for this feature combination
        for cmd_template in "${COMMAND_TEMPLATES[@]}"; do
            # Resolve template variables
            local command=$(render_template \
                "$cmd_template" \
                "$package_json" \
                "$feature_combo" \
                "$required_features" \
                "$iteration" \
                "$total_iterations")

            # Check for doctest and lib target
            if [[ "$command" =~ "test --doc" && "$INPUT_RUN_MATRIX_SKIP_DOCTEST_CHECK" != "true" ]]; then
                local package_path_clean="${package_path#./}"
                if ! cargo metadata --format-version=1 --no-deps 2>/dev/null | \
                    jq -e ".packages[] | select(.manifest_path | contains(\"${package_path_clean}/Cargo.toml\")) | .targets[] | select(.kind[] | contains(\"lib\"))" > /dev/null 2>&1; then
                    echo "⏭️  Skipping doctest - no library target found"
                    continue
                fi
            fi

            total_runs=$((total_runs + 1))

            # Create temp file for output
            local output_file=$(mktemp)
            local start_time=$(date +%s.%N)

            echo "RUNNING \`$command\`"

            # Execute command
            local exit_code=0
            if [[ "$verbose" == "true" ]]; then
                # Show output in real-time for verbose mode
                if (cd "$working_dir" && eval "$command" 2>&1 | tee "$output_file"); then
                    echo "✅ SUCCESS \`$command\`"
                else
                    exit_code=$?
                    echo "❌ FAILED \`$command\` (exit code: $exit_code)" >&2
                fi
            else
                # Capture output silently
                if (cd "$working_dir" && eval "$command" > "$output_file" 2>&1); then
                    echo "✅ SUCCESS \`$command\`"
                else
                    exit_code=$?
                    echo "❌ FAILED \`$command\` (exit code: $exit_code)" >&2
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

                # Create failure JSON
                local failure_json=$(jq -n \
                    --arg pkg "$package_name" \
                    --arg path "$package_path" \
                    --arg features "$feature_combo" \
                    --arg req_features "$required_features" \
                    --arg cmd "$command" \
                    --arg cmd_template "$cmd_template" \
                    --argjson exit_code "$exit_code" \
                    --arg output_file "$output_file" \
                    --arg duration "$duration" \
                    --argjson iteration "$iteration" \
                    '{
                        package: $pkg,
                        path: $path,
                        feature_combo: $features,
                        required_features: $req_features,
                        command: $cmd,
                        command_template: $cmd_template,
                        exit_code: $exit_code,
                        output_file: $output_file,
                        duration_secs: ($duration | tonumber),
                        iteration: $iteration
                    }')

                ALL_FAILURES+=("$failure_json")

                # Show error details immediately
                echo "COMMAND: (cd $working_dir; $command)" >&2

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
    if [[ "$total_failed" -gt 0 ]]; then
        echo "❌ Tests failed: $total_failed/$total_runs"
        exit 1
    else
        echo "✅ All tests passed: $total_runs/$total_runs"
        exit 0
    fi
}

# =============================================================================
# End of run-matrix Command Implementation
# =============================================================================
