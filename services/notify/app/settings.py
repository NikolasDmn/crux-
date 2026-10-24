from functools import lru_cache

from crux_common.settings import CommonSettings


class Settings(CommonSettings):
    service_name: str = "notify"
    smtp_url: str = "smtp://127.0.0.1:1025"
    mail_from: str = "Crux <no-reply@crux.local>"
    #: Spec 8: dispatch every 30 s. Dev sets a few seconds so emails show up quickly.
    dispatch_interval_seconds: float = 30.0
    #: Keys the one-click unsubscribe tokens: token = HMAC(secret, userRef), so the link can be
    #: rebuilt for every email while only its hash is stored (Recipient.UnsubscribeTokenHash).
    unsubscribe_secret: str = "dev-unsubscribe-secret-change-me"  # noqa: S105 - dev default; set in .env
    #: Links in emails (unsubscribe, deep links into the app).
    app_base_url: str = "http://localhost:8000"
    #: Shared secret for the provider's bounce/complaint webhook; empty = webhook off (404).
    email_webhook_secret: str = ""


@lru_cache
def get_settings() -> Settings:
    return Settings()
