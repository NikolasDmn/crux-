# Crux dev tasks. Needs Docker (Compose v2), Rust, uv, Node 20.19+, openssl.
# On Windows, run from Git Bash (make: `winget install ezwinports.make`) or copy the commands.

COMPOSE := docker compose -f deploy/compose.yaml --env-file .env
export CRUX_TEST_MYSQL_URL ?= mysql://crux_test:crux-test@127.0.0.1:3306

.PHONY: help env keys web web-dev up up-ai down logs seed smoke verify test test-core test-py test-web lint fmt invite unlock

help:
	@grep -E '^[a-z-]+:.*?## ' $(MAKEFILE_LIST) | awk -F':.*?## ' '{printf "  %-10s %s\n", $$1, $$2}'

env: ## create .env from .env.example (once)
	@test -f .env || cp .env.example .env

keys: ## create the JWT signing key in deploy/secrets (once)
	@mkdir -p deploy/secrets
	@test -f deploy/secrets/jwt_ed25519.pem || openssl genpkey -algorithm ed25519 -out deploy/secrets/jwt_ed25519.pem
	@chmod 644 deploy/secrets/jwt_ed25519.pem

web: ## build the web app into web/dist (the gateway serves it at /)
	cd web && npm ci --no-audit --no-fund && npm run build

web-dev: ## the web app with hot reload on :5173, proxying the API to the running stack
	cd web && npm run dev

up: env keys web ## run the stack without AI (spec 9.6)
	$(COMPOSE) up -d --build

up-ai: env keys web ## run the stack with the ai service
	$(COMPOSE) --profile ai up -d --build

down: ## stop everything
	$(COMPOSE) --profile ai --profile full down

logs: ## follow logs
	$(COMPOSE) logs -f --tail 100

unlock: ## dev: clear login/signup rate limits ("Too many attempts")
	@$(COMPOSE) exec -T redis sh -c 'redis-cli --scan --pattern "crux:rl:*" | xargs -r redis-cli del' >/dev/null && echo "rate limits cleared"

invite: ## print a fresh invite code
	@$(COMPOSE) exec -T core crux-core create-invite

seed: ## demo data: an admin (demo-admin@crux.test, password in deploy/secrets/demo-accounts.txt) + an invite
	@test -f deploy/secrets/demo-accounts.txt || printf 'demo-admin  demo-admin@crux.test  demo-%s
' "$$(openssl rand -hex 8)" > deploy/secrets/demo-accounts.txt
	./scripts/demo-seed.sh http://localhost:$${CRUX_HTTP_PORT:-8000}

smoke: ## smoke-test a running stack
	./scripts/smoke.sh http://localhost:$${CRUX_HTTP_PORT:-8000}

verify: env keys web ## cold start from an EMPTY database (drops volumes), smoke, log scan
	./scripts/verify-stack.sh http://localhost:$${CRUX_HTTP_PORT:-8000} --ai

test: test-core test-py test-web ## all tests (DB tests need `make up` for MySQL)

test-core:
	cd core && cargo test --workspace

test-web:
	cd web && npm test

test-py:
	cd services && uv run pytest common/tests
	cd services/files && uv run pytest
	cd services/notify && uv run pytest
	cd services/ai && uv run pytest

lint: ## clippy, ruff, mypy, eslint, tsc, prettier
	cd core && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
	cd services && uv run ruff check . && uv run ruff format --check . && uv run mypy common/src
	for s in files notify ai; do (cd services/$$s && MYPYPATH=../common/src uv run mypy --config-file ../pyproject.toml app); done
	cd web && npm run lint && npm run typecheck && npm run format:check

fmt: ## format everything
	cd core && cargo fmt --all
	cd services && uv run ruff format . && uv run ruff check . --fix
	cd web && npm run format
