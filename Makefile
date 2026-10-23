# Crux dev tasks. Needs Docker (Compose v2), Rust, uv, openssl.
# On Windows, run from Git Bash (make: `winget install ezwinports.make`) or copy the commands.

COMPOSE := docker compose -f deploy/compose.yaml --env-file .env
export CRUX_TEST_MYSQL_URL ?= mysql://crux_test:crux-test@127.0.0.1:3306

.PHONY: help env keys up up-ai down logs seed smoke test test-core test-py lint fmt invite

help:
	@grep -E '^[a-z-]+:.*?## ' $(MAKEFILE_LIST) | awk -F':.*?## ' '{printf "  %-10s %s\n", $$1, $$2}'

env: ## create .env from .env.example (once)
	@test -f .env || cp .env.example .env

keys: ## create the JWT signing key in deploy/secrets (once)
	@mkdir -p deploy/secrets
	@test -f deploy/secrets/jwt_ed25519.pem || openssl genpkey -algorithm ed25519 -out deploy/secrets/jwt_ed25519.pem
	@chmod 644 deploy/secrets/jwt_ed25519.pem

up: env keys ## run the stack without AI (spec 9.6)
	$(COMPOSE) up -d --build

up-ai: env keys ## run the stack with the ai service
	$(COMPOSE) --profile ai up -d --build

down: ## stop everything
	$(COMPOSE) --profile ai --profile full down

logs: ## follow logs
	$(COMPOSE) logs -f --tail 100

invite: ## print a fresh invite code
	@$(COMPOSE) exec -T core crux-core create-invite

seed: ## load the demo data (milestone 1: an admin invite; the demo course arrives with milestone 2)
	@echo "Admin invite code: $$($(COMPOSE) exec -T core crux-core create-invite --max-uses 5)"
	@echo "Sign up with it, then: $(COMPOSE) exec core crux-core make-admin <your email>"

smoke: ## smoke-test a running stack
	./scripts/smoke.sh http://localhost:$${CRUX_HTTP_PORT:-8000}

test: test-core test-py ## all tests (DB tests need `make up` for MySQL)

test-core:
	cd core && cargo test --workspace

test-py:
	cd services && uv run pytest common/tests
	cd services/files && uv run pytest
	cd services/notify && uv run pytest
	cd services/ai && uv run pytest

lint: ## clippy, ruff, mypy
	cd core && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
	cd services && uv run ruff check . && uv run ruff format --check . && uv run mypy common/src
	for s in files notify ai; do (cd services/$$s && MYPYPATH=../common/src uv run mypy --config-file ../pyproject.toml app); done

fmt: ## format everything
	cd core && cargo fmt --all
	cd services && uv run ruff format . && uv run ruff check . --fix
