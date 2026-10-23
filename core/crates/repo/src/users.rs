use chrono::{DateTime, Utc};
use sqlx::MySqlConnection;

use crate::{map_unique, RepoError, RepoResult};

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct UserRow {
    pub id: i64,
    pub public_id: String,
    pub username: String,
    pub email: String,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub password_hash: Option<String>,
    pub first_name: String,
    pub last_name: String,
    pub time_zone: String,
    pub locale: String,
    pub role: String,
    pub tier_id: i64,
    pub tier_code: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub row_version: i32,
}

const SELECT: &str = "SELECT u.Id, u.PublicId, u.Username, u.Email, u.EmailVerifiedAt, u.PasswordHash, u.FirstName, \
    u.LastName, u.TimeZone, u.Locale, CAST(u.Role AS CHAR) AS Role, u.TierId, t.Code AS TierCode, \
    CAST(u.Status AS CHAR) AS Status, u.CreatedAt, u.RowVersion \
    FROM Users u JOIN Tier t ON t.Id = u.TierId";

pub struct NewUser<'a> {
    pub public_id: &'a str,
    pub username: &'a str,
    pub email: &'a str,
    pub password_hash: &'a str,
    pub first_name: &'a str,
    pub last_name: &'a str,
    pub time_zone: &'a str,
    pub tier_id: i64,
}

pub async fn insert(conn: &mut MySqlConnection, u: &NewUser<'_>) -> RepoResult<i64> {
    let res = sqlx::query(
        "INSERT INTO Users (PublicId, Username, Email, PasswordHash, FirstName, LastName, TimeZone, TierId) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(u.public_id)
    .bind(u.username)
    .bind(u.email)
    .bind(u.password_hash)
    .bind(u.first_name)
    .bind(u.last_name)
    .bind(u.time_zone)
    .bind(u.tier_id)
    .execute(conn)
    .await
    .map_err(map_unique)?;
    Ok(res.last_insert_id() as i64)
}

pub async fn by_id(conn: &mut MySqlConnection, id: i64) -> RepoResult<Option<UserRow>> {
    Ok(sqlx::query_as(&format!("{SELECT} WHERE u.Id = ?")).bind(id).fetch_optional(conn).await?)
}

pub async fn by_public_id(conn: &mut MySqlConnection, public_id: &str) -> RepoResult<Option<UserRow>> {
    Ok(sqlx::query_as(&format!("{SELECT} WHERE u.PublicId = ?")).bind(public_id).fetch_optional(conn).await?)
}

/// Login accepts email or username; the collation makes both case-insensitive.
pub async fn by_login(conn: &mut MySqlConnection, login: &str) -> RepoResult<Option<UserRow>> {
    Ok(sqlx::query_as(&format!("{SELECT} WHERE u.Email = ? OR u.Username = ? LIMIT 1"))
        .bind(login)
        .bind(login)
        .fetch_optional(conn)
        .await?)
}

pub async fn by_email(conn: &mut MySqlConnection, email: &str) -> RepoResult<Option<UserRow>> {
    Ok(sqlx::query_as(&format!("{SELECT} WHERE u.Email = ?")).bind(email).fetch_optional(conn).await?)
}

pub struct ProfileUpdate<'a> {
    pub username: &'a str,
    pub first_name: &'a str,
    pub last_name: &'a str,
    pub time_zone: &'a str,
}

/// Optimistic concurrency (spec 3.1, review H6): 0 rows = someone else changed it first.
pub async fn update_profile(conn: &mut MySqlConnection, id: i64, expected_version: i32, p: &ProfileUpdate<'_>) -> RepoResult<()> {
    let res = sqlx::query(
        "UPDATE Users SET Username = ?, FirstName = ?, LastName = ?, TimeZone = ?, RowVersion = RowVersion + 1 \
         WHERE Id = ? AND RowVersion = ?",
    )
    .bind(p.username)
    .bind(p.first_name)
    .bind(p.last_name)
    .bind(p.time_zone)
    .bind(id)
    .bind(expected_version)
    .execute(conn)
    .await
    .map_err(map_unique)?;
    if res.rows_affected() == 0 {
        return Err(RepoError::VersionConflict);
    }
    Ok(())
}

pub async fn set_email_verified(conn: &mut MySqlConnection, id: i64, at: DateTime<Utc>) -> RepoResult<()> {
    sqlx::query("UPDATE Users SET EmailVerifiedAt = ?, RowVersion = RowVersion + 1 WHERE Id = ? AND EmailVerifiedAt IS NULL")
        .bind(at)
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn set_password_hash(conn: &mut MySqlConnection, id: i64, hash: &str) -> RepoResult<()> {
    sqlx::query("UPDATE Users SET PasswordHash = ?, RowVersion = RowVersion + 1 WHERE Id = ?")
        .bind(hash)
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn set_tier(conn: &mut MySqlConnection, id: i64, tier_id: i64) -> RepoResult<()> {
    sqlx::query("UPDATE Users SET TierId = ?, RowVersion = RowVersion + 1 WHERE Id = ?")
        .bind(tier_id)
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn set_role_status(conn: &mut MySqlConnection, id: i64, role: Option<&str>, status: Option<&str>) -> RepoResult<()> {
    sqlx::query("UPDATE Users SET Role = COALESCE(?, Role), Status = COALESCE(?, Status), RowVersion = RowVersion + 1 WHERE Id = ?")
        .bind(role)
        .bind(status)
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

/// CLI bootstrap only (`crux-core make-admin`). Returns rows changed.
pub async fn make_admin_by_email(conn: &mut MySqlConnection, email: &str) -> RepoResult<u64> {
    let res = sqlx::query("UPDATE Users SET Role = 'Admin', RowVersion = RowVersion + 1 WHERE Email = ?").bind(email).execute(conn).await?;
    Ok(res.rows_affected())
}

pub async fn public_ids_on_tier(conn: &mut MySqlConnection, tier_id: i64) -> RepoResult<Vec<String>> {
    Ok(sqlx::query_scalar("SELECT PublicId FROM Users WHERE TierId = ?").bind(tier_id).fetch_all(conn).await?)
}

/// Admin list, newest first, keyset on Id.
pub async fn list(conn: &mut MySqlConnection, before_id: Option<i64>, limit: u32, query: Option<&str>) -> RepoResult<Vec<UserRow>> {
    let like = query.map(|q| format!("%{}%", q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")));
    Ok(sqlx::query_as(&format!(
        "{SELECT} WHERE (? IS NULL OR u.Id < ?) AND (? IS NULL OR u.Email LIKE ? OR u.Username LIKE ?) \
         ORDER BY u.Id DESC LIMIT ?"
    ))
    .bind(before_id)
    .bind(before_id)
    .bind(&like)
    .bind(&like)
    .bind(&like)
    .bind(limit)
    .fetch_all(conn)
    .await?)
}
