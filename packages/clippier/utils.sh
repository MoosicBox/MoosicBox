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
    done <<<"$(clippier features .)"

    echo "done"
}

function each_feature_permutation() {
    local ignore=()
    local all_features=()

    while read -r feature; do
        if [[ "$feature" == "default" ]] || [[ "$feature" == "fail-on-warnings" ]]; then
            continue
        fi

        all_features+=("$feature")
    done <<<"$(clippier features .)"

    function each_feature_permutation_inner() {
        local feature_count=$1
        local features=("${@:2:$feature_count}")
        local feature_offset=$((feature_count + 2))
        local ignore_count="${*:$feature_offset:1}"
        local ignore_offset=$((feature_offset + 1))
        local ignore_features=("${@:$ignore_offset:$ignore_count}")

        local features_string=""

        for x in "${features[@]}"; do
            if [[ -n "$features_string" ]]; then
                features_string="${features_string},"
            fi
            features_string="${features_string}${x}"
        done

        echo "$features_string"

        for feature in "${all_features[@]}"; do
            local contains=0

            for x in "${features[@]}"; do
                if [[ "$x" == "$feature" ]]; then
                    contains=1
                fi
            done
            for x in "${ignore[@]}"; do
                if [[ "$x" == "$feature" ]]; then
                    contains=1
                fi
            done
            for x in "${ignore_features[@]}"; do
                if [[ "$x" == "$feature" ]]; then
                    contains=1
                fi
            done

            if [[ "$contains" == "1" ]]; then
                continue
            fi

            local local_features=("${features[@]}")
            local_features+=("$feature")
            local feature_count2=$((feature_count + 1))
            ignore_feature+=("$feature")
            ignore_count=$((ignore_count + 1))

            each_feature_permutation_inner "$feature_count2" "${local_features[@]}" "$ignore_count" "${ignore_feature[@]}"
        done
    }

    echo ""
    echo "default"

    for feature in "${all_features[@]}"; do
        ignore+=("$feature")
        each_feature_permutation_inner 1 "$feature" 0
    done
}

function cargo_each_feature_permutation() {
    while read -r features; do
        if [[ -n "$features" ]]; then
            local command="cargo $* --features \"$features\""

            echo "RUNNING \`$command\`"

            if [[ -z "$CLIPPIER_DRY_RUN" ]]; then
                if cargo "$@" --features "$features"; then
                    echo "FINISHED \`$command\`"
                else
                    >&2 echo "FAILED \`$command\`"
                    return 1
                fi
            fi
        else
            local command="cargo $*"

            echo "RUNNING \`$command\`"

            if [[ -z "$CLIPPIER_DRY_RUN" ]]; then
                if cargo "$@"; then
                    echo "FINISHED \`$command\`"
                else
                    >&2 echo "FAILED \`$command\`"
                    return 1
                fi
            fi
        fi
    done <<<"$(each_feature_permutation)"

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

function cargo_each_package() {
    while read -r package; do
        local command="cargo $*"
        local dir="${package/Cargo.toml/}"

        if ! (
            cd "$dir" || return 1

            echo "IN \`$dir\` RUNNING \`$command\`"

            if [[ -z "$CLIPPIER_DRY_RUN" ]]; then
                if cargo "$@"; then
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

function cargo_each_package_each_feature_permutation() {
    while read -r package; do
        local command="cargo_each_feature_permutation $*"
        local dir="${package/Cargo.toml/}"

        if ! (
            cd "$dir" || return 1

            echo "IN \`$dir\` RUNNING \`$command\`"

            if [[ -z "$CLIPPIER_DRY_RUN" ]]; then
                if cargo_each_feature_permutation "$@"; then
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
