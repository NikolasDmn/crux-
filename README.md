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
| `CRUX_KMS_MASTER_KEY` | ai | envelope key for users' AI keys (milestone 8) |

## How to add an event topic

1. Write `contracts/events/<topic>@1.json` (JSON Schema 2020-12, `additionalProperties: false`).
2. Producer: add the topic to `core/crates/domain/src/topics.rs` and the `SCHEMAS` list in
   `core/crates/service/src/contracts.rs` (Rust), or just call `emit(...)` (Python reads `contracts/`).
   Emit only inside the use case's transaction.
3. Consumer: a handler with `topics` and `handle(session, delivery)`, registered in that service's
   `app/jobs/__main__.py`. Handlers must tolerate gaps (the thing may already be gone).
4. A breaking change is `<topic>@2.json`; publish both until every consumer has moved.

## How to add an AI provider

Milestone 8. The adapter interface (`chat`, `complete_json`, `count_tokens`, `price`) and the
provider registry will live in `services/ai/app/clients/providers/`; this section gets the steps then.

## Conventions worth knowing

- Only PublicIds (ULIDs) leave a service; internal `Id`s never do.
- Errors: `{"error": {"code", "message", "details", "requestId"}}`; "exists but not yours" is 404.
- Edits send `If-Match: <version>`; a stale version is `409 conflict` with the current resource.
- Creates accept `Idempotency-Key` (replayed for 24 h per user + route).
- Every public route has a row in `core/crates/api/src/policy_table.rs`; a unit test fails if a route
  is missing, and the cross-user test proves user B gets 404 on user A's resources.
