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
# Generic grouping: stable within groups, arbitrary nested/composite JSON keys,
# display names independent of numeric artifact identities.
export INPUT_MATRIX_MAX_JOBS=2 INPUT_SHARD_GROUP_BY_JQ='.os'
export INPUT_SHARD_NAME_JQ='"🧪 " + (.group | tostring)'
jq -n '[range(7) | {id:.,os:(if . % 2 == 0 then "linux" else "windows" end), metadata:{tier:(. % 2)}}]' > "$work/group-jobs.json"
shard_job_plan "$work/group-jobs.json" "$work/groups" > /dev/null
jq -e 'map(.group) == ["linux","linux","windows","windows"] and
    map(.name) == ["🧪 linux · 1/2","🧪 linux · 2/2","🧪 windows · 1/2","🧪 windows · 2/2"] and
    map(.id) == [0,1,2,3]' "$work/groups/manifest.json"
jq -e 'map(map(.id)) == [[0,2],[4,6],[1,3],[5]]' "$work/groups/all.json"
jq -s -e --slurpfile original "$work/group-jobs.json" '(add | sort_by(.id)) == $original[0]' "$work/groups/"[0-9]*.json
for expression in '.metadata.tier' '[.os,.metadata.tier]' '{os:.os,tier:.metadata.tier}' '.missing' 'if .id < 3 then "small" else "large" end'; do
    export INPUT_SHARD_GROUP_BY_JQ="$expression"
    shard_job_plan "$work/group-jobs.json" "$work/generic" > /dev/null
    jq -s -e --slurpfile original "$work/group-jobs.json" '(add | sort_by(.id)) == $original[0]' "$work/generic/"[0-9]*.json
    # Deterministic group ordering and IDs on repeat.
    shard_job_plan "$work/group-jobs.json" "$work/repeated-generic" > /dev/null
    cmp "$work/generic/manifest.json" "$work/repeated-generic/manifest.json"
done
for expression in 'empty' '.os,.id' 'error("bad key")' '('; do
    export INPUT_SHARD_GROUP_BY_JQ="$expression"
    if shard_job_plan "$work/group-jobs.json" "$work/invalid-group"; then exit 1; fi
done
export INPUT_SHARD_GROUP_BY_JQ='.os'
for expression in 'empty' '1' '""' '"a","b"' '"bad\nname"' '('; do
    export INPUT_SHARD_NAME_JQ="$expression"
    if shard_job_plan "$work/group-jobs.json" "$work/invalid-name"; then exit 1; fi
done
# Grouping doesn't force nested workflow calls for a fitting plan.
export INPUT_MATRIX_MAX_JOBS=256 INPUT_SHARD_NAME_JQ='"🧪 " + .group'
export GITHUB_OUTPUT="$work/group-direct.outputs"
[[ $(route_job_plan "$work/group-jobs.json" "$work/group-direct") == '[]' ]]
grep -qx 'shard-matrix=\[\]' "$GITHUB_OUTPUT"
[[ ! -d "$work/group-direct" ]]
# Payload limits still split each group independently and reject huge names.
export INPUT_SHARD_GROUP_BY_JQ='"one"' INPUT_SHARD_NAME_JQ='"One"'
shard_job_plan "$work/large.json" "$work/group-payload" > /dev/null
jq -e 'length > 1 and all(.[]; .group == "one")' "$work/group-payload/manifest.json"
export INPUT_SHARD_NAME_JQ='"x" * 200001'
if shard_job_plan "$work/group-jobs.json" "$work/huge-name"; then exit 1; fi
unset INPUT_SHARD_GROUP_BY_JQ INPUT_SHARD_NAME_JQ INPUT_MATRIX_MAX_JOBS
printf 'Grouped/named sharding tests passed\n'
# Exercise the exact MoosicBox expressions and verify full-plan receipts after
# regrouping: global order may change, but no case can disappear or change.
export INPUT_SHARD_GROUP_BY_JQ INPUT_SHARD_NAME_JQ INPUT_MATRIX_MAX_JOBS=2
INPUT_SHARD_GROUP_BY_JQ=$(yq -r '.jobs.determine-affected-packages.steps[] | select(.id == "analyze") | .with.shard-levels' "$root/../../workflows/build-and-test.yml" | yq -r '.[0].group-by-jq')
INPUT_SHARD_NAME_JQ=$(yq -r '.jobs.determine-affected-packages.steps[] | select(.id == "analyze") | .with.shard-levels' "$root/../../workflows/build-and-test.yml" | yq -r '.[0].name-jq')
jq -n '[range(9) | {name:"fixture",path:".",features:[],os:(["windows-latest","ubuntu-latest","macos-latest"][. % 3])}]' > "$work/os-cases.json"
"$CLIPPIER_BIN" plan-matrix "$work/os-cases.json" > "$work/os-jobs.json"
shard_job_plan "$work/os-jobs.json" "$work/os-shards" > /dev/null
jq -e 'map(.name) == ["🍎 macOS builds · 1/2","🍎 macOS builds · 2/2","🐧 Linux builds · 1/2","🐧 Linux builds · 2/2","🪟 Windows builds · 1/2","🪟 Windows builds · 2/2"]' "$work/os-shards/manifest.json"
jq '[.[][] | .cases[] | {case:.,exit_code:0}]' "$work/os-shards/all.json" > "$work/os-receipts.json"
"$CLIPPIER_BIN" verify-matrix "$work/os-jobs.json" "$work/os-receipts.json"
jq '.[1:]' "$work/os-receipts.json" > "$work/os-missing.json"
if "$CLIPPIER_BIN" verify-matrix "$work/os-jobs.json" "$work/os-missing.json"; then exit 1; fi
# Distinct arbitrary keys can exceed the outer GitHub matrix ceiling.
export INPUT_SHARD_GROUP_BY_JQ='.id' INPUT_SHARD_NAME_JQ='"Group"' INPUT_MATRIX_MAX_JOBS=256
jq -n '[range(257) | {id:.}]' > "$work/too-many-groups.json"
if shard_job_plan "$work/too-many-groups.json" "$work/too-many-groups"; then exit 1; fi
unset INPUT_SHARD_GROUP_BY_JQ INPUT_SHARD_NAME_JQ INPUT_MATRIX_MAX_JOBS
echo 'Workflow OS labels and regrouped receipt reconciliation tests passed'
# Explicit nested mode uses real YAML API, even when all jobs fit directly.
export INPUT_SHARD_LEVELS INPUT_SHARD_MATRIX=true INPUT_STACK_CASES=false INPUT_MATRIX_MAX_JOBS=256
INPUT_SHARD_LEVELS=$(yq -r '.jobs.determine-affected-packages.steps[] | select(.id == "analyze") | .with.shard-levels' "$root/../../workflows/build-and-test.yml")
export RUNNER_TEMP="$work/nested" GITHUB_OUTPUT="$work/nested.outputs"
mkdir -p "$RUNNER_TEMP"
output_matrix "$(cat "$work/os-cases.json")"
grep -qx 'matrix=\[\]' "$GITHUB_OUTPUT"
grep -qx 'direct-matrix=\[\]' "$GITHUB_OUTPUT"
sed -n 's/^hierarchy-matrix=//p' "$GITHUB_OUTPUT" | jq -e 'length == 3 and all(.[]; .kind == "group")'
# Dispatchers expand inputs directly and never allocate preparation runners.
! grep -q 'runs-on:' "$root/../../workflows/clippier-build-group.yml"
! grep -q 'prepare:' "$root/../../workflows/clippier-build-leaf.yml"
jq -e 'length == 3 and all(.[]; .children | length > 0)' "$RUNNER_TEMP/clippier-plan/hierarchy/routing.json"
jq '[.nodes[] | select(.kind == "leaf") | .values[] | .cases[] | {case:.,exit_code:0}]' "$RUNNER_TEMP/clippier-plan/hierarchy/tree.json" > "$work/nested-receipts.json"
"$CLIPPIER_BIN" verify-matrix "$RUNNER_TEMP/clippier-plan/jobs.json" "$work/nested-receipts.json"
jq '[.[] | select(.case.package.os != "windows-latest")]' "$work/nested-receipts.json" > "$work/missing-os-receipts.json"
if "$CLIPPIER_BIN" verify-matrix "$RUNNER_TEMP/clippier-plan/jobs.json" "$work/missing-os-receipts.json"; then exit 1; fi
# Empty nested plans and nested stacking retain the same API contract.
export RUNNER_TEMP="$work/nested-empty" GITHUB_OUTPUT="$work/nested-empty.outputs"
mkdir -p "$RUNNER_TEMP"
output_matrix '[]'
grep -qx 'hierarchy-matrix=\[\]' "$GITHUB_OUTPUT"
export RUNNER_TEMP="$work/nested-stacks" GITHUB_OUTPUT="$work/nested-stacks.outputs" INPUT_STACK_CASES=true INPUT_MAX_PARALLEL=3
mkdir -p "$RUNNER_TEMP"
output_matrix "$(cat "$work/compatible.json")"
jq -e '[.nodes[] | select(.kind == "leaf") | .values[] | .cases | length] == [100,100,100]' "$RUNNER_TEMP/clippier-plan/hierarchy/tree.json"
# Conflicts fail before writing outputs, never silently flatten.
export INPUT_SHARD_GROUP_BY_JQ='.os' GITHUB_OUTPUT="$work/conflict.outputs"
: > "$GITHUB_OUTPUT"
if output_matrix '[]'; then exit 1; fi
[[ ! -s "$GITHUB_OUTPUT" ]]
unset INPUT_SHARD_GROUP_BY_JQ INPUT_SHARD_LEVELS INPUT_MATRIX_MAX_JOBS
echo 'Nested API, static dispatcher readers, stacking and subtree receipt tests passed'
# Embedded caller budget is forwarded through the public action API, not only
# supported by direct Python callers. Explicit capacity failures publish nothing.
export INPUT_SHARD_LEVELS='[{"group-by-jq":".os"},{"group-by-jq":".name"}]' INPUT_SHARD_MATRIX=true INPUT_STACK_CASES=false
export INPUT_SHARD_CALLER_DEPTH=8 RUNNER_TEMP="$work/too-deep" GITHUB_OUTPUT="$work/too-deep.outputs"
mkdir -p "$RUNNER_TEMP"
: > "$GITHUB_OUTPUT"
if output_matrix "$(cat "$work/os-cases.json")"; then exit 1; fi
[[ ! -s "$GITHUB_OUTPUT" && ! -d "$RUNNER_TEMP/clippier-plan/hierarchy" ]]
export INPUT_SHARD_CALLER_DEPTH=7 RUNNER_TEMP="$work/embedded" GITHUB_OUTPUT="$work/embedded.outputs"
mkdir -p "$RUNNER_TEMP"
output_matrix "$(cat "$work/os-cases.json")"
grep -qx 'hierarchy-entry=clippier-build-group-7.yml' "$GITHUB_OUTPUT"
unset INPUT_SHARD_LEVELS INPUT_SHARD_CALLER_DEPTH
echo 'Embedded caller depth API tests passed'
