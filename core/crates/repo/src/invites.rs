use chrono::{DateTime, Utc};
use sqlx::MySqlConnection;

use crate::{map_unique, RepoResult};

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct InviteRow {
    pub id: i64,
    pub public_id: Option<String>,
    pub code_hash: String,
    pub email: Option<String>,
    pub invited_by: Option<i64>,
    pub tier_id: Option<i64>,
    pub tier_code: Option<String>,
    pub max_uses: i32,
    pub uses: i32,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

const SELECT: &str = "SELECT i.Id, i.PublicId, i.CodeHash, i.Email, i.InvitedBy, i.TierId, t.Code AS TierCode, i.MaxUses, i.Uses, \
    i.ExpiresAt, i.CreatedAt, i.RevokedAt FROM Invite i LEFT JOIN Tier t ON t.Id = i.TierId";

/// Locks the invite row so two signups can't both take the last use.
pub async fn by_code_hash_for_update(conn: &mut MySqlConnection, code_hash: &str) -> RepoResult<Option<InviteRow>> {
    Ok(sqlx::query_as(&format!("{SELECT} WHERE i.CodeHash = ? FOR UPDATE")).bind(code_hash).fetch_optional(conn).await?)
}

pub async fn increment_uses(conn: &mut MySqlConnection, id: i64) -> RepoResult<()> {
    sqlx::query("UPDATE Invite SET Uses = Uses + 1 WHERE Id = ?").bind(id).execute(conn).await?;
    Ok(())
}

pub struct NewInvite<'a> {
    pub public_id: &'a str,
    pub code_hash: &'a str,
    pub email: Option<&'a str>,
    pub invited_by: Option<i64>,
    pub tier_id: Option<i64>,
    pub max_uses: i32,
    pub expires_at: Option<DateTime<Utc>>,
}

pub async fn insert(conn: &mut MySqlConnection, i: &NewInvite<'_>) -> RepoResult<i64> {
    let res =
        sqlx::query("INSERT INTO Invite (PublicId, CodeHash, Email, InvitedBy, TierId, MaxUses, ExpiresAt) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(i.public_id)
            .bind(i.code_hash)
            .bind(i.email)
            .bind(i.invited_by)
            .bind(i.tier_id)
            .bind(i.max_uses)
            .bind(i.expires_at)
            .execute(conn)
            .await
            .map_err(map_unique)?;
    Ok(res.last_insert_id() as i64)
}

/// `inviter = None` lists every invite (admin).
pub async fn list(conn: &mut MySqlConnection, inviter: Option<i64>, before_id: Option<i64>, limit: u32) -> RepoResult<Vec<InviteRow>> {
    Ok(sqlx::query_as(&format!("{SELECT} WHERE (? IS NULL OR i.InvitedBy = ?) AND (? IS NULL OR i.Id < ?) ORDER BY i.Id DESC LIMIT ?"))
        .bind(inviter)
        .bind(inviter)
        .bind(before_id)
        .bind(before_id)
        .bind(limit)
        .fetch_all(conn)
        .await?)
}

/// Scoped to the inviter unless `inviter` is None (admin). PublicId comes from migration 0002.
pub async fn by_public_id(conn: &mut MySqlConnection, public_id: &str, inviter: Option<i64>) -> RepoResult<Option<InviteRow>> {
    Ok(sqlx::query_as(&format!("{SELECT} WHERE i.PublicId = ? AND (? IS NULL OR i.InvitedBy = ?)"))
        .bind(public_id)
        .bind(inviter)
        .bind(inviter)
        .fetch_optional(conn)
        .await?)
}

pub async fn revoke(conn: &mut MySqlConnection, id: i64, at: DateTime<Utc>) -> RepoResult<()> {
    sqlx::query("UPDATE Invite SET RevokedAt = COALESCE(RevokedAt, ?) WHERE Id = ?").bind(at).bind(id).execute(conn).await?;
    Ok(())
}
