from functools import lru_cache

from crux_common.settings import CommonSettings

# Development-only master key (32 bytes, base64). Production sets CRUX_KMS_MASTER_KEY.
DEV_MASTER_KEY = "ZGV2LW9ubHktbWFzdGVyLWtleS1jaGFuZ2UtbWUtISE="


class Settings(CommonSettings):
    service_name: str = "ai"
    #: Spec 7.2 envelope encryption: wraps each key row's data key. Rotate by adding a new key id.
    kms_master_key: str = DEV_MASTER_KEY
    kms_key_id: str = "dev-1"
    #: The consent text version the settings screen shows; enabling AI must send it back.
    consent_policy_version: str = "2026-10"
    #: The "fake" provider (deterministic, no network) for dev and tests. Never in production.
    allow_fake_provider: bool = True
    #: Transcripts (spec 7.4): object store, kept 90 days.
    s3_endpoint: str = "http://s3:8333"
    s3_access_key: str = "crux"
    s3_secret_key: str = "crux-dev-secret"  # noqa: S105 - dev default; set in .env
    s3_bucket: str = "crux-files"
    s3_region: str = "us-east-1"
    transcript_days: int = 90
    files_internal_url: str = "http://files:8080"

    @property
    def fake_allowed(self) -> bool:
        return self.allow_fake_provider and self.env != "prod"


@lru_cache
def get_settings() -> Settings:
    return Settings()
