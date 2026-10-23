//! Redis Streams transport (spec 3.3): one stream per topic, one consumer group per consuming
//! service. Publishing happens only through the outbox relay; consumers dedupe via ConsumedEvent.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use redis::aio::ConnectionManager;
use redis::streams::{StreamReadOptions, StreamReadReply};
use serde::{Deserialize, Serialize};

use crate::ClientError;

pub fn stream_key(topic: &str) -> String {
    format!("crux:events:{topic}")
}

/// The envelope every event travels in (spec 3.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    pub event_id: String,
    pub topic: String,
    pub schema_version: u16,
    pub occurred_at: String,
    pub producer: String,
    pub aggregate_ref: Option<String>,
    pub payload: serde_json::Value,
}

/// A message read from a stream, with the id needed to ack it.
#[derive(Debug, Clone)]
pub struct Delivery {
    pub stream_id: String,
    pub topic: String,
    pub envelope: Envelope,
}

#[async_trait]
pub trait EventBus: Send + Sync {
    async fn publish(&self, envelope: &Envelope) -> Result<(), ClientError>;
    /// Reads up to `count` new messages for `group` across `topics`, blocking up to `block_ms`.
    async fn read(&self, group: &str, consumer: &str, topics: &[&str], count: usize, block_ms: usize) -> Result<Vec<Delivery>, ClientError>;
    /// This consumer's delivered-but-unacked messages (failed handlers), oldest first.
    async fn read_pending(&self, group: &str, consumer: &str, topics: &[&str], count: usize) -> Result<Vec<Delivery>, ClientError>;
    async fn ack(&self, group: &str, delivery: &Delivery) -> Result<(), ClientError>;
}

#[derive(Clone)]
pub struct RedisBus {
    conn: ConnectionManager,
    /// Stream length cap (approximate trimming); consumers that fall further behind than this lose
    /// messages, which the outbox can replay.
    max_len: usize,
}

impl RedisBus {
    pub fn new(conn: ConnectionManager) -> Self {
        Self { conn, max_len: 100_000 }
    }

    /// Creates the consumer group (and the stream) if missing. Safe to call on every start.
    pub async fn ensure_group(&self, group: &str, topics: &[&str]) -> Result<(), ClientError> {
        let mut c = self.conn.clone();
        for topic in topics {
            let res: Result<(), redis::RedisError> =
                redis::cmd("XGROUP").arg("CREATE").arg(stream_key(topic)).arg(group).arg("0").arg("MKSTREAM").query_async(&mut c).await;
            if let Err(e) = res {
                if e.code() != Some("BUSYGROUP") {
                    return Err(e.into());
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl EventBus for RedisBus {
    async fn publish(&self, envelope: &Envelope) -> Result<(), ClientError> {
        let mut c = self.conn.clone();
        let body = serde_json::to_string(envelope).map_err(|e| ClientError::Other(e.to_string()))?;
        let _: String = redis::cmd("XADD")
            .arg(stream_key(&envelope.topic))
            .arg("MAXLEN")
            .arg("~")
            .arg(self.max_len)
            .arg("*")
            .arg("envelope")
            .arg(body)
            .query_async(&mut c)
            .await?;
        Ok(())
    }

    async fn read(&self, group: &str, consumer: &str, topics: &[&str], count: usize, block_ms: usize) -> Result<Vec<Delivery>, ClientError> {
        self.read_from(group, consumer, topics, count, Some(block_ms), ">").await
    }

    async fn read_pending(&self, group: &str, consumer: &str, topics: &[&str], count: usize) -> Result<Vec<Delivery>, ClientError> {
        self.read_from(group, consumer, topics, count, None, "0").await
    }

    async fn ack(&self, group: &str, delivery: &Delivery) -> Result<(), ClientError> {
        let mut c = self.conn.clone();
        let _: i64 = redis::AsyncCommands::xack(&mut c, stream_key(&delivery.topic), group, &[&delivery.stream_id]).await?;
        Ok(())
    }
}

impl RedisBus {
    /// `from` = ">" for new messages, "0" for this consumer's pending ones.
    async fn read_from(&self, group: &str, consumer: &str, topics: &[&str], count: usize, block_ms: Option<usize>, from: &str) -> Result<Vec<Delivery>, ClientError> {
        let mut c = self.conn.clone();
        let keys: Vec<String> = topics.iter().map(|t| stream_key(t)).collect();
        let ids: Vec<&str> = keys.iter().map(|_| from).collect();
        let mut opts = StreamReadOptions::default().group(group, consumer).count(count);
        if let Some(ms) = block_ms {
            opts = opts.block(ms);
        }
        let reply: Option<StreamReadReply> = redis::AsyncCommands::xread_options(&mut c, &keys, &ids, &opts).await?;
        let mut out = Vec::new();
        for key in reply.map(|r| r.keys).unwrap_or_default() {
            let topic = key.key.trim_start_matches("crux:events:").to_string();
            for entry in key.ids {
                let raw: Option<String> = entry.get("envelope");
                match raw.as_deref().map(serde_json::from_str::<Envelope>) {
                    Some(Ok(envelope)) => out.push(Delivery { stream_id: entry.id, topic: topic.clone(), envelope }),
                    _ => tracing::warn!(stream = %key.key, id = %entry.id, "dropping malformed event"),
                }
            }
        }
        Ok(out)
    }
}

/// In-memory bus for tests: records what was published, and can deliver it back.
#[derive(Default)]
pub struct MemoryBus {
    published: Mutex<Vec<Envelope>>,
    acked: Mutex<HashMap<String, Vec<String>>>,
}

impl MemoryBus {
    pub fn published(&self) -> Vec<Envelope> {
        self.published.lock().unwrap().clone()
    }

    pub fn acked(&self, group: &str) -> Vec<String> {
        self.acked.lock().unwrap().get(group).cloned().unwrap_or_default()
    }
}

#[async_trait]
impl EventBus for MemoryBus {
    async fn publish(&self, envelope: &Envelope) -> Result<(), ClientError> {
        self.published.lock().unwrap().push(envelope.clone());
        Ok(())
    }

    async fn read(&self, _group: &str, _consumer: &str, topics: &[&str], count: usize, _block_ms: usize) -> Result<Vec<Delivery>, ClientError> {
        let all = self.published.lock().unwrap();
        Ok(all
            .iter()
            .enumerate()
            .filter(|(_, e)| topics.contains(&e.topic.as_str()))
            .take(count)
            .map(|(i, e)| Delivery { stream_id: format!("{i}-0"), topic: e.topic.clone(), envelope: e.clone() })
            .collect())
    }

    async fn read_pending(&self, _group: &str, _consumer: &str, _topics: &[&str], _count: usize) -> Result<Vec<Delivery>, ClientError> {
        Ok(Vec::new())
    }

    async fn ack(&self, group: &str, delivery: &Delivery) -> Result<(), ClientError> {
        self.acked.lock().unwrap().entry(group.to_string()).or_default().push(delivery.stream_id.clone());
        Ok(())
    }
}
