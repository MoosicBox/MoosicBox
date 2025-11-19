#!/usr/bin/env bash

# =============================================================================
# Template Evaluation System for Clippier Actions
# =============================================================================
#
# This module provides a template rendering system that supports:
# - Variable interpolation: {{matrix.package.name}}, {{clippier.features}}
# - Conditional blocks: {{if condition}}...{{endif}}
# - JSON-backed variable resolution using jq
#
# Usage:
#   result=$(render_template "$template" "$matrix_json" "$features" "$required" "$iter" "$total")
#
# =============================================================================

# Initialize template context with all available variables
#
# Creates a JSON context object containing:
# - matrix.package.*: All properties from the package matrix JSON
# - clippier.features: Current feature combination
# - clippier.all-features: All features including fail-on-warnings and required
# - clippier.feature-flags: Full cargo feature flag string
# - clippier.iteration: Current iteration number
# - clippier.total-iterations: Total number of iterations
#
# Arguments:
#   $1 - matrix_json: Package matrix JSON object
#   $2 - feature_combo: Current feature combination (comma-separated)
#   $3 - required_features: Required features (comma-separated)
#   $4 - iteration: Current iteration number (0-based)
#   $5 - total_iterations: Total number of iterations
#
# Returns:
#   JSON context object on stdout
init_template_context() {
    local matrix_json="$1"
    local feature_combo="$2"
    local required_features="$3"
    local iteration="$4"
    local total_iterations="$5"

    # Build all features list
    local all_features="fail-on-warnings"
    [[ -n "$required_features" ]] && all_features="$all_features,$required_features"
    [[ -n "$feature_combo" ]] && all_features="$all_features,$feature_combo"

    # Build feature flags
    local feature_flags="--features=\"$all_features\""

    # Create a combined context JSON with all available data
    local context=$(jq -n \
        --argjson matrix "$matrix_json" \
        --arg features "$feature_combo" \
        --arg all_features "$all_features" \
        --arg feature_flags "$feature_flags" \
        --argjson iteration "$iteration" \
        --argjson total_iterations "$total_iterations" \
        '{
            matrix: {
                package: $matrix
            },
            clippier: {
                features: $features,
                "all-features": $all_features,
                "feature-flags": $feature_flags,
                iteration: $iteration,
                "total-iterations": $total_iterations
            }
        }')

    echo "$context"
}

# Resolve a single variable reference using jq path navigation
#
# Arguments:
#   $1 - var_path: Variable path in dot notation (e.g., "matrix.package.name")
#   $2 - context_json: Context JSON object
#
# Returns:
#   Variable value on stdout, empty string if not found
#
# Note:
#   Prints warning to stderr if variable is undefined or null
resolve_variable() {
    local var_path="$1"
    local context_json="$2"

    # Convert dot notation to jq path, handling hyphens in property names
    # Replace segments containing hyphens with quoted versions
    # e.g., "clippier.feature-flags" -> ".clippier.\"feature-flags\""
    local jq_path=".$var_path"

    # Quote any path segments that contain hyphens
    jq_path=$(echo "$jq_path" | sed 's/\.\([^.]*-[^.]*\)/.\"\1\"/g')

    # Try to resolve the path
    local value=$(echo "$context_json" | jq -r "$jq_path // \"\"" 2>/dev/null)

    if [[ "$value" == "null" || -z "$value" ]]; then
        echo "Warning: Template variable {{$var_path}} is undefined or null" >&2
        echo ""
    else
        echo "$value"
    fi
}

# Extract all variable references from template
#
# Finds all {{...}} patterns and extracts the variable names,
# excluding if/endif keywords.
#
# Arguments:
#   $1 - template: Template string
#
# Returns:
#   Unique variable references on stdout, one per line
extract_variables() {
    local template="$1"

    # Find all {{...}} patterns and extract the content, excluding if/endif
    grep -oP '\{\{[^}]+\}\}' <<< "$template" | \
        sed 's/{{//g; s/}}//g' | \
        grep -v '^\s*if\s' | \
        grep -v '^\s*endif\s*$' | \
        sort -u
}

# Evaluate conditional blocks in template
#
# Processes {{if condition}}...{{endif}} blocks by:
# 1. Finding if/endif pairs
# 2. Resolving the condition variable
# 3. Keeping or removing content based on truthiness
#
# Truthiness rules:
# - Non-empty string: true
# - "false" or "0": false
# - null or empty: false
#
# Arguments:
#   $1 - text: Text containing conditional blocks
#   $2 - context_json: Context JSON for variable resolution
#
# Returns:
#   Text with conditionals evaluated on stdout
evaluate_conditionals() {
    local text="$1"
    local context_json="$2"

    local result="$text"
    local max_iterations=10
    local iteration=0

    # Process {{if condition}}...{{endif}} blocks iteratively
    while [[ "$result" =~ \{\{if\ ([^}]+)\}\} ]] && [[ $iteration -lt $max_iterations ]]; do
        iteration=$((iteration + 1))

        local start_pattern="${BASH_REMATCH[0]}"
        local condition="${BASH_REMATCH[1]}"

        # Find the content and {{endif}}
        local temp="${result#*$start_pattern}"

        if [[ "$temp" =~ (.*)\{\{endif\}\}(.*) ]]; then
            local content="${BASH_REMATCH[1]}"
            local full_match="${start_pattern}${content}{{endif}}"

            # Resolve the condition variable
            local cond_value=$(resolve_variable "$condition" "$context_json")

            # Evaluate truthiness
            if [[ -n "$cond_value" && "$cond_value" != "false" && "$cond_value" != "0" ]]; then
                result="${result//$full_match/$content}"
            else
                result="${result//$full_match/}"
            fi
        else
            echo "Warning: No matching {{endif}} found for {{if $condition}}" >&2
            break
        fi
    done

    echo "$result"
}

# Evaluate template with context
#
# Main template evaluation function that:
# 1. Replaces all {{variable}} references with their values
# 2. Evaluates {{if}}...{{endif}} conditional blocks
#
# Arguments:
#   $1 - template: Template string
#   $2 - context_json: Context JSON object
#
# Returns:
#   Evaluated template on stdout
evaluate_template() {
    local template="$1"
    local context_json="$2"

    local result="$template"

    # First pass: Replace all variable references
    while IFS= read -r var_ref; do
        [[ -z "$var_ref" ]] && continue

        local value=$(resolve_variable "$var_ref" "$context_json")
        result="${result//\{\{$var_ref\}\}/$value}"
    done < <(extract_variables "$template")

    # Second pass: Handle conditional blocks
    result=$(evaluate_conditionals "$result" "$context_json")

    echo "$result"
}

# Main template rendering function
#
# Public API for rendering templates with all features.
#
# Arguments:
#   $1 - template: Template string with {{...}} placeholders
#   $2 - matrix_json: Package matrix JSON object
#   $3 - feature_combo: Current feature combination (comma-separated)
#   $4 - required_features: Required features (comma-separated)
#   $5 - iteration: Current iteration number (0-based)
#   $6 - total_iterations: Total number of iterations
#
# Returns:
#   Rendered template string on stdout
#
# Example:
#   template='cargo{{if matrix.package.nightly}} +nightly{{endif}} test {{clippier.feature-flags}}'
#   result=$(render_template "$template" "$matrix_json" "feature-1" "" "0" "5")
render_template() {
    local template="$1"
    local matrix_json="$2"
    local feature_combo="$3"
    local required_features="$4"
    local iteration="$5"
    local total_iterations="$6"

    local context=$(init_template_context \
        "$matrix_json" \
        "$feature_combo" \
        "$required_features" \
        "$iteration" \
        "$total_iterations")

    evaluate_template "$template" "$context"
}

# =============================================================================
# Strategy Support Functions
# =============================================================================

# Parse execution strategy string
#
# Parses strategy strings and returns mode and parameters.
#
# Supported strategies:
# - sequential: Run one feature at a time
# - combined: Run all features together
# - parallel: Run features in parallel (caller handles parallelization)
# - chunked-N: Run N features at a time
#
# Arguments:
#   $1 - strategy: Strategy string
#
# Returns:
#   "mode=<mode> [chunk_size=<N>]" on stdout
#   Exit code 1 on error
parse_strategy() {
    local strategy="$1"

    case "$strategy" in
        "sequential")
            echo "mode=sequential"
            ;;
        "combined")
            echo "mode=combined"
            ;;
        "parallel")
            echo "mode=parallel"
            ;;
        chunked-*)
            local chunk_size="${strategy#chunked-}"
            if [[ "$chunk_size" =~ ^[0-9]+$ ]]; then
                echo "mode=chunked chunk_size=$chunk_size"
            else
                echo "Error: Invalid chunk size in strategy: $strategy" >&2
                return 1
            fi
            ;;
        *)
            echo "Error: Unknown strategy: $strategy" >&2
            return 1
            ;;
    esac
}

# Generate feature combinations based on strategy
#
# Takes a JSON array of features and generates combinations
# according to the specified strategy.
#
# Arguments:
#   $1 - features_json: JSON array of features
#   $2 - strategy: Strategy string (sequential, combined, parallel, chunked-N)
#
# Returns:
#   Feature combinations on stdout, one per line
#   For combined: single line with all features comma-separated
#   For sequential/parallel: one feature per line
#   For chunked-N: N features per line, comma-separated
#
# Example:
#   features='["f1","f2","f3"]'
#   generate_feature_combinations "$features" "chunked-2"
#   # Output:
#   # f1,f2
#   # f3
generate_feature_combinations() {
    local features_json="$1"
    local strategy="$2"

    local strategy_info=$(parse_strategy "$strategy")
    [[ $? -ne 0 ]] && return 1

    local mode=$(echo "$strategy_info" | grep -oP 'mode=\K\w+')
    local chunk_size=$(echo "$strategy_info" | grep -oP 'chunk_size=\K\d+' || echo "0")

    case "$mode" in
        "sequential"|"parallel")
            echo "$features_json" | jq -r '.[]'
            ;;
        "combined")
            echo "$features_json" | jq -r 'join(",")'
            ;;
        "chunked")
            local temp_file=$(mktemp)
            echo "$features_json" > "$temp_file"

            jq -r --argjson size "$chunk_size" '
                . as $arr |
                ([range(0; ($arr | length); $size)] | map(
                    $arr[.:(. + $size)] | join(",")
                )) | .[]
            ' "$temp_file"

            rm -f "$temp_file"
            ;;
    esac
}

# =============================================================================
# End of Template Evaluation System
# =============================================================================
