//! Deadline + DeadlineComponent (schema section 5). Deadlines hang off the user's subscription.

use chrono::{DateTime, Utc};
use sqlx::MySqlConnection;

use crate::{RepoError, RepoResult};

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct DeadlineRow {
    pub id: i64,
    pub public_id: Option<String>,
    pub subscription_id: i64,
    pub user_id: i64,
    pub course_id: i64,
    pub course_ref: String,
    pub course_code: String,
    #[sqlx(rename = "Type")]
    pub dtype: String,
    pub title: Option<String>,
    pub resit_of_ref: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub duration_min: Option<i32>,
    pub format: Option<String>,
    pub pass_mark: Option<f64>,
    pub status: String,
    pub grade_value: Option<f64>,
    pub grade_text: Option<String>,
    pub grade_provisional: bool,
    pub check_in_note: Option<String>,
    pub eve_requested_at: Option<DateTime<Utc>>,
    pub check_in_requested_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub row_version: i32,
}

const SELECT: &str = "SELECT d.Id, d.PublicId, d.CourseSubscriptionId AS SubscriptionId, s.UserId, s.CourseId, c.PublicId AS CourseRef, c.Code AS CourseCode, \
    CAST(d.Type AS CHAR) AS Type, d.Title, r.PublicId AS ResitOfRef, d.StartsAt, d.DurationMin, d.Format, CAST(d.PassMark AS DOUBLE) AS PassMark, \
    CAST(d.Status AS CHAR) AS Status, CAST(d.GradeValue AS DOUBLE) AS GradeValue, d.GradeText, d.GradeProvisional, d.CheckInNote, \
    d.EveRequestedAt, d.CheckInRequestedAt, d.UpdatedAt, d.RowVersion \
    FROM Deadline d JOIN CourseSubscription s ON s.Id = d.CourseSubscriptionId JOIN Course c ON c.Id = s.CourseId \
    LEFT JOIN Deadline r ON r.Id = d.ResitOfDeadlineId";

pub async fn for_course(conn: &mut MySqlConnection, user_id: i64, course_id: i64) -> RepoResult<Vec<DeadlineRow>> {
    Ok(sqlx::query_as(&format!("{SELECT} WHERE s.UserId = ? AND s.CourseId = ? ORDER BY d.StartsAt, d.Id"))
        .bind(user_id)
        .bind(course_id)
        .fetch_all(conn)
        .await?)
}

pub async fn by_public(conn: &mut MySqlConnection, user_id: i64, public_id: &str) -> RepoResult<Option<DeadlineRow>> {
    Ok(sqlx::query_as(&format!("{SELECT} WHERE s.UserId = ? AND d.PublicId = ?"))
        .bind(user_id)
        .bind(public_id)
        .fetch_optional(conn)
        .await?)
}

pub async fn by_id(conn: &mut MySqlConnection, id: i64) -> RepoResult<Option<DeadlineRow>> {
    Ok(sqlx::query_as(&format!("{SELECT} WHERE d.Id = ?")).bind(id).fetch_optional(conn).await?)
}

/// Upcoming exams and resits of a user in live, Active courses (the plan's input).
pub async fn upcoming_exams(conn: &mut MySqlConnection, user_id: i64, after: DateTime<Utc>) -> RepoResult<Vec<DeadlineRow>> {
    Ok(sqlx::query_as(&format!(
        "{SELECT} WHERE s.UserId = ? AND s.Status = 'Active' AND c.ArchivedAt IS NULL AND d.Status = 'Upcoming' \
         AND d.Type IN ('Exam','Resit') AND d.StartsAt > ? ORDER BY d.StartsAt"
    ))
    .bind(user_id)
    .bind(after)
    .fetch_all(conn)
    .await?)
}

/// Exams that started but still say Upcoming: the "how did it go?" prompt (spec 5.10).
pub async fn awaiting_result(conn: &mut MySqlConnection, user_id: i64, now: DateTime<Utc>) -> RepoResult<Vec<DeadlineRow>> {
    Ok(sqlx::query_as(&format!(
        "{SELECT} WHERE s.UserId = ? AND d.Status = 'Upcoming' AND d.Type IN ('Exam','Resit') AND d.StartsAt <= ? ORDER BY d.StartsAt"
    ))
    .bind(user_id)
    .bind(now)
    .fetch_all(conn)
    .await?)
}

/// Exams in a window whose eve reminder or check-in hasn't been requested (deadlines.tick).
pub async fn needing_reminders(conn: &mut MySqlConnection, from: DateTime<Utc>, to: DateTime<Utc>) -> RepoResult<Vec<DeadlineRow>> {
    Ok(sqlx::query_as(&format!(
        "{SELECT} WHERE d.Status = 'Upcoming' AND d.Type IN ('Exam','Resit') AND d.StartsAt >= ? AND d.StartsAt < ? \
         AND s.Status = 'Active' AND (d.EveRequestedAt IS NULL OR d.CheckInRequestedAt IS NULL)"
    ))
    .bind(from)
    .bind(to)
    .fetch_all(conn)
    .await?)
}

pub async fn mark_eve(conn: &mut MySqlConnection, id: i64, at: DateTime<Utc>) -> RepoResult<()> {
    sqlx::query("UPDATE Deadline SET EveRequestedAt = ? WHERE Id = ? AND EveRequestedAt IS NULL").bind(at).bind(id).execute(conn).await?;
    Ok(())
}

pub async fn mark_check_in(conn: &mut MySqlConnection, id: i64, at: DateTime<Utc>) -> RepoResult<()> {
    sqlx::query("UPDATE Deadline SET CheckInRequestedAt = ? WHERE Id = ? AND CheckInRequestedAt IS NULL")
        .bind(at)
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn subscription_id(conn: &mut MySqlConnection, user_id: i64, course_id: i64) -> RepoResult<Option<(i64, String)>> {
    Ok(sqlx::query_as("SELECT Id, CAST(Status AS CHAR) FROM CourseSubscription WHERE UserId = ? AND CourseId = ?")
        .bind(user_id)
        .bind(course_id)
        .fetch_optional(conn)
        .await?)
}

pub struct DeadlineWrite<'a> {
    pub dtype: &'a str,
    pub title: Option<&'a str>,
    pub resit_of: Option<i64>,
    pub starts_at: DateTime<Utc>,
    pub duration_min: Option<i32>,
    pub format: Option<&'a str>,
    pub pass_mark: Option<f64>,
    pub status: &'a str,
    pub grade_value: Option<f64>,
    pub grade_text: Option<&'a str>,
    pub grade_provisional: bool,
    pub check_in_note: Option<&'a str>,
}

pub async fn insert(conn: &mut MySqlConnection, public_id: &str, subscription_id: i64, d: &DeadlineWrite<'_>) -> RepoResult<i64> {
    let res = sqlx::query(
        "INSERT INTO Deadline (PublicId, CourseSubscriptionId, Type, Title, ResitOfDeadlineId, StartsAt, DurationMin, Format, PassMark, Status, \
         GradeValue, GradeText, GradeProvisional, CheckInNote) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(public_id)
    .bind(subscription_id)
    .bind(d.dtype)
    .bind(d.title)
    .bind(d.resit_of)
    .bind(d.starts_at)
    .bind(d.duration_min)
    .bind(d.format)
    .bind(d.pass_mark)
    .bind(d.status)
    .bind(d.grade_value)
    .bind(d.grade_text)
    .bind(d.grade_provisional)
    .bind(d.check_in_note)
    .execute(conn)
    .await?;
    Ok(res.last_insert_id() as i64)
}

/// Reminders of a moved exam are re-requested by the service (notify reschedules by dedupe key).
pub async fn update(conn: &mut MySqlConnection, id: i64, expected: i32, d: &DeadlineWrite<'_>) -> RepoResult<()> {
    let res = sqlx::query(
        "UPDATE Deadline SET Type = ?, Title = ?, ResitOfDeadlineId = ?, \
         StartsAt = ?, DurationMin = ?, Format = ?, PassMark = ?, Status = ?, GradeValue = ?, GradeText = ?, GradeProvisional = ?, CheckInNote = ?, \
         RowVersion = RowVersion + 1 WHERE Id = ? AND RowVersion = ?",
    )
    .bind(d.dtype)
    .bind(d.title)
    .bind(d.resit_of)
    .bind(d.starts_at)
    .bind(d.duration_min)
    .bind(d.format)
    .bind(d.pass_mark)
    .bind(d.status)
    .bind(d.grade_value)
    .bind(d.grade_text)
    .bind(d.grade_provisional)
    .bind(d.check_in_note)
    .bind(id)
    .bind(expected)
    .execute(conn)
    .await?;
    if res.rows_affected() == 0 {
        return Err(RepoError::VersionConflict);
    }
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct LayoutRow {
    pub deadline_id: i64,
    pub component_ref: String,
    pub question_count: Option<i32>,
    pub open_count: Option<i32>,
}

pub async fn layouts(conn: &mut MySqlConnection, deadline_ids: &[i64]) -> RepoResult<Vec<LayoutRow>> {
    if deadline_ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids = deadline_ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
    Ok(sqlx::query_as(&format!(
        "SELECT dc.DeadlineId, k.PublicId AS ComponentRef, dc.QuestionCount, dc.OpenCount FROM DeadlineComponent dc \
         JOIN CourseComponent k ON k.Id = dc.ComponentId WHERE dc.DeadlineId IN ({ids}) ORDER BY k.SortOrder, k.Id"
    ))
    .fetch_all(conn)
    .await?)
}

pub async fn set_layout(conn: &mut MySqlConnection, deadline_id: i64, rows: &[(i64, Option<i32>, Option<i32>)]) -> RepoResult<()> {
    sqlx::query("DELETE FROM DeadlineComponent WHERE DeadlineId = ?").bind(deadline_id).execute(&mut *conn).await?;
    for (component, q, open) in rows {
        sqlx::query("INSERT INTO DeadlineComponent (DeadlineId, ComponentId, QuestionCount, OpenCount) VALUES (?, ?, ?, ?)")
            .bind(deadline_id)
            .bind(component)
            .bind(q)
            .bind(open)
            .execute(&mut *conn)
            .await?;
    }
    sqlx::query("UPDATE Deadline SET RowVersion = RowVersion + 1 WHERE Id = ?").bind(deadline_id).execute(conn).await?;
    Ok(())
}

/// Spec 5.10: on a pass the subscription is Passed and keeps the final grade.
pub async fn set_final_grade(
    conn: &mut MySqlConnection,
    subscription_id: i64,
    status: &str,
    value: Option<f64>,
    text: Option<&str>,
    passed: Option<bool>,
) -> RepoResult<()> {
    sqlx::query(
        "UPDATE CourseSubscription SET Status = ?, FinalGradeValue = ?, FinalGradeText = ?, Passed = ?, RowVersion = RowVersion + 1 WHERE Id = ?",
    )
    .bind(status)
    .bind(value)
    .bind(text)
    .bind(passed)
    .bind(subscription_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// The course's grade scale: (kind, pass value).
pub async fn scale_of_course(conn: &mut MySqlConnection, course_id: i64) -> RepoResult<Option<(String, Option<f64>)>> {
    Ok(sqlx::query_as(
        "SELECT CAST(g.Kind AS CHAR), CAST(g.PassValue AS DOUBLE) FROM Course c JOIN GradeScale g ON g.Id = c.GradeScaleId WHERE c.Id = ?",
    )
    .bind(course_id)
    .fetch_optional(conn)
    .await?)
}

/// Letter scales: `[{label, value, pass}]`.
pub async fn scale_labels(conn: &mut MySqlConnection, course_id: i64) -> RepoResult<Option<serde_json::Value>> {
    let row: Option<(Option<sqlx::types::Json<serde_json::Value>>,)> =
        sqlx::query_as("SELECT g.Labels FROM Course c JOIN GradeScale g ON g.Id = c.GradeScaleId WHERE c.Id = ?")
            .bind(course_id)
            .fetch_optional(conn)
            .await?;
    Ok(row.and_then(|r| r.0).map(|j| j.0))
}
