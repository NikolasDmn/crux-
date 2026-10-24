//! Course, CourseMember, CourseShareInvite, CourseSubscription, GradeScale.
//! Access is resolved here in SQL (spec 4.2: lists apply the predicate in SQL): owner, or an
//! **Accepted** member (D2-2: pending shares grant nothing).

use chrono::{DateTime, NaiveDate, Utc};
use sqlx::types::Json;
use sqlx::MySqlConnection;

use crate::{map_unique, RepoError, RepoResult};

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct CourseRow {
    pub id: i64,
    pub public_id: String,
    pub owner_user_id: i64,
    pub owner_ref: String,
    pub owner_username: String,
    pub name: String,
    pub code: String,
    pub institution: Option<String>,
    pub term: Option<String>,
    pub color: String,
    pub grade_scale_ref: Option<String>,
    pub grade_scale_name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
    pub row_version: i32,
    /// The caller's role: Owner, Editor, Viewer.
    pub my_role: String,
    /// The caller's subscription status, if subscribed.
    pub my_subscription: Option<String>,
}

/// `?` placeholders: actor id ×4 (role, owner check, member join, subscription join).
const SELECT: &str = "SELECT c.Id, c.PublicId, c.OwnerUserId, o.PublicId AS OwnerRef, o.Username AS OwnerUsername, c.Name, c.Code, \
    c.Institution, c.Term, c.Color, g.PublicId AS GradeScaleRef, g.Name AS GradeScaleName, c.CreatedAt, c.UpdatedAt, c.ArchivedAt, \
    c.RowVersion, IF(c.OwnerUserId = ?, 'Owner', CAST(m.Role AS CHAR)) AS MyRole, CAST(s.Status AS CHAR) AS MySubscription \
    FROM Course c \
    JOIN Users o ON o.Id = c.OwnerUserId \
    JOIN GradeScale g ON g.Id = c.GradeScaleId \
    LEFT JOIN CourseMember m ON m.CourseId = c.Id AND m.UserId = ? AND m.Status = 'Accepted' \
    LEFT JOIN CourseSubscription s ON s.CourseId = c.Id AND s.UserId = ? \
    WHERE (c.OwnerUserId = ? OR m.UserId IS NOT NULL)";

/// The course as the actor sees it, or None if they have no access (→ 404). Trashed courses are
/// visible only with `include_trashed` (owner restore / trash list).
pub async fn for_actor(conn: &mut MySqlConnection, actor: i64, course_ref: &str, include_trashed: bool) -> RepoResult<Option<CourseRow>> {
    Ok(sqlx::query_as(&format!("{SELECT} AND c.PublicId = ? AND (? OR c.ArchivedAt IS NULL)"))
        .bind(actor)
        .bind(actor)
        .bind(actor)
        .bind(actor)
        .bind(course_ref)
        .bind(include_trashed)
        .fetch_optional(conn)
        .await?)
}

/// Same, locking the course row (edits, code assignment).
pub async fn for_actor_for_update(conn: &mut MySqlConnection, actor: i64, course_ref: &str) -> RepoResult<Option<CourseRow>> {
    Ok(sqlx::query_as(&format!("{SELECT} AND c.PublicId = ? AND c.ArchivedAt IS NULL FOR UPDATE OF c"))
        .bind(actor)
        .bind(actor)
        .bind(actor)
        .bind(actor)
        .bind(course_ref)
        .fetch_optional(conn)
        .await?)
}

pub async fn by_internal_id(conn: &mut MySqlConnection, actor: i64, course_id: i64) -> RepoResult<Option<CourseRow>> {
    Ok(sqlx::query_as(&format!("{SELECT} AND c.Id = ?"))
        .bind(actor)
        .bind(actor)
        .bind(actor)
        .bind(actor)
        .bind(course_id)
        .fetch_optional(conn)
        .await?)
}

/// Courses I own or am an accepted member of. `trashed` lists my own trashed courses instead.
pub async fn list_for_actor(conn: &mut MySqlConnection, actor: i64, trashed: bool) -> RepoResult<Vec<CourseRow>> {
    let filter = if trashed { "AND c.ArchivedAt IS NOT NULL AND c.OwnerUserId = ?" } else { "AND c.ArchivedAt IS NULL AND ? IS NOT NULL" };
    Ok(sqlx::query_as(&format!("{SELECT} {filter} ORDER BY c.Name, c.Id"))
        .bind(actor)
        .bind(actor)
        .bind(actor)
        .bind(actor)
        .bind(actor)
        .fetch_all(conn)
        .await?)
}

pub async fn count_owned_ever(conn: &mut MySqlConnection, owner: i64) -> RepoResult<i64> {
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM Course WHERE OwnerUserId = ?").bind(owner).fetch_one(conn).await?)
}

pub async fn count_owned_active(conn: &mut MySqlConnection, owner: i64) -> RepoResult<i64> {
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM Course WHERE OwnerUserId = ? AND ArchivedAt IS NULL").bind(owner).fetch_one(conn).await?)
}

pub struct NewCourse<'a> {
    pub public_id: &'a str,
    pub owner: i64,
    pub name: &'a str,
    pub code: &'a str,
    pub institution: Option<&'a str>,
    pub term: Option<&'a str>,
    pub color: &'a str,
    pub grade_scale_id: i64,
}

pub async fn insert(conn: &mut MySqlConnection, c: &NewCourse<'_>) -> RepoResult<i64> {
    let res = sqlx::query(
        "INSERT INTO Course (PublicId, OwnerUserId, Name, Code, Institution, Term, Visibility, Color, GradeScaleId) \
         VALUES (?, ?, ?, ?, ?, ?, 'Private', ?, ?)",
    )
    .bind(c.public_id)
    .bind(c.owner)
    .bind(c.name)
    .bind(c.code)
    .bind(c.institution)
    .bind(c.term)
    .bind(c.color)
    .bind(c.grade_scale_id)
    .execute(conn)
    .await
    .map_err(map_unique)?;
    Ok(res.last_insert_id() as i64)
}

pub struct CourseUpdate<'a> {
    pub name: &'a str,
    pub code: &'a str,
    pub institution: Option<&'a str>,
    pub term: Option<&'a str>,
    pub color: &'a str,
    pub grade_scale_id: i64,
}

pub async fn update(conn: &mut MySqlConnection, id: i64, version: i32, u: &CourseUpdate<'_>) -> RepoResult<()> {
    let res = sqlx::query(
        "UPDATE Course SET Name = ?, Code = ?, Institution = ?, Term = ?, Color = ?, GradeScaleId = ?, RowVersion = RowVersion + 1 \
         WHERE Id = ? AND RowVersion = ?",
    )
    .bind(u.name)
    .bind(u.code)
    .bind(u.institution)
    .bind(u.term)
    .bind(u.color)
    .bind(u.grade_scale_id)
    .bind(id)
    .bind(version)
    .execute(conn)
    .await
    .map_err(map_unique)?;
    if res.rows_affected() == 0 {
        return Err(RepoError::VersionConflict);
    }
    Ok(())
}

/// D2-5: trash (Some(now)) or restore (None).
pub async fn set_archived(conn: &mut MySqlConnection, id: i64, at: Option<DateTime<Utc>>) -> RepoResult<()> {
    sqlx::query("UPDATE Course SET ArchivedAt = ?, RowVersion = RowVersion + 1 WHERE Id = ?").bind(at).bind(id).execute(conn).await?;
    Ok(())
}

/// Courses whose trash window has passed: (Id, PublicId).
pub async fn due_for_purge(conn: &mut MySqlConnection, archived_before: DateTime<Utc>) -> RepoResult<Vec<(i64, String)>> {
    Ok(sqlx::query_as("SELECT Id, PublicId FROM Course WHERE ArchivedAt IS NOT NULL AND ArchivedAt <= ? ORDER BY Id LIMIT 100")
        .bind(archived_before)
        .fetch_all(conn)
        .await?)
}

/// Hard delete of a trashed course. Catalog rows cascade; the per-user and bank rows that RESTRICT
/// (slips, weak spots, plan, sessions, mocks, attempts, cards, questions) go first, in FK order.
pub async fn hard_delete(conn: &mut MySqlConnection, id: i64) -> RepoResult<()> {
    let steps = [
        "DELETE FROM Slip WHERE CourseId = ?",
        "DELETE FROM WeakSpot WHERE CourseId = ?",
        "UPDATE StudyPlanEntry SET MovedToId = NULL WHERE CourseId = ?",
        "DELETE FROM StudyPlanEntry WHERE CourseId = ?",
        "DELETE FROM StudySession WHERE CourseId = ?",
        "DELETE FROM MockExam WHERE CourseId = ?",
        "DELETE a FROM QuestionAttempt a JOIN Question q ON q.Id = a.QuestionId WHERE q.CourseId = ?",
        "DELETE FROM Flashcard WHERE CourseId = ?",
        "UPDATE Question SET CurrentVersionId = NULL WHERE CourseId = ?",
        "DELETE FROM Question WHERE CourseId = ?",
        "DELETE FROM CourseSubscription WHERE CourseId = ?",
    ];
    let live: Option<i64> =
        sqlx::query_scalar("SELECT Id FROM Course WHERE Id = ? AND ArchivedAt IS NOT NULL").bind(id).fetch_optional(&mut *conn).await?;
    if live.is_none() {
        return Ok(());
    }
    for sql in steps {
        sqlx::query(sql).bind(id).execute(&mut *conn).await?;
    }
    sqlx::query("DELETE FROM Course WHERE Id = ? AND ArchivedAt IS NOT NULL").bind(id).execute(conn).await?;
    Ok(())
}

pub async fn owner_ref_and_color(conn: &mut MySqlConnection, course_id: i64) -> RepoResult<Option<(String, String)>> {
    Ok(sqlx::query_as("SELECT u.PublicId, c.Color FROM Course c JOIN Users u ON u.Id = c.OwnerUserId WHERE c.Id = ?")
        .bind(course_id)
        .fetch_optional(conn)
        .await?)
}

/// (owner PublicId, course Id) of a live (not trashed) course.
pub async fn owner_of_live_course(conn: &mut MySqlConnection, course_ref: &str) -> RepoResult<Option<(String, i64)>> {
    Ok(sqlx::query_as(
        "SELECT u.PublicId, c.Id FROM Course c JOIN Users u ON u.Id = c.OwnerUserId WHERE c.PublicId = ? AND c.ArchivedAt IS NULL",
    )
    .bind(course_ref)
    .fetch_optional(conn)
    .await?)
}

pub async fn public_id_of(conn: &mut MySqlConnection, course_id: i64) -> RepoResult<String> {
    Ok(sqlx::query_scalar("SELECT PublicId FROM Course WHERE Id = ?").bind(course_id).fetch_one(conn).await?)
}

/// Every user with access to the course (owner + accepted members): their PublicIds.
pub async fn member_refs(conn: &mut MySqlConnection, course_id: i64) -> RepoResult<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT u.PublicId FROM Course c JOIN Users u ON u.Id = c.OwnerUserId WHERE c.Id = ? \
         UNION SELECT u.PublicId FROM CourseMember m JOIN Users u ON u.Id = m.UserId WHERE m.CourseId = ? AND m.Status = 'Accepted'",
    )
    .bind(course_id)
    .bind(course_id)
    .fetch_all(conn)
    .await?)
}

// ---------- members and share invitations ----------

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct MemberRow {
    pub user_id: i64,
    pub user_ref: String,
    pub username: String,
    pub first_name: String,
    pub last_name: String,
    pub role: String,
    pub status: String,
    pub added_at: DateTime<Utc>,
}

pub async fn members(conn: &mut MySqlConnection, course_id: i64) -> RepoResult<Vec<MemberRow>> {
    Ok(sqlx::query_as(
        "SELECT m.UserId, u.PublicId AS UserRef, u.Username, u.FirstName, u.LastName, CAST(m.Role AS CHAR) AS Role, \
         CAST(m.Status AS CHAR) AS Status, m.AddedAt FROM CourseMember m JOIN Users u ON u.Id = m.UserId \
         WHERE m.CourseId = ? ORDER BY m.AddedAt",
    )
    .bind(course_id)
    .fetch_all(conn)
    .await?)
}

pub async fn member(conn: &mut MySqlConnection, course_id: i64, user_id: i64) -> RepoResult<Option<MemberRow>> {
    Ok(sqlx::query_as(
        "SELECT m.UserId, u.PublicId AS UserRef, u.Username, u.FirstName, u.LastName, CAST(m.Role AS CHAR) AS Role, \
         CAST(m.Status AS CHAR) AS Status, m.AddedAt FROM CourseMember m JOIN Users u ON u.Id = m.UserId \
         WHERE m.CourseId = ? AND m.UserId = ?",
    )
    .bind(course_id)
    .bind(user_id)
    .fetch_optional(conn)
    .await?)
}

/// Adds or re-roles a member. A new row starts Pending (D2-2) unless `accepted`; an existing row
/// keeps its status and gets the new role.
pub async fn upsert_member(
    conn: &mut MySqlConnection,
    course_id: i64,
    user_id: i64,
    role: &str,
    added_by: i64,
    accepted: bool,
) -> RepoResult<()> {
    sqlx::query(
        "INSERT INTO CourseMember (CourseId, UserId, Role, Status, AddedBy, RespondedAt) VALUES (?, ?, ?, ?, ?, IF(? = 'Accepted', UTC_TIMESTAMP(), NULL)) \
         ON DUPLICATE KEY UPDATE Role = VALUES(Role)",
    )
    .bind(course_id)
    .bind(user_id)
    .bind(role)
    .bind(if accepted { "Accepted" } else { "Pending" })
    .bind(added_by)
    .bind(if accepted { "Accepted" } else { "Pending" })
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn accept_member(conn: &mut MySqlConnection, course_id: i64, user_id: i64, at: DateTime<Utc>) -> RepoResult<bool> {
    let res = sqlx::query(
        "UPDATE CourseMember SET Status = 'Accepted', RespondedAt = ? WHERE CourseId = ? AND UserId = ? AND Status = 'Pending'",
    )
    .bind(at)
    .bind(course_id)
    .bind(user_id)
    .execute(conn)
    .await?;
    Ok(res.rows_affected() == 1)
}

pub async fn remove_member(conn: &mut MySqlConnection, course_id: i64, user_id: i64) -> RepoResult<bool> {
    let res = sqlx::query("DELETE FROM CourseMember WHERE CourseId = ? AND UserId = ?").bind(course_id).bind(user_id).execute(conn).await?;
    Ok(res.rows_affected() == 1)
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct InvitationRow {
    pub course_id: i64,
    pub course_ref: String,
    pub course_name: String,
    pub course_code: String,
    pub color: String,
    pub role: String,
    pub invited_by_username: Option<String>,
    pub added_at: DateTime<Utc>,
}

/// Courses shared with me that I haven't accepted yet.
pub async fn pending_invitations(conn: &mut MySqlConnection, user_id: i64) -> RepoResult<Vec<InvitationRow>> {
    Ok(sqlx::query_as(
        "SELECT c.Id AS CourseId, c.PublicId AS CourseRef, c.Name AS CourseName, c.Code AS CourseCode, c.Color, CAST(m.Role AS CHAR) AS Role, \
         b.Username AS InvitedByUsername, m.AddedAt FROM CourseMember m JOIN Course c ON c.Id = m.CourseId \
         LEFT JOIN Users b ON b.Id = m.AddedBy WHERE m.UserId = ? AND m.Status = 'Pending' AND c.ArchivedAt IS NULL ORDER BY m.AddedAt",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await?)
}

/// A pending invitation for this user to this course (by course PublicId), with the course id.
pub async fn pending_invitation(conn: &mut MySqlConnection, user_id: i64, course_ref: &str) -> RepoResult<Option<InvitationRow>> {
    Ok(sqlx::query_as(
        "SELECT c.Id AS CourseId, c.PublicId AS CourseRef, c.Name AS CourseName, c.Code AS CourseCode, c.Color, CAST(m.Role AS CHAR) AS Role, \
         b.Username AS InvitedByUsername, m.AddedAt FROM CourseMember m JOIN Course c ON c.Id = m.CourseId \
         LEFT JOIN Users b ON b.Id = m.AddedBy WHERE m.UserId = ? AND c.PublicId = ? AND m.Status = 'Pending' AND c.ArchivedAt IS NULL",
    )
    .bind(user_id)
    .bind(course_ref)
    .fetch_optional(conn)
    .await?)
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct ShareInviteRow {
    pub id: i64,
    pub public_id: String,
    pub course_id: i64,
    pub email: String,
    pub role: String,
    pub invite_id: i64,
    pub invited_by: Option<i64>,
    pub created_at: DateTime<Utc>,
}

pub struct NewShareInvite<'a> {
    pub public_id: &'a str,
    pub course_id: i64,
    pub email: &'a str,
    pub role: &'a str,
    pub invite_id: i64,
    pub invited_by: i64,
}

pub async fn insert_share_invite(conn: &mut MySqlConnection, s: &NewShareInvite<'_>) -> RepoResult<()> {
    sqlx::query("INSERT INTO CourseShareInvite (PublicId, CourseId, Email, Role, InviteId, InvitedBy) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(s.public_id)
        .bind(s.course_id)
        .bind(s.email)
        .bind(s.role)
        .bind(s.invite_id)
        .bind(s.invited_by)
        .execute(conn)
        .await?;
    Ok(())
}

const SHARE_SELECT: &str =
    "SELECT Id, PublicId, CourseId, Email, CAST(Role AS CHAR) AS Role, InviteId, InvitedBy, CreatedAt FROM CourseShareInvite";

/// Open (not accepted, not revoked) email shares of a course.
pub async fn open_share_invites(conn: &mut MySqlConnection, course_id: i64) -> RepoResult<Vec<ShareInviteRow>> {
    Ok(sqlx::query_as(&format!("{SHARE_SELECT} WHERE CourseId = ? AND AcceptedAt IS NULL AND RevokedAt IS NULL ORDER BY Id"))
        .bind(course_id)
        .fetch_all(conn)
        .await?)
}

pub async fn open_share_invite_for_email(conn: &mut MySqlConnection, course_id: i64, email: &str) -> RepoResult<Option<ShareInviteRow>> {
    Ok(sqlx::query_as(&format!("{SHARE_SELECT} WHERE CourseId = ? AND Email = ? AND AcceptedAt IS NULL AND RevokedAt IS NULL"))
        .bind(course_id)
        .bind(email)
        .fetch_optional(conn)
        .await?)
}

/// Shares attached to an invite (consumed at signup; D2-3).
pub async fn share_invites_for_invite(conn: &mut MySqlConnection, invite_id: i64) -> RepoResult<Vec<ShareInviteRow>> {
    Ok(sqlx::query_as(&format!("{SHARE_SELECT} WHERE InviteId = ? AND AcceptedAt IS NULL AND RevokedAt IS NULL"))
        .bind(invite_id)
        .fetch_all(conn)
        .await?)
}

pub async fn share_invite_by_public_id(conn: &mut MySqlConnection, course_id: i64, public_id: &str) -> RepoResult<Option<ShareInviteRow>> {
    Ok(sqlx::query_as(&format!("{SHARE_SELECT} WHERE CourseId = ? AND PublicId = ? AND AcceptedAt IS NULL AND RevokedAt IS NULL"))
        .bind(course_id)
        .bind(public_id)
        .fetch_optional(conn)
        .await?)
}

pub async fn mark_share_invite(conn: &mut MySqlConnection, id: i64, accepted: bool, at: DateTime<Utc>) -> RepoResult<()> {
    let col = if accepted { "AcceptedAt" } else { "RevokedAt" };
    sqlx::query(&format!("UPDATE CourseShareInvite SET {col} = ? WHERE Id = ?")).bind(at).bind(id).execute(conn).await?;
    Ok(())
}

// ---------- subscriptions ----------

pub async fn subscribe(conn: &mut MySqlConnection, user_id: i64, course_id: i64, today: NaiveDate) -> RepoResult<()> {
    sqlx::query(
        "INSERT INTO CourseSubscription (UserId, CourseId, Status, StartDate) VALUES (?, ?, 'Active', ?) \
         ON DUPLICATE KEY UPDATE Status = IF(Status = 'Archived', 'Active', Status)",
    )
    .bind(user_id)
    .bind(course_id)
    .bind(today)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn set_subscription_status(conn: &mut MySqlConnection, user_id: i64, course_id: i64, status: &str) -> RepoResult<bool> {
    let res = sqlx::query("UPDATE CourseSubscription SET Status = ?, RowVersion = RowVersion + 1 WHERE UserId = ? AND CourseId = ?")
        .bind(status)
        .bind(user_id)
        .bind(course_id)
        .execute(conn)
        .await?;
    Ok(res.rows_affected() == 1)
}

/// Unsubscribe. Deadlines and their history go with it (FK cascade); progress elsewhere stays.
pub async fn unsubscribe(conn: &mut MySqlConnection, user_id: i64, course_id: i64) -> RepoResult<bool> {
    let res =
        sqlx::query("DELETE FROM CourseSubscription WHERE UserId = ? AND CourseId = ?").bind(user_id).bind(course_id).execute(conn).await?;
    Ok(res.rows_affected() == 1)
}

// ---------- grade scales ----------

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct GradeScaleRow {
    pub id: i64,
    pub public_id: Option<String>,
    pub owner_user_id: Option<i64>,
    pub name: String,
    pub kind: String,
    pub scale_min: Option<f64>,
    pub scale_max: Option<f64>,
    pub step: Option<f64>,
    pub pass_value: Option<f64>,
    pub labels: Option<Json<serde_json::Value>>,
}

const SCALE_SELECT: &str = "SELECT Id, PublicId, OwnerUserId, Name, CAST(Kind AS CHAR) AS Kind, CAST(ScaleMin AS DOUBLE) AS ScaleMin, \
    CAST(ScaleMax AS DOUBLE) AS ScaleMax, CAST(Step AS DOUBLE) AS Step, CAST(PassValue AS DOUBLE) AS PassValue, Labels FROM GradeScale";

/// Built-ins + mine.
pub async fn grade_scales_for(conn: &mut MySqlConnection, user_id: i64) -> RepoResult<Vec<GradeScaleRow>> {
    Ok(sqlx::query_as(&format!("{SCALE_SELECT} WHERE OwnerUserId IS NULL OR OwnerUserId = ? ORDER BY OwnerUserId IS NOT NULL, Id"))
        .bind(user_id)
        .fetch_all(conn)
        .await?)
}

/// A scale the user may use: built-in or theirs.
pub async fn grade_scale_usable_by(conn: &mut MySqlConnection, user_id: i64, public_id: &str) -> RepoResult<Option<GradeScaleRow>> {
    Ok(sqlx::query_as(&format!("{SCALE_SELECT} WHERE PublicId = ? AND (OwnerUserId IS NULL OR OwnerUserId = ?)"))
        .bind(public_id)
        .bind(user_id)
        .fetch_optional(conn)
        .await?)
}

pub async fn grade_scale_by_public_id(conn: &mut MySqlConnection, public_id: &str) -> RepoResult<Option<GradeScaleRow>> {
    Ok(sqlx::query_as(&format!("{SCALE_SELECT} WHERE PublicId = ?")).bind(public_id).fetch_optional(conn).await?)
}

pub struct NewGradeScale<'a> {
    pub public_id: &'a str,
    pub owner: i64,
    pub name: &'a str,
    pub kind: &'a str,
    pub scale_min: Option<f64>,
    pub scale_max: Option<f64>,
    pub step: Option<f64>,
    pub pass_value: Option<f64>,
    pub labels: Option<serde_json::Value>,
}

pub async fn insert_grade_scale(conn: &mut MySqlConnection, g: &NewGradeScale<'_>) -> RepoResult<i64> {
    let res = sqlx::query(
        "INSERT INTO GradeScale (PublicId, OwnerUserId, Name, Kind, ScaleMin, ScaleMax, Step, PassValue, Labels) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(g.public_id)
    .bind(g.owner)
    .bind(g.name)
    .bind(g.kind)
    .bind(g.scale_min)
    .bind(g.scale_max)
    .bind(g.step)
    .bind(g.pass_value)
    .bind(g.labels.clone().map(Json))
    .execute(conn)
    .await?;
    Ok(res.last_insert_id() as i64)
}
