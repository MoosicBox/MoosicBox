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
[[ $(sed -n 's/^matrix=//p' "$GITHUB_OUTPUT" | jq 'length') == 5 ]]
for shard in "$work/clippier-plan/shards/"[0-9]*.json; do
    jq -e 'length <= 64' "$shard"
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
jq -e '.requested_jobs == 3 and .effective_jobs == 3 and .stacked_jobs == 3 and .shard_count == 1' "$RUNNER_TEMP/clippier-plan/schedule-summary.json"
jq -s -e --slurpfile jobs "$RUNNER_TEMP/clippier-plan/jobs.json" 'add == $jobs[0]' "$RUNNER_TEMP/clippier-plan/shards/"[0-9]*.json
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
