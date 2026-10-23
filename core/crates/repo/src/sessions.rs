use chrono::{DateTime, Utc};
use sqlx::MySqlConnection;

use crate::RepoResult;

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct SessionRow {
    pub id: i64,
    pub user_id: i64,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

pub async fn insert(
    conn: &mut MySqlConnection,
    user_id: i64,
    token_hash: &str,
    now: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    user_agent: Option<&str>,
) -> RepoResult<i64> {
    let ua = user_agent.map(|s| s.chars().take(255).collect::<String>());
    let res =
        sqlx::query("INSERT INTO UserSession (UserId, TokenHash, CreatedAt, LastSeenAt, ExpiresAt, UserAgent) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(user_id)
            .bind(token_hash)
            .bind(now)
            .bind(now)
            .bind(expires_at)
            .bind(ua)
            .execute(conn)
            .await?;
    Ok(res.last_insert_id() as i64)
}

pub async fn by_token_hash_for_update(conn: &mut MySqlConnection, token_hash: &str) -> RepoResult<Option<SessionRow>> {
    Ok(sqlx::query_as("SELECT Id, UserId, ExpiresAt, RevokedAt FROM UserSession WHERE TokenHash = ? FOR UPDATE")
        .bind(token_hash)
        .fetch_optional(conn)
        .await?)
}

pub async fn revoke(conn: &mut MySqlConnection, id: i64, at: DateTime<Utc>) -> RepoResult<()> {
    sqlx::query("UPDATE UserSession SET RevokedAt = COALESCE(RevokedAt, ?) WHERE Id = ?").bind(at).bind(id).execute(conn).await?;
    Ok(())
}

pub async fn revoke_all_for_user(conn: &mut MySqlConnection, user_id: i64, at: DateTime<Utc>) -> RepoResult<u64> {
    let res = sqlx::query("UPDATE UserSession SET RevokedAt = ? WHERE UserId = ? AND RevokedAt IS NULL")
        .bind(at)
        .bind(user_id)
        .execute(conn)
        .await?;
    Ok(res.rows_affected())
}

/// purge.tokens job: sessions past expiry, or revoked more than `keep_revoked_until` ago (kept a
/// while so reuse of a rotated token is still detected).
pub async fn purge(conn: &mut MySqlConnection, now: DateTime<Utc>, revoked_before: DateTime<Utc>) -> RepoResult<u64> {
    let res =
        sqlx::query("DELETE FROM UserSession WHERE ExpiresAt <= ? OR RevokedAt <= ?").bind(now).bind(revoked_before).execute(conn).await?;
    Ok(res.rows_affected())
}
