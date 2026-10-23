#!/usr/bin/env bash
# Post-start sanity: error lines in service logs, ai absent = 502, merged OpenAPI document.
# Usage: scripts/check-logs.sh [base-url]
BASE="${1:-http://localhost:8000}"
COMPOSE="docker compose -f deploy/compose.yaml --env-file .env"
echo "== log lines mentioning errors"
$COMPOSE logs --no-color core core-jobs files files-worker notify notify-worker gateway 2>&1 \
    | grep -iE '"level":"(error|warn)|panic|traceback|exception' | head -30
echo "== ai (not deployed): $(curl -s -o /dev/null -w '%{http_code}' "$BASE/api/v1/ai/healthz")"
echo "== merged OpenAPI"
curl -s "$BASE/api/v1/openapi.json" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d["openapi"], len(d["paths"]), "paths")'
