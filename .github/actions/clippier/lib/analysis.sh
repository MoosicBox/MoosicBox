#!/usr/bin/env bash
# Pure JSON/command boundary, also sourced by regression tests.
run_clippier_json() {
    local command="$1" output status
    if output=$(eval "$command"); then
        :
    else
        status=$?
        echo "Clippier failed (exit $status)" >&2
        return "$status"
    fi
    # Slurping distinguishes no documents from a valid empty array and rejects
    # multiple documents. jq empty alone accepts empty stdout.
    if ! printf '%s' "$output" | jq -e -s 'length == 1' >/dev/null; then
        echo 'Clippier must produce exactly one JSON document' >&2
        return 1
    fi
    printf '%s' "$output" | jq -c .
}

validate_case_array() {
    jq -e -s 'length == 1 and (.[0] | type == "array" and all(.[];
        type == "object" and (.name | type == "string") and
        (.path | type == "string") and (.os | type == "string") and
        (.features | type == "array" and all(.[]; type == "string"))))' >/dev/null
}
