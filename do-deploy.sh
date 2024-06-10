#!/usr/bin/env bash

SERVICE=$1

docker compose build "$SERVICE" || exit 1
docker tag "$SERVICE" "registry.digitalocean.com/moosicbox/$SERVICE" || exit 1
doctl registry login
docker push "registry.digitalocean.com/moosicbox/$SERVICE" || exit 1
kubectl rollout restart "deployment/$SERVICE"
