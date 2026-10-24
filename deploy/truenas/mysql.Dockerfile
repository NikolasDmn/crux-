# MySQL with Crux's first-start scripts baked in (schemas, service users, passwords from the
# environment). Build context: the repo root.
FROM mysql:8.0.40
COPY deploy/mysql/init.sql /docker-entrypoint-initdb.d/00-init.sql
COPY deploy/mysql/init-prod.sh /docker-entrypoint-initdb.d/01-passwords.sh
