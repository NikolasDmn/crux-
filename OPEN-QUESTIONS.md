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
| B3-2 | **Editors can see and approve the course bank's drafts** | Spec 4.2 shows drafts only to their owner; then nobody could approve bank drafts |
| B3-3 | Readers may add **private** questions; bank questions need Editor | Spec 15 says "read / edit" for POST questions |
| B3-4 | **A guessed right answer doesn't count as a Right** (stats, go-easy, "untested") and doesn't resolve slips or pass weak-spot checks | Spec 5.3 counts a guess as a miss |
| B3-5 | **Right items show their key at once**; misses stay hidden until shown/fixed; Partial counts as a miss in review | Spec 5.4 hides the key "until review"; nothing to hide on a right answer |
| B3-6 | Slip **proposals are computed**, not stored: a Wrong/Partial item without a slip shows `slipProposed`; confirming = `POST /slips` with the mock + order | No schema change; the user confirms each (spec 5.4) |
| B3-7 | A Passed/Archived subscription **still opens old mocks** | Spec 4.2 says "active subscription"; archived courses keep their history (5.10) |
| B3-8 | "Cold" (lecture not studied in the previous 24 h) is approximated by **no answers in that lecture in the 24 h before**, outside the current mock | Study events arrive with milestone 5; then it switches to UserEvents |
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
| B4-1 | **FSRS-6 default weights** (the `fsrs` 6.6 crate, the reference implementation) instead of FSRS-5 | The crate moved to FSRS-6; weights are stored as `FsrsParamsVersion=fsrs6.6-dflt` so cards can be refit from the review log later |
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
| B5-19 | Nightly jobs at fixed UTC hours: `sessions.derive` 02:00, `readmodels.rebuild` 03:00. The `UserQuestionStat` drift check is **not built yet** (planned for M9) | |
| B5-20 | New route: `POST /plan/entries` (spec 15 lists only `/plan/entries/{id}`) | Users add their own entries |
| B5-21 | Fix found while testing: purging a trashed course now deletes its slips, weak spots, plan, sessions, mocks, attempts, cards and questions first (they RESTRICT) | Purge would have failed for any course used since M3 |

## Milestone 6 build choices (notify)

| # | Choice | Why / where |
|---|---|---|
| B6-1 | A request for a kind the user has **no channel on** for is **not stored** at all (no Notification row); unknown kinds are dropped too | Opt-in by default would otherwise fill the table with daily digests nobody wants |
| B6-2 | `PUT /notify/preferences` takes a list of `{kind, channel, enabled}` changes; `GET` returns the full matrix with labels, `inAppAlways` (course.shared, exam.checkin) and the email status (address, verified, suppressed) | Spec says "a matrix" without a shape |
| B6-3 | Same dedupe key again: **before delivery it reschedules** (new dueAt and payload), after delivery it's ignored. `notify.requested@1` gains an optional `cancel: true` (an additive field) to cancel by key | Spec 8: "the old one is cancelled by key" |
| B6-4 | Core: moving an exam re-requests the reminders it already requested (notify moves them); an exam that leaves Upcoming (Done, Missed, Cancelled) cancels both | Same |
| B6-5 | In-app = a delivery row that becomes Sent at `dueAt`: the inbox only lists delivered items (a reminder for tomorrow isn't in today's inbox). Push: Redis pub/sub `crux:notify:<userRef>`; the stream sends `ready`, then `notification` per item, `ping` every 20 s | Spec 3.4/8 |
| B6-6 | Email for non-account kinds is checked **at send time**: verified address, not suppressed, the kind still switched on; otherwise the delivery is Skipped with the reason | Preferences can change between request and send |
| B6-7 | Unsubscribe link: `…/notify/unsubscribe?token=<HMAC(secret, userRef)>&kind=<kind>` (only the hash is stored). **GET shows a confirm page** (mail scanners open links); the button or the RFC 8058 one-click POST applies it. `kind=all` puts the address on EmailSuppression (Unsubscribe) | A GET that changes state gets triggered by link scanners |
| B6-8 | Bounce/complaint webhook: `POST /notify/webhooks/email`, a generic `{events:[{type: bounce|complaint, email}]}` body, secret in `X-Crux-Webhook-Secret`; 404 while no secret is configured. Provider-specific adapters come with the provider choice | Spec names no provider |
| B6-9 | `plan.today` (07:30 local) and `flash.due` (08:00 local, only with cards due) are produced by **core** in `deadlines.tick`, once per user per local day (Redis marker + dedupe key), and not after 12:00 local if the job was down | Core has the plan and the cards |
| B6-10 | `mock.graded`, `import.ready`, `generation.done` have templates now; their producers come with AI grading/import/generation (M8). An auto-graded mock is graded at submit, so it sends nothing | |
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
| B7-6 | Auto purge (`AutoPurgeOriginals`) is scheduled right after extraction with the same **7-day window**; that window is the time to cut question images (crops need the original) | Spec: "once any questions cut from it have their crops", which can't be known in advance |
| B7-7 | Re-uploading the same bytes of a **purged clean** original restores the row **without a new scan or extract** (the sha256 check proves identical bytes; text is still there) and emits `file.restored`. A deleted (infected/removed) one is scanned again | Saves work; same file |
| B7-8 | Thumbnails: 320 px WebP per page as a `Thumbnail` FileJob after extraction; they count against quota and are **skipped when they'd go over it**. Turning the setting on backfills present originals; turning it off **deletes** them and gives the space back. `GET /files/{id}/pages/{n}/thumbnail` returns a signed URL | Spec 6.1 step 5 |
| B7-9 | The purge runs in the files worker's hourly sweep and sends one opt-in `storage.purged` notification per owner per run; `file.infected` became an always-in-app kind | Users should learn originals went |
| B7-10 | Quota view per course adds `pendingPurge` bytes; `purgeable` = extracted originals not yet pending | Spec lists `purgeable` only |
| B7-11 | Fix found by the log scan: core's event consumer uses its **own Redis connection** (its blocking read stalled the relay's XADD on the shared multiplexed connection, making API writes wait ~9 s) | |

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
| Q-arch-3 | sqlx `query!` macros need a database at compile time. | M1 uses runtime-checked `sqlx::query_as`. Switch to `query!` + `cargo sqlx prepare` (offline data committed) once CI has run green on MySQL. |
| Q-arch-4 | SeaORM for CRUD (2.2). | Not used in M1: the auth tables are few and hand-written SQL is clearer. Will be introduced in milestone 2 (catalog CRUD), entities generated from the migrated schema. |
| Q-arch-5 | Unsubscribe links: `Recipient.UnsubscribeTokenHash` stores only a hash, so the token must be re-derivable for each email. | Token = HMAC-SHA256(`CRUX_UNSUBSCRIBE_SECRET`, userRef); only its sha256 is stored. |
| Q-arch-6 | Metrics endpoint (`/metrics`) and Grafana/Loki. | Planned for milestone 9 (hardening); logs are already structured JSON. |
| Q-arch-7 | Prod DB passwords: `deploy/mysql/init.sql` holds dev passwords. | Fine for dev/CI. A prod init script reading `.env` comes with deployment (milestone 9). |
