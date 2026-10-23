#!/usr/bin/env bash
# Demo setup on a running stack: the walkthrough admin (demo-admin@crux.test, password in
# deploy/secrets/demo-accounts.txt), already verified, plus one fresh invite code to print.
set -euo pipefail
BASE="${1:-http://localhost:8000}"
COMPOSE="docker compose -f deploy/compose.yaml --env-file .env"
PW=$(awk '/^demo-admin/ {print $3}' deploy/secrets/demo-accounts.txt)
code=$($COMPOSE exec -T core crux-core create-invite | tr -d '\r')
curl -fsS -X POST "$BASE/api/v1/auth/signup" -H 'content-type: application/json' \
    -d "{\"inviteCode\":\"$code\",\"email\":\"demo-admin@crux.test\",\"username\":\"demo-admin\",\"firstName\":\"Demo\",\"lastName\":\"Admin\",\"password\":\"$PW\",\"timeZone\":\"Europe/Amsterdam\"}" >/dev/null
$COMPOSE exec -T core crux-core make-admin demo-admin@crux.test >/dev/null
$COMPOSE exec -T mysql mysql -uroot -p"${MYSQL_ROOT_PASSWORD:-crux-root}" -e \
    "UPDATE crux_core.Users SET EmailVerifiedAt = UTC_TIMESTAMP() WHERE Email = 'demo-admin@crux.test'" 2>/dev/null
echo "demo-admin ready (admin, verified)"
echo "invite for you: $($COMPOSE exec -T core crux-core create-invite --days 14 | tr -d '\r')"
