#!/usr/bin/env bash
# The real stack coordinator invokes the real case executor in a throwaway repo.
set -eo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
mkdir -p "$work/repo" "$work/temp" "$work/action/lib"
cp "$root"/lib/{analysis,case-stack,run-matrix,template}.sh "$work/action/lib/"
cp "$root/action.sh" "$work/action/action.sh"
export GH_PAGER=cat GH_PROMPT_DISABLED=1 GIT_TERMINAL_PROMPT=0
# No remote access, only a disposable local fixture repository.
git --no-pager -C "$work/repo" init --quiet
printf 'clean\n' > "$work/repo/state"
git --no-pager -C "$work/repo" add state
git --no-pager -C "$work/repo" -c user.name=Test -c user.email=test@example.invalid -c commit.gpgsign=false commit --quiet -m fixture
source "$work/action/lib/case-stack.sh"
export INPUT_COMMAND=run-matrix
export SCRIPT_DIR="$work/action" GITHUB_WORKSPACE="$work/repo" RUNNER_TEMP="$work/temp"
export GITHUB_OUTPUT="$work/output" GITHUB_STEP_SUMMARY="$work/summary"
export INPUT_RUN_MATRIX_STEPS='' INPUT_RUN_MATRIX_STEPS_FILE=''
export INPUT_RUN_MATRIX_TEMPLATE_VARS='{}' INPUT_CASE_COVERAGE=false
export INPUT_RUN_MATRIX_WORKING_DIRECTORY='.' INPUT_RUN_MATRIX_LABEL=fixture
export INPUT_RUN_MATRIX_STREAM_OUTPUT=false INPUT_RUN_MATRIX_GENERATE_SUMMARY=false
export INPUT_RUN_MATRIX_COMMANDS='test "$(cat state)" = clean || exit 8; echo dirty > state; printf "%s\n" "{{clippier.feature-flags}}" >> "$TRACE"; if [[ "{{clippier.feature-flags}}" == *bad* ]]; then exit 7; fi'
export TRACE="$work/trace"
package='{"name":"fixture","path":".","os":"ubuntu-latest"}'
export INPUT_RUN_MATRIX_PACKAGE_JSON
INPUT_RUN_MATRIX_PACKAGE_JSON=$(printf '%s' "$package" | jq '. + {cases:[{id:"case-000000",package:(. + {features:["bad"]})},{id:"case-000001",package:(. + {features:["good"]})}]}')
if run_case_stack > "$work/log" 2>&1; then
    echo 'Failed case incorrectly succeeded' >&2
    exit 1
fi
receipts=$(sed -n 's/^case-receipts-path=//p' "$GITHUB_OUTPUT")
jq -s -e 'length == 2 and .[0].exit_code != 0 and .[1].exit_code == 0 and .[1].case.package.features == ["good"]' "$receipts"/*.json
[[ $(wc -l < "$TRACE") == 2 ]]
[[ $(cat "$work/repo/state") == clean ]]
# The same execution path handles singletons without a secondary implementation.
INPUT_RUN_MATRIX_PACKAGE_JSON=$(printf '%s' "$package" | jq '. + {cases:[{id:"case-000002",package:(. + {features:["good"]})}]}')
run_case_stack >> "$work/log" 2>&1
[[ $(wc -l < "$TRACE") == 3 ]]
[[ $(sed -n '2p' "$TRACE") == "$(sed -n '3p' "$TRACE")" ]]
# Replay the same complete multi-step lifecycle for each case. Failure in the
# middle step must not suppress finalization or the next case.
export INPUT_RUN_MATRIX_COMMANDS=''
export INPUT_RUN_MATRIX_STEPS
INPUT_RUN_MATRIX_STEPS=$(jq -n '{
    setup: {commands: "test \"$(cat state)\" = clean || exit 8; echo dirty > state", strategy:"combined"},
    check: {commands: "printf \"%s\\n\" \"{{clippier.feature-flags}}\" >> \"$TRACE\"; if [[ \"{{clippier.feature-flags}}\" == *bad* ]]; then exit 7; fi", label:"Check", strategy:"combined"},
    finish: {commands: "echo final >> \"$FINAL_TRACE\"", label:"Finalize", strategy:"combined"}
}')
export FINAL_TRACE="$work/final-trace"
INPUT_RUN_MATRIX_PACKAGE_JSON=$(printf '%s' "$package" | jq '. + {cases:[{id:"case-000003",package:(. + {features:["bad","other"]})},{id:"case-000004",package:(. + {features:["good","other"]})}]}')
if run_case_stack >> "$work/log" 2>&1; then
    echo 'Multi-step failed case incorrectly succeeded' >&2
    exit 1
fi
[[ $(wc -l < "$FINAL_TRACE") == 2 ]]
[[ $(wc -l < "$TRACE") == 5 ]]
echo "Stack execution/isolation/failure-continuation tests passed ($work)"
