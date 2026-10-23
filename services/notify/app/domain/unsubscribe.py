"""One-click unsubscribe tokens (spec 8). Pure: the secret is passed in."""

import hashlib
import hmac


def token_for(secret: str, user_ref: str) -> str:
    return hmac.new(secret.encode(), user_ref.encode(), hashlib.sha256).hexdigest()


def token_hash(token: str) -> str:
    return hashlib.sha256(token.encode()).hexdigest()
