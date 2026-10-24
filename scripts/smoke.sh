#!/usr/bin/env bash
# Smoke test against a running stack: health of every service through the gateway, signup with an
# invite, /me, user.created reaching files and notify (M1); then a course with a lecture, a PDF
# uploaded to its pre-signed URL through the gateway, scanned, extracted, attached as a material,
# its page text and a download link (M2); a mock answered and graded (M3); a flashcard reviewed (M4); an exam planned, an event, /next and /week (M5); a live inbox notification (M6); storage saver and thumbnails (M7); AI chat and generation, or service_unavailable without ai (M8).
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

# ---------- milestone 6 ----------
# A second user listens on the inbox stream; sharing the course with them arrives as a live
# `notification` (course.shared is always in-app) and lands in their inbox.
code2=$($COMPOSE exec -T core crux-core create-invite | tr -d '\r')
user2="${user}b"
body2=$(printf '{"inviteCode":"%s","email":"%s@example.com","username":"%s","firstName":"Smoke","lastName":"Two","password":"correct horse battery","timeZone":"Europe/Amsterdam"}' "$code2" "$user2" "$user2")
token2=$(curl -fsS -X POST "$BASE/api/v1/auth/signup" -H 'content-type: application/json' -d "$body2" | j '["accessToken"]')
ticket=$(curl -fsS -X POST "$BASE/api/v1/sse-ticket" -H "authorization: Bearer $token2" | j '["ticket"]')
stream_out=$(mktemp)
curl -sN --max-time 20 "$BASE/api/v1/notify/stream?ticket=$ticket" > "$stream_out" &
stream_pid=$!
for _ in $(seq 1 20); do grep -q '^event: ready' "$stream_out" && break; sleep 0.5; done
grep -q '^event: ready' "$stream_out" || fail "inbox stream did not open through the gateway"
api POST "/courses/$course/members" "{\"usernameOrEmail\":\"$user2\",\"role\":\"Viewer\"}" >/dev/null || fail "share"
for _ in $(seq 1 30); do grep -q '^event: notification' "$stream_out" && break; sleep 0.5; done
grep -q '^event: notification' "$stream_out" || fail "no live notification within 15 s"
kill "$stream_pid" 2>/dev/null || true
kind=$(curl -fsS "$BASE/api/v1/notify/inbox" -H "authorization: Bearer $token2" | j '["items"][0]["kind"]')
[ "$kind" = course.shared ] || fail "inbox: $kind"
prefs=$(curl -fsS -X PUT "$BASE/api/v1/notify/preferences" -H "authorization: Bearer $token2" -H 'content-type: application/json' \
    -d '{"preferences":[{"kind":"flash.due","channel":"Email","enabled":true}]}' | python3 -c "import json,sys; print([k['email'] for k in json.load(sys.stdin)['kinds'] if k['kind']=='flash.due'][0])")
[ "$prefs" = True ] || fail "preferences: $prefs"
echo "ok   live inbox notification through the gateway, inbox, preferences"

# ---------- milestone 7 ----------
# Free space on the material: files moves the original to PendingPurge (event), undo brings it back.
st=$(api POST "/materials/$material/free-space" | j '["originalStatus"]')
[ "$st" = PendingPurge ] || fail "free-space: $st"
for _ in $(seq 1 30); do fst=$(api GET "/files/$file" | j '["status"]'); [ "$fst" = PendingPurge ] && break; sleep 0.5; done
[ "$fst" = PendingPurge ] || fail "files didn't follow the purge request ($fst)"
api POST "/materials/$material/free-space/cancel" >/dev/null || fail "undo"
for _ in $(seq 1 30); do fst=$(api GET "/files/$file" | j '["status"]'); [ "$fst" = Active ] && break; sleep 0.5; done
[ "$fst" = Active ] || fail "undo didn't reach files ($fst)"
# Thumbnails on: they're made for the files already there.
api PATCH /files/storage/settings '{"keepThumbnails":true}' >/dev/null || fail "storage settings"
for _ in $(seq 1 40); do curl -fsS -o /dev/null "$BASE/api/v1/files/$file/pages/1/thumbnail" -H "authorization: Bearer $token" 2>/dev/null && ok=1 && break; ok=0; sleep 0.5; done
[ "$ok" = 1 ] || fail "no thumbnail within 20 s"
echo "ok   storage saver request + undo reached files, thumbnails made"

# ---------- milestone 8 ----------
# With the ai profile: consent, a key (the dev-only fake provider), the gate seen from core, a chat
# reply streamed through the gateway, and a generation job. Without it: core says AI is unavailable.
if [ "$(curl -s -o /dev/null -w '%{http_code}' "$BASE/api/v1/ai/healthz")" = 200 ]; then
    policy=$(api GET /ai/settings | j '["currentPolicyVersion"]')
    api PATCH /ai/settings "{\"enabled\":true,\"consentPolicyVersion\":\"$policy\"}" >/dev/null || fail "AI consent"
    kst=$(api POST /ai/keys '{"provider":"fake","apiKey":"fake-valid-smoke-0001"}' | j '["status"]')
    [ "$kst" = Valid ] || fail "AI key: $kst"
    allowed=$(api GET "/me/ai-allowed?feature=MaterialChat&courseId=$course" | j '["allowed"]')
    [ "$allowed" = True ] || fail "core doesn't see the AI gate open"
    conv=$(api POST /ai/conversations "{\"courseId\":\"$course\",\"scope\":\"Material\",\"materialId\":\"$material\"}" | j '["id"]')
    reply=$(curl -sN --max-time 30 -X POST "$BASE/api/v1/ai/conversations/$conv/messages" -H "authorization: Bearer $token" \
        -H 'content-type: application/json' -d '{"content":"What are confidence intervals?"}')
    echo "$reply" | grep -q '^event: delta' && echo "$reply" | grep -q '^event: done' || fail "chat stream: $(echo "$reply" | head -c 300)"
    job=$(curl -fsS -X POST "$BASE/api/v1/ai/generate" -H "authorization: Bearer $token" -H 'content-type: application/json' \
        -d "{\"courseId\":\"$course\",\"componentId\":\"$lecture\",\"n\":2}" | j '["id"]')
    for _ in $(seq 1 30); do gst=$(api GET "/ai/generate/$job" | j '["status"]'); [ "$gst" = Done ] && break; sleep 1; done
    [ "$gst" = Done ] || fail "generation job: $gst"
    echo "ok   AI: consent, key, gate via core, chat stream, generation job"
else
    reason=$(api GET "/me/ai-allowed?feature=MaterialChat" | j '["reason"]')
    [ "$reason" = service_unavailable ] || fail "without ai, core should say service_unavailable (got $reason)"
    echo "ok   ai profile off: core reports service_unavailable"
fi
