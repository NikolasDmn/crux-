# Open questions

Per backend-spec "Rules for the build agent": anything the spec doesn't cover is listed here, and the
smallest version that doesn't block the question is built. Each item says what was built meanwhile.
Answer inline (or tell the build agent) and the code follows.

## Spec ↔ schema differences (the schema wins; flagged here)

| # | Difference | What was built |
|---|---|---|
| S-1 | Schema headers name the databases `exam_core`, `exam_files`, `exam_ai`, `exam_notify`; the README says `crux_*`. | `crux_*` everywhere (compose, init.sql). The schema files are copied verbatim, so their header comments still say `exam_*`. |
| S-2 | `Invite` has no `PublicId`, but `DELETE /invites/{id}` and `/admin/invites` need an id, and only PublicIds may leave a service (3.1). | Core migration `0002_invite_public_id.sql` adds `Invite.PublicId CHAR(26) NULL UNIQUE`. |
| S-3 | `Tier` has no `PublicId`. | Admin tier routes address tiers by `Code` (`PATCH /admin/tiers/{code}`). |
| S-4 | Built-in tutor PublicIds `01JBUILTIN000…` aren't valid ULIDs (Crockford base32 excludes I, L, O, U). | PublicId parsing is loose everywhere: 26 chars of `[0-9A-Z]`. |
| S-5 | `Course.Color` comment says "owner/Editors change it"; spec 4.2 says Owner only. | Will follow the spec (Owner only) in milestone 2. |
| S-6 | `WeekSummary` (5.11) and the `user.purged` ack tracking (9.4) have no tables. | Not needed in M1. Will be added as core migrations in milestones 5 and 9. |
| S-7 | Spec 1 names MinIO for the object store. MinIO no longer publishes images (Docker Hub and quay.io both refuse pulls, checked 2026-10-23). | SeaweedFS (`chrislusf/seaweedfs`) with its S3 gateway. Services use plain S3 (`CRUX_S3_*`), so another S3 store (Garage, a hosted bucket) is a config change. **Decided (Nick 2026-10-23): SeaweedFS on the VPS in production too.** |

## Milestone 2 decisions (Nick 2026-10-23)

| # | Topic | Decision | Built as |
|---|---|---|---|
| D2-1 | Sharing with an email that has no account | Pending share + invite | `CourseShareInvite` row tied to an `Invite`; Crux emails the signup link; signing up through it adds the member. Uses one invite. |
| D2-2 | Shared course appears… | After the invitee **accepts** | `CourseMember.Status` Pending/Accepted (core migration 0004). Pending grants no access. Routes `GET /course-invitations`, `POST /course-invitations/{id}/accept`, `/decline` (not in spec 15). |
| D2-3 | Signing up through a course-share link | Counts as accepting | Member is `Accepted` right after signup. |
| D2-4 | Editor uploads to someone else's course | Counts against the **course owner's** quota | `StoredFile.OwnerUserRef` = course owner; uploader kept in `UploadedByUserRef` (files migration 0002). |
| D2-5 | Delete course | Trash for 30 days, owner can restore | `Course.ArchivedAt` = trashed; hidden from everyone; `POST /courses/{id}/restore` (not in spec 15); purge job after 30 days. |
| D2-6 | Default grade scale | Percent (0-100, pass 50) | Owner can change it. |
| D2-7 | Virus scan in dev | Fake scanner, unless a test targets ClamAV | `CRUX_SCANNER=fake` in dev; `clamd` in CI/prod; ClamAV tests run in a dedicated job. |
| D2-8 | OCR languages | English | Tesseract `eng`. |
| D2-9 | Plain invite with an email typed in | **Crux emails the invite link to that address** (the link can still be copied). Only that address can use it. | `invite.created` account email via notify. |

### Routes added beyond spec 15 (from the decisions above)

| Route | Why |
|---|---|
| `POST /courses/{id}/restore` | D2-5 trash with undo |
| `DELETE /courses/{id}/members/{memberId}` | Remove a member, withdraw an email share, or leave (spec 15 lists DELETE on `/members` without saying whom) |
| `GET /course-invitations`, `POST /course-invitations/{courseId}/accept`, `/decline` | D2-2 accept first |
| `GET /courses?trashed=true` | D2-5 trash list |

Other M2 build choices: creating a course subscribes the owner; accepting a share does not subscribe
(spec 5.1: "must subscribe to track progress"). Request bodies are checked **after** access, so a
stranger gets 404 rather than a validation error (spec 4.2); a body that can't be parsed at all is
422 for everyone.

### Milestone 2 build choices (files)

| # | Choice | Why |
|---|---|---|
| B2-1 | Uploads go browser → pre-signed URL → **the gateway at `/crux-files/*`** → SeaweedFS | Same origin: no CORS on the store, CSP stays `'self'`; Caddy keeps the Host header so the signature matches |
| B2-2 | Scan/extract work lives in a **`FileJob` table** (files migration 0002), not Redis | Durable, retried 3× with backoff (30 s, 60 s, 120 s), stuck jobs reclaimed after 15 min |
| B2-3 | A user's first upload before `user.created` arrives gets the **default 1 GiB quota** | The event overwrites it moments later; uploads never fail on event lag |
| B2-4 | Re-uploading a purged or deleted original reuses **the same row** (new storage key) | Spec 6.1 restore; also lets a user retry after an infected/abandoned upload |
| B2-5 | Core's trash purge (`course.deleted`) **deletes the course's uploads** and frees the owner's quota | Otherwise a purged course would keep counting against the owner forever |
| B2-6 | Breached list: SecLists **`100k-most-used-passwords-NCSC.txt`** (the "top 100k" file the spec meant has moved), filtered to ≥ 10 chars → 9,126 entries | Offline fallback for HIBP (Q-auth-3) |
| B2-7 | `file.infected` is a **non-transactional** `notify.requested` (in-app once milestone 6 lands) | It's about a user's own upload; not an account email |

## Milestone 3 build choices (bank, mocks, mistakes)

| # | Choice | Why / where |
|---|---|---|
| B3-1 | Core migration 0005 adds PublicIds to QuestionVersion, MCOption, Slip, Trap, Deadline; `UserQuestionStat.LastConfidence`; `WeakSpot.MissesAtLevel` + `ColdPassRun` | The API addresses all of them; rules need the counters |
| B3-2 | **Kept (Nick 2026-10-24).** **Editors can see and approve the course bank's drafts** | Spec 4.2 shows drafts only to their owner; then nobody could approve bank drafts |
| B3-3 | Readers may add **private** questions; bank questions need Editor | Spec 15 says "read / edit" for POST questions |
| B3-4 | **Kept (Nick 2026-10-24).** **A guessed right answer doesn't count as a Right** (stats, go-easy, "untested") and doesn't resolve slips or pass weak-spot checks | Spec 5.3 counts a guess as a miss |
| B3-5 | **Kept (Nick 2026-10-24).** **Right items show their key at once**; misses stay hidden until shown/fixed; Partial counts as a miss in review | Spec 5.4 hides the key "until review"; nothing to hide on a right answer |
| B3-6 | Slip **proposals are computed**, not stored: a Wrong/Partial item without a slip shows `slipProposed`; confirming = `POST /slips` with the mock + order | No schema change; the user confirms each (spec 5.4) |
| B3-7 | A Passed/Archived subscription **still opens old mocks** | Spec 4.2 says "active subscription"; archived courses keep their history (5.10) |
| B3-8 | "Cold" = the lecture wasn't studied in the previous 24 h: **no answers in it** (outside the current mock) **and no material, notes or lecture-focus events on it** (switched to UserEvents 2026-10-24) | Spec 5.6 |
| B3-9 | Warm-up attempts are always cold | Spec 5.9 |
| B3-10 | A manual severity change restarts the miss count at the new level | Otherwise 3 misses could jump two levels at once |
| B3-11 | AI grading mode answers `403 ai_disabled {reason: service_unavailable}` until milestone 8 | The gate isn't there yet |
| B3-12 | Answers don't use If-Match (the last save wins per item) | The client saves on every change (debounced); version checks would conflict constantly. Mock-level state changes are serialized by a row lock |
| B3-13 | Crops count against the source file's owner's quota | Spec says nothing; thumbnails count, crops are similar |
| B3-14 | New routes: `DELETE /courses/{id}/traps/{trapId}` | Spec 15's DELETE on `/traps` doesn't say which |

Infra fixes found by the cold-start check: worker loops are **supervised** (a crash restarts that loop, not the worker),
files waits for the object store, and every compose service has `restart: unless-stopped`.

## Milestone 4 build choices (flashcards)

| # | Choice | Why / where |
|---|---|---|
| B4-1 | **Kept (Nick 2026-10-24).** **FSRS-6 default weights** (the `fsrs` 6.6 crate, the reference implementation) instead of FSRS-5 | The crate moved to FSRS-6; weights are stored as `FsrsParamsVersion=fsrs6.6-dflt` so cards can be refit from the review log later |
| B4-2 | Learning steps outside FSRS: Again → 1 min, Hard → 6 min on new/learning cards; a lapse relearns in 10 min; Good/Easy graduate to FSRS days | FSRS only gives day intervals; Anki-style steps keep a failed card in today's session |
| B4-3 | Exam cap: a due date after the next Exam/Resit moves to **18:00 local the day before** (or now if that's passed) | Spec 5.5 says "before the exam" without a time |
| B4-4 | Codes are `<course code><3+ digits>`, assigned under a lock on the course row, gaps never reused | Spec 5.5 |
| B4-5 | **Viewers may make personal cards**; course (shared) cards need Owner/Editor; personal cards are 404 to everyone else (even the course owner) | Same pattern as private questions (B3-3) |
| B4-6 | A review batch never fails as a whole for one bad card: each item is `applied`, `stale` (older than the card's last review: logged, not rescheduled), `duplicate` (clientId seen) or `unknown` (card gone/not yours) | Offline phones must be able to drain their queue |
| B4-7 | Slip proposals from reviews: a **lapse** (Again on a Review card) or **Again while Sure**. Computed in the response, confirmed with `POST /slips {flashcardReviewClientId}` | Same as mock proposals (B3-6) |
| B4-8 | The daily new-card limit counts cards whose **first review** happened since local midnight (user's time zone); the review limit caps due cards per queue | Spec 5.5 |
| B4-9 | Archiving a subscription suspends that user's cards (`CourseArchived`); re-activating lifts only those, never a manual suspension | Spec 5.5 |
| B4-10 | Custom sets order by most lapses, then most overdue | Spec gives the filters, not the order |
| B4-11 | Reviews don't add weak-spot checks yet (cards aren't linked to concepts) | Revisit if cards get concepts |
| B4-12 | New routes: `PATCH /flashcards/{id}`, `POST /flashcards/{id}/suspend`, `GET /flash/cards?code=` | Spec 15 lists warmup-flag only; editing and suspending were described in 5.5 without routes |

## Milestone 5 build choices (time, plan, next)

| # | Choice | Why / where |
|---|---|---|
| B5-1 | Core migration 0006: `StudySession.PublicId`; `StudyPlanEntry.Kind` (Lecture/Revision/Mock/Other) + `PinnedAt`; `Deadline.EveRequestedAt` / `CheckInRequestedAt`; new `WeekSummary(UserId, IsoWeek, Data JSON, RebuiltAt)` | /next needs an entry's kind; pins need a flag; the 15-min tick must not re-request reminders |
| B5-2 | The user-event registry is **one file**, `contracts/user-events/registry.json`: each type is `client` or `server`, client types carry a payload JSON Schema. `POST /events` accepts client types only and rejects per item (`unknown_type`, `server_only`, `bad_payload`, `unknown_course`, `unknown_component`) instead of failing the batch | Offline batches must drain; server types can't be faked |
| B5-3 | Client `entityId`s are PublicIds, so they ride in `payload.entity`; `UserEvent.EntityId` holds internal ids for server events | The column is BIGINT |
| B5-4 | A lecture without an estimate is planned as **1.5 h**; a lecture's remaining time = estimate − hours already studied on it (at least 30 min while not Done) | Spec only sums estimates |
| B5-5 | Plan layout: study window **09:00–22:00 local**, blocks from 09:00 (or now, rounded up to 15 min), 30-min minimum block; horizon = the last exam (14 days with no exam, 120 max). The **timed mock is on the 2nd day before** the exam, revision the day before; spare final-week time goes to revision; a contested revision day goes to the earlier exam; lecture hours that land in the final week (or don't fit) are reported as "X h short" in `changeSummary` | Spec gives the rules, not the times |
| B5-6 | Pins: any edit sets `PinnedAt`. **Moving** = the old entry becomes `Moved` with `MovedToId` → a new pinned entry (returned). DELETE removes your own entries; on a planner entry it **skips and pins** it so a rebuild doesn't bring it back. A rebuild **keeps identical entries** (same ids) and replaces only what changed | Stable ids for the calendar UI |
| B5-7 | Rebuilds run synchronously in the same transaction on: deadline create/edit, subscribe/archive/unsubscribe, a lecture marked Done (or un-Done) | Spec lists the triggers |
| B5-8 | "Fell a day behind" = entries still Planned that ended before the user's local midnight → `Skipped`, then a rebuild. Checked inside `deadlines.tick` (every 15 min) rather than once a night | Fires once per day per user wherever their midnight is |
| B5-9 | Remaining hours: ±25% until 3 lectures are done; then estimate × actual/estimate ratio (clamped 0.25–4) **±10%** | Spec says "scaled" without a range |
| B5-10 | On track (WeekSummary, per course with an upcoming exam) = studied ≥ **80%** of the hours planned so far this week | Spec doesn't define it |
| B5-11 | `/next` target course: today's first Planned entry → else the nearest-exam course with an unfinished lecture (InProgress first) → else a course with due cards → else the first active course. Steps also carry `planEntryId` and `componentId`; exams awaiting a result come in a separate `checkIns` list | Spec's step list has no "check-in" kind |
| B5-12 | `POST /next/advance` is stateless: the client sends the step's `kind`, target and `planEntryId` (+ `skipped`). A done plan step marks its entry Done | No server-side step state |
| B5-13 | Warm-up: max 3 items; flagged cards first (not suspended); then one **approved** question per open weak spot **that has a concept** (topic-only spots have nothing to ask). The card's mini-lesson text is empty until the AI tutor (M8). Any flashcard review clears the flag | Spec 5.9 |
| B5-14 | Derived sessions are also rebuilt **on read** (`/sessions`, recent `/week`, course summary) for the last 2 days. Kind = majority event family (material/notes/lecture → Lecture, attempt → Practice, mock → Mock, flash → Flashcards, review → Review, ai → AiChat); ties → the first kind; sessions cap at 23 h | Spec: nightly + on demand |
| B5-15 | `WeekSummary` is rebuilt on read for the current and previous week (and any week not stored yet); older weeks come from the nightly build. First-try MC = the user's first ever answer to an MC question (a guessed Right isn't a right) | Spec 5.11 |
| B5-16 | Pass rule: the deadline's PassMark, else the scale's PassValue; Letter scales use the label's `pass` flag; Pass/Fail uses the text. A **pass** → subscription Passed + final grade + cards suspended (`CourseArchived`), out of plan, /next and warm-ups. A **fail** records the final grade with Passed = false and keeps the subscription Active; `offerResit` while no resit points at the exam. A grade needs status Done; leaving Done drops the grade | Spec 5.10 |
| B5-17 | Reminders: moving an exam resets its reminder flags; the eve reminder goes only before the exam starts; the check-in up to 24 h after 17:00 local | Spec 5.10 |
| B5-18 | `/courses/{id}/progress` uses policy Read (your own progress on a course you can read); deadlines need a subscription; warm-up needs an **Active** one | Spec says "owns" / "subscription" |
| B5-19 | Nightly jobs at fixed UTC hours: `sessions.derive` 02:00, `readmodels.rebuild` 03:00. **Changed (Nick 2026-10-24): the `UserQuestionStat` drift check is built** inside `readmodels.rebuild`: each active user's stats are replayed from the answer log (attempts + graded mocks; `Stat::apply` is order-free), drift is logged as a warning and repaired | Spec 10 "UserQuestionStat verify, logs drift" |
| B5-20 | New route: `POST /plan/entries` (spec 15 lists only `/plan/entries/{id}`) | Users add their own entries |
| B5-21 | Fix found while testing: purging a trashed course now deletes its slips, weak spots, plan, sessions, mocks, attempts, cards and questions first (they RESTRICT) | Purge would have failed for any course used since M3 |

## Milestone 6 build choices (notify)

| # | Choice | Why / where |
|---|---|---|
| B6-1 | **Kept (Nick 2026-10-24).** A request for a kind the user has **no channel on** for is **not stored** at all (no Notification row); unknown kinds are dropped too | Opt-in by default would otherwise fill the table with daily digests nobody wants |
| B6-2 | `PUT /notify/preferences` takes a list of `{kind, channel, enabled}` changes; `GET` returns the full matrix with labels, `inAppAlways` (course.shared, exam.checkin) and the email status (address, verified, suppressed) | Spec says "a matrix" without a shape |
| B6-3 | Same dedupe key again: **before delivery it reschedules** (new dueAt and payload), after delivery it's ignored. `notify.requested@1` gains an optional `cancel: true` (an additive field) to cancel by key | Spec 8: "the old one is cancelled by key" |
| B6-4 | Core: moving an exam re-requests the reminders it already requested (notify moves them); an exam that leaves Upcoming (Done, Missed, Cancelled) cancels both | Same |
| B6-5 | In-app = a delivery row that becomes Sent at `dueAt`: the inbox only lists delivered items (a reminder for tomorrow isn't in today's inbox). Push: Redis pub/sub `crux:notify:<userRef>`; the stream sends `ready`, then `notification` per item, `ping` every 20 s | Spec 3.4/8 |
| B6-6 | Email for non-account kinds is checked **at send time**: verified address, not suppressed, the kind still switched on; otherwise the delivery is Skipped with the reason | Preferences can change between request and send |
| B6-7 | Unsubscribe link: `…/notify/unsubscribe?token=<HMAC(secret, userRef)>&kind=<kind>` (only the hash is stored). **GET shows a confirm page** (mail scanners open links); the button or the RFC 8058 one-click POST applies it. `kind=all` puts the address on EmailSuppression (Unsubscribe) | A GET that changes state gets triggered by link scanners |
| B6-8 | **Decided (Nick 2026-10-24): Postmark.** SMTP via `CRUX_SMTP_URL=smtp+starttls://<token>:<token>@smtp.postmarkapp.com:587`; Postmark's bounce and spam-complaint webhooks go to `POST /notify/webhooks/postmark`, authenticated with HTTP Basic (password = `CRUX_EMAIL_WEBHOOK_SECRET`, as Postmark doesn't sign). Complaints and bounces Postmark marked Inactive suppress the address; soft bounces are ignored. The generic `/notify/webhooks/email` stays | Spec names no provider |
| B6-9 | `plan.today` (07:30 local) and `flash.due` (08:00 local, only with cards due) are produced by **core** in `deadlines.tick`, once per user per local day (Redis marker + dedupe key), and not after 12:00 local if the job was down | Core has the plan and the cards |
| B6-10 | `mock.graded` is requested whenever a mock becomes Graded (opt-in, so only users who turned it on get it) and when AI suggestions are ready; `import.ready` and `generation.done` come from ai (M8) | |
| B6-11 | Inbox paging: `?cursor=<id of the last item>`; every inbox/read response carries the unread count | |
| B6-12 | Gateway: compression only for a list of text/JSON types (never `text/event-stream`), `flush_interval -1` on the files and notify proxies | Compressed SSE frames sit in a buffer |
| B6-13 | New routes: `POST /notify/unsubscribe` (beside the GET in spec 15) and `POST /notify/webhooks/email` | B6-7, B6-8 |
| B6-14 | notify depends on `tzdata` (zoneinfo has no time-zone database on Windows dev machines) | Email/inbox times are shown in the user's zone |

## Milestone 7 build choices (storage saver)

| # | Choice | Why / where |
|---|---|---|
| B7-1 | **Files owns the purge state**; core mirrors it on materials. New contracts: core → files `material.purge_requested` / `material.purge_cancelled`; files → core `file.purge_scheduled` (auto purge), `file.purged`, `file.restored` | One source of truth for whether the object exists |
| B7-2 | A purge acts on the **file**, so every material using that file shows the same status (one upload can back several materials) | The object is shared |
| B7-3 | Files only accepts a purge request from the file's owner (the course owner whose quota it uses, D2-4); others are logged and ignored | Spec: only the owner frees space |
| B7-4 | Free space on a material without a file → 422 `no_original`; on a purged one → 422 `already_purged`; again while pending → no change. Undo with nothing pending → 409 | |
| B7-5 | Course free-space also sends past-paper files; papers have no status column in core, so their state is only visible in files | Schema has `Paper.FileRef` only |
| B7-6 | **Kept (Nick 2026-10-24).** Auto purge (`AutoPurgeOriginals`) is scheduled right after extraction with the same **7-day window**; that window is the time to cut question images (crops need the original) | Spec: "once any questions cut from it have their crops", which can't be known in advance |
| B7-7 | Re-uploading the same bytes of a **purged clean** original restores the row **without a new scan or extract** (the sha256 check proves identical bytes; text is still there) and emits `file.restored`. A deleted (infected/removed) one is scanned again | Saves work; same file |
| B7-8 | Thumbnails: 320 px WebP per page as a `Thumbnail` FileJob after extraction; they count against quota and are **skipped when they'd go over it**. Turning the setting on backfills present originals; turning it off **deletes** them and gives the space back. `GET /files/{id}/pages/{n}/thumbnail` returns a signed URL | Spec 6.1 step 5 |
| B7-9 | The purge runs in the files worker's hourly sweep and sends one opt-in `storage.purged` notification per owner per run; `file.infected` became an always-in-app kind | Users should learn originals went |
| B7-10 | Quota view per course adds `pendingPurge` bytes; `purgeable` = extracted originals not yet pending | Spec lists `purgeable` only |
| B7-11 | Fix found by the log scan: core's event consumer uses its **own Redis connection** (its blocking read stalled the relay's XADD on the shared multiplexed connection, making API writes wait ~9 s) | |

## Milestone 8 build choices (AI)

| # | Choice | Why / where |
|---|---|---|
| B8-1 | Adapters: **Anthropic through the official SDK** (`anthropic` 2.x); OpenAI, Mistral (same chat-completions shape) and Google over httpx. Google replies arrive as one piece rather than streamed. A **fake provider** (keys starting `fake-valid`) exists for dev and tests only and is refused when `CRUX_ENV=prod` | Spec 7.2 lists four providers; tests can't call real ones |
| B8-2 | **Kept, and shown (Nick 2026-10-24):** when a fallback model wrote the reply, the chat stream's `done` event and the stored message carry `servedBy` (from `usage.iterations`), and the run is priced at that model's rates. Anthropic calls on Claude Opus 5.5 and Sonnet 5.5 use the **server-side refusal fallback** (beta, `fallbacks: "default"`): if the model declines, the API re-runs the request on another model in the same call. A final refusal shows as an error in the chat | Recommended default for these models; drop it if you'd rather a refusal just stops |
| B8-3 | **Checked (Nick 2026-10-24), against each provider's page on 2026-10-24.** Anthropic: Opus 5.5 $4/$20, Sonnet 5.5 $2/$10, Haiku 5.5 $0.10/$0.50 (≤100K prompt tokens; $0.50/$2.50 above), Opus 4.8 $5/$25 (a fallback target). OpenAI: gpt-6.1-sol (default) $2/$10, gpt-6-astra $10/$50, gpt-6-luna $0.10/$0.50; context capped at 272K, above which OpenAI charges more. Google: gemini-4-argon (default) $2/$10, rising to $4/$20 on 1 Dec 2026; gemini-3.8-flash $0.75/$3.75, rising to $1.50/$7.50 on 1 Jan 2027; gemini-3.5-flash-lite $0.30/$2.50. Mistral: mistral-medium-3-5 (default) $1.50/$7.50, mistral-large-latest $0.50/$1.50, mistral-small-2603 $0.15/$0.60. Scheduled rises and long-prompt rates are in the table. A saved model that left the table falls back to the provider default | Spec: a static list updated by hand |
| B8-4 | Keys: AES-256-GCM envelope encryption (a data key per row wrapped by `CRUX_KMS_MASTER_KEY`, the row's PublicId as associated data); revoking wipes the ciphertext. The first usable key becomes the default, with that provider's default model | Spec 7.2 |
| B8-5 | Consent: turning AI on needs the current policy version (`2026-10`); every change to a switch, share scope or the cap writes `AiConsentLog` with the delta | Spec 15 |
| B8-6 | Core's public gate check is `GET /me/ai-allowed?feature=&courseId=` (everything under `/api/v1/ai/*` goes to the ai service). ai unreachable → `service_unavailable` | Spec 7.1 "core proxies it" |
| B8-7 | Budget: the reservation = price-table cost of (estimated input + max output tokens); over the cap the run is Blocked (402); settled with the actual usage | Spec 7.2 reserve-then-settle |
| B8-8 | Custom tutors: free text up to 2,000 characters, refused when it tries to override safety (a phrase list); rule keys and ranges are fixed (`maxWords` 50–1000, `hintsBeforeSolution` 0–5, …). No tutor chosen → the built-in Friendly explainer. An edit makes the old version inactive | Spec 7.3 |
| B8-9 | **Kept (Nick 2026-10-24).** Chat retrieval: MySQL FULLTEXT, top 8 chunks from AI-allowed, non-excluded materials (the conversation's material or lecture first), plus the anchored page. **Embeddings are not built** | Spec 7.4 allows FULLTEXT when there are no embeddings |
| B8-10 | The approach-check rule uses a **heuristic classifier** (phrases like "is my approach right"), not an extra model call; the judge is a model call and the reply is regenerated once. `maxWords` stops the stream and trims the stored reply | Spec 7.3 allows "a cheap classifier call, or heuristics" |
| B8-11 | Citations: `[p.N]` in the reply matched to the retrieved chunk of that page. Transcripts (prompt + reply, never the key) go to the object store for 90 days; a daily job deletes them | Spec 7.4 |
| B8-12 | Answer keys reach the prompt only when the user may see them: mock items as the mock view shows them; a bank question's key after the user answered it once; a seed question's key only for Owners/Editors | Spec 7.4 "never reveal a hidden key" |
| B8-13 | AI grading: one run per open answer; a blank answer is suggested as Blank without a model call; core fetches the suggestion from ai when the user decides and checks it belongs to that mock, item and user. Self grading also works while suggestions are pending | Core owns the grade (schema note) |
| B8-14 | Generation: `componentId` is required; generated questions are the user's own (private), Source Generated, KeyReliability Derived, Draft unless AutoApproveGenerated. Jobs run one at a time per worker | Spec 7.5; "at most 3 concurrent per user" is met trivially |
| B8-15 | Nightly top-up: an hourly loop asks core for the user's time zone and plan, runs at 03:00 local, at most once per 20 h; a course's NightlyGeneration setting overrides the user's | Spec 7.5 |
| B8-16 | **Changed (Nick 2026-10-24):** large imports are read in up to 6 calls of ~60,000 characters (later calls see the earlier proposals and reuse their tempKeys); the import says how many `parts` and whether text was left out (`truncated`). Proposals that repeat earlier ones are dropped; proposals matching the course's existing lectures (code or name), concepts, papers (key), questions (prompt) or flashcards (front) get `duplicateOfId`; an accepted lecture/concept/paper duplicate **links** to the existing row in core. Was: Import: **one model call over all files** (about 60,000 characters of text, split across files); proposal payloads travel as JSON strings so the output schema stays strict; Question proposals only with `importPastExams`; a Course proposal is ignored when importing into an existing course; Official keys only with a paper link. Accepted and edited proposals are applied; each failed proposal fails alone. **Duplicate detection (`DuplicateOfRef`) is not built** | Spec 7.5 |
| B8-17 | Core applies imports idempotently with a new table `ImportApplication` (core migration 0007): a second apply returns the stored result | Spec 7.5 "idempotent on the import ref" |
| B8-18 | `POST /ai/papers/{id}/extract` makes an import of that paper's file, already queued, with questions linked by `paperId` | Spec 7.5 |
| B8-19 | aiomysql can't send raw bytes with PyMySQL 2.2.8 (it imports a function that became a string), so the key ciphertext is stored base64 in its VARBINARY column | Revisit when aiomysql fixes it |
| B8-20 | New internal routes in core: `ai/materials`, `context/user`, `mocks/{id}/open`, `ai/generation-context`, `ai/topup-plan`, `ai/paper`, `questions/generated`, `import/apply`; in ai: `grades` | Spec 15 lists only some internal routes |
| B8-21 | Verify script fix: the cold start stops every compose profile first (ai containers outlived the wiped database). Core logs "ai unreachable" at info: AI off is a supported setup | Spec 9.6 |

## Milestone 9 build choices (hardening)

| # | Choice | Why / where |
|---|---|---|
| B9-1 | Export: core dumps every row of the user's from `information_schema` (any column named `*Hash` is skipped, internal Ids included for joins); notify, ai and files add their parts (ai **without key material**); files writes the zip: one JSON per service, `README.txt`, and `originals/` with the uploaded files | Spec 15 "export everything" |
| B9-2 | **Decided (Nick 2026-10-24): the export download lasts 24 h**, then the zip is deleted (hourly job); a new export can be asked for any time. Ready → an in-app notice (`account.export_ready`, always on, can't be opted out); asking again while one is in progress returns that one | Personal data on the server as briefly as possible |
| B9-3 | **Decided (Nick 2026-10-24): deletion keeps the 14-day grace, cancellable.** The request needs the password and a decision for every course shared with others: transfer to a named member, or delete. Logging in stays possible (to cancel, Q-auth-7). The job runs hourly (spec says daily); every service deletes its rows and confirms with `user.purged@1`; core removes the user last | Spec 15 |
| B9-4 | Chat rate limit: **30 messages a minute per user** (Redis counter in the ai service, `429 rate_limited` with `retryAfterSecs`) | Spec 3.1 lists the code, not the number |
| B9-5 | Metrics: Prometheus text at `/internal/metrics` on every service (blocked at the gateway): request count and latency by route template and status, plus core gauges (unpublished outbox rows, pending exports and deletions). **No Grafana/Loki containers shipped**: scrape and view with whatever the VPS runs | Spec 12; Q-arch-6 |
| B9-6 | **Changed (Nick 2026-10-24): uploads are encrypted with `age`** to `BACKUP_AGE_RECIPIENT` (the private key stays off the server); the script refuses to upload plain dumps unless `BACKUP_ALLOW_PLAINTEXT=1`. Was: Backups: per database `mysqldump --single-transaction --source-data=2` (binlog position for point-in-time replay), the object store as one tar, `SHA256SUMS`. **Decided (Nick 2026-10-24): one VPS, off-site copy to an S3-compatible bucket**: `BACKUP_S3_*` in `.env`, uploads pruned after `BACKUP_KEEP_DAYS` (default 30). Encryption is left to the bucket (no client-side encryption) | Spec 12 |
| B9-7 | Restore drill: each dump loaded into a scratch database next to the live one; table counts compared, row totals printed, scratch dropped; the tar checked against its checksum. Not a row-by-row comparison | Spec 12 "monthly restore drill" |
| B9-8 | Load test: k6 (`grafana/k6:0.54.0`) with a studying-user script (home, course page, flashcards, events, a mock every 10th loop); thresholds p95 < 300 ms and < 1 % errors on non-AI routes. Users come from `crux-core load-users` (refused when `CRUX_ENV=prod`; 3-hour tokens). **Local laptop result, 500 users: p95 48 ms, 1,063 req/s, 0 errors.** The real run belongs on the target VPS | Spec 14 M9 |
| B9-9 | **Kept (Nick 2026-10-24).** The load test found **MySQL deadlocks** (500s on `/week`, course stats, new deadlines). Fixes: the WeekSummary write is UPDATE-first (row lock, not gap locks), and a middleware **replays a request whose transaction lost a deadlock** (3 tries, short jittered pause). Safe because each request is one transaction with side effects in the outbox; a few internal HTTP reads may repeat | `core/crates/api/src/retry.rs` |
| B9-10 | **Decided (Nick 2026-10-24): the study window is a user setting**: `Extra.studyStart` / `studyEnd` (`"HH:MM"`, 15-minute steps, at least 1 h, default 09:00–22:00). Changing it (or `dailyHours` / `multiCoursePerDay`) rebuilds the plan at once, pinned entries kept. The flashcard exam cap (B4-3) stays at 18:00 the day before, whatever the window | Spec 5.8 |
| B9-11 | Production DB users: `deploy/mysql/init-prod.sh` runs once on an empty volume after `init.sql` and sets the service passwords from `.env`; with `CRUX_ENV=prod` it drops the test user. Later rotation is by hand (README) | Q-arch-7 |
| B9-12 | `cargo audit` ignores **RUSTSEC-2023-0071** (rsa timing attack): rsa comes only through sqlx's MySQL driver, which encrypts with the server's public key; the attack needs private-key decryption. No fixed release exists. `pip-audit`: clean | `core/.cargo/audit.toml` |
| B9-13 | Small fixes: the notify API now reads `CRUX_SMTP_URL` (it was pinned to Mailpit); the README covers adding a provider, operations and a VPS deploy | |
| B9-14 | For the frontend (spec review 2026-10-24): `GET /plan/short` and `RebuildView.short` give the "X h short" courses as data (`courseId`, `courseCode`, `hours`); chat replies carry `contextUsed` (pages with a quote, the weak spots/slips/traps given, the question, the shared scopes) in the `done` event and on the stored message, for the panel's "Context used" list | Frontend spec 5.5, 5.8 |

## Product / behaviour

| # | Question | Built meanwhile |
|---|---|---|
| Q-auth-1 | Refresh-token lifetime? | **Decided (Nick 2026-10-23): 30 days, sliding.** Revoked rows kept 30 days so reuse is still detected. |
| Q-auth-2 | Username rules? | 3-32 of letters, digits, `_ . -`, starting with a letter or digit; unique case-insensitively. |
| Q-auth-3 | Breached passwords: block, or warn? Which list? | **Decided (Nick 2026-10-23): warn, don't block; check via Have I Been Pwned's range API (k-anonymity: only the first 5 hex chars of the SHA-1 leave the server) with the SecLists top-100k list as the offline fallback.** The API answers field code `breached` until the request carries `acceptBreachedPassword: true` (signup and reset). Length rules are never skippable. |
| Q-auth-4 | Verify-email and reset-password emails go to *unverified* addresses and can't be opt-in. Spec 8 says email only to verified addresses and every kind opt-in. | `notify.requested` carries `transactional: true` for `auth.verify_email` / `auth.password_reset`; notify will send those regardless of preferences and verification. Sending lands with notify (milestone 6). Until then, links are in the core Outbox row. |
| Q-auth-5 | Token TTLs for verify (48 h) and reset (1 h)? | As stated. Requesting a new link voids the older one. |
| Q-auth-6 | Two tabs refreshing at once present a just-rotated token. | **Decided (Nick 2026-10-23): 10 s grace.** A token rotated ≤ 10 s ago gets a fresh session; after that, reuse revokes every session (spec 4.1). Build refinement: sessions ended on purpose (logout, password reset, admin) just answer 401 and don't cascade, so a stale tab after logout can't sign out other devices. Needs `UserSession.RevokeReason` (core migration 0003). |
| Q-auth-7 | A logged-in user whose account is `PendingDeletion` can still log in (to cancel). OK? | Yes: only `Disabled` blocks login. |
| Q-inv-1 | Limits on invites made by ordinary users? | ≤ 5 uses, ≤ 30 days (default 14 days, 1 use). Admin: ≤ 10,000 uses, ≤ 365 days, any tier. |
| Q-inv-2 | Error code when an unverified user tries to invite? Not in the 3.1 table. | `403 email_unverified`. |
| Q-admin-1 | Changing a tier's quota affects every user on it. | Core emits `user.tier_changed` per user on that tier (fine at v1 scale; batch later). |
| Q-admin-2 | "Each admin read of another user's data writes a UserEvent": does listing users count? | Yes: one `admin.user_read` event per listed user. |

## Architecture

| # | Question | Built meanwhile |
|---|---|---|
| Q-arch-1 | How do services get their first service token? Spec 3.2: "minted on start". | Each service presents a shared `CRUX_SERVICE_SECRET` to `POST /internal/v1/service-token` and gets a 5-min EdDSA token (`aud` = callee, `sub` = caller). Per-service secrets later if needed. |
| Q-arch-2 | Rate limits "at the gateway": Caddy has no built-in limiter. | Implemented in core on Redis counters (login 5/15 min per account + 20 per IP; signup 5/h per IP). AI and upload limits go in their services. |
| Q-arch-3 | sqlx `query!` macros need a database at compile time. | **Decided (Nick 2026-10-24): a CI SQL check instead of `query!`.** `core/crates/api/tests/sql_check.rs` has MySQL prepare every statement of the repo crate (~300, the shared SELECT prefixes and list placeholders filled in) against the migrated schema and compares `.bind(` counts with the placeholders. Converting to `query!` would have meant aliasing every column and still left the 68 dynamic queries unchecked. Three runtime-built statements are listed as uncheckable. |
| Q-arch-4 | SeaORM for CRUD (2.2). | **Decided (Nick 2026-10-24): not used.** All SQL is hand-written with sqlx, in the repo layer only; a deliberate departure from spec 2.2. |
| Q-arch-5 | Unsubscribe links: `Recipient.UnsubscribeTokenHash` stores only a hash, so the token must be re-derivable for each email. | Token = HMAC-SHA256(`CRUX_UNSUBSCRIBE_SECRET`, userRef); only its sha256 is stored. |
| Q-arch-6 | Metrics endpoint (`/metrics`) and Grafana/Loki. | **Done in M9 (B9-5):** `/internal/metrics` on every service; dashboards are left to the VPS. |
| Q-arch-7 | Prod DB passwords: `deploy/mysql/init.sql` holds dev passwords. | **Done in M9 (B9-11):** `deploy/mysql/init-prod.sh` sets them from `.env`. |

## After the volunteer trial (noted 2026-10-25)

- **Question images by upload.** The question editor should take an image file directly (PNG/JPEG)
  as well as a crop from a course PDF. Lecture materials stay PDF-only for now. (The importer works
  around it by wrapping each figure in a one-page PDF and cropping that.)
