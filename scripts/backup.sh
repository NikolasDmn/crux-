#!/usr/bin/env bash
# Nightly backup (spec 12): one mysqldump per database (consistent, with the binlog position for
# point-in-time recovery) and the object store as a tar. With BACKUP_S3_BUCKET set (see .env.example)
# the result is uploaded to that S3-compatible bucket and uploads older than BACKUP_KEEP_DAYS
# (default 30) are pruned. Keep that bucket private and encrypted: dumps hold personal data.
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
    done < <(tr -d '' < .env)
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
    OFFSITE="$(cat scripts/backup/offsite.py)"
    envs=()
    for v in BACKUP_S3_ENDPOINT BACKUP_S3_BUCKET BACKUP_S3_ACCESS_KEY BACKUP_S3_SECRET_KEY BACKUP_S3_REGION BACKUP_S3_PREFIX; do
        envs+=(-e "$v=${!v:-}")
    done
    stamp="$(basename "$OUT")"
    for f in "$OUT"/*.gz "$OUT"/*.tar "$OUT/SHA256SUMS"; do
        $COMPOSE exec -T "${envs[@]}" files python -c "$OFFSITE" put "$stamp/$(basename "$f")" < "$f"
    done
    $COMPOSE exec -T "${envs[@]}" files python -c "$OFFSITE" prune "${BACKUP_KEEP_DAYS:-30}"
fi
echo "done $OUT"
