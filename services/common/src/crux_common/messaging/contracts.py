"""Payload contracts: JSON Schemas in `contracts/events/<topic>@<version>.json`. Producers validate
before writing the outbox row; consumers validate on read."""

import json
from functools import cache
from pathlib import Path
from typing import Any

from jsonschema import Draft202012Validator


class ContractError(ValueError):
    pass


class Contracts:
    def __init__(self, contracts_dir: Path) -> None:
        self._dir = contracts_dir / "events"

    @cache  # noqa: B019 - one instance per process
    def _validator(self, topic: str, version: int) -> Draft202012Validator:
        path = self._dir / f"{topic}@{version}.json"
        if not path.is_file():
            raise ContractError(f"no contract for {topic}@{version}")
        return Draft202012Validator(json.loads(path.read_text(encoding="utf-8")))

    def validate(self, topic: str, version: int, payload: Any) -> None:
        errors = sorted(self._validator(topic, version).iter_errors(payload), key=lambda e: list(e.path))
        if errors:
            detail = "; ".join(f"{'/'.join(map(str, e.path)) or '<root>'}: {e.message}" for e in errors)
            raise ContractError(f"{topic}@{version} payload invalid: {detail}")

    def has(self, topic: str, version: int = 1) -> bool:
        return (self._dir / f"{topic}@{version}.json").is_file()
