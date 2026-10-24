# Crux

Study and exam-prep platform. The specs are in [`crux-spec/crux-spec/`](crux-spec/crux-spec/README.md);
anything they leave open is in [`OPEN-QUESTIONS.md`](OPEN-QUESTIONS.md).

## Layout

```
core/            Rust (Axum, SQLx): auth, users, tiers, catalog, bank, mocks, flashcards, plan, …
  crates/        api · service · domain · repo · clients · jobs · testkit   (backend-spec 2.2)
  migrations/    0001 = schema-v5-core.sql verbatim, then append-only
services/        Python 3.12 uv workspace
  common/        crux_common: settings, DB, auth (JWKS), errors, outbox/relay/consumer, SSE tickets
  files/ notify/ ai/   one FastAPI app each: app/{http,service,domain,repo,clients,jobs}, Alembic
contracts/       event JSON Schemas (topic@version), shared test fixtures
deploy/          compose.yaml, Caddyfile (gateway), MySQL init
scripts/         smoke, verify-stack, backup, restore-drill, load/ (k6)
web/             React SPA (frontend-spec; not started)
```

## Run it locally

Needs Docker Desktop (Compose v2) and `openssl` (Git Bash has it). `make` is optional; each target
is a couple of plain commands in the [Makefile](Makefile).

```bash
make up            # .env from .env.example, a JWT key in deploy/secrets, then docker compose up
make seed          # prints an invite code; sign up at http://localhost:8000 (API: /api/v1/auth/signup)
make smoke         # health, signup, /me, and user.created reaching files + notify
make verify        # cold start from an EMPTY database (drops volumes), smoke, log scan
make up-ai         # same, plus the ai service (the product works without it)
```

Until the React SPA exists, the gateway serves a small **demo console** (`demo/`, selected by
`CRUX_WEB_DIR=../demo` in `.env`) at `/`: sign up, log in, verify email, reset password, edit
profile/settings, invites, and the admin tools, with a log of every API call. Emails land in
Mailpit at <http://localhost:8025>.

The first admin: sign up with an invite, then
`docker compose -f deploy/compose.yaml exec core crux-core make-admin you@example.com`.

## Tests

```bash
make test          # Rust + Python. DB tests need MySQL: `make up` first, or they skip
make lint          # rustfmt, clippy -D warnings, ruff, mypy --strict
```

DB integration tests read `CRUX_TEST_MYSQL_URL` (default in the Makefile:
`mysql://crux_test:crux-test@127.0.0.1:3306`). Each test creates its own database, migrates it from
empty, and drops it. Without the variable they skip, and they say so.

## Environment

All settings are env vars prefixed `CRUX_`; see [`.env.example`](.env.example) for the full list
with dev defaults. The important ones:

| Var | Used by | Meaning |
|---|---|---|
| `CRUX_DATABASE_URL` | each service | its own database (`crux_core`, `crux_files`, …) |
| `CRUX_REDIS_URL` | all | queues (Redis Streams), SSE tickets, rate limits, idempotency |
| `CRUX_JWT_PRIVATE_KEY_FILE` | core | Ed25519 signing key (PKCS#8 PEM). Others verify via `/internal/v1/jwks` |
| `CRUX_SERVICE_SECRET` | all | bootstrap secret traded for 5-minute service tokens |
| `CRUX_APP_BASE_URL` | core | public origin, for links in emails |
| `CRUX_COOKIE_SECURE` | core | `true` behind HTTPS (production) |
| `CRUX_KMS_MASTER_KEY` | ai | envelope key for users' AI keys (base64, 32 bytes). Losing it loses every stored key |
| `CRUX_UNSUBSCRIBE_SECRET` | notify | HMAC key for one-click unsubscribe links |
| `CRUX_EMAIL_WEBHOOK_SECRET` | notify | shared secret for the bounce/complaint webhook |
| `CRUX_*_DB_PASSWORD` | mysql, each service | service DB users' passwords, set on first start by `deploy/mysql/init-prod.sh` |
| `BACKUP_S3_*`, `BACKUP_KEEP_DAYS` | `scripts/backup.sh` | off-site bucket for nightly backups; empty = local only |

## How to add an event topic

1. Write `contracts/events/<topic>@1.json` (JSON Schema 2020-12, `additionalProperties: false`).
2. Producer: add the topic to `core/crates/domain/src/topics.rs` and the `SCHEMAS` list in
   `core/crates/service/src/contracts.rs` (Rust), or just call `emit(...)` (Python reads `contracts/`).
   Emit only inside the use case's transaction.
3. Consumer: a handler with `topics` and `handle(session, delivery)`, registered in that service's
   `app/jobs/__main__.py`. Handlers must tolerate gaps (the thing may already be gone).
4. A breaking change is `<topic>@2.json`; publish both until every consumer has moved.

## How to add an AI provider

1. Adapter in `services/ai/app/clients/llm.py`: a class with `verify(api_key)`, `stream_chat(...)`
   and `complete_json(...)` (the `Provider` protocol). An OpenAI-compatible API is just
   `ChatCompletionsProvider("<base url>")`; raise `LlmError` with the provider's status mapped.
2. Register it in `registry()` in the same file, and add the name to `PROVIDERS` in
   `app/domain/models.py`.
3. Models and prices: rows in `MODELS` (`app/domain/models.py`), USD per million tokens, one
   `default=True` per provider. Prices are updated by hand.
4. Tests: an adapter test with `httpx.MockTransport` (see `tests/test_account.py`); the chat and
   generation suites run on the fake provider.

## Operations

- **Metrics.** Every service serves Prometheus text at `/internal/metrics` (gateway-blocked; scrape
  it inside the compose network): request counts and latency by route and status, plus core gauges
  (outbox backlog, pending exports and deletions).
- **Backups.** `scripts/backup.sh` (cron: `15 2 * * *`) writes one consistent `mysqldump` per
  database (binlog position included, for point-in-time replay), the object store as a tar, and
  `SHA256SUMS`, into `backups/<UTC stamp>/`. With `BACKUP_S3_BUCKET` set it uploads them and prunes
  uploads older than `BACKUP_KEEP_DAYS`. Use a private, encrypted bucket: dumps hold personal data.
- **Restore drill.** Monthly: `scripts/restore-drill.sh backups/<stamp>` loads each dump into a
  scratch database, compares table counts, checks the checksums and lists the tar.
- **Load test.** `scripts/load/run.sh <base-url> [users] [duration]` creates test users (refused
  when `CRUX_ENV=prod`) and runs k6 with the spec's bar: p95 < 300 ms and < 1 % errors on non-AI
  routes. Run it on the target VPS, against a staging stack, not production.
- **Account export and deletion.** Users ask for an export (zip with every service's part, link
  valid 24 h) or deletion (14-day grace, cancellable). Jobs in `core-jobs` do the work; each
  service confirms with `user.purged`.

## Production on one VPS

1. Docker + Compose on the host; clone the repo; `cp .env.example .env` and `chmod 600 .env`.
2. Change **every** secret in `.env`, and set `CRUX_ENV=prod`, `CRUX_SITE_ADDRESS=<your domain>`
   (Caddy gets the TLS certificate), `CRUX_APP_BASE_URL=https://<your domain>`,
   `CRUX_COOKIE_SECURE=true`, a real `CRUX_SMTP_URL`, `CRUX_SCANNER=clamd` (ClamAV), and the
   `BACKUP_S3_*` bucket.
3. A JWT key: `openssl genpkey -algorithm ed25519 -out deploy/secrets/jwt_ed25519.pem`.
4. `docker compose -f deploy/compose.yaml --env-file .env --profile ai --profile full up -d --build`
   (`full` adds ClamAV). On an empty volume MySQL creates the service users with the `.env` passwords and drops the test user.
5. Make the first admin (above), add the backup cron line, and run a restore drill once.

## Conventions worth knowing

- Only PublicIds (ULIDs) leave a service; internal `Id`s never do.
- Errors: `{"error": {"code", "message", "details", "requestId"}}`; "exists but not yours" is 404.
- Edits send `If-Match: <version>`; a stale version is `409 conflict` with the current resource.
- Creates accept `Idempotency-Key` (replayed for 24 h per user + route).
- A request whose transaction loses a MySQL deadlock is replayed (up to 3 tries; `api/src/retry.rs`).
- Every public route has a row in `core/crates/api/src/policy_table.rs`; a unit test fails if a route
  is missing, and the cross-user test proves user B gets 404 on user A's resources.
