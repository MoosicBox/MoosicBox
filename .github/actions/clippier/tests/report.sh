#!/usr/bin/env bash
# Reports must discover nested cases without flattening or overwriting files.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
source "$root/lib/run-matrix.sh"
work=$(mktemp -d)
export GITHUB_STEP_SUMMARY="$work/summary" GITHUB_OUTPUT="$work/output"
export INPUT_RUN_MATRIX_AGGREGATE_OUTPUT_FILE="$work/report.md"
export INPUT_RUN_MATRIX_WORKING_DIRECTORY="$work/diagnostics"
mkdir -p "$work/diagnostics/job/case-0" "$work/diagnostics/job/case-1"
for case in case-0 case-1; do
    jq -n --arg case "$case" '[{
        package:"fixture", label:"Clippy", total_runs:1, total_passed:0, total_failed:1,
        failures:[{feature_combo:$case, command:"cargo clippy", error_output:$case, exit_code:101}]
    }]' > "$work/diagnostics/job/$case/clippier-diagnostics-same.json"
    printf '%s\n' "$case" > "$work/diagnostics/job/$case/reproduce.sh"
done
(run_matrix_aggregate_failures_command)
grep -q '2 failures' "$work/report.md"
grep -q case-0 "$work/report.md"
grep -q case-1 "$work/report.md"
[[ $(find "$work/diagnostics" -name clippier-diagnostics-same.json | wc -l) == 2 ]]
grep -qx case-0 "$work/diagnostics/job/case-0/reproduce.sh"
grep -qx case-1 "$work/diagnostics/job/case-1/reproduce.sh"

# Cover both absent-directory and existing-but-empty inputs.
for directory in missing empty; do
    export INPUT_RUN_MATRIX_WORKING_DIRECTORY="$work/$directory"
    if [[ "$directory" == empty ]]; then mkdir -p "$work/empty"; fi
    (run_matrix_aggregate_failures_command)
    grep -q 'results are unavailable' "$work/report.md"
    if grep -q 'All tests passed' "$work/report.md"; then exit 1; fi
done
echo 'Nested diagnostics report regression tests passed'
