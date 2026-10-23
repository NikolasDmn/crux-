"""Outbox and ConsumedEvent: the same two tables in every service DB (schema files, section 15)."""

from sqlalchemy import JSON, BigInteger, Column, DateTime, Integer, MetaData, SmallInteger, String, Table

metadata = MetaData()

outbox = Table(
    "Outbox",
    metadata,
    Column("Id", BigInteger, primary_key=True, autoincrement=True),
    Column("EventId", String(26), nullable=False, unique=True),
    Column("Topic", String(64), nullable=False),
    Column("SchemaVersion", SmallInteger, nullable=False, default=1),
    Column("AggregateRef", String(26)),
    Column("Payload", JSON, nullable=False),
    Column("CreatedAt", DateTime(timezone=False), nullable=False),
    Column("PublishedAt", DateTime(timezone=False)),
    Column("Attempts", Integer, nullable=False, default=0),
)

consumed_event = Table(
    "ConsumedEvent",
    metadata,
    Column("EventId", String(26), primary_key=True),
    Column("Topic", String(64), nullable=False),
    Column("ConsumedAt", DateTime(timezone=False)),
)
