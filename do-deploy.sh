#!/usr/bin/env bash

SERVICE=$1
KUBERNETES_DIRECTORY=$2
[[ -z $KUBERNETES_DIRECTORY ]] && { echo "Must pass kubernetes directory as second argument" >&2; exit 1; }

docker compose build "$SERVICE" || exit 1
docker tag "$SERVICE" "registry.digitalocean.com/moosicbox/$SERVICE" || exit 1
doctl registry login
docker push "registry.digitalocean.com/moosicbox/$SERVICE" || exit 1
(cd "$KUBERNETES_DIRECTORY"; kubectl apply -f .) || exit 1
kubectl rollout restart "deployment/$SERVICE"
