#!/usr/bin/env bash
# Cold start from an EMPTY database (drops the crux volumes), then smoke test and log scan.
# Proves migrations run from empty and no process races another at start.
# Usage: scripts/verify-stack.sh [base-url] [--ai]
set -uo pipefail
BASE="${1:-http://localhost:8000}"
PROFILE=""
[ "${2:-}" = "--ai" ] && PROFILE="--profile ai"
COMPOSE="docker compose -f deploy/compose.yaml --env-file .env $PROFILE"

# Stop every profile's containers: profile services left running would outlive the wiped database.
docker compose -f deploy/compose.yaml --env-file .env --profile ai --profile full down -v 2>&1 | tail -1
$COMPOSE up -d --build --wait 2>&1 | tail -1
sleep 8
bash scripts/smoke.sh "$BASE" || exit 1
echo "== idle 20 s (blocking reads must not time out), then scan logs"
sleep 20
echo "== error/warning lines in service logs (excluding Caddy dev notices)"
problems=$($COMPOSE logs --no-color 2>&1 \
    | grep -iE '"level": ?"(error|warn|warning)"|traceback|panicked' \
    | grep -vE 'admin endpoint disabled|automatic HTTPS'     | { if [ -z "$PROFILE" ]; then grep -vE 'lookup ai on'; else cat; fi; } )
if [ -n "$problems" ]; then
    echo "$problems" | cut -c1-220 | head -20
    exit 1
fi
echo "none"
