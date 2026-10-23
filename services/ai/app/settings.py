from functools import lru_cache

from crux_common.settings import CommonSettings


class Settings(CommonSettings):
    service_name: str = "ai"


@lru_cache
def get_settings() -> Settings:
    return Settings()
