#!/bin/bash
set -e

# Change to repository root (clippier needs to run from repo root for git operations)
cd "${GITHUB_WORKSPACE}"

CLIPPIER_BIN="${GITHUB_WORKSPACE}/target/release/clippier"
GIT_BASE=""
GIT_HEAD=""

# Skip clippier binary check for setup command (doesn't need clippier)
if [[ "$INPUT_COMMAND" != "setup" ]]; then
    if [[ ! -f "$CLIPPIER_BIN" ]]; then
        echo "Error: clippier binary not found at $CLIPPIER_BIN"
        exit 1
    fi
fi

detect_git_range() {
    local strategy="${INPUT_GIT_STRATEGY:-auto}"

    if [[ "$strategy" == "manual" ]]; then
        GIT_BASE="$INPUT_GIT_BASE"
        GIT_HEAD="$INPUT_GIT_HEAD"
        echo "Using manual git range: $GIT_BASE -> $GIT_HEAD"
        return
    fi

    if [[ "$strategy" == "branch-comparison" ]]; then
        local compare_branch="${INPUT_GIT_COMPARE_BRANCH:-${INPUT_GIT_DEFAULT_BRANCH}}"
        local target="origin/$compare_branch"

        echo "🔀 Branch comparison mode: comparing HEAD against $target"

        # Ensure target branch exists
        if ! git rev-parse --verify "$target" >/dev/null 2>&1; then
            echo "⚠️  Target branch $target not found locally, fetching..."
            if git fetch origin "$compare_branch" 2>/dev/null; then
                echo "✅ Fetched $compare_branch from origin"
            else
                echo "❌ Failed to fetch $compare_branch, falling back to HEAD~1"
                GIT_BASE="HEAD~1"
                GIT_HEAD="HEAD"
                return
            fi
        fi

        # Find merge-base (common ancestor)
        local merge_base=$(git merge-base HEAD "$target" 2>/dev/null)

        if [[ -z "$merge_base" ]]; then
            echo "⚠️  No common ancestor found, using target branch directly"
            GIT_BASE="$target"
        else
            echo "✅ Found common ancestor: $merge_base"
            GIT_BASE="$merge_base"
        fi

        GIT_HEAD="HEAD"
        echo "Comparing: $GIT_BASE -> $GIT_HEAD"
        return
    fi

    local event_name="${GITHUB_EVENT_NAME:-$INPUT_SUMMARY_EVENT_NAME}"

    echo "🔍 Auto-detecting git range for event: $event_name"

    if [[ "$event_name" == "pull_request" ]]; then
        GIT_BASE="${GITHUB_BASE_REF:-HEAD~1}"
        GIT_HEAD="${GITHUB_SHA:-HEAD}"
        echo "Pull request: comparing $GIT_BASE -> $GIT_HEAD"

    elif [[ "$event_name" == "push" ]]; then
        local before="${GITHUB_EVENT_BEFORE:-}"
        local after="${GITHUB_SHA:-HEAD}"
        local forced="${GITHUB_EVENT_FORCED:-false}"

        if [[ "$before" == "0000000000000000000000000000000000000000" ]]; then
            echo "New branch detected"
            GIT_BASE="origin/${INPUT_GIT_DEFAULT_BRANCH}"
            GIT_HEAD="$after"
            echo "Comparing against default branch: $GIT_BASE -> $GIT_HEAD"

        elif [[ "$forced" == "true" ]] || ! git cat-file -e "$before" 2>/dev/null; then
            echo "Force push or invalid commit detected"

            if [[ "$strategy" == "workflow-history" || "$strategy" == "auto" ]]; then
                echo "Attempting to find valid commit from workflow history..."
                local valid_commit=$(find_valid_commit_from_api)

                if [[ -n "$valid_commit" && "$valid_commit" != "origin/${INPUT_GIT_DEFAULT_BRANCH}" ]]; then
                    GIT_BASE="$valid_commit"
                    GIT_HEAD="$after"
                    echo "Found valid commit: $GIT_BASE -> $GIT_HEAD"
                else
                    GIT_BASE="origin/${INPUT_GIT_DEFAULT_BRANCH}"
                    GIT_HEAD="$after"
                    echo "Fallback to default branch: $GIT_BASE -> $GIT_HEAD"
                fi
            else
                GIT_BASE="origin/${INPUT_GIT_DEFAULT_BRANCH}"
                GIT_HEAD="$after"
                echo "Fallback to default branch: $GIT_BASE -> $GIT_HEAD"
            fi
        else
            GIT_BASE="$before"
            GIT_HEAD="$after"
            echo "Normal push: $GIT_BASE -> $GIT_HEAD"
        fi

    else
        GIT_BASE="HEAD~1"
        GIT_HEAD="HEAD"
        echo "Workflow dispatch/schedule: $GIT_BASE -> $GIT_HEAD"
    fi
}

find_valid_commit_from_api() {
    local workflow_name="${INPUT_GIT_WORKFLOW_NAME:-}"

    if [[ -z "$workflow_name" && -n "$GITHUB_WORKFLOW_REF" ]]; then
        workflow_name=$(basename "${GITHUB_WORKFLOW_REF%@*}")
    fi

    if [[ -z "$workflow_name" ]]; then
        echo "Warning: Could not determine workflow name for API lookup" >&2
        echo "origin/${INPUT_GIT_DEFAULT_BRANCH}"
        return
    fi

    echo "Searching workflow history for: $workflow_name" >&2

    local before_commit="${GITHUB_EVENT_BEFORE:-}"
    local page=1
    local per_page=30
    local max_pages=10  # Check up to 300 workflow runs
    local total_checked=0

    while [[ $page -le $max_pages ]]; do
        echo "Fetching workflow runs page $page..." >&2

        local workflow_runs=$(curl -s \
            -H "Authorization: Bearer ${INPUT_GITHUB_TOKEN}" \
            -H "Accept: application/vnd.github.v3+json" \
            "https://api.github.com/repos/${INPUT_GITHUB_REPOSITORY}/actions/workflows/${workflow_name}/runs?branch=${INPUT_GITHUB_REF_NAME}&per_page=${per_page}&page=${page}")

        local run_count=$(printf '%s' "$workflow_runs" | jq '.workflow_runs | length')

        if [[ "$run_count" == "0" || "$run_count" == "null" ]]; then
            echo "No more workflow runs available (checked $total_checked total runs)" >&2
            break
        fi

        for i in $(seq 0 $((run_count - 1))); do
            local run_sha=$(printf '%s' "$workflow_runs" | jq -r ".workflow_runs[$i].head_sha // empty")

            if [[ -z "$run_sha" || "$run_sha" == "null" || "$run_sha" == "empty" ]]; then
                continue
            fi

            total_checked=$((total_checked + 1))

            if [[ "$run_sha" == "$GITHUB_SHA" ]]; then
                echo "Skipping current commit: $run_sha" >&2
                continue
            fi

            # Skip the BEFORE commit from force push event - it's likely stale/orphaned
            if [[ -n "$before_commit" && "$run_sha" == "$before_commit" ]]; then
                echo "Skipping BEFORE commit from force push: $run_sha" >&2
                continue
            fi

            echo "Checking if commit $run_sha is available in git history..." >&2
            if git cat-file -e "$run_sha" 2>/dev/null; then
                echo "Commit $run_sha exists, checking if it's an ancestor of HEAD..." >&2
                if git merge-base --is-ancestor "$run_sha" HEAD 2>/dev/null; then
                    echo "Found valid commit from workflow run: $run_sha (after checking $total_checked runs)" >&2
                    echo "$run_sha"
                    return
                else
                    echo "Commit $run_sha is not an ancestor of HEAD (likely orphaned from rebase/force push), trying next..." >&2
                fi
            else
                echo "Commit $run_sha not available in git history, trying next..." >&2
            fi
        done

        page=$((page + 1))
    done

    echo "No valid commits found in workflow history (checked $total_checked runs across $((page - 1)) pages)" >&2
    echo "origin/${INPUT_GIT_DEFAULT_BRANCH}"
}

should_force_full_matrix() {
    local condition="${INPUT_FORCE_FULL_MATRIX_CONDITION:-false}"

    if [[ "$condition" == "true" ]]; then
        echo "Force full matrix condition is true" >&2
        return 0
    fi

    return 1
}

should_skip_on_no_changes() {
    echo "🔍 Checking if should skip on no changes..." >&2
    echo "  INPUT_SKIP_ON_NO_CHANGES: $INPUT_SKIP_ON_NO_CHANGES" >&2
    echo "  INPUT_CHANGED_FILES: ${INPUT_CHANGED_FILES:-<empty>}" >&2
    echo "  GIT_BASE: ${GIT_BASE:-<empty>}" >&2
    echo "  GIT_HEAD: ${GIT_HEAD:-<empty>}" >&2

    if [[ "$INPUT_SKIP_ON_NO_CHANGES" != "true" ]]; then
        echo "  Skip-on-no-changes is disabled, will not skip" >&2
        return 1
    fi

    # If changed-files is provided and not empty, there are changes
    if [[ -n "$INPUT_CHANGED_FILES" ]]; then
        echo "  Changed files provided via input, will not skip" >&2
        return 1
    fi

    # If git range is available, check if there are actual changes
    if [[ -n "$GIT_BASE" && -n "$GIT_HEAD" ]]; then
        echo "  Running: git diff --name-only $GIT_BASE $GIT_HEAD" >&2
        local changed_count=$(git diff --name-only "$GIT_BASE" "$GIT_HEAD" 2>/dev/null | wc -l)
        echo "  Changed files count: $changed_count" >&2
        if [[ "$changed_count" -gt 0 ]]; then
            echo "  Detected $changed_count changed files, will not skip" >&2
            return 1
        fi
    else
        echo "  No git range available for checking changes" >&2
    fi

    echo "  No changes detected and skip-on-no-changes is enabled, will skip" >&2
    return 0
}

build_clippier_command() {
    local cmd="$CLIPPIER_BIN $INPUT_COMMAND"

    if [[ "$INPUT_COMMAND" == "features" ]]; then
        cmd="$cmd $INPUT_WORKSPACE_PATH"
        [[ -n "$INPUT_OS" ]] && cmd="$cmd --os $INPUT_OS"
        [[ -n "$INPUT_OFFSET" ]] && cmd="$cmd --offset $INPUT_OFFSET"
        [[ -n "$INPUT_MAX" ]] && cmd="$cmd --max $INPUT_MAX"
        [[ -n "$INPUT_MAX_PARALLEL" ]] && cmd="$cmd --max-parallel $INPUT_MAX_PARALLEL"
        [[ -n "$INPUT_CHUNKED" ]] && cmd="$cmd --chunked $INPUT_CHUNKED"
        [[ "$INPUT_SPREAD" == "true" ]] && cmd="$cmd --spread"
        [[ "$INPUT_RANDOMIZE" == "true" ]] && cmd="$cmd --randomize"
        [[ -n "$INPUT_SEED" ]] && cmd="$cmd --seed $INPUT_SEED"
        [[ -n "$INPUT_FEATURES" ]] && cmd="$cmd --features $INPUT_FEATURES"
        [[ -n "$INPUT_SKIP_FEATURES" ]] && cmd="$cmd --skip-features $INPUT_SKIP_FEATURES"
        [[ -n "$INPUT_REQUIRED_FEATURES" ]] && cmd="$cmd --required-features $INPUT_REQUIRED_FEATURES"
        [[ -n "$INPUT_PACKAGES" ]] && cmd="$cmd --packages $INPUT_PACKAGES"

        if ! should_force_full_matrix; then
            [[ -n "$INPUT_CHANGED_FILES" ]] && cmd="$cmd --changed-files \"$INPUT_CHANGED_FILES\""
            [[ -n "$GIT_BASE" ]] && cmd="$cmd --git-base \"$GIT_BASE\""
            [[ -n "$GIT_HEAD" ]] && cmd="$cmd --git-head \"$GIT_HEAD\""
        fi

        [[ "$INPUT_INCLUDE_REASONING" == "true" ]] && cmd="$cmd --include-reasoning"
        cmd="$cmd --output json"
    elif [[ "$INPUT_COMMAND" == "affected-packages" ]]; then
        cmd="$cmd $INPUT_WORKSPACE_PATH"
        [[ -n "$INPUT_CHANGED_FILES" ]] && cmd="$cmd --changed-files \"$INPUT_CHANGED_FILES\""
        [[ -n "$INPUT_TARGET_PACKAGE" ]] && cmd="$cmd --target-package $INPUT_TARGET_PACKAGE"
        [[ -n "$GIT_BASE" ]] && cmd="$cmd --git-base \"$GIT_BASE\""
        [[ -n "$GIT_HEAD" ]] && cmd="$cmd --git-head \"$GIT_HEAD\""
        [[ "$INPUT_INCLUDE_REASONING" == "true" ]] && cmd="$cmd --include-reasoning"
        cmd="$cmd --output json"
    elif [[ "$INPUT_COMMAND" == "workspace-deps" ]]; then
        cmd="$cmd $INPUT_WORKSPACE_PATH"
        [[ -n "$INPUT_PACKAGE" ]] && cmd="$cmd $INPUT_PACKAGE"
        [[ -n "$INPUT_FEATURES" ]] && cmd="$cmd --features $INPUT_FEATURES"
        [[ "$INPUT_ALL_POTENTIAL_DEPS" == "true" ]] && cmd="$cmd --all-potential-deps"
        cmd="$cmd --format json"
    elif [[ "$INPUT_COMMAND" == "validate-feature-propagation" ]]; then
        cmd="$cmd $INPUT_WORKSPACE_PATH"
        [[ -n "$INPUT_FEATURES" ]] && cmd="$cmd --features $INPUT_FEATURES"
        cmd="$cmd --output json"
    else
        echo "Error: Unknown command '$INPUT_COMMAND'"
        exit 1
    fi

    echo "$cmd"
}

run_clippier() {
    local cmd=$(build_clippier_command)
    echo "Running: $cmd" >&2
    eval "$cmd" | jq -c .
}

inject_custom_reasoning() {
    local output="$1"
    local reasoning="$INPUT_INJECT_REASONING"
    local condition="${INPUT_INJECT_REASONING_CONDITION:-true}"

    if [[ "$condition" == "true" && -n "$reasoning" ]]; then
        echo "🔄 Injecting custom reasoning" >&2

        # Convert multi-line string to JSON array, filtering empty/whitespace-only lines
        # test("\\S") matches any line containing at least one non-whitespace character
        local reasoning_array=$(printf '%s' "$reasoning" | jq -R -s -c 'split("\n") | map(select(test("\\S")))')

        # Add reasoning array to each package's existing reasoning
        printf '%s' "$output" | jq -c --argjson reasons "$reasoning_array" '
            map(
                . + {
                    reasoning: ((.reasoning // []) + $reasons)
                }
            )
        '
    else
        printf '%s' "$output"
    fi
}

transform_output() {
    local raw_output="$1"

    if [[ "$INPUT_COMMAND" != "features" ]]; then
        echo "$raw_output"
        return
    fi

    local transformed="$raw_output"

    local jq_filter='.'

    if [[ -n "$INPUT_TRANSFORM_NAME_REGEX" ]]; then
        jq_filter="$jq_filter | map(.name |= sub(\"$INPUT_TRANSFORM_NAME_REGEX\"; \"$INPUT_TRANSFORM_NAME_REPLACEMENT\"))"
    fi

    if [[ -n "$INPUT_OS_SUFFIX" ]]; then
        jq_filter="$jq_filter | map(if .os != null then .os = (.os + \"$INPUT_OS_SUFFIX\") else . end)"
    fi

    local properties_array="[\"$(echo "$INPUT_MATRIX_PROPERTIES" | sed 's/,/","/g')\"]"
    jq_filter="$jq_filter | map({
        \"name\": .name,
        \"path\": .path,
        \"features\": .features,
        \"requiredFeatures\": (if .requiredFeatures != null then .requiredFeatures | join(\",\") else null end),
        \"os\": .os,
        \"dependencies\": .dependencies,
        \"toolchains\": .toolchains,
        \"ciSteps\": .ciSteps,
        \"ciToolchains\": .ciToolchains,
        \"env\": (if .env != null then .env | gsub(\"\\n\";\" \") else null end),
        \"nightly\": .nightly,
        \"gitSubmodules\": .gitSubmodules
    } | with_entries(select(.key as \$k | $properties_array | index(\$k))) | del(.. | nulls))"

    printf '%s' "$raw_output" | jq -rc "$jq_filter"
}

run_additional_checks() {
    if [[ -z "$INPUT_ADDITIONAL_PACKAGE_CHECKS" || "$INPUT_ADDITIONAL_PACKAGE_CHECKS" == "null" ]]; then
        return
    fi

    echo "🔍 Running additional package checks..."

    local changed_files="$INPUT_CHANGED_FILES"
    local git_base="$GIT_BASE"
    local git_head="$GIT_HEAD"

    mkdir -p /tmp/clippier_checks

    # Initialize empty JSON object to collect all check results
    local additional_checks="{}"

    while IFS= read -r check; do
        local package=$(printf '%s' "$check" | jq -r '.package')
        local output_key=$(printf '%s' "$check" | jq -r '."output-key" // .package')

        echo "  Checking package: $package (output-key: $output_key)"

        local cmd="$CLIPPIER_BIN affected-packages $INPUT_WORKSPACE_PATH"
        cmd="$cmd --target-package $package"
        [[ -n "$changed_files" ]] && cmd="$cmd --changed-files \"$changed_files\""
        [[ -n "$git_base" ]] && cmd="$cmd --git-base \"$git_base\""
        [[ -n "$git_head" ]] && cmd="$cmd --git-head \"$git_head\""
        cmd="$cmd --include-reasoning --output json"

        local result=$(eval "$cmd" 2>&1 | jq -c .)
        local affected=$(printf '%s' "$result" | jq -r '.affected // false')

        # Add this check to the aggregated JSON
        additional_checks=$(printf '%s' "$additional_checks" | jq -c \
            --arg key "$output_key" \
            --argjson result "$result" \
            '. + {($key): $result}')

        printf '%s' "$check" > "/tmp/clippier_checks/check_${output_key}.json"
        printf '%s' "$result" > "/tmp/clippier_checks/result_${output_key}.json"

        echo "  ✅ $package is $([ "$affected" == "true" ] && echo "affected" || echo "not affected")"
    done < <(printf '%s' "$INPUT_ADDITIONAL_PACKAGE_CHECKS" | jq -c '.[]')

    # Output the complete JSON object containing all checks
    echo "additional-checks=$additional_checks" >> $GITHUB_OUTPUT
}

analyze_docker_packages() {
    local matrix="$1"
    local reasoning="$2"

    if [[ -z "$INPUT_DOCKER_PACKAGES" ]]; then
        echo "Error: enable-docker-analysis is true but docker-packages is not provided"
        exit 1
    fi

    local docker_matrix="[]"
    local packages_list=""

    while IFS= read -r pkg_name; do
        local full_pkg_name="${INPUT_DOCKER_NAME_PREFIX}${pkg_name}"

        local docker_info=$(printf '%s' "$INPUT_DOCKER_PACKAGES" | jq -r ".\"$full_pkg_name\" // empty")

        if [[ -n "$docker_info" && "$docker_info" != "null" && "$docker_info" != "empty" ]]; then
            local package_env=$(printf '%s' "$matrix" | jq -r --arg pkg "$pkg_name" '.[] | select(.name == $pkg) | .env // empty' | head -1)
            local package_git_submodules=$(printf '%s' "$matrix" | jq -r --arg pkg "$pkg_name" '.[] | select(.name == $pkg) | .gitSubmodules // empty' | head -1)

            local docker_entry=$(printf '%s' "$docker_info" | jq --arg env "$package_env" --arg submodules "$package_git_submodules" \
                '. + (if $env != "" and $env != "empty" then {env: $env} else {} end) + (if $submodules != "" and $submodules != "empty" then {gitSubmodules: ($submodules | test("true"))} else {} end)')

            docker_matrix=$(printf '%s' "$docker_matrix" | jq -c ". + [$docker_entry]")

            local pkg_display=$(printf '%s' "$docker_info" | jq -r '.name')

            # Create formatted entry with reasoning if available
            if [[ -n "$reasoning" && "$reasoning" != "null" && "$reasoning" != "" ]]; then
                # Get reasoning for this package (match against full package name)
                local package_reasoning=$(printf '%s' "$reasoning" | jq -r --arg pkg "$full_pkg_name" '[.[] | select(.name == $pkg) | .reasoning // []] | flatten | unique | map("  - " + .) | join("\n")')

                if [[ -n "$package_reasoning" && "$package_reasoning" != "" ]]; then
                    local docker_entry_text="- <details>\\n  <summary>$pkg_display</summary>\\n  \\n  **Why this package is affected:**\\n$package_reasoning\\n  </details>"
                else
                    local docker_entry_text="- $pkg_display"
                fi
            else
                local docker_entry_text="- $pkg_display"
            fi

            if [[ -z "$packages_list" ]]; then
                packages_list="$docker_entry_text"
            else
                packages_list="$packages_list\\n$docker_entry_text"
            fi
        fi
    done < <(printf '%s' "$matrix" | jq -r '[.[].name] | unique | .[]')

    local docker_count=$(printf '%s' "$docker_matrix" | jq 'length')

    if [[ "$docker_count" -eq 0 ]]; then
        echo '{"matrix": {"include": []}, "has_changes": false, "count": 0, "packages_list": "none"}'
    else
        local full_matrix="{\"include\": $docker_matrix}"
        echo "{\"matrix\": $full_matrix, \"has_changes\": true, \"count\": $docker_count, \"packages_list\": \"$packages_list\"}"
    fi
}

generate_summary() {
    local matrix="$1"
    local reasoning="$2"

    echo "## $INPUT_SUMMARY_TITLE" >> $GITHUB_STEP_SUMMARY
    echo "" >> $GITHUB_STEP_SUMMARY

    if [[ "$INPUT_SUMMARY_SHOW_TRIGGER" == "true" && -n "$INPUT_SUMMARY_EVENT_NAME" ]]; then
        case "$INPUT_SUMMARY_EVENT_NAME" in
            "workflow_dispatch")
                echo "🚀 **Trigger**: Manual workflow dispatch" >> $GITHUB_STEP_SUMMARY
                [[ -n "$INPUT_SUMMARY_TRIGGER_INPUT" ]] && echo "  - Selected: $INPUT_SUMMARY_TRIGGER_INPUT" >> $GITHUB_STEP_SUMMARY
                ;;
            "schedule")
                echo "⏰ **Trigger**: Scheduled run" >> $GITHUB_STEP_SUMMARY
                ;;
            "push")
                echo "📤 **Trigger**: Push to $INPUT_SUMMARY_REF_NAME" >> $GITHUB_STEP_SUMMARY
                ;;
            "pull_request")
                echo "🔀 **Trigger**: Pull request" >> $GITHUB_STEP_SUMMARY
                ;;
        esac
        echo "" >> $GITHUB_STEP_SUMMARY
    fi

    if [[ "$INPUT_SUMMARY_INCLUDE_SEED" == "true" && -n "$INPUT_SEED" ]]; then
        echo "🎲 **Randomization Seed**: $INPUT_SEED" >> $GITHUB_STEP_SUMMARY
        echo "" >> $GITHUB_STEP_SUMMARY
        echo "> To reproduce this exact matrix, run the workflow with seed: \`$INPUT_SEED\`" >> $GITHUB_STEP_SUMMARY
        echo "" >> $GITHUB_STEP_SUMMARY
    fi

    local matrix_length=$(printf '%s' "$matrix" | jq 'length')
    local packages_length=$(printf '%s' "$matrix" | jq '[.[].name] | unique | length')

    local job_plural="jobs"
    [[ "$matrix_length" -eq 1 ]] && job_plural="job"

    local package_plural="packages"
    [[ "$packages_length" -eq 1 ]] && package_plural="package"

    if [[ "$matrix_length" -gt 0 ]]; then
        echo "📊 **Build Matrix**: $matrix_length $job_plural for $packages_length $package_plural will be built/tested" >> $GITHUB_STEP_SUMMARY
        echo "" >> $GITHUB_STEP_SUMMARY
        echo "<details><summary>Affected packages</summary>" >> $GITHUB_STEP_SUMMARY
        echo "" >> $GITHUB_STEP_SUMMARY

        if [[ "$INPUT_SUMMARY_SHOW_JOBS_DETAILS" == "true" && -n "$reasoning" && "$reasoning" != "null" ]]; then
            # Complex JQ transformation that creates collapsible sections with full job details
            printf '%s' "$matrix" | jq -r --argjson reasoning "$reasoning" '
                # Group packages and collect all job details
                group_by(.name) |
                map({
                    name: .[0].name,
                    job_count: length,
                    jobs: .
                }) |
                # Add reasoning by matching package names
                # Reasoning has original full names, matrix has transformed names
                map(
                    . as $pkg |
                    $pkg + {
                        reasoning: (
                            $reasoning |
                            map(select(
                                # Match if reasoning name equals matrix name
                                .name == $pkg.name or
                                # Or if reasoning name without prefix equals matrix name
                                (.name | sub("^(moosicbox|switchy|hyperchad)_"; "")) == $pkg.name
                            )) |
                            map(.reasoning // []) |
                            flatten |
                            unique
                        )
                    }
                ) |
                # Generate markdown output with collapsible job details
                map(
                    if (.reasoning | length) > 0 then
                        "<details>\n  <summary>" + .name + " (" + (.job_count | tostring) + " job" + (if .job_count > 1 then "s" else "" end) + ")</summary>\n  \n" +
                        "  **Why this package is affected:**\n" +
                        (.reasoning | map("  - " + .) | join("\n")) +
                        "\n  \n  **Jobs to run:**\n" +
                        (.jobs | map("  - **" + .os + "** " + (if .nightly then "(nightly)" else "(stable)" end) + "\n    - Features: `" + (.features | join("`, `")) + "`" + (if .requiredFeatures then "\n    - Required Features: `" + .requiredFeatures + "`" else "" end)) | join("\n")) +
                        "\n  </details>"
                    else
                        .name + " (" + (.job_count | tostring) + " job" + (if .job_count > 1 then "s" else "" end) + ")"
                    end
                ) |
                join("\n")
            ' >> $GITHUB_STEP_SUMMARY
        else
            # Fallback to simple list without reasoning
            printf '%s' "$matrix" | jq -r 'group_by(.name) | map("- \(.[0].name) (\(length) job\(if length > 1 then "s" else "" end))") | .[]' >> $GITHUB_STEP_SUMMARY
        fi

        echo "</details>" >> $GITHUB_STEP_SUMMARY
    else
        echo "🎉 **No Changes**: No packages affected - builds will be skipped!" >> $GITHUB_STEP_SUMMARY
    fi
}

generate_additional_check_summary() {
    for check_file in /tmp/clippier_checks/check_*.json; do
        if [[ ! -f "$check_file" ]]; then
            continue
        fi

        local output_key=$(basename "$check_file" | sed 's/check_\(.*\)\.json/\1/')
        local check_config=$(cat "$check_file")
        local result=$(cat "/tmp/clippier_checks/result_${output_key}.json")

        local summary_config=$(printf '%s' "$check_config" | jq -r '."summary-section" // {}')

        if [[ "$summary_config" == "null" || "$summary_config" == "{}" ]]; then
            continue
        fi

        local title=$(printf '%s' "$summary_config" | jq -r '.title')
        local show_reasoning=$(printf '%s' "$summary_config" | jq -r '."show-reasoning" // true')
        local show_all_affected=$(printf '%s' "$summary_config" | jq -r '."show-all-affected" // true')
        local affected=$(printf '%s' "$result" | jq -r '.affected // false')

        echo "" >> $GITHUB_STEP_SUMMARY
        echo "## $title" >> $GITHUB_STEP_SUMMARY
        echo "" >> $GITHUB_STEP_SUMMARY

        local status_labels=$(printf '%s' "$summary_config" | jq -r '."status-labels" // {}')
        if [[ "$affected" == "true" ]]; then
            local label=$(printf '%s' "$status_labels" | jq -r '.affected // "Affected"')
            echo "✅ **Status**: $label" >> $GITHUB_STEP_SUMMARY
        else
            local label=$(printf '%s' "$status_labels" | jq -r '."not-affected" // "Not affected"')
            echo "⏭️ **Status**: $label" >> $GITHUB_STEP_SUMMARY
        fi

        if [[ "$show_reasoning" == "true" ]]; then
            local reasoning=$(printf '%s' "$result" | jq -r '.reasoning // []')

            if [[ "$reasoning" != "[]" && "$reasoning" != "null" ]]; then
                echo "" >> $GITHUB_STEP_SUMMARY
                echo "<details><summary>Why this package is affected</summary>" >> $GITHUB_STEP_SUMMARY
                echo "" >> $GITHUB_STEP_SUMMARY
                printf '%s' "$reasoning" | jq -r '.[]' | sed 's/^/  - /' >> $GITHUB_STEP_SUMMARY

                if [[ "$show_all_affected" == "true" ]]; then
                    local all_affected=$(printf '%s' "$result" | jq -r '.all_affected // []')

                    if [[ "$all_affected" != "[]" && "$all_affected" != "null" ]]; then
                        echo "" >> $GITHUB_STEP_SUMMARY
                        echo "**All affected packages in dependency chain:**" >> $GITHUB_STEP_SUMMARY
                        printf '%s' "$all_affected" | jq -r '.[] | "  • " + .name + (if .reasoning and (.reasoning | length) > 0 then "\n" + (.reasoning | map("    ○ " + .) | join("\n")) else "" end)' >> $GITHUB_STEP_SUMMARY
                    fi
                fi

                echo "</details>" >> $GITHUB_STEP_SUMMARY
            fi
        fi
    done
}

generate_docker_summary() {
    local docker_result="$1"

    echo "" >> $GITHUB_STEP_SUMMARY
    echo "## 🐳 Docker Build Summary" >> $GITHUB_STEP_SUMMARY
    echo "" >> $GITHUB_STEP_SUMMARY

    local has_changes=$(printf '%s' "$docker_result" | jq -r '.has_changes')
    local docker_count=$(printf '%s' "$docker_result" | jq -r '.count')
    local packages_list=$(printf '%s' "$docker_result" | jq -r '.packages_list')

    if [[ "$has_changes" == "true" && "$docker_count" -gt 0 ]]; then
        local docker_plural="image"
        [[ "$docker_count" -gt 1 ]] && docker_plural="images"

        echo "🐳 **Docker Images**: $docker_count Docker $docker_plural will be built" >> $GITHUB_STEP_SUMMARY
        echo "" >> $GITHUB_STEP_SUMMARY
        echo "**Docker images to build:**" >> $GITHUB_STEP_SUMMARY
        echo -e "$packages_list" >> $GITHUB_STEP_SUMMARY
    else
        echo "✨ **Docker Images**: No Docker-enabled packages affected - Docker builds will be skipped!" >> $GITHUB_STEP_SUMMARY
    fi
}

setup_ci_environment() {
    echo "🔧 Setting up CI environment"

    if [[ -z "$INPUT_PACKAGE_JSON" ]]; then
        echo "❌ ERROR: package-json input is required for setup command"
        exit 1
    fi

    local package_json="$INPUT_PACKAGE_JSON"

    local name=$(echo "$package_json" | jq -r '.name // ""')
    local path=$(echo "$package_json" | jq -r '.path // "."')
    local os=$(echo "$package_json" | jq -r '.os // "ubuntu-latest"')
    local git_submodules=$(echo "$package_json" | jq -r '.gitSubmodules // false')
    local toolchains=$(echo "$package_json" | jq -r '.toolchains // [] | @json')
    local ci_toolchains=$(echo "$package_json" | jq -r '.ciToolchains // [] | @json')
    local ci_steps=$(echo "$package_json" | jq -r '.ciSteps // ""')
    local dependencies=$(echo "$package_json" | jq -r '.dependencies // ""')
    local env_vars=$(echo "$package_json" | jq -r '.env // ""')

    echo "📦 Package: $name"
    echo "📂 Path: $path"
    echo "🖥️  OS: $os"

    local needs_free_disk_space=false

    if echo "$toolchains" | jq -e 'contains(["free_disk_space"])' >/dev/null 2>&1 || \
       echo "$ci_toolchains" | jq -e 'contains(["free_disk_space"])' >/dev/null 2>&1; then
        needs_free_disk_space=true
    fi

    if [[ "$needs_free_disk_space" == "true" && "$os" == "ubuntu-latest" ]]; then
        echo "⚠️  Note: free_disk_space toolchain detected. Please add jlumbroso/free-disk-space@main action before clippier setup in your workflow"
    fi

    if [[ "$INPUT_SKIP_CHECKOUT" == "true" && "$git_submodules" == "true" ]]; then
        echo "🔀 Initializing git submodules (checkout was skipped)"
        git submodule update --init --recursive
    fi

    if [[ -n "$env_vars" ]]; then
        echo "🌍 Exporting environment variables to GITHUB_ENV"
        echo "$env_vars" | tr ' ' '\n' | while IFS='=' read -r key value; do
            if [[ -n "$key" && -n "$value" ]]; then
                echo "  $key=$value"
                echo "$key=$value" >> "$GITHUB_ENV"
            fi
        done
    fi

    if [[ -n "$ci_steps" ]]; then
        echo "⚙️  Running CI setup steps"
        eval "$ci_steps"
    fi

    if [[ -n "$dependencies" ]]; then
        echo "📥 Installing dependencies"
        eval "$dependencies"
    fi

    echo "✅ CI environment setup completed"
}

main() {
    echo "🚀 Running clippier action for command: $INPUT_COMMAND"

    if [[ "$INPUT_COMMAND" == "setup" ]]; then
        setup_ci_environment
        return
    fi

    detect_git_range

    echo "git-base=$GIT_BASE" >> $GITHUB_OUTPUT
    echo "git-head=$GIT_HEAD" >> $GITHUB_OUTPUT

    # Check force-full-matrix BEFORE skip-on-no-changes
    # This ensures workflow_dispatch/schedule events build all packages
    if should_force_full_matrix; then
        echo "🚀 Force full matrix mode enabled - will analyze all packages regardless of changes"
    elif should_skip_on_no_changes; then
        echo "⏭️ Skipping - no changes detected and not in force-full-matrix mode"
        echo "matrix=[]" >> $GITHUB_OUTPUT
        echo "has-changes=false" >> $GITHUB_OUTPUT
        return
    fi

    RAW_OUTPUT=$(run_clippier)

    # Debug: Log raw clippier output for diagnostics
    echo "📋 Clippier raw output:" >&2
    echo "$RAW_OUTPUT" >&2
    echo "..." >&2

    # Validate JSON from clippier
    echo "🔍 Validating clippier JSON output..." >&2
    if ! printf '%s' "$RAW_OUTPUT" | jq empty 2>&1; then
        echo "❌ ERROR: Clippier produced invalid JSON" >&2
        echo "Full output:" >&2
        echo "$RAW_OUTPUT" >&2
        exit 1
    fi
    echo "✅ Clippier JSON is valid" >&2

    RAW_OUTPUT=$(inject_custom_reasoning "$RAW_OUTPUT")

    # Debug: Log output after reasoning injection
    echo "📋 After reasoning injection:" >&2
    echo "$RAW_OUTPUT" | jq >&2
    echo "..." >&2

    echo "raw-output<<EOF" >> $GITHUB_OUTPUT
    printf '%s\n' "$RAW_OUTPUT" >> $GITHUB_OUTPUT
    echo "EOF" >> $GITHUB_OUTPUT

    if [[ "$INPUT_COMMAND" == "affected-packages" ]]; then
        AFFECTED=$(printf '%s' "$RAW_OUTPUT" | jq -r '.affected // false')
        echo "affected=$AFFECTED" >> $GITHUB_OUTPUT

        if [[ "$INPUT_INCLUDE_REASONING" == "true" ]]; then
            REASONING=$(printf '%s' "$RAW_OUTPUT" | jq -c '.reasoning // null')
            echo "reasoning=$REASONING" >> $GITHUB_OUTPUT
        fi
    fi

    if [[ "$INPUT_COMMAND" == "features" ]]; then
        TRANSFORMED_OUTPUT=$(transform_output "$RAW_OUTPUT")

        # Debug: Log transformed matrix output
        echo "📋 Transformed matrix output:" >&2
        printf '%s' "$TRANSFORMED_OUTPUT" | jq -c '.[0:3]' >&2 || echo "Failed to display matrix" >&2
        echo "Matrix length: $(printf '%s' "$TRANSFORMED_OUTPUT" | jq 'length')" >&2

        echo "matrix=$TRANSFORMED_OUTPUT" >> $GITHUB_OUTPUT

        local matrix_length=$(printf '%s' "$TRANSFORMED_OUTPUT" | jq 'length')
        if [[ "$matrix_length" -gt 0 ]]; then
            echo "has-changes=true" >> $GITHUB_OUTPUT
        else
            echo "has-changes=false" >> $GITHUB_OUTPUT
        fi

        run_additional_checks

        if [[ "$INPUT_GENERATE_SUMMARY" == "true" ]]; then
            if [[ "$INPUT_INCLUDE_REASONING" == "true" ]]; then
                generate_summary "$TRANSFORMED_OUTPUT" "$RAW_OUTPUT"
            else
                generate_summary "$TRANSFORMED_OUTPUT" ""
            fi

            generate_additional_check_summary
        fi

        if [[ "$INPUT_ENABLE_DOCKER_ANALYSIS" == "true" ]]; then
            if [[ "$INPUT_INCLUDE_REASONING" == "true" ]]; then
                DOCKER_RESULT=$(analyze_docker_packages "$TRANSFORMED_OUTPUT" "$RAW_OUTPUT")
            else
                DOCKER_RESULT=$(analyze_docker_packages "$TRANSFORMED_OUTPUT" "")
            fi

            echo "DOCKER_RESULT: $(echo "$DOCKER_RESULT" | jq)"
            DOCKER_MATRIX=$(printf '%s' "$DOCKER_RESULT" | jq -rc '.matrix')
            HAS_DOCKER_CHANGES=$(printf '%s' "$DOCKER_RESULT" | jq -r '.has_changes')
            DOCKER_COUNT=$(printf '%s' "$DOCKER_RESULT" | jq -r '.count')
            DOCKER_PACKAGES_LIST=$(printf '%s' "$DOCKER_RESULT" | jq -r '.packages_list')

            echo "docker-matrix=$DOCKER_MATRIX" >> $GITHUB_OUTPUT
            echo "has-docker-changes=$HAS_DOCKER_CHANGES" >> $GITHUB_OUTPUT
            echo "docker-count=$DOCKER_COUNT" >> $GITHUB_OUTPUT
            echo "docker-packages-list<<EOF" >> $GITHUB_OUTPUT
            printf '%s\n' "$DOCKER_PACKAGES_LIST" >> $GITHUB_OUTPUT
            echo "EOF" >> $GITHUB_OUTPUT

            if [[ "$INPUT_GENERATE_SUMMARY" == "true" ]]; then
                generate_docker_summary "$DOCKER_RESULT"
            fi
        fi
    else
        echo "matrix=$RAW_OUTPUT" >> $GITHUB_OUTPUT
    fi

    echo "✅ Clippier action completed successfully"
}

main
