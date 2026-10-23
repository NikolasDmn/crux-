//! Outbox relay (spec 3.3): publishes unpublished rows in `Id` order and stamps `PublishedAt`.
//! At-least-once: a crash between publish and commit republishes; consumers dedupe.

use std::time::Duration;

use crux_clients::bus::{Envelope, EventBus};
use crux_clients::clock::Clock;
use crux_repo::{messaging, MySqlPool};
use crux_service::time::iso;

pub const BATCH: u32 = 100;

pub fn envelope(row: &messaging::OutboxRow, producer: &str) -> Envelope {
    Envelope {
        event_id: row.event_id.clone(),
        topic: row.topic.clone(),
        schema_version: row.schema_version as u16,
        occurred_at: iso(row.created_at),
        producer: producer.to_string(),
        aggregate_ref: row.aggregate_ref.clone(),
        payload: row.payload.0.clone(),
    }
}

/// Publishes one batch. Stops at the first failure so order is kept; returns how many went out.
pub async fn publish_batch(pool: &MySqlPool, bus: &dyn EventBus, clock: &dyn Clock, producer: &str) -> anyhow::Result<u64> {
    let mut tx = crux_repo::begin(pool).await?;
    let rows = messaging::lock_unpublished(&mut tx, BATCH).await?;
    let mut published = 0;
    for row in &rows {
        match bus.publish(&envelope(row, producer)).await {
            Ok(()) => {
                messaging::mark_published(&mut tx, row.id, clock.now()).await?;
                published += 1;
            }
            Err(e) => {
                tracing::warn!(event_id = %row.event_id, topic = %row.topic, error = %e, "publish failed; will retry");
                messaging::mark_attempt_failed(&mut tx, row.id).await?;
                break;
            }
        }
    }
    tx.commit().await?;
    Ok(published)
}

/// Continuous relay: drains while there is work, then polls every `idle`.
pub async fn run(
    pool: MySqlPool,
    bus: std::sync::Arc<dyn EventBus>,
    clock: std::sync::Arc<dyn Clock>,
    producer: &'static str,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let idle = Duration::from_millis(500);
    tracing::info!(producer, "outbox relay started");
    loop {
        let wait = match publish_batch(&pool, bus.as_ref(), clock.as_ref(), producer).await {
            Ok(n) if n == BATCH as u64 => Duration::ZERO,
            Ok(_) => idle,
            Err(e) => {
                tracing::error!(error = %e, alert = true, "outbox relay error");
                Duration::from_secs(2)
            }
        };
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = shutdown.changed() => break,
        }
    }
    tracing::info!(producer, "outbox relay stopped");
}
