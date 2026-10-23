"""Publishing = an Outbox row in the caller's transaction (spec 3.3)."""

from typing import Any

from sqlalchemy import insert
from sqlalchemy.ext.asyncio import AsyncSession

from crux_common.ids import new_id
from crux_common.messaging.contracts import Contracts
from crux_common.messaging.tables import outbox
from crux_common.timeutil import utcnow


async def emit(
    session: AsyncSession,
    contracts: Contracts,
    topic: str,
    aggregate_ref: str | None,
    payload: dict[str, Any],
    schema_version: int = 1,
) -> str:
    """Validates against the contract, then writes the row. Returns the event id."""
    contracts.validate(topic, schema_version, payload)
    event_id = new_id()
    await session.execute(
        insert(outbox).values(
            EventId=event_id,
            Topic=topic,
            SchemaVersion=schema_version,
            AggregateRef=aggregate_ref,
            Payload=payload,
            CreatedAt=utcnow(),
            Attempts=0,
        )
    )
    return event_id
