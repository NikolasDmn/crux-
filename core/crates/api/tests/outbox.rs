//! Outbox → bus → consumer with duplicate delivery (spec 3.3, 9.5, 13).

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};

use crux_clients::bus::{EventBus, MemoryBus};
use crux_jobs::consumer::{process, Handler, Outcome};
use crux_jobs::relay::publish_batch;
use crux_testkit::signup;

struct Counting(AtomicUsize);

#[async_trait::async_trait]
impl Handler for Counting {
    fn topics(&self) -> Vec<&'static str> {
        vec!["user.created"]
    }

    async fn handle(&self, _conn: &mut crux_repo::MySqlConnection, _d: &crux_clients::bus::Delivery) -> anyhow::Result<()> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn relay_publishes_in_order_once_and_consumers_dedupe() {
    let Some(h) = common::harness().await else { return };
    signup(&h.app, &h.t.ctx, "one").await;
    signup(&h.app, &h.t.ctx, "two").await;

    let bus = MemoryBus::default();
    let n = publish_batch(&h.db.pool, &bus, h.t.clock.as_ref(), "core").await.unwrap();
    assert_eq!(n, 4);
    let topics: Vec<String> = bus.published().iter().map(|e| e.topic.clone()).collect();
    assert_eq!(topics, ["user.created", "notify.requested", "user.created", "notify.requested"]);
    assert!(bus.published().iter().all(|e| e.producer == "core" && e.occurred_at.ends_with('Z')));
    assert_eq!(publish_batch(&h.db.pool, &bus, h.t.clock.as_ref(), "core").await.unwrap(), 0, "PublishedAt is set");

    // Deliver every user.created twice: the handler runs once per event.
    let handler = Counting(AtomicUsize::new(0));
    let deliveries = bus.read("files", "c1", &["user.created"], 10, 0).await.unwrap();
    for d in deliveries.iter().chain(deliveries.iter()) {
        let _ = process(&h.db.pool, &handler, d).await.unwrap();
    }
    assert_eq!(handler.0.load(Ordering::SeqCst), 2);
    assert_eq!(process(&h.db.pool, &handler, &deliveries[0]).await.unwrap(), Outcome::Duplicate);
    h.db.drop_db().await;
}
