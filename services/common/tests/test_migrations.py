from pathlib import Path

from crux_common.migrations import split_sql

SCHEMAS = Path(__file__).resolve().parents[3] / "crux-spec" / "crux-spec" / "schema"


def test_splits_on_semicolons_outside_quotes_and_comments() -> None:
    sql = """
    -- a comment; with a semicolon
    CREATE TABLE a (x INT); # hash comment; here
    INSERT INTO a VALUES ('It''s; fine', "dq;", `b;t`, 'back\\'slash;');
    /* block; comment */ SELECT 1
    """
    assert split_sql(sql) == [
        "CREATE TABLE a (x INT)",
        "INSERT INTO a VALUES ('It''s; fine', \"dq;\", `b;t`, 'back\\'slash;')",
        "SELECT 1",
    ]


def test_double_dash_needs_a_space_to_be_a_comment() -> None:
    assert split_sql("SELECT 1--1;") == ["SELECT 1--1"]


def test_real_schemas_split_into_whole_statements() -> None:
    expected = {"files": 5, "notify": 7, "ai": 20}  # top-level CREATE/ALTER/INSERT lines
    for name, count in expected.items():
        statements = split_sql((SCHEMAS / f"schema-v5-{name}.sql").read_text(encoding="utf-8"))
        assert len(statements) == count, name
        assert all(s.split()[0].upper() in {"CREATE", "ALTER", "INSERT"} for s in statements), name
    ai = split_sql((SCHEMAS / "schema-v5-ai.sql").read_text(encoding="utf-8"))
    seed = next(s for s in ai if s.startswith("INSERT INTO AiTutorTemplate"))
    assert "real exam''s style" in seed, "doubled quotes survive"
