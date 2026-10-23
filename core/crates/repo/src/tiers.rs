use chrono::{DateTime, Utc};
use sqlx::types::Json;
use sqlx::MySqlConnection;

use crate::{map_unique, RepoResult};

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct TierRow {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub storage_quota_bytes: i64,
    pub max_courses: Option<i32>,
    pub limits: Option<Json<serde_json::Value>>,
    pub is_default: bool,
    pub is_active: bool,
    pub sort_order: i32,
}

const SELECT: &str = "SELECT Id, Code, Name, StorageQuotaBytes, MaxCourses, Limits, IsDefault, IsActive, SortOrder FROM Tier";

/// App rule: exactly one default tier. The first one wins if the data ever disagrees.
pub async fn default_tier(conn: &mut MySqlConnection) -> RepoResult<Option<TierRow>> {
    Ok(sqlx::query_as(&format!("{SELECT} WHERE IsDefault ORDER BY Id LIMIT 1")).fetch_optional(conn).await?)
}

pub async fn by_id(conn: &mut MySqlConnection, id: i64) -> RepoResult<Option<TierRow>> {
    Ok(sqlx::query_as(&format!("{SELECT} WHERE Id = ?")).bind(id).fetch_optional(conn).await?)
}

pub async fn by_code(conn: &mut MySqlConnection, code: &str) -> RepoResult<Option<TierRow>> {
    Ok(sqlx::query_as(&format!("{SELECT} WHERE Code = ?")).bind(code).fetch_optional(conn).await?)
}

pub async fn list(conn: &mut MySqlConnection) -> RepoResult<Vec<TierRow>> {
    Ok(sqlx::query_as(&format!("{SELECT} ORDER BY SortOrder, Id")).fetch_all(conn).await?)
}

pub struct TierWrite<'a> {
    pub code: &'a str,
    pub name: &'a str,
    pub storage_quota_bytes: i64,
    pub max_courses: Option<i32>,
    pub limits: Option<&'a serde_json::Value>,
    pub is_active: bool,
    pub sort_order: i32,
}

pub async fn insert(conn: &mut MySqlConnection, t: &TierWrite<'_>) -> RepoResult<i64> {
    let res = sqlx::query(
        "INSERT INTO Tier (Code, Name, StorageQuotaBytes, MaxCourses, Limits, IsActive, SortOrder) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(t.code)
    .bind(t.name)
    .bind(t.storage_quota_bytes)
    .bind(t.max_courses)
    .bind(t.limits.map(Json))
    .bind(t.is_active)
    .bind(t.sort_order)
    .execute(conn)
    .await
    .map_err(map_unique)?;
    Ok(res.last_insert_id() as i64)
}

pub async fn update(conn: &mut MySqlConnection, id: i64, t: &TierWrite<'_>) -> RepoResult<()> {
    sqlx::query(
        "UPDATE Tier SET Code = ?, Name = ?, StorageQuotaBytes = ?, MaxCourses = ?, Limits = ?, IsActive = ?, SortOrder = ? WHERE Id = ?",
    )
    .bind(t.code)
    .bind(t.name)
    .bind(t.storage_quota_bytes)
    .bind(t.max_courses)
    .bind(t.limits.map(Json))
    .bind(t.is_active)
    .bind(t.sort_order)
    .bind(id)
    .execute(conn)
    .await
    .map_err(map_unique)?;
    Ok(())
}

/// Moves the default flag so exactly one tier has it (app rule from the schema).
pub async fn make_default(conn: &mut MySqlConnection, id: i64) -> RepoResult<()> {
    sqlx::query("UPDATE Tier SET IsDefault = (Id = ?)").bind(id).execute(conn).await?;
    Ok(())
}

pub async fn record_change(
    conn: &mut MySqlConnection,
    user_id: i64,
    from_tier_id: Option<i64>,
    to_tier_id: i64,
    changed_by: Option<i64>,
    reason: &str,
    at: DateTime<Utc>,
) -> RepoResult<()> {
    sqlx::query("INSERT INTO UserTierChange (UserId, FromTierId, ToTierId, ChangedAt, ChangedBy, Reason) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(user_id)
        .bind(from_tier_id)
        .bind(to_tier_id)
        .bind(at)
        .bind(changed_by)
        .bind(reason)
        .execute(conn)
        .await?;
    Ok(())
}
