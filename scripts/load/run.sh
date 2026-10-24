#!/usr/bin/env bash
# Runs the k6 load test against a stack (spec 14 M9). Creates the load users first (a test-stack
# CLI that refuses CRUX_ENV=prod), then runs k6 from its official image on the host network.
# Usage: scripts/load/run.sh [base-url] [vus] [duration]   e.g. scripts/load/run.sh http://localhost:8000 500 5m
set -euo pipefail

BASE="${1:-http://localhost:8000}"
VUS="${2:-500}"
DURATION="${3:-5m}"
COMPOSE="docker compose -f deploy/compose.yaml --env-file .env"
DIR="$(cd "$(dirname "$0")" && pwd)"

$COMPOSE exec -T core crux-core load-users --count "$VUS" --hours 3 | tr -d '\r' > "$DIR/tokens.txt"
echo "load users: $(wc -l < "$DIR/tokens.txt")"
docker run --rm --network host -v "$DIR:/load" -e BASE="$BASE" -e VUS="$VUS" -e DURATION="$DURATION" -e RAMP="${RAMP:-1m}" \
    -e TOKENS_FILE=/load/tokens.txt grafana/k6:0.54.0 run --summary-trend-stats "avg,p(50),p(95),p(99),max" /load/crux.js
rm -f "$DIR/tokens.txt"
