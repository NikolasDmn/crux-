# Contracts

Shared, versioned message schemas (backend-spec 3.3).

- `events/envelope.json`: the envelope every event travels in.
- `events/<topic>@<version>.json`: one JSON Schema (draft 2020-12) per payload version. Producers
  validate before writing the outbox row; consumers validate on read.
- `user-events/`: the registry of `UserEvent` types accepted by `POST /events` (milestone 5).

## Adding or changing a topic

1. Add `events/<topic>@1.json` and list the topic in the producer's topic registry
   (`core/crates/domain/src/topics.rs`, or `app/domain/topics.py` in a Python service).
2. Add a producer fixture under `fixtures/` and a consumer test that parses it.
3. A breaking change is a new file `<topic>@2.json`. The producer publishes both versions until
   every consumer has moved over, then drops v1.
