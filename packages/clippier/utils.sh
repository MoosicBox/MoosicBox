#!/usr/bin/env bash

function cargo_each_feature() {
    while read -r feature; do
        local command="cargo $* --features \"$feature\""

        echo "RUNNING \`$command\`"

        if cargo "$@" --features "$feature"; then
            echo "FINISHED \`$command\`"
        else
            >&2 echo "FAILED \`$command\`"
            return 1
        fi
    done <<<"$(moosicbox_clippier Cargo.toml)"

    echo "done"
}
