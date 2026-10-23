"""Migration helpers. Each service's migration 0001 is its `schema-v5-*.sql`, verbatim (spec 2.2);
this splits that file into statements so it runs without the MULTI_STATEMENTS client flag."""

from pathlib import Path


def split_sql(sql: str) -> list[str]:
    """Splits on `;` outside quotes, backticks and comments. Comments are dropped."""
    statements: list[str] = []
    buf: list[str] = []
    i, n = 0, len(sql)
    quote: str | None = None
    while i < n:
        c = sql[i]
        if quote:
            buf.append(c)
            if c == "\\" and quote != "`" and i + 1 < n:
                buf.append(sql[i + 1])
                i += 2
                continue
            if c == quote:
                if i + 1 < n and sql[i + 1] == quote:  # doubled quote escape: 'It''s'
                    buf.append(sql[i + 1])
                    i += 2
                    continue
                quote = None
            i += 1
            continue
        if c in ("'", '"', "`"):
            quote = c
            buf.append(c)
        elif sql.startswith("--", i) and (i + 2 >= n or sql[i + 2] in " \t\r\n"):
            end = sql.find("\n", i)
            i = n if end == -1 else end
            continue
        elif c == "#":
            end = sql.find("\n", i)
            i = n if end == -1 else end
            continue
        elif sql.startswith("/*", i):
            end = sql.find("*/", i + 2)
            i = n if end == -1 else end + 2
            continue
        elif c == ";":
            stmt = "".join(buf).strip()
            if stmt:
                statements.append(stmt)
            buf = []
        else:
            buf.append(c)
        i += 1
    tail = "".join(buf).strip()
    if tail:
        statements.append(tail)
    return statements


def read_statements(path: Path) -> list[str]:
    return split_sql(path.read_text(encoding="utf-8"))


MIGRATION_LOCK_TIMEOUT_SECONDS = 300


def run_online(database_url: str) -> None:
    """Body of every service's Alembic env.py. API and worker containers both migrate at start;
    a MySQL named lock makes the second wait for the first, then find nothing left to do."""
    from alembic import context
    from sqlalchemy import create_engine, text

    engine = create_engine(database_url.replace("+aiomysql", "+pymysql"))
    with engine.connect() as connection:
        connection.execute(text("SET time_zone = '+00:00'"))
        lock = f"crux_migrate:{connection.engine.url.database}"
        got = connection.execute(
            text("SELECT GET_LOCK(:n, :t)"), {"n": lock, "t": MIGRATION_LOCK_TIMEOUT_SECONDS}
        ).scalar()
        if got != 1:
            raise RuntimeError(f"could not take migration lock {lock}")
        # SQLAlchemy 2 autobegan a transaction on the statements above. End it, or Alembic runs
        # inside it and its alembic_version row is rolled back on close (DDL itself autocommits).
        connection.commit()
        try:
            context.configure(connection=connection, target_metadata=None, transaction_per_migration=True)
            with context.begin_transaction():
                context.run_migrations()
        finally:
            connection.execute(text("SELECT RELEASE_LOCK(:n)"), {"n": lock})
            connection.commit()
    engine.dispose()
