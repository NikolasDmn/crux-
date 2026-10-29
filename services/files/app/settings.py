from functools import lru_cache

from pydantic import model_validator

from crux_common.settings import CommonSettings

DEV_S3_SECRET = "crux-dev-secret"  # noqa: S105 - the published dev default, refused in prod


class Settings(CommonSettings):
    service_name: str = "files"

    # Object store (S3 API; SeaweedFS in dev and prod, OPEN-QUESTIONS S-7).
    s3_endpoint: str = "http://127.0.0.1:8333"
    #: What the browser uploads to and downloads from: pre-signed URLs are signed for this origin.
    #: The gateway proxies /<bucket>/* to the store, so it's the app's own origin (same-origin, no CORS).
    s3_public_endpoint: str = "http://localhost:8000"
    s3_access_key: str = "crux"
    s3_secret_key: str = DEV_S3_SECRET
    s3_bucket: str = "crux-files"
    s3_region: str = "us-east-1"

    #: Spec 0/6.1: accepted original types are config, not code.
    allowed_original_types: str = "application/pdf"
    #: Spec 6.1: per-file maximum, default 100 MB.
    max_upload_bytes: int = 100 * 1024 * 1024
    #: Quota for a user whose user.created event hasn't arrived yet (it overwrites this).
    default_quota_bytes: int = 1024 * 1024 * 1024

    #: D2-7: "fake" in dev (EICAR-aware), "clamd" in CI/prod.
    scanner: str = "fake"
    clamd_host: str = "clamav"
    clamd_port: int = 3310

    #: Tesseract languages for OCR (D2-8: English).
    ocr_languages: str = "eng"

    @model_validator(mode="after")
    def _no_dev_store_keys_in_prod(self) -> "Settings":
        # The dev key is in the repo, and the store is reachable through the site (audit C1).
        if self.env == "prod" and self.s3_secret_key == DEV_S3_SECRET:
            raise ValueError("CRUX_S3_SECRET_KEY is the published dev default; set a real one for prod.")
        return self

    @property
    def allowed_types(self) -> set[str]:
        return {t.strip() for t in self.allowed_original_types.split(",") if t.strip()}


@lru_cache
def get_settings() -> Settings:
    return Settings()
