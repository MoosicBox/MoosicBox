#!/usr/bin/env bash
# Exercise the real matrix runner with logs larger than Linux's per-argument cap.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
source "$root/lib/template.sh"
source "$root/lib/run-matrix.sh"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
export INPUT_RUN_MATRIX_PACKAGE_JSON='{"name":"large-output","path":".","features":[]}'
export INPUT_RUN_MATRIX_COMMANDS='printf "\033[31m"; head -c 300000 /dev/zero | tr "\0" x; printf "\033[0m"; exit 7'
export INPUT_RUN_MATRIX_WORKING_DIRECTORY="$work"
export INPUT_RUN_MATRIX_STREAM_OUTPUT=false
export INPUT_RUN_MATRIX_GENERATE_SUMMARY=false
export GITHUB_OUTPUT="$work/outputs"
if (set +u; run_matrix_single_command_mode) > "$work/runner.log" 2>&1; then
    echo 'Expected a failed matrix command' >&2
    exit 1
fi
set -e
sed -n '/^run-results<<EOF$/,/^EOF$/p' "$GITHUB_OUTPUT" | sed '1d;$d' > "$work/results.json"
jq -e 'length == 1 and .[0].exit_code == 7 and (.[0].error_output | length) == 300000 and (.[0].error_output | contains("\u001b") | not)' "$work/results.json"
grep -q 'run-failed=1' "$GITHUB_OUTPUT"
! grep -q 'Argument list too long' "$work/runner.log"
echo 'Large failure output preserved without argv overflow'
