#!/usr/bin/env bash
# Render the actual CI templates and execute them against a fake Cargo boundary.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
source "$root/lib/template.sh"
work=$(mktemp -d)
export INPUT_RUN_MATRIX_TEMPLATE_VARS='{"coverage-output":"coverage.lcov"}'
steps="$root/../../clippier/run-matrix/rust-validation.yml"
format=$(yq -r '.format.commands' "$steps")
coverage=$(yq -r '.coverage.commands' "$steps")
export TRACE="$work/trace"
mkdir -p "$work/bin"
printf '#!/usr/bin/env bash\nprintf "%%s\\n" "$*" >> "$TRACE"\nexit "${CARGO_STATUS:-0}"\n' > "$work/bin/cargo"
chmod +x "$work/bin/cargo"
export PATH="$work/bin:$PATH"
for os in windows-latest ubuntu-latest macos-latest; do
    package=$(jq -nc --arg os "$os" '{name:"fixture",os:$os}')
    rendered=$(render_template "$format" "$package" '' '' 0 1)
    : > "$TRACE"
    bash -c "$rendered"
    if [[ "$os" == windows-latest ]]; then
        grep -qx 'fmt --package fixture -- --check' "$TRACE"
    else
        grep -qx 'fmt --all -- --check' "$TRACE"
    fi
done
# Render the actual coverage template and preserve feature/nightly arguments.
export GITHUB_WORKSPACE=$(cd "$root/../../.." && pwd)
for name in moosicbox fixture; do
    for features in '' player default; do
        package=$(jq -nc --arg name "$name" '{name:$name,os:"ubuntu-latest"}')
        rendered=$(render_template "$coverage" "$package" "$features" '' 0 1)
        : > "$TRACE"
        bash -c "$rendered"
        grep -qx 'llvm-cov report --lcov --output-path coverage.lcov' "$TRACE"
        if CARGO_STATUS=7 bash -c "$rendered"; then exit 1; else [[ $? == 7 ]]; fi
    done
done
# Exercise the real helper with controlled Cargo JSON/test-list evidence.
cat > "$work/bin/cargo" <<'CARGO'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$TRACE"
if [[ "$*" == *'llvm-cov report'* ]]; then
    case "${SCENARIO:-missing}" in
        missing|tests|bad-list|build-failure|partial)
            [[ "${SCENARIO:-}" != partial ]] || printf broken > "$OUTPUT"
            echo 'error: failed to merge profile data: not found *.profraw files in /tmp/fixture' >&2 ;;
        no-targets)
            echo 'warning: not found *.profraw files in /tmp/fixture' >&2
            echo 'error: no input files specified. See llvm-profdata merge -help' >&2
            echo "error: failed to merge profile data: process didn't exit successfully: llvm-profdata" >&2 ;;
        no-mapping)
            echo "error: failed to generate report: process didn't exit successfully: llvm-cov" >&2
            echo 'error: failed to load coverage: fixture: no coverage data found' >&2
            echo 'error: could not load coverage information' >&2 ;;
        corrupt) echo 'error: malformed instrumentation profile' >&2 ;;
        mixed)
            echo 'error: failed to load coverage: fixture: no coverage data found' >&2
            echo 'error: permission denied' >&2 ;;
        *) exit 0 ;;
    esac
    exit 7
fi
[[ "$*" == *'llvm-cov test --no-report --message-format=json'* ]] || exit 9
[[ "${SCENARIO:-}" != build-failure ]] || exit 8
if [[ "${SCENARIO:-}" == no-targets ]]; then
    echo '{"reason":"build-finished","success":true}'
    exit 0
fi
printf '%s\n' '{"reason":"compiler-artifact","profile":{"test":true},"executable":"fixture"}' '{"reason":"build-finished","success":true}'
case "${SCENARIO:-}" in
    tests) printf 'regression_test: test\n1 test, 0 benchmarks\n' ;;
    bad-list) echo 'unexpected test runner output' ;;
    *) echo '0 tests, 0 benchmarks' ;;
esac
CARGO
helper="$GITHUB_WORKSPACE/.github/clippier/run-matrix/coverage.sh"
export OUTPUT="$work/coverage.lcov"
SCENARIO=missing bash "$helper" "$OUTPUT" +nightly --no-default-features --features fixture
[[ ! -f "$OUTPUT" ]]
jq -e '.status == "NO_INSTRUMENTABLE_COVERAGE" and .test_count == 0' "$OUTPUT.status.json"
grep -qx '+nightly llvm-cov test --no-report --message-format=json --no-default-features --features fixture -- --list' "$TRACE"
for scenario in no-targets no-mapping; do
    SCENARIO="$scenario" bash "$helper" "$work/$scenario.lcov" --no-default-features
    jq -e '.status == "NO_INSTRUMENTABLE_COVERAGE"' "$work/$scenario.lcov.status.json"
done
for scenario in tests bad-list build-failure corrupt mixed partial; do
    if SCENARIO="$scenario" bash "$helper" "$OUTPUT" --no-default-features; then exit 1; fi
done
# No output requested means no report invocation.
export INPUT_RUN_MATRIX_TEMPLATE_VARS='{}'
rendered=$(render_template "$coverage" '{"name":"fixture"}' '' '' 0 1)
: > "$TRACE"
bash -c "$rendered"
[[ ! -s "$TRACE" ]]
echo 'CI formatting, coverage evidence and failure propagation tests passed'
