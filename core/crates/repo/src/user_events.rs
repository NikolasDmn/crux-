//! `UserEvent` (spec 5.11): what the user did, from the client (`POST /events`) and the server.
//! Derived sessions and `WeekSummary` are built from it.

use chrono::{DateTime, Utc};
use sqlx::types::Json;
use sqlx::MySqlConnection;

use crate::RepoResult;

pub struct NewUserEvent<'a> {
    pub user_id: i64,
    pub actor_user_id: Option<i64>,
    pub client_id: Option<&'a str>,
    pub occurred_at: DateTime<Utc>,
    pub event_type: &'a str,
    pub course_id: Option<i64>,
    pub component_id: Option<i64>,
    pub entity_type: Option<&'a str>,
    pub entity_id: Option<i64>,
    pub payload: Option<serde_json::Value>,
}

pub async fn insert(conn: &mut MySqlConnection, e: &NewUserEvent<'_>) -> RepoResult<()> {
    sqlx::query(
        "INSERT INTO UserEvent (UserId, ActorUserId, ClientId, OccurredAt, Type, CourseId, ComponentId, EntityType, EntityId, Payload) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(e.user_id)
    .bind(e.actor_user_id)
    .bind(e.client_id)
    .bind(e.occurred_at)
    .bind(e.event_type)
    .bind(e.course_id)
    .bind(e.component_id)
    .bind(e.entity_type)
    .bind(e.entity_id)
    .bind(e.payload.clone().map(Json))
    .execute(conn)
    .await?;
    Ok(())
}

/// Dedupe on ingest (`idx_event_client`; can't be UNIQUE once the table is partitioned).
pub async fn client_id_seen(conn: &mut MySqlConnection, user_id: i64, client_id: &str) -> RepoResult<bool> {
    Ok(sqlx::query_scalar::<_, i64>("SELECT 1 FROM UserEvent WHERE UserId = ? AND ClientId = ? LIMIT 1")
        .bind(user_id)
        .bind(client_id)
        .fetch_optional(conn)
        .await?
        .is_some())
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct EventRow {
    pub occurred_at: DateTime<Utc>,
    #[sqlx(rename = "Type")]
    pub event_type: String,
    pub course_id: Option<i64>,
    pub component_id: Option<i64>,
    pub payload: Option<Json<serde_json::Value>>,
}

pub async fn between(conn: &mut MySqlConnection, user_id: i64, from: DateTime<Utc>, to: DateTime<Utc>) -> RepoResult<Vec<EventRow>> {
    Ok(sqlx::query_as(
        "SELECT OccurredAt, Type, CourseId, ComponentId, Payload FROM UserEvent WHERE UserId = ? AND OccurredAt >= ? AND OccurredAt < ? ORDER BY OccurredAt",
    )
    .bind(user_id)
    .bind(from)
    .bind(to)
    .fetch_all(conn)
    .await?)
}

/// Users with any event since `since` (nightly jobs only look at active users).
pub async fn active_users_since(conn: &mut MySqlConnection, since: DateTime<Utc>) -> RepoResult<Vec<i64>> {
    Ok(sqlx::query_scalar("SELECT DISTINCT UserId FROM UserEvent WHERE OccurredAt >= ?").bind(since).fetch_all(conn).await?)
}
