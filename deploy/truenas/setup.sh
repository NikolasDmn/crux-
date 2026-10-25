#!/usr/bin/env bash
# One-time setup next to compose.yaml on the server: writes .env with fresh secrets (keeping any
# that exist), the object store's credentials file and the token signing key. Safe to run again.
#
#   CRUX_IMAGES=ghcr.io/<you>   CRUX_APP_BASE_URL=https://crux.example.org   ./setup.sh
set -euo pipefail
cd "$(dirname "$0")"

: "${CRUX_IMAGES:?set CRUX_IMAGES, e.g. ghcr.io/<your-github-user>}"
: "${CRUX_APP_BASE_URL:?set CRUX_APP_BASE_URL, e.g. https://crux.example.org}"
DATA="${CRUX_DATA_DIR:-./data}"

rand() { openssl rand -hex 24; }
touch .env
# Datasets with an SMB/NFSv4 ACL refuse chmod; the ACL governs access there, so carry on.
chmod 600 .env 2>/dev/null || true
# Keep a value already in .env; otherwise write the given one.
put() {
    if grep -q "^$1=" .env; then return; fi
    printf '%s=%s\n' "$1" "$2" >> .env
}
put CRUX_IMAGES "$CRUX_IMAGES"
put CRUX_TAG "${CRUX_TAG:-latest}"
put CRUX_APP_BASE_URL "$CRUX_APP_BASE_URL"
put CRUX_DATA_DIR "$DATA"
put CRUX_HTTP_PORT "${CRUX_HTTP_PORT:-8100}"
put CRUX_MAIL_PORT "${CRUX_MAIL_PORT:-8025}"
put MYSQL_ROOT_PASSWORD "$(rand)"
put CRUX_CORE_DB_PASSWORD "$(rand)"
put CRUX_FILES_DB_PASSWORD "$(rand)"
put CRUX_AI_DB_PASSWORD "$(rand)"
put CRUX_NOTIFY_DB_PASSWORD "$(rand)"
put CRUX_SERVICE_SECRET "$(rand)"
put CRUX_UNSUBSCRIBE_SECRET "$(rand)"
put CRUX_S3_ACCESS_KEY "crux"
put CRUX_S3_SECRET_KEY "$(rand)"
put CRUX_KMS_MASTER_KEY "$(openssl rand -base64 32)"

set -a
# shellcheck disable=SC1091
. ./.env
set +a

mkdir -p "$DATA"/{mysql,redis,s3,clamav,config,secrets}

# SeaweedFS reads its S3 credentials from a file; they must match CRUX_S3_* above.
cat > "$DATA/config/s3.json" <<JSON
{
  "identities": [
    {
      "name": "crux",
      "credentials": [{ "accessKey": "$CRUX_S3_ACCESS_KEY", "secretKey": "$CRUX_S3_SECRET_KEY" }],
      "actions": ["Admin", "Read", "Write", "List", "Tagging"]
    }
  ]
}
JSON
chmod 600 "$DATA/config/s3.json" 2>/dev/null || true

# The access-token signing key (core runs as uid 10001 and reads it).
if [ ! -f "$DATA/secrets/jwt_ed25519.pem" ]; then
    if chown 10001 "$DATA/secrets" 2>/dev/null; then owned=1; else owned=0; chmod 777 "$DATA/secrets" 2>/dev/null || true; fi
    docker run --rm --user 10001 -v "$(cd "$DATA/secrets" && pwd)":/out \
        "$CRUX_IMAGES/crux-core:$CRUX_TAG" crux-core gen-keys /out/jwt_ed25519.pem
    # Only core's user may read the key; without root (no chown), fall back to readable by all.
    if [ "$owned" = 1 ]; then chmod 700 "$DATA/secrets" 2>/dev/null || true; else chmod 755 "$DATA/secrets" 2>/dev/null || true; fi
fi

echo "Ready. Start with: docker compose up -d"
echo "Then point your proxy at http://<this server>:${CRUX_HTTP_PORT} for ${CRUX_APP_BASE_URL}."
