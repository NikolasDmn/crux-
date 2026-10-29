"""Token verification (spec 1, 3.2, 4.1). Every service validates the access token itself against
core's JWKS; the gateway is not a trust boundary.

Access token: EdDSA JWT, iss=crux-core, aud=crux-api, sub=user PublicId, role, tier.
Service token: EdDSA JWT, iss=crux-core, aud=<this service>, sub=<calling service>, typ=service.
"""

import time
from dataclasses import dataclass
from typing import Annotated, Any

import httpx
import jwt
from fastapi import Depends, Request

from crux_common.errors import Unauthenticated
from crux_common.logging import set_user

ISSUER = "crux-core"
ACCESS_AUDIENCE = "crux-api"
JWKS_TTL_SECONDS = 300


@dataclass(frozen=True)
class Principal:
    user_ref: str
    role: str
    tier: str

    @property
    def is_admin(self) -> bool:
        return self.role == "Admin"


@dataclass(frozen=True)
class ServiceCaller:
    service: str
    on_behalf_of: str | None


class Jwks:
    """Fetches and caches core's key set; refetches on an unknown `kid` (key rotation)."""

    def __init__(self, url: str, client: httpx.AsyncClient | None = None) -> None:
        self._url = url
        self._client = client
        self._keys: dict[str, Any] = {}
        self._fetched_at = 0.0

    def load(self, jwks: dict[str, Any]) -> None:
        self._keys = {k["kid"]: jwt.PyJWK(k).key for k in jwks.get("keys", [])}
        self._fetched_at = time.monotonic()

    async def _refresh(self) -> None:
        client = self._client or httpx.AsyncClient(timeout=2.0)
        try:
            resp = await client.get(self._url)
            resp.raise_for_status()
            self.load(resp.json())
        finally:
            if self._client is None:
                await client.aclose()

    async def key_for(self, kid: str) -> Any:
        stale = time.monotonic() - self._fetched_at > JWKS_TTL_SECONDS
        if stale or kid not in self._keys:
            try:
                await self._refresh()
            except httpx.HTTPError as e:
                if kid not in self._keys:
                    raise Unauthenticated() from e
        if kid not in self._keys:
            raise Unauthenticated()
        return self._keys[kid]


#: Set by core when every session of a user ends (audit L1); the value is a unix time.
REVOKED_KEY = "crux:revoked:{}"


class Authenticator:
    """`redis` (optional) lets a token be refused before it expires: core writes
    `crux:revoked:<user>` when it ends all of a user's sessions, and tokens issued before that
    time stop working here too."""

    def __init__(self, jwks: Jwks, service_name: str, redis: Any | None = None) -> None:
        self.jwks = jwks
        self.service_name = service_name
        self.redis = redis

    async def _decode(self, token: str, audience: str) -> dict[str, Any]:
        try:
            header = jwt.get_unverified_header(token)
            if header.get("alg") != "EdDSA" or "kid" not in header:
                raise Unauthenticated()
            key = await self.jwks.key_for(header["kid"])
            claims: dict[str, Any] = jwt.decode(
                token,
                key=key,
                algorithms=["EdDSA"],
                audience=audience,
                issuer=ISSUER,
                options={"require": ["exp", "sub"]},
            )
            return claims
        except jwt.PyJWTError as e:
            raise Unauthenticated() from e

    async def verify_access(self, token: str) -> Principal:
        claims = await self._decode(token, ACCESS_AUDIENCE)
        if self.redis is not None:
            revoked = await self.redis.get(REVOKED_KEY.format(claims["sub"]))
            if revoked is not None and int(claims.get("iat", 0)) < int(revoked):
                raise Unauthenticated()
        return Principal(user_ref=claims["sub"], role=claims.get("role", "User"), tier=claims.get("tier", ""))

    async def verify_service(self, token: str) -> str:
        claims = await self._decode(token, self.service_name)
        if claims.get("typ") != "service":
            raise Unauthenticated()
        return str(claims["sub"])


def _bearer(request: Request) -> str:
    header = request.headers.get("authorization", "")
    if not header.startswith("Bearer "):
        raise Unauthenticated()
    return header.removeprefix("Bearer ").strip()


async def current_user(request: Request) -> Principal:
    auth: Authenticator = request.app.state.auth
    principal = await auth.verify_access(_bearer(request))
    set_user(principal.user_ref)
    return principal


async def service_caller(request: Request) -> ServiceCaller:
    """Internal routes: a service token for this service. `X-On-Behalf-Of` names the user the
    callee must re-check permissions for (spec 3.2)."""
    auth: Authenticator = request.app.state.auth
    service = await auth.verify_service(_bearer(request))
    return ServiceCaller(service=service, on_behalf_of=request.headers.get("x-on-behalf-of"))


User = Annotated[Principal, Depends(current_user)]
Caller = Annotated[ServiceCaller, Depends(service_caller)]
