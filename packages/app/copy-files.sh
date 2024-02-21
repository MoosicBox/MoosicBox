#!/usr/bin/env bash

dir=$1
[[ -z $dir ]] && dir='../../../MoosicBoxUI'

echo "Copying $dir/src/*/"
rm -rf src/*/
echo "Copying $dir/public"
rm -rf public && cp -r "$dir/public" public
echo "Copying $dir/src/components"
cp -r "$dir/src/components" src/components
echo "Copying $dir/src/layouts"
cp -r "$dir/src/layouts" src/layouts
echo "Copying $dir/src/middleware"
cp -r "$dir/src/middleware" src/middleware
echo "Copying $dir/src/pages"
cp -r "$dir/src/pages" src/pages
echo "Copying $dir/src/routes"
cp -r "$dir/src/routes" src/routes
echo "Copying $dir/src/services"
cp -r "$dir/src/services" src/services
echo "Copying $dir/src/env.d.ts"
cp "$dir/src/env.d.ts" src/env.d.ts
echo "Copying $dir/src/sst-env.d.ts"
cp "$dir/src/sst-env.d.ts" src/sst-env.d.ts
echo "Copying $dir/render-directive"
cp -r "$dir/render-directive" ./
