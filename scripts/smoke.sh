#!/usr/bin/env bash
# Smoke test against a running stack: health of every service through the gateway, signup with an
# invite, /me, user.created reaching files and notify (M1); then a course with a lecture, a PDF
# uploaded to its pre-signed URL through the gateway, scanned, extracted, attached as a material,
# its page text and a download link (M2); a mock answered and graded (M3); a flashcard reviewed (M4); an exam planned, an event, /next and /week (M5).
# Usage: scripts/smoke.sh [base-url]   (default http://localhost:8000)
set -euo pipefail

BASE="${1:-http://localhost:8000}"
COMPOSE="docker compose -f deploy/compose.yaml"
fail() { echo "FAIL: $*" >&2; exit 1; }

# Services may still be finishing start-up (migrations, upstream connect): retry up to 60 s.
for path in /api/v1/healthz /api/v1/files/healthz /api/v1/notify/healthz; do
    for i in $(seq 1 60); do
        curl -fsS "$BASE$path" >/dev/null 2>&1 && break
        [ "$i" = 60 ] && fail "$path"
        sleep 1
    done
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
    [ "$files" = 1 ] && [ "$notify" = 1 ] && break
    sleep 1
done
[ "$files" = 1 ] && [ "$notify" = 1 ] || fail "user.created did not reach files ($files) and notify ($notify) within 30 s"
echo "ok   user.created reached files and notify"

# ---------- milestone 2 ----------
j() { python3 -c "import json,sys; d=json.load(sys.stdin); print(eval('d' + sys.argv[1]))" "$1"; }
api() { curl -fsS -X "$1" "$BASE/api/v1$2" -H "authorization: Bearer $token" -H 'content-type: application/json' ${3:+-d "$3"}; }

course=$(api POST /courses '{"name":"Smoke Statistics","code":"SMK"}' | j '["id"]')
lecture=$(api POST "/courses/$course/components" '{"code":"L1","name":"P-values","estHours":2}' | j '["id"]')
echo "ok   course $course with lecture $lecture"

pdf="scripts/fixtures/sample.pdf"
size=$(wc -c < "$pdf" | tr -d ' ')
sha=$(sha256sum "$pdf" | cut -d' ' -f1)
started=$(api POST /files/uploads "{\"fileName\":\"sample.pdf\",\"sizeBytes\":$size,\"sha256\":\"$sha\",\"courseId\":\"$course\"}")
file=$(echo "$started" | j '["file"]["id"]')
url=$(echo "$started" | j '["uploadUrl"]')
case "$url" in "$BASE"/crux-files/*) ;; *) fail "upload URL should be same-origin under /crux-files, got $url" ;; esac
curl -fsS -X PUT -H 'content-type: application/pdf' --data-binary "@$pdf" "$url" >/dev/null || fail "PUT to the pre-signed URL"
api POST "/files/$file/complete" >/dev/null || fail "complete"
for _ in $(seq 1 30); do
    state=$(api GET "/files/$file" | j '["scanStatus"] + "/" + d["extractStatus"]')
    [ "$state" = "Clean/Done" ] && break
    sleep 1
done
[ "$state" = "Clean/Done" ] || fail "file not scanned+extracted in 30 s ($state)"
echo "ok   upload through the gateway, scanned and extracted"

material=$(api POST "/components/$lecture/materials" "{\"kind\":\"Slides\",\"title\":\"Week 1\",\"fileId\":\"$file\"}" | j '["id"]')
api GET "/files/$file/pages/2/text" | grep -q "confidence intervals" || fail "page text"
dl=$(api GET "/files/$file/url" | j '["url"]')
curl -fsS "$dl" | head -c 5 | grep -q '%PDF-' || fail "download through the signed URL"
echo "ok   material $material, page text, signed download"

# ---------- milestone 3 ----------
q=$(api POST "/courses/$course/questions" "{\"componentId\":\"$lecture\",\"type\":\"MC\",\"content\":{\"prompt\":\"A p-value is…\",\"options\":[{\"text\":\"P(data at least this extreme | H0)\",\"isCorrect\":true},{\"text\":\"P(H0 is true)\",\"isCorrect\":false}]}}")
right=$(echo "$q" | python3 -c "import json,sys; print([o['id'] for o in json.load(sys.stdin)['current']['options'] if o['isCorrect']][0])")
mock=$(api POST "/courses/$course/mocks" '{"n":5}' | j '["id"]')
api POST "/mocks/$mock/start" >/dev/null
api PUT "/mocks/$mock/questions/1/answer" "{\"choiceOptionId\":\"$right\",\"confidence\":\"Sure\"}" >/dev/null
score=$(api POST "/mocks/$mock/submit" | j '["status"] + " " + str(d["score"]) + "/" + str(d["maxScore"])')
[ "$score" = "Graded 1.0/1.0" ] || fail "mock result: $score"
echo "ok   mock built, answered, auto-graded ($score)"

# ---------- milestone 4 ----------
card=$(api POST "/courses/$course/flashcards" "{\"componentId\":\"$lecture\",\"front\":\"p-value?\",\"back\":\"P(data | H0)\",\"shared\":true}")
code=$(echo "$card" | j '["code"]')
[ "$code" = SMK001 ] || fail "card code $code"
cardid=$(echo "$card" | j '["id"]')
api POST "/courses/$course/subscription" '{}' >/dev/null
now=$(date -u +%Y-%m-%dT%H:%M:%S.000Z)
rid=$(python3 -c "import random,string; print('01'+''.join(random.choice('0123456789ABCDEFGHJKMNPQRSTVWXYZ') for _ in range(24)))")
st=$(api POST /flash/reviews "{\"reviews\":[{\"clientId\":\"$rid\",\"flashcardId\":\"$cardid\",\"rating\":3,\"reviewedAt\":\"$now\"}]}" | j '["results"][0]["status"] + "/" + d["results"][0]["card"]["progress"]["state"]')
[ "$st" = "applied/Review" ] || fail "flash review: $st"
echo "ok   flashcard $code reviewed ($st)"

# ---------- milestone 5 ----------
exam_at=$(date -u -d '+10 days' +%Y-%m-%dT08:00:00.000Z)
api POST "/courses/$course/deadlines" "{\"type\":\"Exam\",\"startsAt\":\"$exam_at\",\"durationMin\":90}" >/dev/null || fail "create exam"
planned=$(api GET "/plan?to=$exam_at" | python3 -c "import json,sys; print(len(json.load(sys.stdin)))")
[ "$planned" -gt 0 ] || fail "no plan entries after adding an exam"
eid=$(python3 -c "import random; print('01'+''.join(random.choice('0123456789ABCDEFGHJKMNPQRSTVWXYZ') for _ in range(24)))")
acc=$(api POST /events "{\"events\":[{\"clientId\":\"$eid\",\"type\":\"material.page\",\"occurredAt\":\"$now\",\"componentId\":\"$lecture\",\"payload\":{\"page\":1}}]}" | j '["accepted"]')
[ "$acc" = 1 ] || fail "event not accepted"
reason=$(api GET /next | j '["reason"]')
case "$reason" in SMK*) ;; *) fail "/next reason: $reason" ;; esac
api GET /week | grep -q '"isoWeek"' || fail "/week"
echo "ok   exam planned ($planned entries), event accepted, /next: $reason"
