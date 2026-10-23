"""Service-to-service auth (spec 3.2): trade the shared bootstrap secret for a 5-minute token from
core, cache it per audience, refresh a minute before expiry."""

import time

import httpx


class ServiceTokens:
    def __init__(self, core_url: str, service: str, secret: str, client: httpx.AsyncClient | None = None) -> None:
        self._core_url = core_url.rstrip("/")
        self._service = service
        self._secret = secret
        self._client = client
        self._cache: dict[str, tuple[str, float]] = {}

    async def token(self, audience: str) -> str:
        cached = self._cache.get(audience)
        if cached and cached[1] - 60 > time.monotonic():
            return cached[0]
        client = self._client or httpx.AsyncClient(timeout=2.0)
        try:
            resp = await client.post(
                f"{self._core_url}/internal/v1/service-token",
                json={"service": self._service, "audience": audience},
                headers={"authorization": f"Bearer {self._secret}"},
            )
            resp.raise_for_status()
            data = resp.json()
        finally:
            if self._client is None:
                await client.aclose()
        self._cache[audience] = (data["token"], time.monotonic() + float(data["expiresIn"]))
        return str(data["token"])

    async def headers(self, audience: str, on_behalf_of: str | None = None) -> dict[str, str]:
        h = {"authorization": f"Bearer {await self.token(audience)}"}
        if on_behalf_of:
            h["x-on-behalf-of"] = on_behalf_of
        return h
