"""crux_ai migrates from empty and seeds the three built-in tutors (spec 7.3)."""

from pathlib import Path

from sqlalchemy import text

from crux_common.testing import migrated_db

SERVICE_DIR = Path(__file__).resolve().parents[1]


async def test_migrates_from_empty_with_builtin_tutors() -> None:
    async with migrated_db(SERVICE_DIR, "ai") as sessions, sessions() as s:
        keys = (await s.execute(text("SELECT TemplateKey FROM AiTutorTemplate ORDER BY Id"))).scalars().all()
        assert keys == ["strict-coach", "friendly-explainer", "exam-drill"]


def test_app_imports(monkeypatch) -> None:  # type: ignore[no-untyped-def]
    monkeypatch.setenv("CRUX_DATABASE_URL", "mysql+aiomysql://u:p@127.0.0.1:1/db")
    from app.main import app

    assert app.title == "Crux AI"
