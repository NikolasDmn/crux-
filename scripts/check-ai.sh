#!/usr/bin/env bash
# Starts the optional ai profile and checks it: health through the gateway, built-in tutors seeded,
# no errors in its logs. Usage: scripts/check-ai.sh [base-url]
set -uo pipefail
BASE="${1:-http://localhost:8000}"
COMPOSE="docker compose -f deploy/compose.yaml --env-file .env --profile ai"
$COMPOSE up -d --build --wait ai ai-worker 2>&1 | tail -1
sleep 5
echo "ai health: $(curl -s -o /dev/null -w '%{http_code}' "$BASE/api/v1/ai/healthz")"
echo "tutors: $($COMPOSE exec -T mysql mysql -uroot -p"${MYSQL_ROOT_PASSWORD:-crux-root}" -N -e 'SELECT GROUP_CONCAT(TemplateKey) FROM crux_ai.AiTutorTemplate' 2>/dev/null)"
echo "== ai log problems"
$COMPOSE logs --no-color ai ai-worker 2>&1 | grep -iE '"level": ?"(error|warning)"|traceback' | head -5
