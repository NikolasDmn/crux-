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
web/             React SPA (frontend-spec): Vite, TanStack Router + Query, Tailwind on the spec 3 tokens
```

## Run it locally

Needs Docker Desktop (Compose v2), Node 20.19+ and `openssl` (Git Bash has it). `make` is optional;
each target is a couple of plain commands in the [Makefile](Makefile).

```bash
make up            # .env from .env.example, a JWT key in deploy/secrets, then docker compose up
make seed          # prints an invite code; sign up at http://localhost:8000 (API: /api/v1/auth/signup)
make smoke         # health, signup, /me, and user.created reaching files + notify
make verify        # cold start from an EMPTY database (drops volumes), smoke, log scan
make up-ai         # same, plus the ai service (the product works without it)
```

The gateway serves the web app (`web/dist`, built by `make web`) at <http://localhost:8000>. For
frontend work, `make web-dev` runs it with hot reload on <http://localhost:5173>, proxying the API
to the running stack (`CRUX_GATEWAY` overrides the target). In dev the M1 **demo console** (`demo/`)
is still at `/demo`: a raw API client for poking endpoints. Emails land in Mailpit at
<http://localhost:8025>. With `npm run dev`, `/dev/ui` shows every component skin in both themes.

After an API change, regenerate the frontend client from the running stack (ai profile on):
`cd web && npm run api:fetch && npm run api`; CI fails when `src/lib/api.gen.ts` doesn't match
`web/openapi.json`.

The first admin: sign up with an invite, then
`docker compose -f deploy/compose.yaml exec core crux-core make-admin you@example.com`.

## Tests

```bash
make test          # Rust + Python + web. DB tests need MySQL: `make up` first, or they skip
make lint          # rustfmt, clippy -D warnings, ruff, mypy --strict, eslint, tsc, prettier
```

DB integration tests read `CRUX_TEST_MYSQL_URL` (default in the Makefile:
`mysql://crux_test:crux-test@127.0.0.1:3306`). Each test creates its own database, migrates it from
empty, and drops it. Without the variable they skip, and they say so.

End to end (Playwright, frontend-spec 9): the 22 journeys and an axe sweep of every screen in light
and dark at 1280 and 375 px, against the running stack with the dev fake AI provider.

```bash
make up-ai seed                          # the stack, the demo admin (it hands out the invites)
cd web && npx playwright install chromium
npm run e2e                              # all of it; `npx playwright test --project a11y` for the sweep
npm run size                             # after a build: shell + Home JS <= 250 KB gzipped
```

The suite signs up new users each run (invites from the demo admin, verification links from Mailpit
on :8025), so runs never collide. It clears the signup rate limits with `make unlock` and skips the
7-day wait of "Free up space" with `make purge-now FILE=<id>`; set `CRUX_E2E_UNLOCK` and
`CRUX_E2E_PURGE` to other commands (`{file}` is filled in) where `make` can't reach Docker, e.g.
through WSL. `CRUX_E2E_BASE_URL` points it at `npm run dev` (http://localhost:5173) instead of the
gateway.

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
| `BACKUP_S3_*`, `BACKUP_KEEP_DAYS`, `BACKUP_AGE_RECIPIENT` | `scripts/backup.sh` | off-site bucket for nightly backups (age-encrypted); empty = local only |

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
  `SHA256SUMS`, into `backups/<UTC stamp>/`. With `BACKUP_S3_BUCKET` set it uploads them, encrypted
  with `age` to `BACKUP_AGE_RECIPIENT` (it refuses to upload plain dumps), and prunes uploads older
  than `BACKUP_KEEP_DAYS`. Keep the private key off the server; to restore an upload:
  `age -d -i crux-backup.key crux_core.sql.gz.age > crux_core.sql.gz`.
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
   `CRUX_COOKIE_SECURE=true`, Postmark in `CRUX_SMTP_URL` and its webhook secret,
   `CRUX_SCANNER=clamd` (ClamAV), the `BACKUP_S3_*` bucket and `BACKUP_AGE_RECIPIENT`
   (`apt install age`; make the key pair on your own machine). With `CRUX_ENV=prod` every service
   refuses to start on a development secret or password, a secret under 32 characters
   (`openssl rand -base64 33`), or `CRUX_COOKIE_SECURE=false`.
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
