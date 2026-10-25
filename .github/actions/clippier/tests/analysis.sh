#!/usr/bin/env bash
# Regression coverage for failed/empty analysis and transformed-plan boundaries.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
source "$root/lib/analysis.sh"
for command in 'exit 7' 'printf "[]"; exit 9' ':' 'printf null; printf "[]"' 'printf nope'; do
    if run_clippier_json "$command" >/dev/null 2>&1; then
        echo "Unexpected success: $command" >&2
        exit 1
    fi
done
[[ $(run_clippier_json 'printf "[\n]"') == '[]' ]]
for input in '' null '{}' '[{}]' '[][]' '[{"name":"x","path":".","os":"linux","features":[1]}]'; do
    if printf '%s' "$input" | validate_case_array; then
        echo "Unexpected valid cases: $input" >&2
        exit 1
    fi
done
printf '[]' | validate_case_array
printf '[{"name":"x","path":".","os":"linux","features":[]}]' | validate_case_array
work=$(mktemp -d)
export GITHUB_WORKSPACE="$work" GITHUB_STEP_SUMMARY="$work/summary" INPUT_COMMAND=run-matrix
source "$root/action.sh"
export INPUT_COMMAND=features INPUT_STACK_CASES=false INPUT_JQ_FILTER='map(.features = null)'
export GITHUB_OUTPUT="$work/output"
if output_matrix '[{"name":"x","path":".","os":"linux","features":[]}]'; then
    echo 'Invalid post-transform plan was accepted' >&2
    exit 1
fi
[[ ! -s "$GITHUB_OUTPUT" ]]
export INPUT_MATRIX_PROPERTIES='name,path,features,os'
export INPUT_OS_SUFFIX='' INPUT_TRANSFORM_NAME_REGEX=''
transformed=$(transform_output '[{"name":"x","path":".","os":"linux","features":[],"freshRunner":true}]')
[[ $(printf '%s' "$transformed" | jq .[0].freshRunner) == true ]]
# Post-scheduling additions are allowed, execution mutations are not.
export INPUT_JQ_FILTER='' INPUT_COMMAND=features RUNNER_TEMP="$work"
case_array='[{"name":"x","path":".","os":"linux","features":[]}]'
export INPUT_JQ_POST_FILTER='map(. + {label: "custom"})'
output_matrix "$case_array"
for filter in 'map(.name = "changed")' 'map(del(.features))' '[]' '., .' 'null'; do
    export INPUT_JQ_POST_FILTER="$filter"
    if output_matrix "$case_array"; then
        echo "Unsafe naming filter accepted: $filter" >&2
        exit 1
    fi
done
# Exercise the actual workflow naming jq on final jobs, including a stacked job.
export INPUT_COMMAND=run-matrix INPUT_STACK_CASES=false
export INPUT_JQ_POST_FILTER
INPUT_JQ_POST_FILTER=$(yq -r '.jobs.determine-affected-packages.steps[] | select(.id == "analyze") | .with.jq-post-filter' "$root/../../workflows/build-and-test.yml")
[[ -n "$INPUT_JQ_POST_FILTER" && "$INPUT_JQ_POST_FILTER" != null ]]
jobs=$(jq -n '[
    {name:"multi", os:"ubuntu-latest", cases:[{},{}]},
    {name:"multi", os:"ubuntu-latest", cases:[{}]},
    {name:"multi", os:"macos-latest", cases:[{}]},
    {name:"multi", os:"windows-latest", cases:[{}]},
    {name:"single", os:"ubuntu-latest", cases:[{}]},
    {name:"custom", os:"self-hosted", cases:[{}, {}, {}]}
]')
# The filter is order-preserving and repeatable, even across setup groups with
# identical package/OS labels. Counts refer to jobs, not the cases inside them.
: > "$GITHUB_OUTPUT"
output_matrix "$jobs"
named=$(sed -n 's/^matrix=//p' "$GITHUB_OUTPUT")
labels=$(printf '%s' "$named" | jq '[.[] |
    .name + (if .hasMultipleOs then " " + .osEmoji else "" end) +
    (if .osCount > 1 then " " + (.osIndex | tostring) else "" end) + .stackSuffix]')
printf '%s' "$labels" | jq -e '. == ["multi 🐧 1 · 2 cases", "multi 🐧 2", "multi 🍎", "multi 🪟", "single", "custom · 3 cases"]'
printf '%s' "$named" | jq -e '.[5].osEmoji == "self-hosted"'
# Every original field, including nested cases, survives decoration unchanged.
jq -e -n --argjson before "$jobs" --argjson after "$named" '
    all(range(0; $before | length); . as $i |
        all($before[$i] | keys[]; . as $key | $before[$i][$key] == $after[$i][$key]))'
printf '%s' "$jobs" | jq -c "$INPUT_JQ_POST_FILTER" > "$work/repeated.json"
printf '%s' "$named" > "$work/named.json"
jq -e -n --slurpfile repeated "$work/repeated.json" --slurpfile named "$work/named.json" '$repeated == $named'
# Empty plans remain valid and job removal/reordering cannot be disguised.
output_matrix '[]'
export INPUT_JQ_POST_FILTER=reverse
if output_matrix "$jobs"; then exit 1; fi
echo 'Analysis and workflow naming regression tests passed'
