#!/usr/bin/env bash
# Failed validation must retain diagnostics without weakening matrix failures.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
export GITHUB_WORKSPACE="$work" GITHUB_STEP_SUMMARY="$work/summary" INPUT_COMMAND=run-matrix
export GITHUB_OUTPUT="$work/output"
source "$root/action.sh"

# Exercise the actual JSON boundary with a failing CLI stub.
build_clippier_command() {
    printf '%s\n' 'printf "%s" "$VALIDATION_JSON"; exit 7'
}
export VALIDATION_JSON='{"errors":[{"package":"fixture","errors":[{"feature":"fail-on-warnings"}]}]}'
export INPUT_COMMAND=validate-feature-propagation
output=$(run_clippier) && exit 1 || status=$?
[[ "$status" == 7 ]]
[[ $(printf '%s' "$output" | jq -r '.errors[0].package') == fixture ]]

# Stub only the command construction; main still runs run_clippier and the
# JSON/output/summary handling. A failed validation exits from main.
should_skip_on_no_changes() { return 1; }
print_debug_environment() { :; }
print_action_invocation() { :; }
detect_git_range() { GIT_BASE=fixture; GIT_HEAD=fixture; }
inject_custom_reasoning() { printf '%s' "$1"; }
generate_validation_summary() { printf '%s' "$1" > "$GITHUB_STEP_SUMMARY"; }
export INPUT_GENERATE_VALIDATION_SUMMARY=true
if (main); then
    echo 'Failed validation was accepted' >&2
    exit 1
fi
grep -q 'raw-output<<EOF' "$GITHUB_OUTPUT"
grep -q fixture "$GITHUB_OUTPUT"
grep -q fixture "$GITHUB_STEP_SUMMARY"

# Non-error validation exits (e.g. expired overrides) retain their exit code.
export VALIDATION_JSON='{"errors":[]}'
if (main); then
    echo 'Nonzero validator exit was lost' >&2
    exit 1
else
    [[ $? == 7 ]]
fi

# Ordinary analysis must continue to fail closed and discard failed output.
export INPUT_COMMAND=packages
if output=$(run_clippier); then
    echo 'Failed analysis was accepted' >&2
    exit 1
fi
[[ -z "$output" ]]
echo 'Validation diagnostics regression tests passed'
