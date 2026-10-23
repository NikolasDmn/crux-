//! Small key-value needs on Redis: rate-limit counters, idempotency records, SSE tickets.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;

use crate::ClientError;

#[async_trait]
pub trait KeyValue: Send + Sync {
    /// Increments a fixed-window counter; the window starts at the first hit. Returns the new count.
    async fn incr_window(&self, key: &str, window: Duration) -> Result<u64, ClientError>;
    /// SET key value NX EX ttl. `true` when this call created the key.
    async fn set_nx(&self, key: &str, value: &str, ttl: Duration) -> Result<bool, ClientError>;
    async fn set(&self, key: &str, value: &str, ttl: Duration) -> Result<(), ClientError>;
    async fn get(&self, key: &str) -> Result<Option<String>, ClientError>;
    /// GETDEL: read once. Single-use tickets rely on this being atomic.
    async fn take(&self, key: &str) -> Result<Option<String>, ClientError>;
    async fn del(&self, key: &str) -> Result<(), ClientError>;
    async fn ping(&self) -> Result<(), ClientError>;
}

#[derive(Clone)]
pub struct RedisKv {
    conn: ConnectionManager,
}

impl RedisKv {
    pub async fn connect(url: &str) -> Result<Self, ClientError> {
        let client = redis::Client::open(url)?;
        Ok(Self { conn: ConnectionManager::new(client).await? })
    }

    pub fn connection(&self) -> ConnectionManager {
        self.conn.clone()
    }
}

#[async_trait]
impl KeyValue for RedisKv {
    async fn incr_window(&self, key: &str, window: Duration) -> Result<u64, ClientError> {
        let mut c = self.conn.clone();
        let (count,): (u64,) = redis::pipe()
            .atomic()
            .incr(key, 1u64)
            .cmd("EXPIRE")
            .arg(key)
            .arg(window.as_secs().max(1))
            .arg("NX")
            .ignore()
            .query_async(&mut c)
            .await?;
        Ok(count)
    }

    async fn set_nx(&self, key: &str, value: &str, ttl: Duration) -> Result<bool, ClientError> {
        let mut c = self.conn.clone();
        let res: Option<String> =
            redis::cmd("SET").arg(key).arg(value).arg("NX").arg("PX").arg(ttl.as_millis() as u64).query_async(&mut c).await?;
        Ok(res.is_some())
    }

    async fn set(&self, key: &str, value: &str, ttl: Duration) -> Result<(), ClientError> {
        let mut c = self.conn.clone();
        let _: () = c.pset_ex(key, value, ttl.as_millis() as u64).await?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<String>, ClientError> {
        let mut c = self.conn.clone();
        Ok(c.get(key).await?)
    }

    async fn take(&self, key: &str) -> Result<Option<String>, ClientError> {
        let mut c = self.conn.clone();
        Ok(redis::cmd("GETDEL").arg(key).query_async(&mut c).await?)
    }

    async fn del(&self, key: &str) -> Result<(), ClientError> {
        let mut c = self.conn.clone();
        let _: () = c.del(key).await?;
        Ok(())
    }

    async fn ping(&self) -> Result<(), ClientError> {
        let mut c = self.conn.clone();
        let _: String = redis::cmd("PING").query_async(&mut c).await?;
        Ok(())
    }
}

/// In-memory double for unit tests.
#[derive(Default)]
pub struct MemoryKv {
    map: Mutex<HashMap<String, (String, Instant)>>,
}

impl MemoryKv {
    fn live(map: &mut HashMap<String, (String, Instant)>, key: &str) -> Option<String> {
        match map.get(key) {
            Some((v, until)) if *until > Instant::now() => Some(v.clone()),
            Some(_) => {
                map.remove(key);
                None
            }
            None => None,
        }
    }
}

#[async_trait]
impl KeyValue for MemoryKv {
    async fn incr_window(&self, key: &str, window: Duration) -> Result<u64, ClientError> {
        let mut m = self.map.lock().unwrap();
        let next = match Self::live(&mut m, key) {
            Some(v) => v.parse::<u64>().unwrap_or(0) + 1,
            None => 1,
        };
        let until = m.get(key).filter(|_| next > 1).map(|(_, u)| *u).unwrap_or_else(|| Instant::now() + window);
        m.insert(key.to_string(), (next.to_string(), until));
        Ok(next)
    }

    async fn set_nx(&self, key: &str, value: &str, ttl: Duration) -> Result<bool, ClientError> {
        let mut m = self.map.lock().unwrap();
        if Self::live(&mut m, key).is_some() {
            return Ok(false);
        }
        m.insert(key.to_string(), (value.to_string(), Instant::now() + ttl));
        Ok(true)
    }

    async fn set(&self, key: &str, value: &str, ttl: Duration) -> Result<(), ClientError> {
        self.map.lock().unwrap().insert(key.to_string(), (value.to_string(), Instant::now() + ttl));
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<String>, ClientError> {
        Ok(Self::live(&mut self.map.lock().unwrap(), key))
    }

    async fn take(&self, key: &str) -> Result<Option<String>, ClientError> {
        let mut m = self.map.lock().unwrap();
        let v = Self::live(&mut m, key);
        m.remove(key);
        Ok(v)
    }

    async fn del(&self, key: &str) -> Result<(), ClientError> {
        self.map.lock().unwrap().remove(key);
        Ok(())
    }

    async fn ping(&self) -> Result<(), ClientError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_kv_counts_and_takes_once() {
        let kv = MemoryKv::default();
        assert_eq!(kv.incr_window("k", Duration::from_secs(60)).await.unwrap(), 1);
        assert_eq!(kv.incr_window("k", Duration::from_secs(60)).await.unwrap(), 2);
        assert!(kv.set_nx("t", "v", Duration::from_secs(60)).await.unwrap());
        assert!(!kv.set_nx("t", "w", Duration::from_secs(60)).await.unwrap());
        assert_eq!(kv.take("t").await.unwrap().as_deref(), Some("v"));
        assert_eq!(kv.take("t").await.unwrap(), None);
    }

    #[tokio::test]
    async fn memory_kv_expires() {
        let kv = MemoryKv::default();
        kv.set("e", "v", Duration::from_millis(1)).await.unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert_eq!(kv.get("e").await.unwrap(), None);
    }
}
