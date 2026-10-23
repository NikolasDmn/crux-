//! Event consumer (spec 3.3): in one transaction insert `ConsumedEvent(EventId)` and apply the
//! change; a duplicate key means already handled, so ack and skip. Ack after commit.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use crux_clients::bus::{Delivery, EventBus};
use crux_repo::{messaging, MySqlConnection, MySqlPool};

/// How often a consumer re-reads its own unacked messages to retry failed handlers.
pub const PENDING_SWEEP: Duration = Duration::from_secs(60);

#[async_trait]
pub trait Handler: Send + Sync {
    /// Topics this handler consumes.
    fn topics(&self) -> Vec<&'static str>;
    /// Applies the event inside the consumer's transaction. Must tolerate gaps: the thing the event
    /// is about may already be gone (spec 3.3, 9.1).
    async fn handle(&self, conn: &mut MySqlConnection, delivery: &Delivery) -> anyhow::Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Applied,
    Duplicate,
}

/// Handles one delivery exactly once in effect. Returns after commit; the caller acks.
pub async fn process(pool: &MySqlPool, handler: &dyn Handler, delivery: &Delivery) -> anyhow::Result<Outcome> {
    let mut tx = crux_repo::begin(pool).await?;
    let fresh = messaging::mark_consumed(&mut tx, &delivery.envelope.event_id, &delivery.envelope.topic).await?;
    if !fresh {
        tx.rollback().await?;
        return Ok(Outcome::Duplicate);
    }
    handler.handle(&mut tx, delivery).await?;
    tx.commit().await?;
    Ok(Outcome::Applied)
}

pub async fn run(
    pool: MySqlPool,
    bus: Arc<dyn EventBus>,
    group: &'static str,
    consumer: String,
    handler: Arc<dyn Handler>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let topics = handler.topics();
    tracing::info!(group, ?topics, "consumer started");
    let mut last_sweep = std::time::Instant::now();
    loop {
        if *shutdown.borrow() {
            break;
        }
        let batch = if last_sweep.elapsed() >= PENDING_SWEEP {
            last_sweep = std::time::Instant::now();
            bus.read_pending(group, &consumer, &topics, 50).await
        } else {
            tokio::select! {
                r = bus.read(group, &consumer, &topics, 50, 5_000) => r,
                _ = shutdown.changed() => break,
            }
        };
        let deliveries = match batch {
            Ok(d) => d,
            Err(e) => {
                tracing::error!(error = %e, "consumer read failed");
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };
        for d in deliveries {
            match process(&pool, handler.as_ref(), &d).await {
                Ok(outcome) => {
                    tracing::debug!(event_id = %d.envelope.event_id, ?outcome, "event handled");
                    if let Err(e) = bus.ack(group, &d).await {
                        tracing::warn!(error = %e, "ack failed; event will be redelivered and deduped");
                    }
                }
                // Not acked: stays in this consumer's pending list; the next pending sweep retries it.
                Err(e) => {
                    tracing::error!(event_id = %d.envelope.event_id, topic = %d.topic, error = %e, alert = true, "event handler failed")
                }
            }
        }
    }
    tracing::info!(group, "consumer stopped");
}
