#!/usr/bin/env bash
# Runs once on an empty MySQL volume, right after 00-init.sql: sets each service user's password
# from .env (compose passes them in), so production never runs on the dev passwords in init.sql.
# With CRUX_ENV=prod the integration-test user is dropped too.
# To change a password later: ALTER USER by hand, then update .env and restart that service.
set -euo pipefail

# A string literal for MySQL: backslashes are escapes there, so double them, then double quotes
# (audit L7: a password with a backslash used to fail, or set a different password).
q() { printf "%s" "$1" | sed -e 's/\\/\\\\/g' -e "s/'/''/g"; }
sql="ALTER USER 'crux_core'@'%' IDENTIFIED BY '$(q "${CRUX_CORE_DB_PASSWORD:-crux-core}")';
ALTER USER 'crux_files'@'%' IDENTIFIED BY '$(q "${CRUX_FILES_DB_PASSWORD:-crux-files}")';
ALTER USER 'crux_ai'@'%' IDENTIFIED BY '$(q "${CRUX_AI_DB_PASSWORD:-crux-ai}")';
ALTER USER 'crux_notify'@'%' IDENTIFIED BY '$(q "${CRUX_NOTIFY_DB_PASSWORD:-crux-notify}")';"
if [ "${CRUX_ENV:-dev}" = prod ]; then
    sql="$sql DROP USER IF EXISTS 'crux_test'@'%';"
fi
mysql --protocol=socket -uroot -p"$MYSQL_ROOT_PASSWORD" -e "$sql FLUSH PRIVILEGES;"
echo "crux: service passwords set from .env (env=${CRUX_ENV:-dev})"
