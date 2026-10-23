"""Outbox relay: publishes unpublished rows in Id order, stamps PublishedAt. At least once."""

import asyncio
import logging
from typing import Any

from sqlalchemy import select, update
from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker

from crux_common.messaging.bus import Envelope, EventBus
from crux_common.messaging.tables import outbox
from crux_common.timeutil import iso, utcnow

log = logging.getLogger("crux.relay")

BATCH = 100


def envelope_of(row: Any, producer: str) -> Envelope:
    return Envelope(
        eventId=row.EventId,
        topic=row.Topic,
        schemaVersion=row.SchemaVersion,
        occurredAt=iso(row.CreatedAt),
        producer=producer,
        aggregateRef=row.AggregateRef,
        payload=row.Payload,
    )


async def publish_batch(sessions: async_sessionmaker[AsyncSession], bus: EventBus, producer: str) -> int:
    """One batch; stops at the first publish failure so order is kept."""
    published = 0
    async with sessions() as session, session.begin():
        rows = (
            await session.execute(
                select(outbox)
                .where(outbox.c.PublishedAt.is_(None))
                .order_by(outbox.c.Id)
                .limit(BATCH)
                .with_for_update(skip_locked=True)
            )
        ).all()
        for row in rows:
            try:
                await bus.publish(envelope_of(row, producer))
            except Exception as e:
                log.warning("publish failed; will retry", extra={"event_id": row.EventId, "error": repr(e)})
                await session.execute(
                    update(outbox).where(outbox.c.Id == row.Id).values(Attempts=outbox.c.Attempts + 1)
                )
                break
            await session.execute(
                update(outbox).where(outbox.c.Id == row.Id).values(PublishedAt=utcnow(), Attempts=outbox.c.Attempts + 1)
            )
            published += 1
    return published


async def run(sessions: async_sessionmaker[AsyncSession], bus: EventBus, producer: str, stop: asyncio.Event) -> None:
    log.info("outbox relay started", extra={"producer": producer})
    while not stop.is_set():
        try:
            n = await publish_batch(sessions, bus, producer)
            wait = 0.0 if n == BATCH else 0.5
        except Exception as e:
            log.error("outbox relay error", extra={"error": repr(e), "alert": True})
            wait = 2.0
        try:
            await asyncio.wait_for(stop.wait(), timeout=wait or 0.001)
        except TimeoutError:
            pass
    log.info("outbox relay stopped", extra={"producer": producer})
