#!/usr/bin/env bash
# Full scheduled builds may require more isolated setups than one GH matrix.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
export GITHUB_WORKSPACE="$work" INPUT_COMMAND=run-matrix
export GITHUB_OUTPUT="$work/output" GITHUB_STEP_SUMMARY="$work/summary" RUNNER_TEMP="$work"
source "$root/action.sh"
export INPUT_COMMAND=features INPUT_STACK_CASES=true INPUT_SHARD_MATRIX=true INPUT_ALLOW_ISOLATION_OVERFLOW=true
export INPUT_JQ_FILTER='' INPUT_JQ_POST_FILTER='' INPUT_MAX_PARALLEL=256
# Use a fresh build of the real scheduler, not a mock that could drop cases.
CLIPPIER_BIN="${CLIPPIER_TEST_BIN:-$root/../../../target/debug/clippier}"
CLIPPIER_BIN=$(realpath "$CLIPPIER_BIN")
jq -n '[range(300) | {name:("pkg-" + tostring),path:".",os:"ubuntu-latest",features:["default"],freshRunner:true}]' > "$work/cases.json"
# Sharding is transport, not implicit permission to exceed the budget.
export INPUT_ALLOW_ISOLATION_OVERFLOW=false
if output_matrix "$(cat "$work/cases.json")"; then exit 1; fi
export INPUT_ALLOW_ISOLATION_OVERFLOW=true
output_matrix "$(cat "$work/cases.json")"
jq -e 'length == 300 and all(.[]; .cases | length == 1)' "$work/clippier-plan/jobs.json"
jq -s -e --slurpfile jobs "$work/clippier-plan/jobs.json" 'add == $jobs[0]' "$work/clippier-plan/shards/"[0-9]*.json
[[ $(sed -n 's/^matrix=//p' "$GITHUB_OUTPUT" | jq 'length') == 2 ]]
for shard in "$work/clippier-plan/shards/"[0-9]*.json; do
    jq -e 'length <= 256' "$shard"
    [[ $(wc -c < "$shard") -le 200001 ]]
done
# Transport must not disable stacking when the original budget is feasible.
export INPUT_MAX_PARALLEL=3
jq -n '[range(300) | {name:"fixture",path:".",os:"ubuntu-latest",features:[("feature-" + tostring)]}]' > "$work/compatible.json"
# Use a new plan location so this also checks that each publication is complete.
export RUNNER_TEMP="$work/compatible"
mkdir -p "$RUNNER_TEMP"
output_matrix "$(cat "$work/compatible.json")"
jq -e 'length == 3 and all(.[]; .cases | length == 100)' "$RUNNER_TEMP/clippier-plan/jobs.json"
jq -e '.requested_jobs == 3 and .effective_jobs == 3 and .stacked_jobs == 3 and .shard_count == 0' "$RUNNER_TEMP/clippier-plan/schedule-summary.json"
[[ ! -d "$RUNNER_TEMP/clippier-plan/shards" ]]
sed -n 's/^direct-matrix=//p' "$GITHUB_OUTPUT" | tail -1 > "$work/direct.json"
jq -e --slurpfile jobs "$RUNNER_TEMP/clippier-plan/jobs.json" '. == $jobs[0]' "$work/direct.json"
[[ $(sed -n 's/^matrix=//p' "$GITHUB_OUTPUT" | tail -1) == '[]' ]]
# Removing an entire shard must be caught by complete-plan receipt verification.
jq '[.[0].cases[] | {case:.,exit_code:0}]' "$work/clippier-plan/shards/0.json" > "$work/incomplete-receipts.json"
if "$CLIPPIER_BIN" verify-matrix "$work/clippier-plan/jobs.json" "$work/incomplete-receipts.json"; then exit 1; fi
# Size, not only job count, limits shard output. Preserve multibyte data too.
jq -n '[range(12) | {payload:("🎵" * 10000),id:.}]' > "$work/large.json"
shard_job_plan "$work/large.json" "$work/large-shards" > /dev/null
jq -s -e --slurpfile jobs "$work/large.json" 'add == $jobs[0]' "$work/large-shards/"[0-9]*.json
jq -n '[{payload:("x" * 200001)}]' > "$work/oversize.json"
if shard_job_plan "$work/oversize.json" "$work/oversize-shards"; then exit 1; fi
printf '[]' > "$work/empty.json"
[[ $(shard_job_plan "$work/empty.json" "$work/empty-shards") == '[]' ]]
echo 'Lossless scheduled matrix sharding tests passed'

# Exercise routing at both sides of GitHub's job-count boundary. Neither
# route may drop/reorder jobs; a fitting plan must not create shard files.
for count in 0 1 255 256 257; do
    jq -n --argjson count "$count" '[range($count) | {id:.}]' > "$work/boundary.json"
    export GITHUB_OUTPUT="$work/boundary-$count.outputs"
    ids=$(route_job_plan "$work/boundary.json" "$work/boundary-$count")
    if (( count <= 256 )); then
        [[ "$ids" == '[]' && ! -d "$work/boundary-$count" ]]
        sed -n 's/^direct-matrix=//p' "$GITHUB_OUTPUT" > "$work/emitted.json"
        jq -e --slurpfile expected "$work/boundary.json" '. == $expected[0]' "$work/emitted.json"
        grep -qx 'matrix=\[\]' "$GITHUB_OUTPUT"
    else
        [[ "$ids" == '[0,1]' ]]
        jq -e 'length == 256' "$work/boundary-$count/0.json"
        jq -s -e --slurpfile expected "$work/boundary.json" 'add == $expected[0]' "$work/boundary-$count/"[0-9]*.json
        grep -qx 'direct-matrix=\[\]' "$GITHUB_OUTPUT"
    fi
done
# A small job count may still require sharding because of payload size.
export GITHUB_OUTPUT="$work/size.outputs"
ids=$(route_job_plan "$work/large.json" "$work/size-shards")
[[ $(printf '%s' "$ids" | jq length) -gt 1 ]]
grep -qx 'direct-matrix=\[\]' "$GITHUB_OUTPUT"
echo 'Direct/sharded routing boundary tests passed'
# Execution must not drift between direct and sharded workflow routes. Only
# the receipt artifact namespace differs; setup, features, tests and coverage do not.
yq -o=json '.jobs.build.steps' "$root/../../workflows/build-and-test.yml" > "$work/direct-steps.json"
yq -o=json '.jobs.build.steps' "$root/../../workflows/clippier-build-shard.yml" > "$work/shard-steps.json"
jq -e --slurpfile shard "$work/shard-steps.json" '
    def normalize: map(if .name == "Upload case receipts" then .with.name = "receipts" else . end);
    normalize == ($shard[0] | normalize)
' "$work/direct-steps.json"
# The actual reusable reader must accept a full 256-job shard, not the old 64.
yq -r '.jobs.prepare.steps[] | select(.id == "matrix") | .run' "$root/../../workflows/clippier-build-shard.yml" > "$work/read-shard.sh"
mkdir -p "$work/reader/plan/shards"
cp "$work/boundary-257/0.json" "$work/reader/plan/shards/0.json"
(cd "$work/reader"; SHARD_ID=0 GITHUB_OUTPUT="$work/read-output" bash -e -o pipefail "$work/read-shard.sh")
sed -n 's/^value=//p' "$work/read-output" | jq -e 'length == 256'
if (cd "$work/reader"; SHARD_ID=missing GITHUB_OUTPUT="$work/missing-output" bash -e -o pipefail "$work/read-shard.sh"); then exit 1; fi
echo 'Direct/sharded execution parity and reader tests passed'
# All four public option combinations preserve cases. Only stacking may group
# them; only sharding may transport an oversized plan across matrices.
for stack in false true; do
    for shard in false true; do
        export INPUT_STACK_CASES="$stack" INPUT_SHARD_MATRIX="$shard" INPUT_MAX_PARALLEL=3
        export INPUT_ALLOW_ISOLATION_OVERFLOW=false
        export RUNNER_TEMP="$work/combo-$stack-$shard" GITHUB_OUTPUT="$work/combo-$stack-$shard.outputs"
        mkdir -p "$RUNNER_TEMP"
        if [[ "$stack" == false && "$shard" == false ]]; then
            # Legacy direct transport remains supported for fitting matrices.
            output_matrix "$(jq -c '.[0:3]' "$work/compatible.json")"
            sed -n 's/^matrix=//p' "$GITHUB_OUTPUT" | jq -e 'length == 3 and all(.[]; has("cases") | not)'
            if output_matrix "$(cat "$work/compatible.json")"; then exit 1; fi
            continue
        fi
        output_matrix "$(cat "$work/compatible.json")"
        jq -e --arg stack "$stack" '
            if $stack == "true" then length == 3 and all(.[]; .cases | length == 100)
            else length == 300 and all(.[]; .cases | length == 1) end
        ' "$RUNNER_TEMP/clippier-plan/jobs.json"
        jq -e --slurpfile original "$work/compatible.json" '
            [.[] | .cases[].package] | sort == ($original[0] | sort)
        ' "$RUNNER_TEMP/clippier-plan/jobs.json"
        if [[ "$shard" == true && "$stack" == false ]]; then
            grep -qx 'matrix=\[0,1\]' "$GITHUB_OUTPUT"
        fi
    done
done
# Overflow policy is a scheduling choice, independent of matrix transport.
export INPUT_STACK_CASES=true INPUT_SHARD_MATRIX=false INPUT_ALLOW_ISOLATION_OVERFLOW=true INPUT_MAX_PARALLEL=1
export RUNNER_TEMP="$work/overflow-direct" GITHUB_OUTPUT="$work/overflow-direct.outputs"
mkdir -p "$RUNNER_TEMP"
output_matrix "$(jq -c '.[0:2]' "$work/cases.json")"
sed -n 's/^matrix=//p' "$GITHUB_OUTPUT" | jq -e 'length == 2 and all(.[]; .cases | length == 1)'
if output_matrix "$(cat "$work/cases.json")"; then exit 1; fi
# Execution selection follows the explicit job format, never stack-cases.
export INPUT_STACK_CASES=false
run_case_stack() { echo job; }
run_matrix_command() { echo legacy; }
for entry in '{"cases":[]}' '{"cases":null}' '{"cases":[{}]}'; do
    export INPUT_RUN_MATRIX_PACKAGE_JSON="$entry"
    [[ $(execute_matrix_entry) == job ]]
done
export INPUT_RUN_MATRIX_PACKAGE_JSON='{"name":"legacy","features":[]}'
[[ $(execute_matrix_entry) == legacy ]]
echo 'Independent option and execution-selection tests passed'
# The actual CLI command builder must not turn a runner budget into legacy
# feature unions for either opt-in. Both-off retains its existing CLI behavior.
for stack in false true; do
    for shard in false true; do
        export INPUT_COMMAND=features INPUT_STACK_CASES="$stack" INPUT_SHARD_MATRIX="$shard" INPUT_MAX_PARALLEL=3
        command=$(set +u; build_clippier_command)
        if [[ "$stack" == false && "$shard" == false ]]; then
            [[ "$command" == *' --max-parallel 3'* ]]
        else
            [[ "$command" != *' --max-parallel '* ]]
        fi
    done
done
# End-to-end sharding-only boundaries, including empty plans.
export INPUT_COMMAND=features INPUT_STACK_CASES=false INPUT_SHARD_MATRIX=true
for count in 0 1 255 256 257; do
    export RUNNER_TEMP="$work/singletons-$count" GITHUB_OUTPUT="$work/singletons-$count.outputs"
    mkdir -p "$RUNNER_TEMP"
    output_matrix "$(jq -c --argjson count "$count" '.[:$count]' "$work/compatible.json")"
    jq -e --argjson count "$count" 'length == $count and all(.[]; .cases | length == 1)' "$RUNNER_TEMP/clippier-plan/jobs.json"
    if (( count <= 256 )); then
        grep -qx 'matrix=\[\]' "$GITHUB_OUTPUT"
        [[ ! -d "$RUNNER_TEMP/clippier-plan/shards" ]]
    else
        grep -qx 'matrix=\[0,1\]' "$GITHUB_OUTPUT"
    fi
done
# Caller-selected transport limits do not change jobs or enable sharding.
export INPUT_COMMAND=features INPUT_STACK_CASES=false INPUT_MATRIX_MAX_JOBS=2
for shard in false true; do
    export INPUT_SHARD_MATRIX="$shard" RUNNER_TEMP="$work/custom-$shard" GITHUB_OUTPUT="$work/custom-$shard.outputs"
    mkdir -p "$RUNNER_TEMP"
    output_matrix "$(jq -c '.[0:2]' "$work/compatible.json")"
    grep -q '2 jobs → direct; matrix-max-jobs=2' "$GITHUB_STEP_SUMMARY"
    if [[ "$shard" == false ]]; then
        if output_matrix "$(jq -c '.[0:3]' "$work/compatible.json")"; then exit 1; fi
        [[ ! -d "$RUNNER_TEMP/clippier-plan/shards" ]]
    else
        output_matrix "$(jq -c '.[0:3]' "$work/compatible.json")"
        jq -e '.mode == "sharded" and .shard_sizes == [2,1] and .matrix_max_jobs == 2 and
            (.reasons | index("matrix job-count limit")) != null' "$RUNNER_TEMP/clippier-plan/transport-summary.json"
        jq -s -e --slurpfile jobs "$RUNNER_TEMP/clippier-plan/jobs.json" 'add == $jobs[0]' "$RUNNER_TEMP/clippier-plan/shards/"[0-9]*.json
    fi
done
export INPUT_MATRIX_MAX_JOBS=1 INPUT_SHARD_MATRIX=true RUNNER_TEMP="$work/limit-one" GITHUB_OUTPUT="$work/limit-one.outputs"
mkdir -p "$RUNNER_TEMP"
output_matrix "$(jq -c '.[0:3]' "$work/compatible.json")"
jq -e '.shard_sizes == [1,1,1]' "$RUNNER_TEMP/clippier-plan/transport-summary.json"
# Invalid API values fail before publishing any matrix output or plan files.
for value in '' 0 257 -1 1.5 abc 01 999999999999999999999; do
    export INPUT_MATRIX_MAX_JOBS="$value" GITHUB_OUTPUT="$work/invalid.outputs"
    : > "$GITHUB_OUTPUT"
    if output_matrix '[]'; then exit 1; fi
    [[ ! -s "$GITHUB_OUTPUT" ]]
done
unset INPUT_MATRIX_MAX_JOBS
[[ $(matrix_max_jobs) == 256 ]]
# Size-driven splitting is reported separately from the job-count limit.
export GITHUB_OUTPUT="$work/payload-report.outputs"
route_job_plan "$work/large.json" "$work/payload-report" > /dev/null
# This fixture has no schedule summary, so the human-readable decision is still
# guaranteed for callers that invoke transport directly.
grep -q 'reason: matrix payload budget' "$GITHUB_STEP_SUMMARY"
echo 'Configurable matrix transport limits and reporting tests passed'
