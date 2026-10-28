#!/usr/bin/env bash
# The real stack coordinator invokes the real case executor in a throwaway repo.
set -eo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
mkdir -p "$work/repo" "$work/temp" "$work/action/lib"
cp "$root"/lib/{analysis,case-stack,run-matrix,shard-plan,template}.sh "$work/action/lib/"
cp "$root/action.sh" "$work/action/action.sh"
export GH_PAGER=cat GH_PROMPT_DISABLED=1 GIT_TERMINAL_PROMPT=0
# No remote access, only a disposable local fixture repository.
git --no-pager -C "$work/repo" init --quiet
printf 'clean\n' > "$work/repo/state"
git --no-pager -C "$work/repo" add state
git --no-pager -C "$work/repo" -c user.name=Test -c user.email=test@example.invalid -c commit.gpgsign=false commit --quiet -m fixture
export INPUT_COMMAND=run-matrix
export INPUT_STACK_CASES=false
export SCRIPT_DIR="$work/action" GITHUB_WORKSPACE="$work/repo" RUNNER_TEMP="$work/temp"
export GITHUB_OUTPUT="$work/output" GITHUB_STEP_SUMMARY="$work/summary"
source "$work/action/action.sh"
export INPUT_RUN_MATRIX_STEPS='' INPUT_RUN_MATRIX_STEPS_FILE=''
export INPUT_RUN_MATRIX_TEMPLATE_VARS='{}' INPUT_CASE_COVERAGE=false
export INPUT_RUN_MATRIX_WORKING_DIRECTORY='.' INPUT_RUN_MATRIX_LABEL=fixture
export INPUT_RUN_MATRIX_STREAM_OUTPUT=false INPUT_RUN_MATRIX_GENERATE_SUMMARY=false
export INPUT_RUN_MATRIX_COMMANDS='test "$(cat state)" = clean || exit 8; echo dirty > state; printf "%s\n" "{{clippier.feature-flags}}" >> "$TRACE"; if [[ "{{clippier.feature-flags}}" == *bad* ]]; then exit 7; fi'
export TRACE="$work/trace"
package='{"name":"fixture","path":".","os":"ubuntu-latest"}'
export INPUT_RUN_MATRIX_PACKAGE_JSON
INPUT_RUN_MATRIX_PACKAGE_JSON=$(printf '%s' "$package" | jq '. + {cases:[{id:"case-000000",package:(. + {features:["bad"]})},{id:"case-000001",package:(. + {features:["good"]})}]}')
# Workflow-added naming fields must neither affect execution nor identity.
INPUT_RUN_MATRIX_PACKAGE_JSON=$(printf '%s' "$INPUT_RUN_MATRIX_PACKAGE_JSON" | jq '. + {osEmoji:"🐧", stackSuffix:" · 2 cases", customLabel:"anything"}')
if execute_matrix_entry > "$work/log" 2>&1; then
    echo 'Failed case incorrectly succeeded' >&2
    exit 1
fi
receipts=$(sed -n 's/^case-receipts-path=//p' "$GITHUB_OUTPUT")
canonical_hash=$(printf '%s' "$INPUT_RUN_MATRIX_PACKAGE_JSON" | jq -c '(.cases[0].package | del(.features)) + {cases: .cases}' | tr -d '\n' | md5sum | cut -c1-12)
grep -q "^failure-artifact-name=clippier-diagnostics-job-$canonical_hash$" "$GITHUB_OUTPUT"
# Cosmetic additions cannot mask a changed setup value.
original_job="$INPUT_RUN_MATRIX_PACKAGE_JSON"
INPUT_RUN_MATRIX_PACKAGE_JSON=$(printf '%s' "$original_job" | jq '.os = "windows-latest"')
if execute_matrix_entry >> "$work/log" 2>&1; then exit 1; fi
INPUT_RUN_MATRIX_PACKAGE_JSON="$original_job"
jq -s -e 'length == 2 and .[0].exit_code != 0 and .[1].exit_code == 0 and .[1].case.package.features == ["good"]' "$receipts"/*.json
[[ $(wc -l < "$TRACE") == 2 ]]
[[ $(cat "$work/repo/state") == clean ]]
# The same execution path handles singletons without a secondary implementation.
INPUT_RUN_MATRIX_PACKAGE_JSON=$(printf '%s' "$package" | jq '. + {cases:[{id:"case-000002",package:(. + {features:["good"]})}]}')
execute_matrix_entry >> "$work/log" 2>&1
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
if execute_matrix_entry >> "$work/log" 2>&1; then
    echo 'Multi-step failed case incorrectly succeeded' >&2
    exit 1
fi
[[ $(wc -l < "$FINAL_TRACE") == 2 ]]
[[ $(wc -l < "$TRACE") == 5 ]]
echo "Stack execution/isolation/failure-continuation tests passed ($work)"
# Empty features and enabled features must be separate full invocations. Each
# case gets a clean process and checkout, including after the middle failure.
export INPUT_RUN_MATRIX_STEPS
INPUT_RUN_MATRIX_STEPS=$(jq -n '{
    check: {commands: "test \"$(cat state)\" = clean || exit 8; test -z \"${CASE_LEAK:-}\" || exit 9; export CASE_LEAK=dirty; echo dirty > state; printf \"%s\\n\" \"{{clippier.feature-flags}}\" >> \"$EXACT_TRACE\"; if [[ \"{{clippier.feature-flags}}\" == *bad* ]]; then exit 7; fi", label:"Check", strategy:"sequential"},
    finish: {commands: "echo finalized >> \"$EXACT_FINAL\"", label:"Finalize", strategy:"combined"}
}')
export EXACT_TRACE="$work/exact-trace" EXACT_FINAL="$work/exact-final"
INPUT_RUN_MATRIX_PACKAGE_JSON=$(printf '%s' "$package" | jq '. + {cases:[{id:"case-000005",package:(. + {features:[]})},{id:"case-000006",package:(. + {features:["bad"]})},{id:"case-000007",package:(. + {features:["good"]})}]}')
root_state_before=$(cat "$work/repo/state")
if execute_matrix_entry >> "$work/log" 2>&1; then exit 1; fi
[[ $(wc -l < "$EXACT_TRACE") == 3 ]]
[[ $(wc -l < "$EXACT_FINAL") == 3 ]]
[[ $(sed -n '1p' "$EXACT_TRACE") == '--features=fail-on-warnings' ]]
[[ $(sed -n '2p' "$EXACT_TRACE") == '--features=fail-on-warnings,bad' ]]
[[ $(sed -n '3p' "$EXACT_TRACE") == '--features=fail-on-warnings,good' ]]
receipts=$(sed -n 's/^case-receipts-path=//p' "$GITHUB_OUTPUT" | tail -1)
jq -s -e 'sort_by(.case.id) | map(.exit_code) == [0,1,0]' "$receipts"/*.json
[[ $(cat "$work/repo/state") == "$root_state_before" ]]
echo 'Empty/enabled feature invocations and process isolation preserved'
