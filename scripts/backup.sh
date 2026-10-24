#!/usr/bin/env bash
# Nightly backup (spec 12): one mysqldump per database (consistent, with the binlog position for
# point-in-time recovery) and the object store as a tar. With BACKUP_S3_BUCKET set (see .env.example)
# the result is uploaded to that S3-compatible bucket and uploads older than BACKUP_KEEP_DAYS
# (default 30) are pruned. Uploads are encrypted with age to BACKUP_AGE_RECIPIENT (B9-6): the
# bucket only ever holds ciphertext, and the private key lives off the server. Needs `age` on the
# host (apt install age). The local copy stays plain for the restore drill.
# Usage: scripts/backup.sh [out-dir]   (default backups/<UTC timestamp>)
# Cron:  15 2 * * * cd /srv/crux && scripts/backup.sh >> backups/backup.log 2>&1
set -euo pipefail

OUT="${1:-backups/$(date -u +%Y%m%dT%H%M%SZ)}"
COMPOSE="docker compose -f deploy/compose.yaml --env-file .env"
ROOT_PW="${MYSQL_ROOT_PASSWORD:-crux-root}"
# BACKUP_* from .env unless already set (cron runs with a bare environment).
if [ -f .env ]; then
    while IFS='=' read -r k v; do
        [[ "$k" =~ ^BACKUP_[A-Z0-9_]+$ ]] && [ -z "${!k:-}" ] && export "$k=$v"
    done < <(tr -d '\r' < .env)
fi
mkdir -p "$OUT"
chmod 700 "$OUT"

for db in crux_core crux_files crux_notify crux_ai; do
    if ! $COMPOSE exec -T mysql mysql -uroot -p"$ROOT_PW" -N -e "SHOW DATABASES LIKE '$db'" 2>/dev/null | grep -q "$db"; then
        echo "skip $db (not present)"
        continue
    fi
    # --single-transaction: a consistent snapshot without locking InnoDB tables;
    # --source-data=2: the binlog file/position as a comment, the start point for PITR replay.
    $COMPOSE exec -T mysql mysqldump -uroot -p"$ROOT_PW" --single-transaction --source-data=2 --routines --triggers \
        --set-gtid-purged=OFF --databases "$db" 2>/dev/null | gzip -9 > "$OUT/$db.sql.gz"
    echo "ok   $db ($(du -h "$OUT/$db.sql.gz" | cut -f1))"
done

$COMPOSE exec -T files python - < scripts/backup/s3_dump.py 2> "$OUT/objects.log" > "$OUT/objects.tar"
echo "ok   objects: $(cat "$OUT/objects.log") ($(du -h "$OUT/objects.tar" | cut -f1))"

( cd "$OUT" && sha256sum ./*.gz ./*.tar > SHA256SUMS )

if [ -n "${BACKUP_S3_BUCKET:-}" ]; then
    if [ -n "${BACKUP_AGE_RECIPIENT:-}" ]; then
        command -v "${AGE:-age}" >/dev/null || { echo "FAIL age not installed (apt install age)"; exit 1; }
        encrypt() { ${AGE:-age} -r "$BACKUP_AGE_RECIPIENT"; }
        suffix=".age"
    elif [ "${BACKUP_ALLOW_PLAINTEXT:-}" = 1 ]; then
        encrypt() { cat; }
        suffix=""
    else
        echo "FAIL BACKUP_AGE_RECIPIENT is empty: refusing to upload plain dumps (BACKUP_ALLOW_PLAINTEXT=1 overrides)"
        exit 1
    fi
    OFFSITE="$(cat scripts/backup/offsite.py)"
    envs=()
    for v in BACKUP_S3_ENDPOINT BACKUP_S3_BUCKET BACKUP_S3_ACCESS_KEY BACKUP_S3_SECRET_KEY BACKUP_S3_REGION BACKUP_S3_PREFIX; do
        envs+=(-e "$v=${!v:-}")
    done
    stamp="$(basename "$OUT")"
    for f in "$OUT"/*.gz "$OUT"/*.tar "$OUT/SHA256SUMS"; do
        encrypt < "$f" | $COMPOSE exec -T "${envs[@]}" files python -c "$OFFSITE" put "$stamp/$(basename "$f")$suffix"
    done
    $COMPOSE exec -T "${envs[@]}" files python -c "$OFFSITE" prune "${BACKUP_KEEP_DAYS:-30}"
fi
echo "done $OUT"
