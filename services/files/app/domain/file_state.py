"""The one place that decides StoredFile.Status changes (spec 2.3 state module).

Uploading → Active → PendingPurge → Purged; Active → Deleted (infected, removed); Uploading →
Deleted (rejected or abandoned). A purged or deleted original can be uploaded again into the same
row (dedupe per owner + sha256): Purged/Deleted → Uploading. Every write of the Status column goes
through `transition`; tests/test_state_module.py checks no other code writes it.
"""

from typing import Final

ALLOWED: Final[dict[str, frozenset[str]]] = {
    "Uploading": frozenset({"Active", "Deleted"}),
    "Active": frozenset({"PendingPurge", "Deleted"}),
    "PendingPurge": frozenset({"Active", "Purged", "Deleted"}),
    "Purged": frozenset({"Uploading", "Deleted"}),
    "Deleted": frozenset({"Uploading"}),
}


class IllegalTransition(ValueError):
    pass


def transition(current: str, target: str) -> str:
    """Returns `target` if the move is allowed, else raises."""
    if target not in ALLOWED.get(current, frozenset()):
        raise IllegalTransition(f"StoredFile {current} -> {target} is not allowed")
    return target


def counts_against_quota(status: str) -> bool:
    """Purged and Deleted objects don't count; PendingPurge still does until the purge runs."""
    return status in {"Active", "PendingPurge"}
