"""Milestone 7: storage saver (request, undo, purge, re-upload restore, auto purge) and thumbnails
(spec 6.1 step 5, 6.2)."""

from datetime import timedelta

from sqlalchemy import select, update

from app.repo import files as repo
from app.repo import storage
from app.repo.files import stored_file
from app.service import files as svc
from app.service import saver
from crux_common.messaging.bus import Delivery, Envelope
from crux_common.messaging.consumer import Outcome, process
from crux_common.timeutil import iso, utcnow
from tests.test_upload_flow import COURSE, OWNER, STRANGER, drain, env, make_pdf, topics, upload

_n = 0


def event(topic: str, payload: dict[str, object]) -> Delivery:
    global _n
    _n += 1
    return Delivery("1-0", topic, Envelope(f"01SAVER{_n:019d}", topic, 1, "x", "core", COURSE, payload))


async def test_request_undo_purge_and_restore() -> None:
    async with env() as (deps, store, _):
        data = make_pdf(["Hypothesis tests compare a statistic with its null distribution."])
        f = await upload(deps, store, OWNER, data, course=COURSE)
        await drain(deps)
        view = await svc.storage_view(deps, OWNER)
        assert (view.by_course[0].purgeable, view.by_course[0].pending_purge) == (len(data), 0)

        handler = saver.PurgeRequestsHandler(deps.contracts)
        later = iso(utcnow() + timedelta(days=7))
        # Someone else's request is ignored.
        bad = event("material.purge_requested", {"fileRef": f.id, "ownerRef": STRANGER.user_ref, "purgeAt": later})
        assert await process(deps.sessions, deps.contracts, handler, bad) == Outcome.APPLIED
        assert (await svc.get(deps, OWNER, f.id)).status == "Active"

        req = {"fileRef": f.id, "ownerRef": OWNER.user_ref, "courseRef": COURSE, "purgeAt": later}
        await process(deps.sessions, deps.contracts, handler, event("material.purge_requested", req))
        assert (await svc.get(deps, OWNER, f.id)).status == "PendingPurge"
        assert (await svc.storage_view(deps, OWNER)).by_course[0].pending_purge == len(data)
        # Undo.
        cancel = {"fileRef": f.id, "ownerRef": OWNER.user_ref}
        await process(deps.sessions, deps.contracts, handler, event("material.purge_cancelled", cancel))
        assert (await svc.get(deps, OWNER, f.id)).status == "Active"

        # Request again; nothing happens before purgeAt, then the object goes.
        await process(deps.sessions, deps.contracts, handler, event("material.purge_requested", req))
        assert await saver.purge_due(deps) == 0
        async with deps.sessions() as s, s.begin():
            await s.execute(
                update(stored_file)
                .where(stored_file.c.PublicId == f.id)
                .values(PurgeAt=utcnow() - timedelta(seconds=1))
            )
        assert await saver.purge_due(deps) == 1
        got = await svc.get(deps, OWNER, f.id)
        assert got.status == "Purged"
        assert store.objects == {}, "the original object is deleted"
        assert "hypothesis tests" in (await svc.page_text(deps, OWNER, f.id, 1)).text.lower(), "text survives"
        assert (await svc.storage_view(deps, OWNER)).used_bytes == 0, "quota released"
        t = await topics(deps)
        assert "file.purged" in t and "notify.requested" in t

        # Re-upload the same bytes: the same row comes back, no second scan or extract.
        back = await upload(deps, store, OWNER, data, course=COURSE)
        assert (back.id, back.status, back.scan_status, back.extract_status) == (f.id, "Active", "Clean", "Done")
        assert await topics(deps) == [*t, "file.restored"]
        await drain(deps)
        assert (await topics(deps))[-1] == "file.restored", "no new scan"


async def test_auto_purge_and_thumbnails() -> None:
    async with env() as (deps, store, _):
        await svc.patch_storage_settings(
            deps, OWNER, svc.StorageSettings(auto_purge_originals=True, keep_thumbnails=True)
        )
        data = make_pdf(["Page one", "Page two"])
        f = await upload(deps, store, OWNER, data)
        await drain(deps)
        got = await svc.get(deps, OWNER, f.id)
        assert got.status == "PendingPurge", "auto purge scheduled after extraction"
        assert "file.purge_scheduled" in await topics(deps)
        thumb = await svc.thumbnail_url(deps, OWNER, f.id, 2)
        assert thumb.url.startswith("memory://get/")
        async with deps.sessions() as s:
            thumbs = await repo.derived(s, (await repo.by_public_id(s, f.id)).Id, "Thumbnail")
            q = await storage.get(s, OWNER.user_ref)
        assert len(thumbs) == 2 and all(t.MimeType == "image/webp" for t in thumbs)
        assert q.UsedBytes == len(data) + sum(t.SizeBytes for t in thumbs), "thumbnails count against the quota"

        # Purging keeps the thumbnails; turning thumbnails off deletes them.
        async with deps.sessions() as s, s.begin():
            await s.execute(
                update(stored_file)
                .where(stored_file.c.PublicId == f.id)
                .values(PurgeAt=utcnow() - timedelta(seconds=1))
            )
        assert await saver.purge_due(deps) == 1
        assert (await svc.thumbnail_url(deps, OWNER, f.id, 1)).url
        await svc.patch_storage_settings(deps, OWNER, svc.StorageSettings(keep_thumbnails=False))
        async with deps.sessions() as s:
            active = (await s.execute(select(stored_file).where(stored_file.c.Status == "Active"))).all()
        assert active == []
        assert (await svc.storage_view(deps, OWNER)).used_bytes == 0


async def test_chunk_search_for_ai() -> None:
    async with env() as (deps, store, _):
        a = await upload(
            deps, store, OWNER, make_pdf(["Regression fits a line to data.", "Residuals measure the error of the fit."])
        )
        b = await upload(deps, store, OWNER, make_pdf(["Probability of independent events multiplies."]), name="b.pdf")
        await drain(deps)
        async with deps.sessions() as s:
            hits = await repo.search_chunks(s, [a.id, b.id], "residuals error", 5)
            assert [(h.FileRef, h.PageNo) for h in hits][:1] == [(a.id, 2)]
            assert await repo.search_chunks(s, [b.id], "residuals", 5) == []
            row = await repo.by_public_id(s, a.id)
            assert [c.PageNo for c in await repo.chunks_of(s, row.Id, [1])] == [1]


async def test_export_and_user_deletion() -> None:
    import zipfile
    from io import BytesIO

    from app.service import lifecycle

    async with env() as (deps, store, _):
        data = make_pdf(["My notes"])
        f = await upload(deps, store, OWNER, data, course=COURSE)
        await drain(deps)
        out = await lifecycle.build_export(deps, OWNER.user_ref, "01EXPORT000000000000000001", {"core": {"user": "x"}})
        again = await lifecycle.build_export(deps, OWNER.user_ref, "01EXPORT000000000000000001", {"core": {}})
        assert again == out, "idempotent per export"
        async with deps.sessions() as s:
            row = await repo.by_public_id(s, out["fileRef"])
        z = zipfile.ZipFile(BytesIO(store.objects[row.StorageKey]))
        names = z.namelist()
        assert "core.json" in names and "files.json" in names and any(n.startswith("originals/") for n in names)
        assert (await svc.storage_view(deps, OWNER)).used_bytes == len(data), "exports don't count"
        url = await svc.download_url(deps, OWNER, out["fileRef"])
        assert url.url

        async with deps.sessions() as s, s.begin():
            await s.execute(
                update(stored_file)
                .where(stored_file.c.PublicId == out["fileRef"])
                .values(UploadedAt=utcnow() - timedelta(days=2))
            )
        assert await lifecycle.expire_exports(deps) == 1

        h = lifecycle.UserDeletedHandler(deps, deps.contracts)
        d = Delivery(
            "1-0",
            "user.deleted",
            Envelope(
                "01EVTDEL00000000000000000A",
                "user.deleted",
                1,
                "x",
                "core",
                OWNER.user_ref,
                {"userRef": OWNER.user_ref},
            ),
        )
        assert await process(deps.sessions, deps.contracts, h, d) == Outcome.APPLIED
        async with deps.sessions() as s:
            row = await repo.by_public_id(s, f.id)
            q = await storage.get(s, OWNER.user_ref)
        assert (row.Status, row.StorageKey, q) == ("Deleted", None, None)
        assert store.objects == {}
        assert "user.purged" in await topics(deps)
