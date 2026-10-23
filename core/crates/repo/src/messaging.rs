//! Transactional outbox and consumer dedupe (spec 3.3). Same two tables in every service DB.

use chrono::{DateTime, Utc};
use sqlx::types::Json;
use sqlx::MySqlConnection;

use crate::RepoResult;

pub struct NewOutbox<'a> {
    pub event_id: &'a str,
    pub topic: &'a str,
    pub schema_version: u16,
    pub aggregate_ref: Option<&'a str>,
    pub payload: &'a serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Must run in the same transaction as the change it announces.
pub async fn insert_outbox(conn: &mut MySqlConnection, e: &NewOutbox<'_>) -> RepoResult<()> {
    sqlx::query("INSERT INTO Outbox (EventId, Topic, SchemaVersion, AggregateRef, Payload, CreatedAt) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(e.event_id)
        .bind(e.topic)
        .bind(e.schema_version)
        .bind(e.aggregate_ref)
        .bind(Json(e.payload))
        .bind(e.created_at)
        .execute(conn)
        .await?;
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct OutboxRow {
    pub id: i64,
    pub event_id: String,
    pub topic: String,
    pub schema_version: i16,
    pub aggregate_ref: Option<String>,
    pub payload: Json<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub attempts: i32,
}

/// Next unpublished rows in `Id` order, locked so two relays never publish the same batch.
pub async fn lock_unpublished(conn: &mut MySqlConnection, limit: u32) -> RepoResult<Vec<OutboxRow>> {
    Ok(sqlx::query_as(
        "SELECT Id, EventId, Topic, SchemaVersion, AggregateRef, Payload, CreatedAt, Attempts FROM Outbox \
         WHERE PublishedAt IS NULL ORDER BY Id LIMIT ? FOR UPDATE SKIP LOCKED",
    )
    .bind(limit)
    .fetch_all(conn)
    .await?)
}

pub async fn mark_published(conn: &mut MySqlConnection, id: i64, at: DateTime<Utc>) -> RepoResult<()> {
    sqlx::query("UPDATE Outbox SET PublishedAt = ?, Attempts = Attempts + 1 WHERE Id = ?")
        .bind(at)
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn mark_attempt_failed(conn: &mut MySqlConnection, id: i64) -> RepoResult<()> {
    sqlx::query("UPDATE Outbox SET Attempts = Attempts + 1 WHERE Id = ?").bind(id).execute(conn).await?;
    Ok(())
}

pub async fn pending_count(conn: &mut MySqlConnection) -> RepoResult<i64> {
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM Outbox WHERE PublishedAt IS NULL").fetch_one(conn).await?)
}

/// Records the event as handled. `false` = already handled (duplicate key): ack and skip.
pub async fn mark_consumed(conn: &mut MySqlConnection, event_id: &str, topic: &str) -> RepoResult<bool> {
    let res = sqlx::query("INSERT IGNORE INTO ConsumedEvent (EventId, Topic) VALUES (?, ?)")
        .bind(event_id)
        .bind(topic)
        .execute(conn)
        .await?;
    Ok(res.rows_affected() == 1)
}
