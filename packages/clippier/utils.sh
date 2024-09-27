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

function cargo_each_feature_permutation() {
    function cargo_each_feature_permutation_inner() {
        local count=$1
        local end=$((count))
        local features=("${@:2:$end}")
        local offset=$((end + 2))

        while read -r feature; do
            if [[ "$feature" == "fail-on-warnings" ]]; then
                continue
            fi
            if [[ "$feature" == "default" ]]; then
                continue
            fi

            local contains=0

            for x in "${features[@]}"; do
                if [[ "$x" == "$feature" ]]; then
                    contains=1
                fi
            done

            if [[ "$contains" == "1" ]]; then
                continue
            fi

            features+=("$feature")
            count=$((count + 1))

            local features_string=""

            for x in "${features[@]}"; do
                if [[ -n "$features_string" ]]; then
                    features_string="${features_string},"
                fi
                features_string="${features_string}${x}"
            done

            local command="cargo ${*:$offset} --features \"$features_string\""

            echo "RUNNING \`$command\`"

            if [[ -z "$CLIPPIER_DRY_RUN" ]]; then
                if cargo "${@:$offset}" --features "$features_string"; then
                    echo "FINISHED \`$command\`"
                else
                    >&2 echo "FAILED \`$command\`"
                    return 1
                fi
            fi

            cargo_each_feature_permutation_inner "$count" "${features[@]}" "${@:$offset}"
        done <<<"$(moosicbox_clippier Cargo.toml)"
    }

    while read -r feature; do
        if [[ "$feature" == "fail-on-warnings" ]]; then
            continue
        fi

        local command="cargo ${*} --features \"$feature\""

        echo "RUNNING \`$command\`"

        if [[ -z "$CLIPPIER_DRY_RUN" ]]; then
            if cargo "${@}" --features "$feature"; then
                echo "FINISHED \`$command\`"
            else
                >&2 echo "FAILED \`$command\`"
                return 1
            fi
        fi

        if [[ "$feature" == "default" ]]; then
            continue
        fi

        local features=("$feature")
        cargo_each_feature_permutation_inner 1 "${features[@]}" "${@}"
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
