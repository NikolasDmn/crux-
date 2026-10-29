//! Audit L4: malformed and repeatedly failing events go to a dead-letter stream. Needs a Redis:
//! CRUX_TEST_REDIS_URL=redis://:crux-redis-dev@127.0.0.1:6379/15 (skipped without it).

use crux_clients::bus::{Envelope, EventBus, RedisBus, MAX_DELIVERIES};

#[tokio::test]
async fn dead_letters() {
    let Ok(url) = std::env::var("CRUX_TEST_REDIS_URL") else {
        eprintln!("CRUX_TEST_REDIS_URL not set: skipped");
        return;
    };
    let client = redis::Client::open(url).unwrap();
    let conn = redis::aio::ConnectionManager::new(client).await.unwrap();
    let mut c = conn.clone();
    let topic = format!("test.dead.{}", ulid::Ulid::new());
    let (key, dead) = (format!("crux:events:{topic}"), format!("crux:events:dead:{topic}"));
    let bus = RedisBus::new(conn);
    bus.ensure_group("g", &[&topic]).await.unwrap();
    let env = Envelope {
        event_id: "01EVT".into(),
        topic: topic.clone(),
        schema_version: 1,
        occurred_at: "2026-10-29T00:00:00.000Z".into(),
        producer: "test".into(),
        aggregate_ref: None,
        payload: serde_json::json!({}),
    };
    bus.publish(&env).await.unwrap();
    let _: String = redis::cmd("XADD").arg(&key).arg("*").arg("envelope").arg("not json").query_async(&mut c).await.unwrap();

    let got = bus.read("g", "c", &[&topic], 10, 10).await.unwrap();
    assert_eq!(got.len(), 1, "the good one");
    let n: i64 = redis::cmd("XLEN").arg(&dead).query_async(&mut c).await.unwrap();
    assert_eq!(n, 1, "the malformed one is dead-lettered at once");

    for _ in 0..MAX_DELIVERIES - 1 {
        assert_eq!(bus.read_pending("g", "c", &[&topic], 10).await.unwrap().len(), 1);
    }
    assert!(bus.read_pending("g", "c", &[&topic], 10).await.unwrap().is_empty(), "buried after the last try");
    let n: i64 = redis::cmd("XLEN").arg(&dead).query_async(&mut c).await.unwrap();
    assert_eq!(n, 2);
    let _: i64 = redis::cmd("DEL").arg(&key).arg(&dead).query_async(&mut c).await.unwrap();
}
