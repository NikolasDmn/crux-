"""Transactional outbox + idempotent consumers on Redis Streams (backend-spec 3.3).

- Publish: `emit(session, ...)` writes an Outbox row in the caller's transaction. Never publish directly.
- Relay: `relay.run(...)` publishes unpublished rows in Id order and stamps PublishedAt.
- Consume: `consumer.process(...)` inserts ConsumedEvent(EventId) and applies the change in one
  transaction (duplicate key = already handled); the runner acks after commit.

Same wire format as core (core/crates/clients/src/bus.rs): stream `crux:events:<topic>`, field
`envelope` holding the JSON envelope.
"""

from crux_common.messaging.bus import Delivery, Envelope, EventBus, MemoryBus, RedisBus
from crux_common.messaging.outbox import emit

__all__ = ["Delivery", "Envelope", "EventBus", "MemoryBus", "RedisBus", "emit"]
