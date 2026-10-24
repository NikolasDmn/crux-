#!/usr/bin/env bash
# Demo data on a running stack (spec 13 "make seed loads one demo course"):
# - demo-admin@crux.test (admin, verified; password in deploy/secrets/demo-accounts.txt)
# - the demo course "Business Statistics" (BSA): 2 lectures, notes, an uploaded PDF, a past paper
# - one fresh invite code, printed
# Safe to run twice: it skips what already exists.
set -euo pipefail
BASE="${1:-http://localhost:8000}"
COMPOSE="docker compose -f deploy/compose.yaml --env-file .env"
PW=$(awk '/^demo-admin/ {print $3}' deploy/secrets/demo-accounts.txt)
j() { python3 -c "import json,sys; d=json.load(sys.stdin); print(eval('d' + sys.argv[1]))" "$1"; }

login() {
    curl -fsS -X POST "$BASE/api/v1/auth/login" -H 'content-type: application/json' \
        -d "{\"login\":\"demo-admin@crux.test\",\"password\":\"$PW\"}" 2>/dev/null | j '["accessToken"]'
}

token=$(login 2>/dev/null || true)
if [ -z "$token" ]; then
    code=$($COMPOSE exec -T core crux-core create-invite | tr -d '\r')
    curl -fsS -X POST "$BASE/api/v1/auth/signup" -H 'content-type: application/json' \
        -d "{\"inviteCode\":\"$code\",\"email\":\"demo-admin@crux.test\",\"username\":\"demo-admin\",\"firstName\":\"Demo\",\"lastName\":\"Admin\",\"password\":\"$PW\",\"timeZone\":\"Europe/Amsterdam\"}" >/dev/null
    $COMPOSE exec -T core crux-core make-admin demo-admin@crux.test >/dev/null
    $COMPOSE exec -T mysql mysql -uroot -p"${MYSQL_ROOT_PASSWORD:-crux-root}" -e \
        "UPDATE crux_core.Users SET EmailVerifiedAt = UTC_TIMESTAMP() WHERE Email = 'demo-admin@crux.test'" 2>/dev/null
    token=$(login)
fi
api() { curl -fsS -X "$1" "$BASE/api/v1$2" -H "authorization: Bearer $token" -H 'content-type: application/json' ${3:+-d "$3"}; }
echo "demo-admin ready (admin, verified)"

# A plain user for trying the web app (verified; password on its line in demo-accounts.txt).
WPW=$(awk '/^webtest/ {print $3}' deploy/secrets/demo-accounts.txt)
if [ -n "$WPW" ]; then
    if curl -fsS -o /dev/null -X POST "$BASE/api/v1/auth/login" -H 'content-type: application/json'         -d "{\"login\":\"webtest\",\"password\":\"$WPW\"}" 2>/dev/null; then
        echo "webtest already there"
    else
        wcode=$($COMPOSE exec -T core crux-core create-invite | tr -d '')
        curl -fsS -o /dev/null -X POST "$BASE/api/v1/auth/signup" -H 'content-type: application/json'             -d "{\"inviteCode\":\"$wcode\",\"email\":\"webtest@crux.test\",\"username\":\"webtest\",\"firstName\":\"Web\",\"lastName\":\"Tester\",\"password\":\"$WPW\",\"timeZone\":\"Europe/Amsterdam\",\"acceptBreachedPassword\":true}"
        $COMPOSE exec -T mysql mysql -uroot -p"${MYSQL_ROOT_PASSWORD:-crux-root}" -e             "UPDATE crux_core.Users SET EmailVerifiedAt = UTC_TIMESTAMP() WHERE Email = 'webtest@crux.test'" 2>/dev/null
        echo "webtest ready (verified)"
    fi
fi

if api GET /courses | grep -q '"code":"BSA"'; then
    echo "demo course already there"
else
    course=$(api POST /courses '{"name":"Business Statistics","code":"BSA","institution":"Demo University","term":"2026 Q1"}' | j '["id"]')
    l1=$(api POST "/courses/$course/components" '{"code":"L1","name":"Descriptive statistics","estHours":3}' | j '["id"]')
    l2=$(api POST "/courses/$course/components" "{\"code\":\"L2\",\"name\":\"Hypothesis testing\",\"estHours\":4,\"prerequisiteIds\":[\"$l1\"]}" | j '["id"]')
    api POST "/components/$l1/materials" '{"kind":"Notes","title":"Mean, median, spread","content":"# Descriptive statistics\n\nThe mean is $\\bar{x} = \\frac{1}{n}\\sum x_i$; the median is the middle value.\n\nSpread: variance $s^2$ and standard deviation $s$."}' >/dev/null
    for c in "Central tendency" "Spread" "P-values" "Confidence intervals"; do api POST "/courses/$course/concepts" "{\"name\":\"$c\"}" >/dev/null; done

    pdf="scripts/fixtures/sample.pdf"
    started=$(api POST /files/uploads "{\"fileName\":\"hypothesis-testing.pdf\",\"sizeBytes\":$(wc -c < "$pdf" | tr -d ' '),\"sha256\":\"$(sha256sum "$pdf" | cut -d' ' -f1)\",\"courseId\":\"$course\"}")
    file=$(echo "$started" | j '["file"]["id"]')
    if [ "$(echo "$started" | j '["alreadyPresent"]')" = "False" ]; then
        curl -fsS -X PUT -H 'content-type: application/pdf' --data-binary "@$pdf" "$(echo "$started" | j '["uploadUrl"]')" >/dev/null
        api POST "/files/$file/complete" >/dev/null
    fi
    for _ in $(seq 1 30); do
        [ "$(api GET "/files/$file" | j '["scanStatus"]')" = "Clean" ] && break
        sleep 1
    done
    api POST "/components/$l2/materials" "{\"kind\":\"Slides\",\"title\":\"Week 2 slides\",\"fileId\":\"$file\"}" >/dev/null
    api POST "/courses/$course/papers" '{"paperKey":"2025-06","label":"June 2025 exam","examDate":"2025-06-20","totalQuestions":30,"keyStatus":"Official","omissions":[{"questionNumber":"17","reason":"Topic dropped from the syllabus"}]}' >/dev/null
    echo "demo course BSA created ($course)"
fi
echo "invite for you: $($COMPOSE exec -T core crux-core create-invite --days 14 | tr -d '\r')"
