//! Account export dump and the deletion bookkeeping (spec 11). The export reads each table's
//! columns from information_schema, so new columns are exported without code changes; secrets
//! (`*Hash` columns) never are.

use chrono::{DateTime, Utc};
use sqlx::types::Json;
use sqlx::MySqlConnection;

use crate::RepoResult;

/// Tables with a `UserId` column that hold no data of the user's own (sessions and tokens).
pub const SKIP_TABLES: &[&str] = &["UserSession", "AuthToken", "Outbox", "ConsumedEvent", "AccountDeletion"];

pub async fn columns(conn: &mut MySqlConnection, table: &str) -> RepoResult<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT CAST(COLUMN_NAME AS CHAR) FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION",
    )
    .bind(table)
    .fetch_all(conn)
    .await?)
}

/// Every table with a `UserId` column (the user's own rows), minus sessions and tokens.
pub async fn user_tables(conn: &mut MySqlConnection) -> RepoResult<Vec<String>> {
    let all: Vec<String> = sqlx::query_scalar(
        "SELECT CAST(TABLE_NAME AS CHAR) FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = DATABASE() AND COLUMN_NAME = 'UserId' ORDER BY TABLE_NAME",
    )
    .fetch_all(conn)
    .await?;
    Ok(all.into_iter().filter(|t| !SKIP_TABLES.contains(&t.as_str()) && !t.starts_with('_')).collect())
}

/// Rows of `table` as JSON objects. `where_sql` uses `?` for each of `binds`.
pub async fn dump(conn: &mut MySqlConnection, table: &str, where_sql: &str, binds: &[i64]) -> RepoResult<Vec<serde_json::Value>> {
    let cols: Vec<String> = columns(conn, table).await?.into_iter().filter(|c| !c.ends_with("Hash")).collect();
    if cols.is_empty() {
        return Ok(Vec::new());
    }
    let obj = cols.iter().map(|c| format!("'{c}', `{c}`")).collect::<Vec<_>>().join(", ");
    let sql = format!("SELECT JSON_OBJECT({obj}) FROM `{table}` WHERE {where_sql}");
    let mut q = sqlx::query_scalar::<_, Json<serde_json::Value>>(&sql);
    for b in binds {
        q = q.bind(*b);
    }
    Ok(q.fetch_all(conn).await?.into_iter().map(|j| j.0).collect())
}

// ---------- exports ----------

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct ExportRow {
    pub id: i64,
    pub public_id: String,
    pub user_id: i64,
    pub status: String,
    pub requested_at: DateTime<Utc>,
    pub ready_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub file_ref: Option<String>,
    pub size_bytes: Option<i64>,
    pub error: Option<String>,
}

const E_SELECT: &str =
    "SELECT Id, PublicId, UserId, CAST(Status AS CHAR) AS Status, RequestedAt, ReadyAt, ExpiresAt, FileRef, SizeBytes, Error FROM AccountExport";

pub async fn insert_export(conn: &mut MySqlConnection, public_id: &str, user_id: i64, at: DateTime<Utc>) -> RepoResult<()> {
    sqlx::query("INSERT INTO AccountExport (PublicId, UserId, Status, RequestedAt) VALUES (?, ?, 'Queued', ?)")
        .bind(public_id)
        .bind(user_id)
        .bind(at)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn exports(conn: &mut MySqlConnection, user_id: i64) -> RepoResult<Vec<ExportRow>> {
    Ok(sqlx::query_as(&format!("{E_SELECT} WHERE UserId = ? ORDER BY Id DESC LIMIT 20")).bind(user_id).fetch_all(conn).await?)
}

pub async fn open_export(conn: &mut MySqlConnection, user_id: i64) -> RepoResult<Option<ExportRow>> {
    Ok(sqlx::query_as(&format!("{E_SELECT} WHERE UserId = ? AND Status IN ('Queued','Building') LIMIT 1"))
        .bind(user_id)
        .fetch_optional(conn)
        .await?)
}

/// The next Queued export, locked (one builder at a time per export).
pub async fn claim_export(conn: &mut MySqlConnection) -> RepoResult<Option<ExportRow>> {
    let row: Option<ExportRow> = sqlx::query_as(&format!("{E_SELECT} WHERE Status = 'Queued' ORDER BY Id LIMIT 1 FOR UPDATE SKIP LOCKED"))
        .fetch_optional(&mut *conn)
        .await?;
    if let Some(r) = &row {
        sqlx::query("UPDATE AccountExport SET Status = 'Building' WHERE Id = ?").bind(r.id).execute(conn).await?;
    }
    Ok(row)
}

#[allow(clippy::too_many_arguments)]
pub async fn finish_export(
    conn: &mut MySqlConnection,
    id: i64,
    status: &str,
    file_ref: Option<&str>,
    size: Option<i64>,
    at: DateTime<Utc>,
    expires: Option<DateTime<Utc>>,
    error: Option<&str>,
) -> RepoResult<()> {
    sqlx::query("UPDATE AccountExport SET Status = ?, FileRef = ?, SizeBytes = ?, ReadyAt = ?, ExpiresAt = ?, Error = ? WHERE Id = ?")
        .bind(status)
        .bind(file_ref)
        .bind(size)
        .bind(at)
        .bind(expires)
        .bind(error)
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

// ---------- deletion ----------

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct DeletionRow {
    pub user_ref: String,
    pub user_id: i64,
    pub requested_at: DateTime<Utc>,
    pub course_decisions: Json<serde_json::Value>,
    pub status: String,
    pub acks: Option<Json<serde_json::Value>>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

const D_SELECT: &str = "SELECT UserRef, UserId, RequestedAt, CourseDecisions, CAST(Status AS CHAR) AS Status, Acks, StartedAt, FinishedAt FROM AccountDeletion";

pub async fn deletion(conn: &mut MySqlConnection, user_ref: &str) -> RepoResult<Option<DeletionRow>> {
    Ok(sqlx::query_as(&format!("{D_SELECT} WHERE UserRef = ? FOR UPDATE")).bind(user_ref).fetch_optional(conn).await?)
}

pub async fn upsert_deletion(
    conn: &mut MySqlConnection,
    user_ref: &str,
    user_id: i64,
    at: DateTime<Utc>,
    decisions: &serde_json::Value,
) -> RepoResult<()> {
    sqlx::query(
        "INSERT INTO AccountDeletion (UserRef, UserId, RequestedAt, CourseDecisions, Status) VALUES (?, ?, ?, ?, 'Pending') \
         ON DUPLICATE KEY UPDATE RequestedAt = VALUES(RequestedAt), CourseDecisions = VALUES(CourseDecisions), Status = 'Pending', Acks = NULL, \
         StartedAt = NULL, FinishedAt = NULL",
    )
    .bind(user_ref)
    .bind(user_id)
    .bind(at)
    .bind(Json(decisions))
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn set_deletion_status(conn: &mut MySqlConnection, user_ref: &str, status: &str, at: DateTime<Utc>) -> RepoResult<()> {
    let col = match status {
        "Purging" => "StartedAt",
        "Done" | "Cancelled" => "FinishedAt",
        _ => "RequestedAt",
    };
    sqlx::query(&format!("UPDATE AccountDeletion SET Status = ?, {col} = ? WHERE UserRef = ?"))
        .bind(status)
        .bind(at)
        .bind(user_ref)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn set_acks(conn: &mut MySqlConnection, user_ref: &str, acks: &serde_json::Value) -> RepoResult<()> {
    sqlx::query("UPDATE AccountDeletion SET Acks = ? WHERE UserRef = ?").bind(Json(acks)).bind(user_ref).execute(conn).await?;
    Ok(())
}

/// Purges started before `before` that some service never acked: (UserRef, UserId, Acks).
pub async fn stalled_purges(
    conn: &mut MySqlConnection,
    before: DateTime<Utc>,
) -> RepoResult<Vec<(String, i64, Option<Json<serde_json::Value>>)>> {
    Ok(sqlx::query_as("SELECT UserRef, UserId, Acks FROM AccountDeletion WHERE Status = 'Purging' AND StartedAt <= ?")
        .bind(before)
        .fetch_all(conn)
        .await?)
}

/// Pending deletions whose grace period has ended.
pub async fn due_deletions(conn: &mut MySqlConnection, before: DateTime<Utc>) -> RepoResult<Vec<String>> {
    Ok(sqlx::query_scalar("SELECT UserRef FROM AccountDeletion WHERE Status = 'Pending' AND RequestedAt <= ?")
        .bind(before)
        .fetch_all(conn)
        .await?)
}

pub async fn set_user_status(conn: &mut MySqlConnection, user_id: i64, status: &str, deletion_at: Option<DateTime<Utc>>) -> RepoResult<()> {
    sqlx::query("UPDATE Users SET Status = ?, DeletionRequestedAt = ?, RowVersion = RowVersion + 1 WHERE Id = ?")
        .bind(status)
        .bind(deletion_at)
        .bind(user_id)
        .execute(conn)
        .await?;
    Ok(())
}

/// Hands a course to a member (who stops being a member: the owner isn't listed in CourseMember).
/// A custom grade scale of the old owner is swapped for the built-in Percent scale.
pub async fn transfer_course(conn: &mut MySqlConnection, course_id: i64, from_user: i64, to_user: i64) -> RepoResult<()> {
    sqlx::query("DELETE FROM CourseMember WHERE CourseId = ? AND UserId = ?").bind(course_id).bind(to_user).execute(&mut *conn).await?;
    sqlx::query(
        "UPDATE Course c JOIN GradeScale g ON g.Id = c.GradeScaleId \
         SET c.GradeScaleId = (SELECT Id FROM (SELECT Id FROM GradeScale WHERE OwnerUserId IS NULL AND Name = 'Percent') p) \
         WHERE c.Id = ? AND g.OwnerUserId = ?",
    )
    .bind(course_id)
    .bind(from_user)
    .execute(&mut *conn)
    .await?;
    sqlx::query("UPDATE Course SET OwnerUserId = ?, RowVersion = RowVersion + 1 WHERE Id = ?")
        .bind(to_user)
        .bind(course_id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn delete_user(conn: &mut MySqlConnection, user_id: i64) -> RepoResult<()> {
    // Rows that point at the user without a cascade: the user's events and invites they created.
    sqlx::query("DELETE FROM UserEvent WHERE UserId = ?").bind(user_id).execute(&mut *conn).await?;
    sqlx::query("UPDATE UserEvent SET ActorUserId = NULL WHERE ActorUserId = ?").bind(user_id).execute(&mut *conn).await?;
    sqlx::query("DELETE FROM Users WHERE Id = ?").bind(user_id).execute(conn).await?;
    Ok(())
}
