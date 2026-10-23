#!/bin/sh
# serve: migrate, then run the API. Any other command (python -m app.jobs) runs as given.
set -e
if [ "$1" = "serve" ]; then
    alembic upgrade head
    exec uvicorn app.main:app --host 0.0.0.0 --port "${CRUX_LISTEN_PORT:-8080}" --proxy-headers --no-access-log
fi
exec "$@"
