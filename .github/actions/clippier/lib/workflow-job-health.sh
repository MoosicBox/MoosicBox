#!/usr/bin/env bash
# Keep API errors and partial pagination out of the published job metadata.
set -euo pipefail

case "${1:?Expected capture or render}" in
    capture)
        output=${2:?Expected output path}
        mkdir -p "$(dirname "$output")"
        temporary=$(mktemp "${output}.XXXXXX")
        trap 'rm -f "$temporary" "$temporary.pages"' EXIT
        # Publish an explicit unavailable state even if every request fails.
        printf '%s\n' '{"jobs":[],"metadata_available":false}' > "$temporary"
        mv "$temporary" "$output"
        for attempt in 1 2 3; do
            if gh api --paginate \
                -H "Accept: application/vnd.github+json" \
                "/repos/$REPOSITORY/actions/runs/$RUN_ID/jobs?per_page=100" \
                > "$temporary.pages" \
                && jq -se '
                    length > 0 and all(.[];
                        (.jobs | type == "array") and all(.jobs[];
                            (.name | type == "string") and
                            (.html_url | type == "string") and
                            (.status | type == "string") and
                            (.conclusion == null or (.conclusion | type == "string"))))
                ' "$temporary.pages" > /dev/null \
                && jq -s '{jobs: [.[].jobs[]], metadata_available: true}' \
                    "$temporary.pages" > "$temporary"; then
                mv "$temporary" "$output"
                exit 0
            fi
            echo "::warning::Workflow job metadata request $attempt/3 failed or returned invalid data"
            if [[ $attempt != 3 ]]; then sleep "$((attempt * 2))"; fi
        done
        echo '::error::Workflow job metadata unavailable after 3 attempts; job health is unknown'
        exit 1
        ;;
    render)
        input=${2:?Expected metadata path}
        if ! jq -e '
            .metadata_available == true and (.jobs | type == "array") and
            all(.jobs[];
                (.name | type == "string") and (.html_url | type == "string") and
                (.status | type == "string") and
                (.conclusion == null or (.conclusion | type == "string")))
        ' "$input" > /dev/null 2>&1; then
            echo '**Workflow job metadata unavailable. Job health is unknown; this is not evidence of success.**'
            exit 0
        fi
        echo '| Job | Conclusion | URL |'
        echo '| --- | --- | --- |'
        jq -r '.jobs[] | select(.name != "Generate Clippier Report") | "| \(.name | gsub("\\|"; "\\|")) | \(.conclusion // .status) | [Open job](\(.html_url)) |"' "$input"
        ;;
    *) echo 'Expected capture or render' >&2; exit 2 ;;
esac
