#!/usr/bin/env bash
# Exercise the exact capture/render helpers used by the report workflow.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/bin"
export PATH="$work/bin:$PATH" MOCK_STATE="$work/count" REPOSITORY=fixture/repo RUN_ID=123
cat > "$work/bin/gh" <<'MOCK'
#!/usr/bin/env bash
count=$(cat "$MOCK_STATE")
count=$((count + 1))
echo "$count" > "$MOCK_STATE"
if [[ $MOCK_MODE == recovery && $count == 1 ]] || [[ $MOCK_MODE == failure ]]; then
    echo '{"message":"Server Error"}'
    exit 1
fi
if [[ $MOCK_MODE == partial ]]; then
    echo '{"jobs":[{"name":"discarded","html_url":"url","status":"completed","conclusion":"success"}]}'
    echo '{"message":"Server Error"}'
    exit 1
fi
if [[ $MOCK_MODE == malformed ]]; then
    echo '{"jobs":[null]}'
    exit 0
fi
if [[ $MOCK_MODE == empty ]]; then exit 0; fi
printf '%s\n' '{"jobs":[{"name":"Tests | Linux","html_url":"https://example.com/job","status":"completed","conclusion":"success"}]}'
printf '%s\n' '{"jobs":[{"name":"Queued","html_url":"https://example.com/queued","status":"queued","conclusion":null}]}'
MOCK
# Avoid delays in the bounded-retry tests.
printf '#!/usr/bin/env bash\nexit 0\n' > "$work/bin/sleep"
chmod +x "$work/bin/gh" "$work/bin/sleep"
for mode in recovery failure partial malformed empty; do
    export MOCK_MODE="$mode"
    echo 0 > "$MOCK_STATE"
    if bash "$root/lib/workflow-job-health.sh" capture "$work/jobs.json"; then
        [[ $mode == recovery ]]
        [[ $(cat "$MOCK_STATE") == 2 ]]
        jq -e '.metadata_available and (.jobs | length == 2)' "$work/jobs.json"
        bash "$root/lib/workflow-job-health.sh" render "$work/jobs.json" > "$work/report"
        grep -Fq 'Tests \| Linux' "$work/report"
        grep -Fq '| queued |' "$work/report"
    else
        [[ $mode != recovery ]]
        [[ $(cat "$MOCK_STATE") == 3 ]]
        jq -e '.metadata_available == false and .jobs == []' "$work/jobs.json"
        bash "$root/lib/workflow-job-health.sh" render "$work/jobs.json" > "$work/report"
        grep -q 'Job health is unknown' "$work/report"
        ! grep -q 'discarded' "$work/report"
    fi
done
# Old error-shaped files and missing inputs must also render without jq errors.
for data in '{"jobs":[{"message":"Server Error"}]}' '{"metadata_available":true,"jobs":[null]}' 'invalid'; do
    printf '%s\n' "$data" > "$work/jobs.json"
    bash "$root/lib/workflow-job-health.sh" render "$work/jobs.json" > "$work/report"
    grep -q 'Job health is unknown' "$work/report"
done
bash "$root/lib/workflow-job-health.sh" render "$work/missing.json" | grep -q 'Job health is unknown'
echo 'Workflow job health regression tests passed'
