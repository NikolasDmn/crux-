#!/bin/sh
# Every container migrates first (a MySQL lock serializes them), so a worker never runs against
# an empty database. serve = the API; anything else (python -m app.jobs) runs as given.
set -e
alembic upgrade head
if [ "$1" = "serve" ]; then
    exec uvicorn app.main:app --host 0.0.0.0 --port "${CRUX_LISTEN_PORT:-8080}" --proxy-headers --no-access-log
fi
exec "$@"
