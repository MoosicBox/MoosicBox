#!/usr/bin/env bash

set -euo pipefail

# Safe hook runner - silently does nothing if hook isn't defined
run_hook() {
    local hook_name="$1"

    # Check if function exists
    if declare -f "$hook_name" &>/dev/null; then
        "$hook_name"
        return
    fi

    # Check if environment variable exists
    local env_var="COVERAGE_${hook_name^^}"
    if [[ -n "${!env_var:-}" ]]; then
        eval "${!env_var}"
    fi

    # Otherwise, do nothing silently
}

# Main coverage logic
PACKAGES=($(cargo metadata --no-deps --format-version=1 | jq -r '.packages[].name'))
BATCH_SIZE="${COVERAGE_BATCH_SIZE:-20}"

run_hook "pre_all"

for ((i=0; i<${#PACKAGES[@]}; i+=BATCH_SIZE)); do
    BATCH=("${PACKAGES[@]:i:BATCH_SIZE}")
    echo "Processing batch $((i/BATCH_SIZE + 1)): ${BATCH[*]}"

    run_hook "pre_batch"

    # Build the -p arguments string
    PACKAGE_ARGS=""
    for pkg in "${BATCH[@]}"; do
        PACKAGE_ARGS="$PACKAGE_ARGS -p $pkg"
    done

    CMD="cargo llvm-cov test --no-report$PACKAGE_ARGS"
    echo "Running '$CMD'"

    # Run all packages in this batch together
    if ! cargo llvm-cov test --no-report$PACKAGE_ARGS; then
        >&2 echo "Error: Failed to generate coverage report"
        >&2 echo "Failed to run '$CMD'"
        exit 1
    fi

    run_hook "post_batch"
done

run_hook "post_all"
