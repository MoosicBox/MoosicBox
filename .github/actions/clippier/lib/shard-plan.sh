#!/usr/bin/env bash
# Partition a complete immutable job plan, without changing or dropping jobs.
# Use GitHub's full 256-job matrix capacity while keeping payloads safely below
# the 1 MB (UTF-16) job-output limit.
shard_job_plan() {
    local plan_file="$1" shard_dir="$2"
    mkdir -p "$shard_dir" || return $?
    jq -ce '
        reduce .[] as $job ([[]];
            if ((.[-1] | length) >= 256) or
               ((.[-1] + [$job] | tojson | utf8bytelength) > 200000)
            then . + [[$job]] else .[-1] += [$job] end)
        | map(select(length > 0))
        | if length > 256 then error("Plan requires more than 256 matrix shards") else . end
        | if any(.[]; (tojson | utf8bytelength) > 200000)
          then error("A scheduled job exceeds the matrix output budget") else . end
    ' "$plan_file" > "$shard_dir/all.json" || return $?
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
    if jq -e 'length <= 256 and (tojson | utf8bytelength) <= 200000' "$plan_file" >/dev/null; then
        echo "direct-matrix=$(jq -c . "$plan_file")" >> "$GITHUB_OUTPUT"
        echo 'matrix=[]' >> "$GITHUB_OUTPUT"
        printf '[]'
    else
        local shards
        shards=$(shard_job_plan "$plan_file" "$shard_dir") || return $?
        echo 'direct-matrix=[]' >> "$GITHUB_OUTPUT"
        echo "matrix=$shards" >> "$GITHUB_OUTPUT"
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
