#!/usr/bin/env bash
# Smoke test against a running stack (milestone 1): health of every service through the gateway,
# signup with an invite, /me, and the user.created event reaching files and notify.
# Usage: scripts/smoke.sh [base-url]   (default http://localhost:8000)
set -euo pipefail

BASE="${1:-http://localhost:8000}"
COMPOSE="docker compose -f deploy/compose.yaml"
fail() { echo "FAIL: $*" >&2; exit 1; }

for path in /api/v1/healthz /api/v1/files/healthz /api/v1/notify/healthz; do
    curl -fsS "$BASE$path" >/dev/null || fail "$path"
done
echo "ok   health through the gateway"

code=$($COMPOSE exec -T core crux-core create-invite | tr -d '\r')
user="smoke$(date +%s)"
body=$(printf '{"inviteCode":"%s","email":"%s@example.com","username":"%s","firstName":"Smoke","lastName":"Test","password":"correct horse battery","timeZone":"Europe/Amsterdam"}' "$code" "$user" "$user")
resp=$(curl -fsS -X POST "$BASE/api/v1/auth/signup" -H 'content-type: application/json' -d "$body") || fail "signup"
token=$(echo "$resp" | python3 -c 'import json,sys; print(json.load(sys.stdin)["accessToken"])')
ref=$(echo "$resp" | python3 -c 'import json,sys; print(json.load(sys.stdin)["user"]["id"])')
echo "ok   signup $user ($ref)"

curl -fsS "$BASE/api/v1/me" -H "authorization: Bearer $token" | grep -q "\"$user\"" || fail "/me"
echo "ok   /me"

status=$(curl -s -o /dev/null -w '%{http_code}' "$BASE/api/v1/me")
[ "$status" = 401 ] || fail "/me without a token gave $status"
status=$(curl -s -o /dev/null -w '%{http_code}' "$BASE/internal/v1/jwks")
[ "$status" = 404 ] || fail "internal routes must not be exposed (got $status)"
echo "ok   auth required, internal routes hidden"

sql() { $COMPOSE exec -T mysql mysql -uroot -p"${MYSQL_ROOT_PASSWORD:-crux-root}" -N -e "$1" 2>/dev/null | tr -d '\r'; }
for _ in $(seq 1 30); do
    files=$(sql "SELECT COUNT(*) FROM crux_files.UserStorage WHERE UserRef='$ref'")
    notify=$(sql "SELECT COUNT(*) FROM crux_notify.Recipient WHERE UserRef='$ref'")
    [ "$files" = 1 ] && [ "$notify" = 1 ] && { echo "ok   user.created reached files and notify"; exit 0; }
    sleep 1
done
fail "user.created did not reach files ($files) and notify ($notify) within 30 s"
