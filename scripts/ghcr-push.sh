#!/usr/bin/env bash
# Builds Crux's images for a hosted install (linux/amd64) and pushes them to your registry, tagged
# with the commit and `latest`. Log in first (a GitHub token with write:packages):
#   echo "$TOKEN" | docker login ghcr.io -u <you> --password-stdin
#   CRUX_IMAGES=ghcr.io/<you> scripts/ghcr-push.sh          # add --ai to include the ai image
# The packages are private by default on GHCR; the server pulls with a read:packages token.
set -euo pipefail
cd "$(dirname "$0")/.."

: "${CRUX_IMAGES:?set CRUX_IMAGES, e.g. ghcr.io/<your-github-user>}"
if [ -n "$(git status --porcelain)" ]; then
    echo "warning: uncommitted changes are in the build, but the tag names the last commit" >&2
fi
TAG="$(git rev-parse --short HEAD)"
PLATFORM="${CRUX_PLATFORM:-linux/amd64}"
WITH_AI=0
[ "${1:-}" = "--ai" ] && WITH_AI=1

build() { # name dockerfile [build-arg]
    local name=$1 file=$2
    shift 2
    echo "== crux-$name ($TAG)"
    docker buildx build --platform "$PLATFORM" -f "$file" "$@" \
        -t "$CRUX_IMAGES/crux-$name:$TAG" -t "$CRUX_IMAGES/crux-$name:latest" --push .
}

build core core/Dockerfile
build files services/Dockerfile --build-arg SERVICE=files
build notify services/Dockerfile --build-arg SERVICE=notify
build gateway deploy/truenas/gateway.Dockerfile
build mysql deploy/truenas/mysql.Dockerfile
[ "$WITH_AI" = 1 ] && build ai services/Dockerfile --build-arg SERVICE=ai

echo "Pushed $TAG. On the server: docker compose pull && docker compose up -d"
