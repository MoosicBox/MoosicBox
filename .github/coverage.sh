#!/usr/bin/env bash

set -euo pipefail

# Get all package names
PACKAGES=($(cargo metadata --no-deps --format-version=1 | jq -r '.packages[].name'))
BATCH_SIZE=20

for ((i=0; i<${#PACKAGES[@]}; i+=BATCH_SIZE)); do
  BATCH=("${PACKAGES[@]:i:BATCH_SIZE}")
  echo "Processing batch $((i/BATCH_SIZE + 1)): ${BATCH[*]}"

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

  # Critical: Remove profraw files after each batch
  find target -name "*.profraw" -delete
done
