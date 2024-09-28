#!/usr/bin/env bash

dir=$1
[[ -z $dir ]] && dir='../../website'

echo "Copying $dir/public"
rm -rf public
mkdir -p public
cp -r "$dir/public/." app-public/. public

for srcDir in "$dir"/src/*/; do
    srcDirName=$(basename "${srcDir%*/}")

    echo "Copying $dir/src/${srcDirName}"
    rm -rf "src/${srcDirName}"
    mkdir -p "src/${srcDirName}"
    cp -r "$dir/src/${srcDirName}/." "src/app-${srcDirName}/." "src/${srcDirName}"
done

echo "Copying $dir/src/env.d.ts"
cp "$dir/src/env.d.ts" src/env.d.ts
echo "Copying $dir/src/sst-env.d.ts"
cp "$dir/src/sst-env.d.ts" src/sst-env.d.ts
echo "Copying $dir/render-directive"
cp -r "$dir/render-directive" ./
