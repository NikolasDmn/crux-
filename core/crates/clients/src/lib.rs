pub mod bus;
pub mod clock;
pub mod files;
pub mod ids;
pub mod kv;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("redis: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("{0}")]
    Other(String),
}
