#!/usr/bin/env bash
# Report coverage, accepting absent mappings only with independently verified
# zero-test evidence for this exact package/feature configuration.
set -euo pipefail
output=$1
shift
cargo_prefix=()
if [[ ${1:-} == +* ]]; then cargo_prefix+=("$1"); shift; fi
log=$(mktemp "${RUNNER_TEMP:-/tmp}/clippier-coverage.XXXXXX")
listing=$(mktemp "${RUNNER_TEMP:-/tmp}/clippier-test-list.XXXXXX")
trap 'rm -f "$log" "$listing"' EXIT
if cargo "${cargo_prefix[@]}" llvm-cov report --lcov --output-path "$output" > "$log" 2>&1; then
    cat "$log"
    exit 0
else
    status=$?
fi
cat "$log" >&2
# Never turn corrupt profiles, compiler failures or arbitrary report failures
# into an empty-coverage success. The Python classifier fails closed.
python3 "$GITHUB_WORKSPACE/.github/clippier/run-matrix/coverage_evidence.py" classify "$log" || exit "$status"
cargo "${cargo_prefix[@]}" llvm-cov test --no-report --message-format=json "$@" -- --list > "$listing"
python3 "$GITHUB_WORKSPACE/.github/clippier/run-matrix/coverage_evidence.py" verify "$listing" || exit "$status"
# A failed report may have created a partial file. Do not upload it as coverage.
if [[ -f "$output" ]]; then
    [[ ! -s "$output" ]] || { echo 'Unexpected partial coverage output' >&2; exit "$status"; }
    rm "$output"
fi
printf '%s\n' '{"status":"NO_INSTRUMENTABLE_COVERAGE","test_count":0,"evidence":"successful exact-configuration test enumeration"}' > "$output.status.json"
echo 'No instrumentable coverage: exact configuration has zero tests (status artifact recorded)'
