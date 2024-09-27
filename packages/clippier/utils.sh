#!/usr/bin/env bash

function cargo_each_feature() {
    while read -r feature; do
        local command="cargo $* --features \"$feature\""

        echo "RUNNING \`$command\`"

        if [[ -z "$CLIPPIER_DRY_RUN" ]]; then
            if cargo "$@" --features "$feature"; then
                echo "FINISHED \`$command\`"
            else
                >&2 echo "FAILED \`$command\`"
                return 1
            fi
        fi
    done <<<"$(moosicbox_clippier Cargo.toml)"

    echo "done"
}

function cargo_each_package_each_feature() {
    while read -r package; do
        local command="cargo_each_feature $*"
        local dir="${package/Cargo.toml/}"

        if ! (
            cd "$dir" || return 1

            echo "IN \`$dir\` RUNNING \`$command\`"

            if [[ -z "$CLIPPIER_DRY_RUN" ]]; then
                if cargo_each_feature "$@"; then
                    echo "IN \`$dir\` FINISHED \`$command\`"
                else
                    >&2 echo "IN \`$dir\` FAILED \`$command\`"
                    return 1
                fi
            fi
        ); then
            return 1
        fi
    done <<<"$(find packages -name 'Cargo.toml')"

    echo "done"
}
