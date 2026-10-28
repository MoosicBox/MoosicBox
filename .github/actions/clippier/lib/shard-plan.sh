#!/usr/bin/env bash
# Partition a complete immutable job plan, without changing or dropping jobs.
# Use GitHub's full 256-job matrix capacity while keeping payloads safely below
# the 1 MB (UTF-16) job-output limit.
# Platform ceilings belong to GitHub transport, not the case/job planner.
CLIPPIER_GITHUB_MATRIX_MAX_JOBS=256
CLIPPIER_MATRIX_PAYLOAD_BYTES=200000

matrix_max_jobs() {
    local value="${INPUT_MATRIX_MAX_JOBS-256}"
    if [[ ! "$value" =~ ^[1-9][0-9]{0,2}$ ]] || (( value > CLIPPIER_GITHUB_MATRIX_MAX_JOBS )); then
        echo 'matrix-max-jobs must be an integer from 1 to 256' >&2
        return 1
    fi
    printf '%s' "$value"
}

# Shared predicate for direct output and the sharding fast path.
matrix_fits() {
    local max_jobs
    max_jobs=$(matrix_max_jobs) || return $?
    jq -e --argjson max "$max_jobs" --argjson bytes "$CLIPPIER_MATRIX_PAYLOAD_BYTES" \
        'length <= $max and (tojson | utf8bytelength) <= $bytes' "$@" >/dev/null
}

# Report transport decisions separately from allocation. Never writes outputs
# to stdout: shard callers reserve that channel for their JSON result.
report_matrix_route() {
    local plan_file="$1" shard_dir="$2" max_jobs report
    max_jobs=$(matrix_max_jobs) || return $?
    report=$(jq -cn --slurpfile jobs "$plan_file" --argjson max "$max_jobs" \
        --argjson bytes "$CLIPPIER_MATRIX_PAYLOAD_BYTES" '
        {job_count: ($jobs[0] | length), matrix_max_jobs: $max, payload_budget_bytes: $bytes,
         reasons: ([if ($jobs[0] | length) > $max then "matrix job-count limit" else empty end,
                    if ($jobs[0] | tojson | utf8bytelength) > $bytes then "matrix payload budget" else empty end])}') || return $?
    if [[ -n "$shard_dir" ]]; then
        report=$(jq -c --slurpfile shards "$shard_dir/all.json" --slurpfile manifest "$shard_dir/manifest.json" '. + {mode:"sharded", shard_sizes: ($shards[0] | map(length)), shards: $manifest[0]}' <<< "$report") || return $?
    else
        report=$(jq -c '. + {mode:(if .job_count == 0 then "empty" else "direct" end), shard_sizes:[]}' <<< "$report") || return $?
    fi
    local message
    message=$(jq -r '"Matrix transport: \(.job_count) jobs → \(.mode)" +
        (if .mode == "sharded" then " \(.shard_sizes)" else "" end) +
        "; matrix-max-jobs=\(.matrix_max_jobs), payload budget=\(.payload_budget_bytes) UTF-8 bytes; " +
        (if (.reasons | length) == 0 then "plan fits within transport limits" else "reason: " + (.reasons | join(" and ")) end)' <<< "$report") || return $?
    if [[ -n "$shard_dir" ]]; then
        message+="; names=$(jq -c 'map(.name)' "$shard_dir/manifest.json")"
    fi
    echo "$message" >&2
    if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then printf '%s\n\n' "$message" >> "$GITHUB_STEP_SUMMARY"; fi
    if [[ -f "$(dirname "$plan_file")/schedule-summary.json" ]]; then
        printf '%s\n' "$report" > "$(dirname "$plan_file")/transport-summary.json" || return $?
    fi
}

shard_job_plan() {
    local plan_file="$1" shard_dir="$2"
    local max_jobs
    max_jobs=$(matrix_max_jobs) || return $?
    mkdir -p "$shard_dir" || return $?
    # Expressions are jq source, never shell source. Capture their streams to
    # reject empty/multiple keys and names rather than dropping/duplicating jobs.
    local group_expr="${INPUT_SHARD_GROUP_BY_JQ:-null}"
    local name_expr="${INPUT_SHARD_NAME_JQ:-}"
    if [[ -z "$name_expr" ]]; then name_expr='"Build shard " + (.id | tostring)'; fi
    local filter
    filter="def group_key: ($group_expr); def shard_name: ($name_expr);"
    jq -ce --argjson max "$max_jobs" --argjson ceiling "$CLIPPIER_GITHUB_MATRIX_MAX_JOBS" --argjson bytes "$CLIPPIER_MATRIX_PAYLOAD_BYTES" "$filter"'
        def split_jobs:
            reduce .[] as $job ([[]];
                if ((.[-1] | length) >= $max) or
                   ((.[-1] + [$job] | tojson | utf8bytelength) > $bytes)
                then . + [[$job]] else .[-1] += [$job] end)
            | map(select(length > 0));
        map(. as $job | [group_key] |
            if length != 1 then error("shard-group-by-jq must produce exactly one JSON key per job")
            else {key:.[0], job:$job} end)
        | group_by(.key)
        | map(.[0].key as $key | map(.job) | split_jobs | . as $parts |
            to_entries | map({group:$key, group_index:(.key + 1), group_count:($parts | length), jobs:.value}))
        | add // []
        | if length > $ceiling then error("Plan requires more than 256 matrix shards") else . end
        | if any(.[]; (.jobs | tojson | utf8bytelength) > $bytes)
          then error("A scheduled job exceeds the matrix output budget") else . end
        | to_entries | map(.key as $id | .value + {id:$id} |
            . as $shard | [del(.jobs) | shard_name] |
            if length != 1 or (.[0] | type) != "string" then error("shard-name-jq must produce exactly one string")
            elif (.[0] | length) == 0 or (.[0] | test("[\\x00-\\x1f\\x7f]")) then error("Shard names must be nonempty single-line strings")
            else $shard + {name: (.[0] + (if $shard.group_count > 1 then " · \($shard.group_index)/\($shard.group_count)" else "" end))} end)
    ' "$plan_file" > "$shard_dir/grouped.json" || return $?
    jq -c 'map(.jobs)' "$shard_dir/grouped.json" > "$shard_dir/all.json" || return $?
    jq -c 'map(del(.jobs))' "$shard_dir/grouped.json" > "$shard_dir/manifest.json" || return $?
    jq -e --argjson max "$CLIPPIER_GITHUB_MATRIX_MAX_JOBS" --argjson bytes "$CLIPPIER_MATRIX_PAYLOAD_BYTES" 'length <= $max and (tojson | utf8bytelength) <= $bytes' "$shard_dir/manifest.json" >/dev/null || {
        echo 'Shard descriptor matrix exceeds the matrix count or payload budget' >&2
        return 1
    }
    local count index
    count=$(jq 'length' "$shard_dir/all.json") || return $?
    for ((index=0; index<count; index++)); do
        jq -c --argjson index "$index" '.[$index]' "$shard_dir/all.json" > "$shard_dir/$index.json" || return $?
    done
    jq -c '[range(length)]' "$shard_dir/all.json"
}

# Choose direct top-level execution whenever one bounded matrix can carry the
# complete plan. The two outputs are mutually exclusive, including empty plans.
route_job_plan() {
    local plan_file="$1" shard_dir="$2"
    matrix_max_jobs >/dev/null || return $?
    if matrix_fits "$plan_file"; then
        echo "direct-matrix=$(jq -c . "$plan_file")" >> "$GITHUB_OUTPUT"
        echo 'shard-matrix=[]' >> "$GITHUB_OUTPUT"
        echo 'matrix=[]' >> "$GITHUB_OUTPUT"
        report_matrix_route "$plan_file" "" || return $?
        printf '[]'
    else
        local shards
        shards=$(shard_job_plan "$plan_file" "$shard_dir") || return $?
        echo 'direct-matrix=[]' >> "$GITHUB_OUTPUT"
        echo "shard-matrix=$(cat "$shard_dir/manifest.json")" >> "$GITHUB_OUTPUT"
        echo "matrix=$shards" >> "$GITHUB_OUTPUT"
        report_matrix_route "$plan_file" "$shard_dir" || return $?
        printf '%s' "$shards"
    fi
}

# Persist a machine-readable and human-readable allocation audit with the plan.
write_schedule_summary() {
    local summary_file="$1" shards="$2" report
    report="$(dirname "$summary_file")/schedule-summary.md"
    jq --argjson shards "$shards" '. + {shard_count: ($shards | length)}' "$summary_file" > "$summary_file.tmp" || return $?
    mv "$summary_file.tmp" "$summary_file" || return $?
    jq -r '
        "## Lossless CI scheduling",
        "",
        "| Cases | Minimum safe jobs | Requested budget | Effective budget | Jobs | Singletons | Stacks | Shards |",
        "| --- | --- | --- | --- | --- | --- | --- | --- |",
        "| \(.case_count) | \(.minimum_jobs) | \(.requested_jobs) | \(.effective_jobs) | \(.execution_jobs) | \(.singleton_jobs) | \(.stacked_jobs) | \(.shard_count) |",
        "",
        "Isolation reasons (case counts; reasons may overlap):",
        (.isolation_reasons | to_entries[] | "* \(.key): \(.value)"),
        "",
        "Cases and feature lists are unchanged. Stacking shares runners, never feature invocations."
    ' "$summary_file" > "$report" || return $?
    cat "$report" >&2
    if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then cat "$report" >> "$GITHUB_STEP_SUMMARY"; fi
}
