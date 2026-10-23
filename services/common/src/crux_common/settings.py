"""Environment configuration shared by every Python service."""

from pathlib import Path

from pydantic_settings import BaseSettings, SettingsConfigDict

# Longer than the 5 s blocking XREADGROUP in the consumers (redis-py 8 defaults to 5 s, which
# turns every idle read into a timeout error).
REDIS_SOCKET_TIMEOUT = 30.0


def _default_contracts_dir() -> Path:
    # services/common/src/crux_common/settings.py -> repo root / contracts
    here = Path(__file__).resolve()
    for parent in here.parents:
        if (parent / "contracts" / "events").is_dir():
            return parent / "contracts"
    return Path("/app/contracts")


class CommonSettings(BaseSettings):
    model_config = SettingsConfigDict(env_prefix="CRUX_", extra="ignore")

    env: str = "dev"
    service_name: str
    database_url: str
    redis_url: str = "redis://127.0.0.1:6379/0"
    core_internal_url: str = "http://core:8080"
    jwks_url: str = "http://core:8080/internal/v1/jwks"
    service_secret: str = ""
    contracts_dir: Path = _default_contracts_dir()
    listen_port: int = 8080

    @property
    def sync_database_url(self) -> str:
        """Alembic runs synchronously on PyMySQL; the app runs on aiomysql."""
        return self.database_url.replace("+aiomysql", "+pymysql")
