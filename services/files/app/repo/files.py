"""StoredFile, FileChunk, FileJob (schema-v5-files.sql + migration 0002).

Status is written only by `set_status`, which takes a value already checked by
app.domain.file_state.transition (spec 2.3; tests/test_state_module.py)."""

from datetime import datetime
from typing import Any, cast

from sqlalchemy import (
    JSON,
    BigInteger,
    Column,
    DateTime,
    Integer,
    MetaData,
    String,
    Table,
    Text,
    and_,
    case,
    func,
    select,
    update,
)
from sqlalchemy.dialects.mysql import insert
from sqlalchemy.engine import CursorResult
from sqlalchemy.ext.asyncio import AsyncSession

from app.domain import file_state

metadata = MetaData()

stored_file = Table(
    "StoredFile",
    metadata,
    Column("Id", BigInteger, primary_key=True, autoincrement=True),
    Column("PublicId", String(26), nullable=False),
    Column("OwnerUserRef", String(26), nullable=False),
    Column("UploadedByUserRef", String(26)),
    Column("CourseRef", String(26)),
    Column("Purpose", String(16), nullable=False),
    Column("DerivedFromFileId", BigInteger),
    Column("SourcePage", Integer),
    Column("CropBox", JSON),
    Column("StorageKey", String(512)),
    Column("FileName", String(255), nullable=False),
    Column("MimeType", String(127), nullable=False),
    Column("SizeBytes", BigInteger, nullable=False),
    Column("Sha256", String(64), nullable=False),
    Column("PageCount", Integer),
    Column("Status", String(16), nullable=False),
    Column("ScanStatus", String(16), nullable=False),
    Column("ExtractStatus", String(16), nullable=False),
    Column("ExtractorVersion", String(16)),
    Column("PurgeRequestedAt", DateTime),
    Column("PurgeAt", DateTime),
    Column("PurgedAt", DateTime),
    Column("UploadedAt", DateTime),
    Column("RowVersion", Integer, nullable=False),
)

file_chunk = Table(
    "FileChunk",
    metadata,
    Column("Id", BigInteger, primary_key=True, autoincrement=True),
    Column("FileId", BigInteger, nullable=False),
    Column("PageNo", Integer),
    Column("ChunkNo", Integer, nullable=False),
    Column("Heading", String(255)),
    Column("Text", Text, nullable=False),
    Column("ContentSha", String(64), nullable=False),
    Column("ExtractMethod", String(8), nullable=False),
    Column("ExtractorVersion", String(16), nullable=False),
)

file_job = Table(
    "FileJob",
    metadata,
    Column("Id", BigInteger, primary_key=True, autoincrement=True),
    Column("FileId", BigInteger, nullable=False),
    Column("Kind", String(16), nullable=False),
    Column("Status", String(8), nullable=False),
    Column("Attempts", Integer, nullable=False),
    Column("NextAttemptAt", DateTime, nullable=False),
    Column("StartedAt", DateTime),
    Column("LastError", String(500)),
)


async def by_public_id(session: AsyncSession, public_id: str, for_update: bool = False) -> Any | None:
    stmt = select(stored_file).where(stored_file.c.PublicId == public_id)
    if for_update:
        stmt = stmt.with_for_update()
    return (await session.execute(stmt)).first()


async def by_id(session: AsyncSession, file_id: int, for_update: bool = False) -> Any | None:
    stmt = select(stored_file).where(stored_file.c.Id == file_id)
    if for_update:
        stmt = stmt.with_for_update()
    return (await session.execute(stmt)).first()


async def original_by_sha(session: AsyncSession, owner_ref: str, sha256: str) -> Any | None:
    """The owner's Original with this fingerprint (one per owner; uq_file_original), locked."""
    stmt = (
        select(stored_file)
        .where(
            stored_file.c.OwnerUserRef == owner_ref, stored_file.c.Purpose == "Original", stored_file.c.Sha256 == sha256
        )
        .with_for_update()
    )
    return (await session.execute(stmt)).first()


async def insert_original(session: AsyncSession, **values: Any) -> int:
    result = cast(
        CursorResult[Any],
        await session.execute(
            insert(stored_file).values(
                Purpose="Original",
                Status="Uploading",
                ScanStatus="Pending",
                ExtractStatus="Pending",
                RowVersion=0,
                **values,
            )
        ),
    )
    pk = result.inserted_primary_key
    assert pk is not None
    return int(pk[0])


async def insert_derived(session: AsyncSession, **values: Any) -> int:
    """A crop or thumbnail made from a clean original: born Active, Clean, nothing to extract."""
    result = cast(
        CursorResult[Any],
        await session.execute(
            insert(stored_file).values(
                Status="Active", ScanStatus="Clean", ExtractStatus="NotNeeded", RowVersion=0, **values
            )
        ),
    )
    pk = result.inserted_primary_key
    assert pk is not None
    return int(pk[0])


async def set_status(session: AsyncSession, file_id: int, current: str, target: str, **other: Any) -> None:
    """The only writer of StoredFile.Status. `other` sets accompanying columns (StorageKey, …)."""
    file_state.transition(current, target)
    await session.execute(
        update(stored_file)
        .where(stored_file.c.Id == file_id, stored_file.c.Status == current)
        .values(Status=target, RowVersion=stored_file.c.RowVersion + 1, **other)
    )


async def set_fields(session: AsyncSession, file_id: int, **values: Any) -> None:
    """Non-status columns (ScanStatus, ExtractStatus, PageCount, …)."""
    assert "Status" not in values, "Status changes go through set_status"
    await session.execute(
        update(stored_file).where(stored_file.c.Id == file_id).values(RowVersion=stored_file.c.RowVersion + 1, **values)
    )


async def abandoned_uploads(session: AsyncSession, before: datetime, limit: int = 100) -> list[Any]:
    stmt = (
        select(stored_file)
        .where(stored_file.c.Status == "Uploading", stored_file.c.UploadedAt < before)
        .limit(limit)
        .with_for_update(skip_locked=True)
    )
    return list((await session.execute(stmt)).all())


async def by_course(session: AsyncSession, course_ref: str) -> list[Any]:
    stmt = select(stored_file).where(stored_file.c.CourseRef == course_ref).with_for_update()
    return list((await session.execute(stmt)).all())


async def usage_by_course(session: AsyncSession, owner_ref: str) -> list[Any]:
    """Bytes per course for the quota view (spec 6.2): counted originals and their count."""
    stmt = (
        select(
            stored_file.c.CourseRef,
            func.coalesce(func.sum(stored_file.c.SizeBytes), 0).label("Bytes"),
            func.count().label("Originals"),
        )
        .where(
            stored_file.c.OwnerUserRef == owner_ref,
            stored_file.c.Purpose == "Original",
            stored_file.c.Status.in_(["Active", "PendingPurge"]),
        )
        .group_by(stored_file.c.CourseRef)
    )
    return list((await session.execute(stmt)).all())


# ---------- chunks ----------


async def replace_chunks(session: AsyncSession, file_id: int, rows: list[dict[str, Any]]) -> None:
    await session.execute(file_chunk.delete().where(file_chunk.c.FileId == file_id))
    if rows:
        await session.execute(insert(file_chunk), rows)


async def page_text(session: AsyncSession, file_id: int, page: int) -> str | None:
    rows = (
        (
            await session.execute(
                select(file_chunk.c.Text)
                .where(file_chunk.c.FileId == file_id, file_chunk.c.PageNo == page)
                .order_by(file_chunk.c.ChunkNo)
            )
        )
        .scalars()
        .all()
    )
    return "\n".join(rows) if rows else None


# ---------- jobs ----------


async def enqueue(session: AsyncSession, file_id: int, kind: str, now: datetime) -> None:
    """Idempotent per (file, kind): re-enqueueing resets a finished job (re-upload restore)."""
    stmt = insert(file_job).values(FileId=file_id, Kind=kind, Status="Pending", Attempts=0, NextAttemptAt=now)
    await session.execute(stmt.on_duplicate_key_update(Status="Pending", Attempts=0, NextAttemptAt=now, LastError=None))


async def claim_due(session: AsyncSession, now: datetime) -> Any | None:
    stmt = (
        select(file_job)
        .where(file_job.c.Status == "Pending", file_job.c.NextAttemptAt <= now)
        .order_by(file_job.c.NextAttemptAt)
        .limit(1)
        .with_for_update(skip_locked=True)
    )
    job = (await session.execute(stmt)).first()
    if job:
        await session.execute(update(file_job).where(file_job.c.Id == job.Id).values(Status="Running", StartedAt=now))
    return job


async def finish_job(session: AsyncSession, job_id: int, job_status: str, **values: Any) -> None:
    """FileJob.Status (not StoredFile.Status): Pending (retry), Done, Failed."""
    await session.execute(update(file_job).where(file_job.c.Id == job_id).values({"Status": job_status, **values}))


async def reclaim_stuck(session: AsyncSession, started_before: datetime) -> int:
    """Jobs left Running by a crashed worker go back to Pending."""
    result = cast(
        CursorResult[Any],
        await session.execute(
            update(file_job)
            .where(and_(file_job.c.Status == "Running", file_job.c.StartedAt < started_before))
            .values(Status="Pending")
        ),
    )
    return int(result.rowcount)


# ---------- storage saver (spec 6.2) ----------


async def due_purges(session: AsyncSession, now: datetime, limit: int = 100) -> list[Any]:
    """PendingPurge originals whose undo window has ended, locked."""
    stmt = (
        select(stored_file)
        .where(stored_file.c.Status == "PendingPurge", stored_file.c.PurgeAt <= now)
        .order_by(stored_file.c.PurgeAt)
        .limit(limit)
        .with_for_update(skip_locked=True)
    )
    return list((await session.execute(stmt)).all())


async def derived(session: AsyncSession, source_id: int, purpose: str) -> list[Any]:
    stmt = select(stored_file).where(
        stored_file.c.DerivedFromFileId == source_id, stored_file.c.Purpose == purpose, stored_file.c.Status == "Active"
    )
    return list((await session.execute(stmt.order_by(stored_file.c.SourcePage))).all())


async def thumbnail(session: AsyncSession, source_id: int, page: int) -> Any | None:
    stmt = select(stored_file).where(
        stored_file.c.DerivedFromFileId == source_id,
        stored_file.c.Purpose == "Thumbnail",
        stored_file.c.SourcePage == page,
        stored_file.c.Status == "Active",
    )
    return (await session.execute(stmt)).first()


async def owner_thumbnails(session: AsyncSession, owner_ref: str) -> list[Any]:
    stmt = (
        select(stored_file)
        .where(
            stored_file.c.OwnerUserRef == owner_ref,
            stored_file.c.Purpose == "Thumbnail",
            stored_file.c.Status == "Active",
        )
        .with_for_update()
    )
    return list((await session.execute(stmt)).all())


async def originals_for_thumbnails(session: AsyncSession, owner_ref: str) -> list[Any]:
    """Extracted originals whose object is still there (thumbnails need the PDF)."""
    stmt = select(stored_file).where(
        stored_file.c.OwnerUserRef == owner_ref,
        stored_file.c.Purpose == "Original",
        stored_file.c.Status.in_(["Active", "PendingPurge"]),
        stored_file.c.ExtractStatus == "Done",
    )
    return list((await session.execute(stmt)).all())


async def purgeable_by_course(session: AsyncSession, owner_ref: str) -> dict[str | None, tuple[int, int]]:
    """Per course: (bytes the saver could free now: Active + extracted originals, bytes already pending)."""
    stmt = (
        select(
            stored_file.c.CourseRef,
            func.coalesce(
                func.sum(
                    case(
                        (
                            and_(stored_file.c.Status == "Active", stored_file.c.ExtractStatus == "Done"),
                            stored_file.c.SizeBytes,
                        ),
                        else_=0,
                    )
                ),
                0,
            ).label("Purgeable"),
            func.coalesce(
                func.sum(case((stored_file.c.Status == "PendingPurge", stored_file.c.SizeBytes), else_=0)), 0
            ).label("Pending"),
        )
        .where(stored_file.c.OwnerUserRef == owner_ref, stored_file.c.Purpose == "Original")
        .group_by(stored_file.c.CourseRef)
    )
    return {r.CourseRef: (int(r.Purgeable), int(r.Pending)) for r in await session.execute(stmt)}
