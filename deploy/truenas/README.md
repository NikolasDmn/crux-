# Crux on TrueNAS (volunteer trial)

A small install behind your own reverse proxy at `crux.<your domain>`. AI is off and there's no
email provider: verification and reset emails land in Mailpit on your LAN, and you verify
volunteers with a command.

What runs: the gateway (web app + API routing), core and its jobs, files (uploads, scanned by
ClamAV), notify (inbox, emails to Mailpit), MySQL, Redis and SeaweedFS (the object store). About
3 GB of RAM, most of it ClamAV.

## 1. Publish the images (on your PC)

```bash
echo "$GITHUB_TOKEN" | docker login ghcr.io -u <you> --password-stdin   # token with write:packages
CRUX_IMAGES=ghcr.io/<you> scripts/ghcr-push.sh
```

The packages are private. Repeat this for every update you want to roll out.

## 2. Install on TrueNAS (shell, as root)

Put it on a dataset created with the **Apps** preset (Datasets → Add Dataset → Dataset Preset:
Apps), or any dataset with a POSIX ACL. The containers change ownership of their folders on
first start (MySQL, Redis, core's key), which a dataset with an SMB or NFSv4 ACL refuses
("Operation not permitted").

```bash
cd /mnt/<pool>/<apps dataset> && mkdir -p crux && cd crux
# copy compose.yaml and setup.sh from deploy/truenas/ into this folder
echo "$READ_TOKEN" | docker login ghcr.io -u <you> --password-stdin     # token with read:packages
CRUX_IMAGES=ghcr.io/<you> CRUX_APP_BASE_URL=https://crux.<your domain> bash setup.sh
docker compose up -d
```

`setup.sh` writes `.env` (random secrets; keep a copy somewhere safe), the object store's
credentials and the token signing key, all under `./data`. Run it again any time; it keeps what
exists. The first start takes a few minutes (ClamAV downloads its signatures; core migrates the
database). `docker compose ps` shows everything healthy when it's ready.

Ports: the app on `8100` (`CRUX_HTTP_PORT`), Mailpit's inbox on `8025` (`CRUX_MAIL_PORT`, LAN only).
Change them in `.env` if they clash, then `docker compose up -d`.

## 3. Point your reverse proxy at it

`crux.<your domain>` → `http://<truenas IP>:8100`, with HTTPS at the proxy, and:

- **pass the Host header through** (uploads use URLs signed for `crux.<your domain>`);
- **request body up to 110 MB** (uploads go through it; Nginx: `client_max_body_size 110m`);
- **no response buffering and long timeouts** for `/api/v1/notify/stream` and `/api/v1/files/`
  (live updates use server-sent events; Nginx: `proxy_buffering off; proxy_read_timeout 1h;`).
  In Nginx Proxy Manager, put those lines in the host's *Advanced* box.

The gateway trusts `X-Forwarded-For` from private addresses (`CRUX_TRUSTED_PROXIES`), so each
user gets their own rate limit. If your proxy isn't on the LAN, set its address there instead.

## 4. People

Everything below runs on the server, in the install folder.

```bash
# An invite code (one person, valid 14 days; --max-uses 10 for a group code)
docker compose exec core crux-core create-invite --max-uses 1 --days 14

# After someone signs up: mark their email verified (there's no email to click)
docker compose exec core crux-core verify-user <username or email>

# Forgot a password: prints a temporary one and signs them out everywhere
docker compose exec core crux-core reset-password <username or email>

# You, once you've signed up yourself
docker compose exec core crux-core make-admin <your email>
```

Emails the app tries to send (verification links, password resets, digests) are in Mailpit at
`http://<truenas IP>:8025`.

## 5. Update

```bash
# PC:      CRUX_IMAGES=ghcr.io/<you> scripts/ghcr-push.sh
# Server:
docker compose pull && docker compose up -d
```

Database migrations run when core starts. To pin a version, set `CRUX_TAG=<commit>` in `.env`.

## 6. Backups

Snapshot the dataset (TrueNAS → Data Protection → Periodic Snapshot Tasks; daily, keep two
weeks). MySQL recovers from a snapshot like from a power cut. For a clean dump as well:

```bash
docker compose exec mysql sh -c 'mysqldump -uroot -p"$MYSQL_ROOT_PASSWORD" --single-transaction --all-databases' | gzip > crux-$(date +%F).sql.gz
```

## Troubleshooting

- `docker compose logs -f core` (or `files`, `gateway`, …) shows what a service is doing.
- Uploads stuck on "Scanning": ClamAV is still downloading signatures (`docker compose logs clamav`).
- "Too many attempts" after a burst of failed logins: wait 15 minutes, or
  `docker compose exec redis sh -c 'redis-cli --scan --pattern "crux:rl:*" | xargs -r redis-cli del'`.
