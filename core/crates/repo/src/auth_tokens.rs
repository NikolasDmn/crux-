use chrono::{DateTime, Utc};
use sqlx::MySqlConnection;

use crate::RepoResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    VerifyEmail,
    ResetPassword,
}

impl Purpose {
    fn as_db(self) -> &'static str {
        match self {
            Purpose::VerifyEmail => "VerifyEmail",
            Purpose::ResetPassword => "ResetPassword",
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct AuthTokenRow {
    pub id: i64,
    pub user_id: i64,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
}

pub async fn insert(
    conn: &mut MySqlConnection,
    user_id: i64,
    purpose: Purpose,
    token_hash: &str,
    now: DateTime<Utc>,
    expires_at: DateTime<Utc>,
) -> RepoResult<()> {
    sqlx::query("INSERT INTO AuthToken (UserId, Purpose, TokenHash, CreatedAt, ExpiresAt) VALUES (?, ?, ?, ?, ?)")
        .bind(user_id)
        .bind(purpose.as_db())
        .bind(token_hash)
        .bind(now)
        .bind(expires_at)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn by_hash_for_update(conn: &mut MySqlConnection, purpose: Purpose, token_hash: &str) -> RepoResult<Option<AuthTokenRow>> {
    Ok(sqlx::query_as("SELECT Id, UserId, ExpiresAt, UsedAt FROM AuthToken WHERE TokenHash = ? AND Purpose = ? FOR UPDATE")
        .bind(token_hash)
        .bind(purpose.as_db())
        .fetch_optional(conn)
        .await?)
}

pub async fn mark_used(conn: &mut MySqlConnection, id: i64, at: DateTime<Utc>) -> RepoResult<()> {
    sqlx::query("UPDATE AuthToken SET UsedAt = ? WHERE Id = ?").bind(at).bind(id).execute(conn).await?;
    Ok(())
}

/// A new token of a purpose voids the older unused ones (only the latest link works).
pub async fn void_unused(conn: &mut MySqlConnection, user_id: i64, purpose: Purpose, at: DateTime<Utc>) -> RepoResult<()> {
    sqlx::query("UPDATE AuthToken SET UsedAt = ? WHERE UserId = ? AND Purpose = ? AND UsedAt IS NULL")
        .bind(at)
        .bind(user_id)
        .bind(purpose.as_db())
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn purge(conn: &mut MySqlConnection, now: DateTime<Utc>) -> RepoResult<u64> {
    let res = sqlx::query("DELETE FROM AuthToken WHERE ExpiresAt <= ? OR UsedAt IS NOT NULL").bind(now).execute(conn).await?;
    Ok(res.rows_affected())
}
