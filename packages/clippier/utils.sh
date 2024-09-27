#!/usr/bin/env bash

function cargo_each_feature() {
    while read -r feature; do
        echo "cargo $* --features \"$feature\""
        cargo "$@" --features "$feature" || return 1
    done <<<"$(moosicbox_clippier Cargo.toml)"

    echo "done"
}
