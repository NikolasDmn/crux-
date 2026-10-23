from fastapi import APIRouter
from fastapi.testclient import TestClient
from pydantic import BaseModel

from crux_common.app import create_app
from crux_common.auth import User
from crux_common.errors import NotFound, ValidationFailed
from crux_common.settings import CommonSettings

router = APIRouter()


class Body(BaseModel):
    n: int


@router.get("/missing")
async def missing() -> None:
    raise NotFound()


@router.get("/bad")
async def bad() -> None:
    raise ValidationFailed.field("name", "invalid", "Too short.")


@router.post("/body")
async def body(b: Body) -> dict[str, int]:
    return {"n": b.n}


@router.get("/me")
async def me(user: User) -> dict[str, str]:
    return {"id": user.user_ref}


@router.get("/boom")
async def boom() -> None:
    raise RuntimeError("kaboom")


def client() -> TestClient:
    settings = CommonSettings(service_name="test", database_url="mysql+aiomysql://u:p@127.0.0.1:1/db")
    return TestClient(create_app(settings, "test", [router]), raise_server_exceptions=False)


def test_errors_use_the_spec_format_with_request_id() -> None:
    with client() as c:
        r = c.get("/missing", headers={"x-request-id": "req-1"})
        assert r.status_code == 404
        assert r.json() == {
            "error": {"code": "not_found", "message": "Not found.", "details": None, "requestId": "req-1"}
        }
        assert r.headers["x-request-id"] == "req-1"

        r = c.get("/bad")
        assert r.status_code == 422
        assert r.json()["error"]["details"]["fields"][0]["field"] == "name"


def test_body_validation_maps_to_validation_failed() -> None:
    with client() as c:
        r = c.post("/body", json={"n": "x"})
        assert r.status_code == 422
        err = r.json()["error"]
        assert err["code"] == "validation_failed"
        assert err["details"]["fields"][0]["field"] == "n"


def test_no_token_is_401_and_unknown_route_404_and_crash_500() -> None:
    with client() as c:
        assert c.get("/me").json()["error"]["code"] == "unauthenticated"
        assert c.get("/nope").json()["error"]["code"] == "not_found"
        r = c.get("/boom")
        assert r.status_code == 500
        assert "kaboom" not in r.text, "internals don't leak"


def test_healthz_is_up_without_dependencies() -> None:
    with client() as c:
        assert c.get("/healthz").json() == {"status": "ok"}
        assert c.get("/readyz").status_code == 503
