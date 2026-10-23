"""Idempotent consumers (spec 3.3): ConsumedEvent insert + the change in one transaction; ack after
commit. Handlers must tolerate gaps (the thing an event is about may be gone; spec 9.1)."""

import asyncio
import logging
import time
from enum import Enum
from typing import Protocol

from sqlalchemy.dialects.mysql import insert as mysql_insert
from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker

from crux_common.messaging.bus import Delivery, EventBus
from crux_common.messaging.contracts import ContractError, Contracts
from crux_common.messaging.tables import consumed_event
from crux_common.timeutil import utcnow

log = logging.getLogger("crux.consumer")

PENDING_SWEEP_SECONDS = 60


class Handler(Protocol):
    @property
    def topics(self) -> tuple[str, ...]: ...

    async def handle(self, session: AsyncSession, delivery: Delivery) -> None: ...


class Outcome(Enum):
    APPLIED = "applied"
    DUPLICATE = "duplicate"
    REJECTED = "rejected"


async def process(
    sessions: async_sessionmaker[AsyncSession], contracts: Contracts, handler: Handler, delivery: Delivery
) -> Outcome:
    env = delivery.envelope
    try:
        contracts.validate(env.topic, env.schemaVersion, env.payload)
    except ContractError as e:
        # A payload that breaks its contract will never succeed; record it so it isn't retried forever.
        log.error("event violates its contract", extra={"event_id": env.eventId, "error": str(e), "alert": True})
        async with sessions() as session, session.begin():
            await session.execute(
                mysql_insert(consumed_event)
                .prefix_with("IGNORE")
                .values(EventId=env.eventId, Topic=env.topic, ConsumedAt=utcnow())
            )
        return Outcome.REJECTED
    async with sessions() as session, session.begin():
        result = await session.execute(
            mysql_insert(consumed_event)
            .prefix_with("IGNORE")
            .values(EventId=env.eventId, Topic=env.topic, ConsumedAt=utcnow())
        )
        if result.rowcount == 0:  # type: ignore[attr-defined]
            return Outcome.DUPLICATE  # INSERT IGNORE wrote nothing; the empty commit is harmless
        await handler.handle(session, delivery)
    return Outcome.APPLIED


async def run(
    sessions: async_sessionmaker[AsyncSession],
    bus: EventBus,
    contracts: Contracts,
    group: str,
    consumer: str,
    handler: Handler,
    stop: asyncio.Event,
) -> None:
    log.info("consumer started", extra={"group": group, "topics": handler.topics})
    last_sweep = time.monotonic()
    while not stop.is_set():
        try:
            if time.monotonic() - last_sweep >= PENDING_SWEEP_SECONDS:
                last_sweep = time.monotonic()
                batch = await bus.read_pending(group, consumer, list(handler.topics), 50)
            else:
                batch = await bus.read(group, consumer, list(handler.topics), 50, 5000)
        except Exception as e:
            log.error("consumer read failed", extra={"error": repr(e)})
            await asyncio.sleep(2)
            continue
        for d in batch:
            try:
                outcome = await process(sessions, contracts, handler, d)
            except Exception as e:
                # Not acked: stays pending for this consumer; the next sweep retries it.
                log.error(
                    "event handler failed",
                    extra={"event_id": d.envelope.eventId, "topic": d.topic, "error": repr(e), "alert": True},
                )
                continue
            log.debug("event handled", extra={"event_id": d.envelope.eventId, "outcome": outcome.value})
            try:
                await bus.ack(group, d)
            except Exception as e:
                log.warning("ack failed; will be redelivered and deduped", extra={"error": repr(e)})
    log.info("consumer stopped", extra={"group": group})
