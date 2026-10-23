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
    pub entity_type: Option<&'a str>,
    pub entity_id: Option<i64>,
    pub payload: Option<serde_json::Value>,
}

pub async fn insert(conn: &mut MySqlConnection, e: &NewUserEvent<'_>) -> RepoResult<()> {
    sqlx::query(
        "INSERT INTO UserEvent (UserId, ActorUserId, ClientId, OccurredAt, Type, EntityType, EntityId, Payload) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(e.user_id)
    .bind(e.actor_user_id)
    .bind(e.client_id)
    .bind(e.occurred_at)
    .bind(e.event_type)
    .bind(e.entity_type)
    .bind(e.entity_id)
    .bind(e.payload.clone().map(Json))
    .execute(conn)
    .await?;
    Ok(())
}
