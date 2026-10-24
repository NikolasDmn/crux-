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
