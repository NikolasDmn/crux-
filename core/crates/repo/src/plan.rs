//! StudyPlanEntry (schema section 9). The planner writes only unpinned `Planner` entries; a user's
//! own, edited or moved entries carry `PinnedAt` and are never touched by a rebuild (spec 5.8).

use chrono::{DateTime, Utc};
use sqlx::MySqlConnection;

use crate::{RepoError, RepoResult};

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct EntryRow {
    pub id: i64,
    pub public_id: String,
    pub course_id: i64,
    pub course_ref: String,
    pub course_code: String,
    pub component_id: Option<i64>,
    pub component_ref: Option<String>,
    pub kind: String,
    pub description: String,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub status: String,
    pub moved_to_ref: Option<String>,
    pub created_by: String,
    pub pinned_at: Option<DateTime<Utc>>,
    pub row_version: i32,
}

const SELECT: &str =
    "SELECT e.Id, e.PublicId, e.CourseId, c.PublicId AS CourseRef, c.Code AS CourseCode, e.ComponentId, k.PublicId AS ComponentRef, \
    CAST(e.Kind AS CHAR) AS Kind, e.Description, e.StartAt, e.EndAt, CAST(e.Status AS CHAR) AS Status, m.PublicId AS MovedToRef, \
    CAST(e.CreatedBy AS CHAR) AS CreatedBy, e.PinnedAt, e.RowVersion \
    FROM StudyPlanEntry e JOIN Course c ON c.Id = e.CourseId LEFT JOIN CourseComponent k ON k.Id = e.ComponentId \
    LEFT JOIN StudyPlanEntry m ON m.Id = e.MovedToId";

pub async fn between(conn: &mut MySqlConnection, user_id: i64, from: DateTime<Utc>, to: DateTime<Utc>) -> RepoResult<Vec<EntryRow>> {
    Ok(sqlx::query_as(&format!("{SELECT} WHERE e.UserId = ? AND e.StartAt < ? AND e.EndAt > ? ORDER BY e.StartAt, e.Id"))
        .bind(user_id)
        .bind(to)
        .bind(from)
        .fetch_all(conn)
        .await?)
}

pub async fn by_public(conn: &mut MySqlConnection, user_id: i64, public_id: &str) -> RepoResult<Option<EntryRow>> {
    Ok(sqlx::query_as(&format!("{SELECT} WHERE e.UserId = ? AND e.PublicId = ?"))
        .bind(user_id)
        .bind(public_id)
        .fetch_optional(conn)
        .await?)
}

pub struct NewEntry<'a> {
    pub public_id: &'a str,
    pub user_id: i64,
    pub course_id: i64,
    pub component_id: Option<i64>,
    pub kind: &'a str,
    pub description: &'a str,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub status: &'a str,
    pub created_by: &'a str,
    pub pinned_at: Option<DateTime<Utc>>,
}

pub async fn insert(conn: &mut MySqlConnection, e: &NewEntry<'_>) -> RepoResult<i64> {
    let res = sqlx::query(
        "INSERT INTO StudyPlanEntry (PublicId, UserId, CourseId, ComponentId, Kind, Description, StartAt, EndAt, Status, CreatedBy, PinnedAt) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(e.public_id)
    .bind(e.user_id)
    .bind(e.course_id)
    .bind(e.component_id)
    .bind(e.kind)
    .bind(e.description)
    .bind(e.start_at)
    .bind(e.end_at)
    .bind(e.status)
    .bind(e.created_by)
    .bind(e.pinned_at)
    .execute(conn)
    .await?;
    Ok(res.last_insert_id() as i64)
}

#[allow(clippy::too_many_arguments)]
pub async fn update(
    conn: &mut MySqlConnection,
    id: i64,
    expected: i32,
    description: &str,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    status: &str,
    pinned_at: Option<DateTime<Utc>>,
    moved_to: Option<i64>,
) -> RepoResult<()> {
    let res = sqlx::query(
        "UPDATE StudyPlanEntry SET Description = ?, StartAt = ?, EndAt = ?, Status = ?, PinnedAt = COALESCE(PinnedAt, ?), \
         MovedToId = COALESCE(?, MovedToId), RowVersion = RowVersion + 1 WHERE Id = ? AND RowVersion = ?",
    )
    .bind(description)
    .bind(start_at)
    .bind(end_at)
    .bind(status)
    .bind(pinned_at)
    .bind(moved_to)
    .bind(id)
    .bind(expected)
    .execute(conn)
    .await?;
    if res.rows_affected() == 0 {
        return Err(RepoError::VersionConflict);
    }
    Ok(())
}

pub async fn delete(conn: &mut MySqlConnection, id: i64) -> RepoResult<()> {
    sqlx::query("UPDATE StudyPlanEntry SET MovedToId = NULL WHERE MovedToId = ?").bind(id).execute(&mut *conn).await?;
    sqlx::query("DELETE FROM StudyPlanEntry WHERE Id = ?").bind(id).execute(conn).await?;
    Ok(())
}

/// What a rebuild replaces: unpinned planner entries still Planned, from `from` on.
pub async fn replaceable_from(conn: &mut MySqlConnection, user_id: i64, from: DateTime<Utc>) -> RepoResult<Vec<EntryRow>> {
    Ok(sqlx::query_as(&format!(
        "{SELECT} WHERE e.UserId = ? AND e.StartAt >= ? AND e.CreatedBy = 'Planner' AND e.PinnedAt IS NULL AND e.Status = 'Planned' ORDER BY e.StartAt"
    ))
    .bind(user_id)
    .bind(from)
    .fetch_all(conn)
    .await?)
}

/// Still-planned time from `from` on that a rebuild keeps, so the planner works around it: pinned and
/// your own entries, and a planner session already under way (it started, so it isn't replaced).
pub async fn busy_from(conn: &mut MySqlConnection, user_id: i64, from: DateTime<Utc>) -> RepoResult<Vec<(DateTime<Utc>, DateTime<Utc>)>> {
    Ok(sqlx::query_as(
        "SELECT StartAt, EndAt FROM StudyPlanEntry WHERE UserId = ? AND EndAt > ? AND Status = 'Planned' AND (PinnedAt IS NOT NULL OR CreatedBy <> 'Planner' OR StartAt < ?)",
    )
    .bind(user_id)
    .bind(from)
    .bind(from)
    .fetch_all(conn)
    .await?)
}

/// Spec 5.8 "the user falls a day behind": planner entries that ended before `before` and are still
/// Planned become Skipped (so the next rebuild re-plans the work, and this fires once).
pub async fn skip_missed(conn: &mut MySqlConnection, user_id: i64, before: DateTime<Utc>) -> RepoResult<u64> {
    let res = sqlx::query(
        "UPDATE StudyPlanEntry SET Status = 'Skipped', RowVersion = RowVersion + 1 WHERE UserId = ? AND Status = 'Planned' AND EndAt < ?",
    )
    .bind(user_id)
    .bind(before)
    .execute(conn)
    .await?;
    Ok(res.rows_affected())
}

/// Users with missed entries (any time zone: the caller checks each user's local midnight).
pub async fn users_with_planned_before(conn: &mut MySqlConnection, before: DateTime<Utc>) -> RepoResult<Vec<i64>> {
    Ok(sqlx::query_scalar("SELECT DISTINCT UserId FROM StudyPlanEntry WHERE Status = 'Planned' AND EndAt < ?")
        .bind(before)
        .fetch_all(conn)
        .await?)
}

/// Marks the user's planned entries for a lecture as Done (the lecture was finished).
pub async fn done_for_component(conn: &mut MySqlConnection, user_id: i64, component_id: i64, until: DateTime<Utc>) -> RepoResult<u64> {
    let res = sqlx::query(
        "UPDATE StudyPlanEntry SET Status = 'Done', RowVersion = RowVersion + 1 WHERE UserId = ? AND ComponentId = ? AND Status = 'Planned' AND StartAt <= ?",
    )
    .bind(user_id)
    .bind(component_id)
    .bind(until)
    .execute(conn)
    .await?;
    Ok(res.rows_affected())
}
