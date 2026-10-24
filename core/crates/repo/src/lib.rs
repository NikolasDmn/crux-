//! Repository layer for `crux_core`. Functions take a `&mut MySqlConnection` so the service layer
//! decides the transaction (one per use case, outbox rows included; spec 2.1).
//!
//! Every query on user data is scoped by the caller's `UserId` here (schema header).

pub mod auth_tokens;
pub mod bank;
pub mod catalog;
pub mod courses;
pub mod db;
pub mod invites;
pub mod messaging;
pub mod mistakes;
pub mod mocks;
pub mod sessions;
pub mod settings;
pub mod tiers;
pub mod user_events;
pub mod users;

pub use sqlx::{MySqlConnection, MySqlPool};

/// The service layer opens transactions and needs the error type; it never builds queries.
pub mod sqlx_reexport {
    pub use sqlx::types::Json;
    pub use sqlx::{Error, MySql, Transaction};
}

pub type Tx = sqlx::Transaction<'static, sqlx::MySql>;

pub async fn begin(pool: &MySqlPool) -> Result<Tx, sqlx::Error> {
    pool.begin().await
}

#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("row changed since it was read")]
    VersionConflict,
    #[error("duplicate value for {0}")]
    Duplicate(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

pub type RepoResult<T> = Result<T, RepoError>;

/// Maps a MySQL duplicate-key error (1062) to `Duplicate(<key name>)`.
pub(crate) fn map_unique(err: sqlx::Error) -> RepoError {
    if let sqlx::Error::Database(db) = &err {
        if db.code().as_deref() == Some("23000") && db.message().contains("Duplicate entry") {
            let key = db.message().rsplit("for key ").next().unwrap_or("").trim_matches(|c| c == '\'' || c == '`');
            let key = key.rsplit('.').next().unwrap_or(key);
            return RepoError::Duplicate(key.to_string());
        }
    }
    RepoError::Db(err)
}
