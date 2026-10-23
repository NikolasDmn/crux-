"""PublicIds are ULIDs made by the app (schema header). Loose check: built-in rows use ids like
`01JBUILTIN...` that aren't strict Crockford base32 (OPEN-QUESTIONS.md)."""

import re

from ulid import ULID

_PUBLIC_ID = re.compile(r"^[0-9A-Z]{26}$")


def new_id() -> str:
    return str(ULID())


def is_public_id(value: str) -> bool:
    return bool(_PUBLIC_ID.match(value))
