"""Audit M6: with CRUX_ENV=prod, refuse to start on development secrets.

Compose falls back to dev values for every secret, so one forgotten line would leave a deploy open.
Each service runs this at startup (CommonSettings); core has the same rules in Rust."""

import os
from collections.abc import Mapping
from urllib.parse import urlsplit

#: Secrets that must be long and random in production.
SECRETS = ("CRUX_SERVICE_SECRET", "CRUX_KMS_MASTER_KEY", "CRUX_UNSUBSCRIBE_SECRET")
#: Other values compose defaults to for development.
DEV_VALUES = {
    "dev-service-secret-change-me",
    "dev-unsubscribe-secret-change-me",
    "ZGV2LW9ubHktbWFzdGVyLWtleS1jaGFuZ2UtbWUtISE=",
    "crux-dev-secret",
    "crux-redis-dev",
    "crux-root",
    "crux-core",
    "crux-files",
    "crux-ai",
    "crux-notify",
    "change-me",
}
MIN_SECRET = 32


def problems(env: Mapping[str, str]) -> list[str]:
    """What's wrong with this environment for production (empty when it isn't prod)."""
    if env.get("CRUX_ENV") != "prod":
        return []
    out = []
    for name in SECRETS:
        value = env.get(name)
        if value is None:
            continue  # not this service's secret
        if value in DEV_VALUES or len(value) < MIN_SECRET:
            out.append(f"{name} is a development value or shorter than {MIN_SECRET} characters")
    if env.get("CRUX_S3_SECRET_KEY") in DEV_VALUES:
        out.append("CRUX_S3_SECRET_KEY is the development value")
    for name in ("CRUX_DATABASE_URL", "CRUX_REDIS_URL"):
        url = env.get(name)
        if url and urlsplit(url).password in DEV_VALUES:
            out.append(f"{name} uses a development password")
    if env.get("CRUX_COOKIE_SECURE", "").lower() == "false":
        out.append("CRUX_COOKIE_SECURE=false (production is served over HTTPS)")
    return out


def check(env: Mapping[str, str] | None = None) -> None:
    found = problems(os.environ if env is None else env)
    if found:
        raise RuntimeError("Refusing to start with CRUX_ENV=prod: " + "; ".join(found))
