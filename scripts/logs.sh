#!/usr/bin/env bash
# Tail selected services' logs, trimmed. Usage: scripts/logs.sh <lines> <service>...
N="$1"; shift
docker compose -f deploy/compose.yaml --env-file .env --profile ai logs --no-color --tail "$N" "$@" 2>&1 | cut -c1-260
