import json
import time
from pathlib import Path

import jwt
import pytest
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

from crux_common.auth import ACCESS_AUDIENCE, ISSUER, Authenticator, Jwks
from crux_common.errors import Unauthenticated

FIXTURE = Path(__file__).resolve().parents[3] / "contracts" / "fixtures" / "jwt-interop.json"


def _auth(jwks: dict, service: str = "files") -> Authenticator:
    j = Jwks("http://unused.invalid/jwks")
    j.load(jwks)
    return Authenticator(j, service)


async def test_verifies_tokens_signed_by_core() -> None:
    """Interop: the token core signed (Rust) verifies here with core's JWKS."""
    f = json.loads(FIXTURE.read_text())
    auth = _auth(f["jwks"])
    p = await auth.verify_access(f["accessToken"])
    assert (p.user_ref, p.role, p.tier) == ("01TEST00000000000000000001", "User", "free")
    assert await auth.verify_service(f["serviceToken"]) == "core"


async def test_service_token_is_bound_to_this_service() -> None:
    f = json.loads(FIXTURE.read_text())
    with pytest.raises(Unauthenticated):
        await _auth(f["jwks"], service="ai").verify_service(f["serviceToken"])
    with pytest.raises(Unauthenticated):
        await _auth(f["jwks"]).verify_access(f["serviceToken"])


def _local_key() -> tuple[Ed25519PrivateKey, dict]:
    key = Ed25519PrivateKey.generate()
    jwk = json.loads(jwt.algorithms.OKPAlgorithm.to_jwk(key.public_key()))
    jwk["kid"] = "k1"
    return key, {"keys": [jwk]}


def _sign(key: Ed25519PrivateKey, **claims: object) -> str:
    pem = key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption())
    base = {"iss": ISSUER, "aud": ACCESS_AUDIENCE, "sub": "01TEST00000000000000000002", "exp": int(time.time()) + 60}
    return jwt.encode({**base, **claims}, pem, algorithm="EdDSA", headers={"kid": "k1"})


async def test_rejects_expired_wrong_audience_and_unknown_issuer() -> None:
    key, jwks = _local_key()
    auth = _auth(jwks)
    assert (await auth.verify_access(_sign(key))).user_ref == "01TEST00000000000000000002"
    for bad in (_sign(key, exp=int(time.time()) - 1), _sign(key, aud="other"), _sign(key, iss="evil")):
        with pytest.raises(Unauthenticated):
            await auth.verify_access(bad)


async def test_rejects_other_keys_and_alg_none() -> None:
    _key, jwks = _local_key()
    other, _ = _local_key()
    auth = _auth(jwks)
    with pytest.raises(Unauthenticated):
        await auth.verify_access(_sign(other))
    unsigned = jwt.encode({"sub": "x", "aud": ACCESS_AUDIENCE, "iss": ISSUER}, None, algorithm="none")  # type: ignore[arg-type]
    with pytest.raises(Unauthenticated):
        await auth.verify_access(unsigned)


class _Markers:
    def __init__(self, values: dict[str, str]) -> None:
        self.values = values

    async def get(self, key: str) -> str | None:
        return self.values.get(key)


async def test_tokens_issued_before_a_revocation_are_refused() -> None:
    """Audit L1: core's `crux:revoked:<user>` marker ends older tokens here too."""
    f = json.loads(FIXTURE.read_text())
    iat = jwt.decode(f["accessToken"], options={"verify_signature": False})["iat"]
    j = Jwks("http://unused.invalid/jwks")
    j.load(f["jwks"])
    key = "crux:revoked:01TEST00000000000000000001"
    assert (await Authenticator(j, "files", _Markers({})).verify_access(f["accessToken"])).user_ref
    assert await Authenticator(j, "files", _Markers({key: str(iat)})).verify_access(f["accessToken"])
    with pytest.raises(Unauthenticated):
        await Authenticator(j, "files", _Markers({key: str(iat + 1)})).verify_access(f["accessToken"])
