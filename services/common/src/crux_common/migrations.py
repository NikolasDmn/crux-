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
