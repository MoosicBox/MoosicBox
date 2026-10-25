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
echo 'Analysis boundary regression tests passed'
