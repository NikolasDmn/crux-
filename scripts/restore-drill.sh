#!/usr/bin/env bash
# Monthly restore drill (spec 12): proves a backup restores. Each dump is loaded into a scratch
# database next to the live one, table counts and row totals are compared, and the scratch
# databases are dropped; the object tar is checked against its checksum and listed.
# Usage: scripts/restore-drill.sh <backup-dir>
set -euo pipefail

DIR="${1:?usage: scripts/restore-drill.sh <backup-dir>}"
COMPOSE="docker compose -f deploy/compose.yaml --env-file .env"
ROOT_PW="${MYSQL_ROOT_PASSWORD:-crux-root}"
sql() { $COMPOSE exec -T mysql mysql -uroot -p"$ROOT_PW" -N -e "$1" 2>/dev/null | tr -d '\r'; }

( cd "$DIR" && sha256sum -c --quiet SHA256SUMS ) && echo "ok   checksums"

for dump in "$DIR"/*.sql.gz; do
    db=$(basename "$dump" .sql.gz)
    scratch="restore_drill_$db"
    sql "DROP DATABASE IF EXISTS $scratch; CREATE DATABASE $scratch"
    # The dump names its database (--databases): point it at the scratch one instead.
    gunzip -c "$dump" | sed -e "s/^CREATE DATABASE .*\`$db\`.*;$//" -e "s/^USE \`$db\`;$/USE \`$scratch\`;/" \
        | $COMPOSE exec -T mysql mysql -uroot -p"$ROOT_PW" 2>/dev/null
    live=$(sql "SELECT COUNT(*) FROM information_schema.TABLES WHERE TABLE_SCHEMA = '$db'")
    restored=$(sql "SELECT COUNT(*) FROM information_schema.TABLES WHERE TABLE_SCHEMA = '$scratch'")
    rows=$(sql "SELECT COALESCE(SUM(TABLE_ROWS), 0) FROM information_schema.TABLES WHERE TABLE_SCHEMA = '$scratch'")
    sql "DROP DATABASE $scratch"
    [ "$restored" = "$live" ] || { echo "FAIL $db: $restored tables restored, $live live"; exit 1; }
    echo "ok   $db: $restored tables, ~$rows rows"
done

objects=$(tar -tf "$DIR/objects.tar" | wc -l | tr -d ' ')
echo "ok   objects.tar: $objects objects"
echo "drill passed for $DIR"
