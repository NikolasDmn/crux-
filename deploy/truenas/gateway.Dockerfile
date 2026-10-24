# The gateway for a hosted install: Caddy with the Caddyfile and the built web app baked in, so the
# server needs no checkout. Build context: the repo root.
FROM node:20-bookworm-slim AS web
WORKDIR /web
COPY web/package.json web/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY web ./
RUN npm run build

FROM caddy:2.8
COPY deploy/caddy/Caddyfile /etc/caddy/Caddyfile
COPY --from=web /web/dist /srv/web
