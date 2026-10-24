import re
from pathlib import Path

import pytest

from app.domain import file_state, uploads
from app.domain.uploads import QuotaState, Rejection

APP = Path(__file__).resolve().parents[1] / "app"


def test_state_transitions() -> None:
    assert file_state.transition("Uploading", "Active") == "Active"
    assert file_state.transition("Purged", "Uploading") == "Uploading", "re-upload restores the row"
    for bad in [("Active", "Uploading"), ("Deleted", "Active"), ("Purged", "Active"), ("Uploading", "Purged")]:
        with pytest.raises(file_state.IllegalTransition):
            file_state.transition(*bad)
    assert file_state.counts_against_quota("PendingPurge")
    assert not file_state.counts_against_quota("Purged")


def test_only_the_state_helper_writes_status() -> None:
    """Spec 2.3: one module owns StoredFile.Status. `Status=` may appear only in repo.set_status."""
    offenders = []
    for path in APP.rglob("*.py"):
        for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            if re.search(r"\bvalues\([^)]*\bStatus=", line) or re.search(r"[^a-zA-Z]Status=\"", line):
                offenders.append(f"{path.relative_to(APP)}:{n}")
    allowed = {"repo/files.py"}
    assert all(o.split(":")[0].replace("\\", "/") in allowed for o in offenders), offenders


def test_upload_checks() -> None:
    ok = uploads.check_start("Week 1.PDF", 1000, "a" * 64, {"application/pdf"}, 10_000)
    assert ok == "application/pdf"
    for args, reason in [
        (("notes.docx", 1000, "a" * 64), "type"),
        (("a.pdf", 0, "a" * 64), "size"),
        (("a.pdf", 20_000, "a" * 64), "size"),
        (("a.pdf", 10, "xyz"), "checksum"),
    ]:
        r = uploads.check_start(*args, {"application/pdf"}, 10_000)
        assert isinstance(r, Rejection) and r.reason == reason
    assert isinstance(uploads.check_start("a.pdf", 10, "a" * 64, set(), 10_000), Rejection), "the allowlist is config"


def test_sniff_and_names_and_quota() -> None:
    assert uploads.sniff_ok("application/pdf", b"%PDF-1.7")
    assert not uploads.sniff_ok("application/pdf", b"PK\x03\x04")
    assert uploads.safe_file_name("..\\..\\evil\x00/x.pdf") == "x.pdf"
    q = QuotaState(quota=100, used=60, reserved=30)
    assert q.can_reserve(10) and not q.can_reserve(11)
    assert q.free == 10
    assert uploads.needs_ocr("   short   ") and not uploads.needs_ocr("x" * 20)
