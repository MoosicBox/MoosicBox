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
# Only the empty facade has no executable code. All other cases must report,
# including facade cases with features, and must propagate reporting failures.
for name in moosicbox fixture; do
    for features in '' player default; do
        package=$(jq -nc --arg name "$name" '{name:$name,os:"ubuntu-latest"}')
        rendered=$(render_template "$coverage" "$package" "$features" '' 0 1)
        : > "$TRACE"
        bash -c "$rendered"
        if [[ "$name" == moosicbox && -z "$features" ]]; then
            [[ ! -s "$TRACE" ]]
        else
            grep -qx 'llvm-cov report --lcov --output-path coverage.lcov' "$TRACE"
            if CARGO_STATUS=7 bash -c "$rendered"; then exit 1; else [[ $? == 7 ]]; fi
        fi
    done
done
echo 'CI formatting and coverage template tests passed'
