"""Audit M6: production refuses development secrets."""

import pytest

from crux_common import prod_guard

GOOD = {
    "CRUX_ENV": "prod",
    "CRUX_SERVICE_SECRET": "s" * 40,
    "CRUX_KMS_MASTER_KEY": "k" * 44,
    "CRUX_DATABASE_URL": "mysql+aiomysql://crux_ai:a-long-random-password@mysql:3306/crux_ai",
    "CRUX_REDIS_URL": "redis://:another-random-password@redis:6379/0",
    "CRUX_S3_SECRET_KEY": "s3-random",
}


def test_dev_is_never_checked() -> None:
    assert prod_guard.problems({"CRUX_ENV": "dev", "CRUX_SERVICE_SECRET": "dev-service-secret-change-me"}) == []


def test_good_production_passes() -> None:
    assert prod_guard.problems(GOOD) == []
    prod_guard.check(GOOD)


@pytest.mark.parametrize(
    ("name", "value", "says"),
    [
        ("CRUX_SERVICE_SECRET", "dev-service-secret-change-me", "CRUX_SERVICE_SECRET"),
        ("CRUX_SERVICE_SECRET", "short", "CRUX_SERVICE_SECRET"),
        ("CRUX_KMS_MASTER_KEY", "ZGV2LW9ubHktbWFzdGVyLWtleS1jaGFuZ2UtbWUtISE=", "CRUX_KMS_MASTER_KEY"),
        ("CRUX_DATABASE_URL", "mysql+aiomysql://crux_ai:crux-ai@mysql:3306/crux_ai", "CRUX_DATABASE_URL"),
        ("CRUX_REDIS_URL", "redis://:crux-redis-dev@redis:6379/0", "CRUX_REDIS_URL"),
        ("CRUX_S3_SECRET_KEY", "crux-dev-secret", "CRUX_S3_SECRET_KEY"),
        ("CRUX_COOKIE_SECURE", "false", "CRUX_COOKIE_SECURE"),
    ],
)
def test_each_dev_value_is_refused(name: str, value: str, says: str) -> None:
    found = prod_guard.problems({**GOOD, name: value})
    assert len(found) == 1 and says in found[0]
    with pytest.raises(RuntimeError, match="Refusing to start"):
        prod_guard.check({**GOOD, name: value})
