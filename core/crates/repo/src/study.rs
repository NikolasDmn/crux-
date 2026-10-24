//! StudySession, ComponentProgress and the WeekSummary read model (schema section 9, spec 5.7, 5.11).

use chrono::{DateTime, Utc};
use sqlx::types::Json;
use sqlx::MySqlConnection;

use crate::RepoResult;

// ---------- sessions ----------

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct SessionRow {
    pub id: i64,
    pub public_id: Option<String>,
    pub course_id: i64,
    pub course_ref: String,
    pub component_id: Option<i64>,
    pub component_ref: Option<String>,
    pub kind: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub source: String,
    pub note: Option<String>,
}

const S_SELECT: &str = "SELECT s.Id, s.PublicId, s.CourseId, c.PublicId AS CourseRef, s.ComponentId, k.PublicId AS ComponentRef, \
    CAST(s.Kind AS CHAR) AS Kind, s.StartedAt, s.EndedAt, CAST(s.Source AS CHAR) AS Source, s.Note \
    FROM StudySession s JOIN Course c ON c.Id = s.CourseId LEFT JOIN CourseComponent k ON k.Id = s.ComponentId";

pub async fn sessions_between(
    conn: &mut MySqlConnection,
    user_id: i64,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    course_id: Option<i64>,
) -> RepoResult<Vec<SessionRow>> {
    Ok(sqlx::query_as(&format!(
        "{S_SELECT} WHERE s.UserId = ? AND s.StartedAt < ? AND s.EndedAt > ? AND (? IS NULL OR s.CourseId = ?) ORDER BY s.StartedAt"
    ))
    .bind(user_id)
    .bind(to)
    .bind(from)
    .bind(course_id)
    .bind(course_id)
    .fetch_all(conn)
    .await?)
}

pub struct NewSession<'a> {
    pub public_id: &'a str,
    pub user_id: i64,
    pub course_id: i64,
    pub component_id: Option<i64>,
    pub kind: &'a str,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub source: &'a str,
    pub note: Option<&'a str>,
}

pub async fn insert_session(conn: &mut MySqlConnection, s: &NewSession<'_>) -> RepoResult<i64> {
    let res = sqlx::query(
        "INSERT INTO StudySession (PublicId, UserId, CourseId, ComponentId, Kind, StartedAt, EndedAt, Source, Note) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(s.public_id)
    .bind(s.user_id)
    .bind(s.course_id)
    .bind(s.component_id)
    .bind(s.kind)
    .bind(s.started_at)
    .bind(s.ended_at)
    .bind(s.source)
    .bind(s.note)
    .execute(conn)
    .await?;
    Ok(res.last_insert_id() as i64)
}

pub async fn delete_derived_from(conn: &mut MySqlConnection, user_id: i64, from: DateTime<Utc>) -> RepoResult<u64> {
    let res = sqlx::query("DELETE FROM StudySession WHERE UserId = ? AND Source = 'Derived' AND StartedAt >= ?")
        .bind(user_id)
        .bind(from)
        .execute(conn)
        .await?;
    Ok(res.rows_affected())
}

pub async fn explicit_from(
    conn: &mut MySqlConnection,
    user_id: i64,
    from: DateTime<Utc>,
) -> RepoResult<Vec<(DateTime<Utc>, DateTime<Utc>)>> {
    Ok(sqlx::query_as("SELECT StartedAt, EndedAt FROM StudySession WHERE UserId = ? AND Source <> 'Derived' AND EndedAt > ?")
        .bind(user_id)
        .bind(from)
        .fetch_all(conn)
        .await?)
}

/// Hours per lecture of a course (and unattributed, as None).
pub async fn hours_by_component(conn: &mut MySqlConnection, user_id: i64, course_id: i64) -> RepoResult<Vec<(Option<i64>, f64)>> {
    Ok(sqlx::query_as(
        "SELECT ComponentId, CAST(SUM(TIMESTAMPDIFF(SECOND, StartedAt, EndedAt)) / 3600.0 AS DOUBLE) FROM StudySession \
         WHERE UserId = ? AND CourseId = ? GROUP BY ComponentId",
    )
    .bind(user_id)
    .bind(course_id)
    .fetch_all(conn)
    .await?)
}

// ---------- progress ----------

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct ProgressRow {
    pub component_id: i64,
    pub component_ref: String,
    pub code: String,
    pub name: String,
    pub sort_order: i32,
    pub est_hours: Option<f64>,
    pub status: Option<String>,
    pub steps_done: Option<i32>,
    pub steps_total: Option<i32>,
    pub finished_at: Option<DateTime<Utc>>,
}

/// Every live lecture of the course with the user's progress (Todo when there's no row).
pub async fn progress(conn: &mut MySqlConnection, user_id: i64, course_id: i64) -> RepoResult<Vec<ProgressRow>> {
    Ok(sqlx::query_as(
        "SELECT k.Id AS ComponentId, k.PublicId AS ComponentRef, k.Code, k.Name, k.SortOrder, CAST(k.EstHours AS DOUBLE) AS EstHours, \
         CAST(p.Status AS CHAR) AS Status, p.StepsDone, p.StepsTotal, p.FinishedAt \
         FROM CourseComponent k LEFT JOIN ComponentProgress p ON p.ComponentId = k.Id AND p.UserId = ? \
         WHERE k.CourseId = ? AND k.ArchivedAt IS NULL ORDER BY k.SortOrder, k.Id",
    )
    .bind(user_id)
    .bind(course_id)
    .fetch_all(conn)
    .await?)
}

pub async fn upsert_progress(
    conn: &mut MySqlConnection,
    user_id: i64,
    component_id: i64,
    status: &str,
    steps_done: i32,
    steps_total: Option<i32>,
    finished_at: Option<DateTime<Utc>>,
) -> RepoResult<()> {
    sqlx::query(
        "INSERT INTO ComponentProgress (UserId, ComponentId, Status, StepsDone, StepsTotal, FinishedAt) VALUES (?, ?, ?, ?, ?, ?) \
         ON DUPLICATE KEY UPDATE Status = VALUES(Status), StepsDone = VALUES(StepsDone), StepsTotal = VALUES(StepsTotal), FinishedAt = VALUES(FinishedAt)",
    )
    .bind(user_id)
    .bind(component_id)
    .bind(status)
    .bind(steps_done)
    .bind(steps_total)
    .bind(finished_at)
    .execute(conn)
    .await?;
    Ok(())
}

// ---------- week summary ----------

pub async fn week(
    conn: &mut MySqlConnection,
    user_id: i64,
    iso_week: &str,
) -> RepoResult<Option<(Json<serde_json::Value>, DateTime<Utc>)>> {
    Ok(sqlx::query_as("SELECT Data, RebuiltAt FROM WeekSummary WHERE UserId = ? AND IsoWeek = ?")
        .bind(user_id)
        .bind(iso_week)
        .fetch_optional(conn)
        .await?)
}

pub async fn weeks(
    conn: &mut MySqlConnection,
    user_id: i64,
    from_week: &str,
    to_week: &str,
) -> RepoResult<Vec<(String, Json<serde_json::Value>)>> {
    Ok(sqlx::query_as("SELECT IsoWeek, Data FROM WeekSummary WHERE UserId = ? AND IsoWeek >= ? AND IsoWeek <= ? ORDER BY IsoWeek")
        .bind(user_id)
        .bind(from_week)
        .bind(to_week)
        .fetch_all(conn)
        .await?)
}

pub async fn upsert_week(
    conn: &mut MySqlConnection,
    user_id: i64,
    iso_week: &str,
    data: &serde_json::Value,
    at: DateTime<Utc>,
) -> RepoResult<()> {
    sqlx::query(
        "INSERT INTO WeekSummary (UserId, IsoWeek, Data, RebuiltAt) VALUES (?, ?, ?, ?) ON DUPLICATE KEY UPDATE Data = VALUES(Data), RebuiltAt = VALUES(RebuiltAt)",
    )
    .bind(user_id)
    .bind(iso_week)
    .bind(Json(data))
    .bind(at)
    .execute(conn)
    .await?;
    Ok(())
}

/// Weak spots opened and resolved in a window, per course.
pub async fn weak_spot_counts(
    conn: &mut MySqlConnection,
    user_id: i64,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> RepoResult<Vec<(i64, i64, i64)>> {
    Ok(sqlx::query_as(
        "SELECT CourseId, CAST(SUM(FirstSeenAt >= ? AND FirstSeenAt < ?) AS SIGNED), CAST(SUM(ResolvedAt IS NOT NULL AND ResolvedAt >= ? AND ResolvedAt < ?) AS SIGNED) \
         FROM WeakSpot WHERE UserId = ? AND (FirstSeenAt < ? AND (ResolvedAt IS NULL OR ResolvedAt >= ?)) GROUP BY CourseId",
    )
    .bind(from)
    .bind(to)
    .bind(from)
    .bind(to)
    .bind(user_id)
    .bind(to)
    .bind(from)
    .fetch_all(conn)
    .await?)
}

/// Slips per course and local day are counted by the caller; this returns (course, occurredAt).
pub async fn slips_between(
    conn: &mut MySqlConnection,
    user_id: i64,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> RepoResult<Vec<(i64, DateTime<Utc>)>> {
    Ok(sqlx::query_as("SELECT CourseId, OccurredAt FROM Slip WHERE UserId = ? AND OccurredAt >= ? AND OccurredAt < ?")
        .bind(user_id)
        .bind(from)
        .bind(to)
        .fetch_all(conn)
        .await?)
}

// ---------- warm-up and /next helpers ----------

/// One question for a weak spot's concept (spec 5.9): visible to the user, preferring questions not
/// answered right in the last 7 days, then the least recently answered.
pub async fn warmup_question(
    conn: &mut MySqlConnection,
    user_id: i64,
    course_id: i64,
    concept_id: i64,
    right_since: DateTime<Utc>,
    exclude: &[i64],
) -> RepoResult<Option<i64>> {
    let not_in = if exclude.is_empty() {
        String::new()
    } else {
        format!("AND q.Id NOT IN ({})", exclude.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(","))
    };
    Ok(sqlx::query_scalar(&format!(
        "SELECT q.Id FROM Question q JOIN QuestionConcept qc ON qc.QuestionId = q.Id JOIN CourseComponent k ON k.Id = q.CourseComponentId \
         LEFT JOIN UserQuestionStat s ON s.QuestionId = q.Id AND s.UserId = ? \
         WHERE q.CourseId = ? AND qc.ConceptId = ? AND q.ArchivedAt IS NULL AND k.ArchivedAt IS NULL AND q.CurrentVersionId IS NOT NULL \
           AND (q.OwnerUserId IS NULL OR q.OwnerUserId = ?) AND q.ReviewState = 'Approved' {not_in} \
         ORDER BY (s.LastOutcome = 'Right' AND s.LastAnsweredAt >= ?) IS TRUE, s.LastAnsweredAt IS NOT NULL, s.LastAnsweredAt, q.Id LIMIT 1"
    ))
    .bind(user_id)
    .bind(course_id)
    .bind(concept_id)
    .bind(user_id)
    .bind(right_since)
    .fetch_optional(conn)
    .await?)
}

/// (Id, PublicId, Code) of courses, for read-model labels.
pub async fn course_labels(conn: &mut MySqlConnection, ids: &[i64]) -> RepoResult<Vec<(i64, String, String)>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let list = ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
    Ok(sqlx::query_as(&format!("SELECT Id, PublicId, Code FROM Course WHERE Id IN ({list})")).fetch_all(conn).await?)
}

/// Due flashcards in a course (not suspended), for /next.
pub async fn due_cards(conn: &mut MySqlConnection, user_id: i64, course_id: i64, now: DateTime<Utc>) -> RepoResult<i64> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM FlashcardProgress p JOIN Flashcard f ON f.Id = p.FlashcardId \
         WHERE p.UserId = ? AND f.CourseId = ? AND f.ArchivedAt IS NULL AND (f.OwnerUserId IS NULL OR f.OwnerUserId = ?) \
           AND p.State <> 'New' AND NOT p.Suspended AND p.DueAt <= ?",
    )
    .bind(user_id)
    .bind(course_id)
    .bind(user_id)
    .bind(now)
    .fetch_one(conn)
    .await?)
}
