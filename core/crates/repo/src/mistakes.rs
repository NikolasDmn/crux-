//! Slip, WeakSpot, WeakSpotCheck, Trap (schema section 10). Every query scoped by UserId.

use chrono::{DateTime, Utc};
use sqlx::MySqlConnection;

use crate::{RepoError, RepoResult};

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct SlipRow {
    pub id: i64,
    pub public_id: Option<String>,
    pub course_id: i64,
    pub course_ref: String,
    pub component_ref: Option<String>,
    pub weak_spot_ref: Option<String>,
    pub stype: String,
    pub description: String,
    pub occurred_at: DateTime<Utc>,
    pub confidence: Option<String>,
    pub logged_by: String,
    pub mock_ref: Option<String>,
    pub mock_question_order: Option<i32>,
    pub question_attempt_id: Option<i64>,
    pub resolved_at: Option<DateTime<Utc>>,
}

const S_SELECT: &str = "SELECT s.Id, s.PublicId, s.CourseId, c.PublicId AS CourseRef, k.PublicId AS ComponentRef, w.PublicId AS WeakSpotRef, \
    CAST(s.Type AS CHAR) AS Stype, s.Description, s.OccurredAt, CAST(s.Confidence AS CHAR) AS Confidence, CAST(s.LoggedBy AS CHAR) AS LoggedBy, \
    m.PublicId AS MockRef, s.MockQuestionOrder, s.QuestionAttemptId, s.ResolvedAt FROM Slip s JOIN Course c ON c.Id = s.CourseId \
    LEFT JOIN CourseComponent k ON k.Id = s.ComponentId LEFT JOIN WeakSpot w ON w.Id = s.WeakSpotId LEFT JOIN MockExam m ON m.Id = s.MockExamId";

pub async fn slips(
    conn: &mut MySqlConnection,
    user_id: i64,
    course_id: Option<i64>,
    open: Option<bool>,
    stype: Option<&str>,
) -> RepoResult<Vec<SlipRow>> {
    Ok(sqlx::query_as(&format!(
        "{S_SELECT} WHERE s.UserId = ? AND (? IS NULL OR s.CourseId = ?) AND (? IS NULL OR (s.ResolvedAt IS NULL) = ?) AND (? IS NULL OR s.Type = ?) \
         ORDER BY s.OccurredAt DESC, s.Id DESC LIMIT 500"
    ))
    .bind(user_id)
    .bind(course_id)
    .bind(course_id)
    .bind(open)
    .bind(open)
    .bind(stype)
    .bind(stype)
    .fetch_all(conn)
    .await?)
}

pub async fn slip(conn: &mut MySqlConnection, user_id: i64, public_id: &str) -> RepoResult<Option<SlipRow>> {
    Ok(sqlx::query_as(&format!("{S_SELECT} WHERE s.UserId = ? AND s.PublicId = ?"))
        .bind(user_id)
        .bind(public_id)
        .fetch_optional(conn)
        .await?)
}

pub struct NewSlip<'a> {
    pub public_id: &'a str,
    pub user_id: i64,
    pub course_id: i64,
    pub component_id: Option<i64>,
    pub weak_spot_id: Option<i64>,
    pub stype: &'a str,
    pub description: &'a str,
    pub occurred_at: DateTime<Utc>,
    pub confidence: Option<&'a str>,
    pub logged_by: &'a str,
    pub mock_id: Option<i64>,
    pub mock_order: Option<i32>,
    pub attempt_id: Option<i64>,
    pub review_id: Option<i64>,
}

pub async fn insert_slip(conn: &mut MySqlConnection, s: &NewSlip<'_>) -> RepoResult<i64> {
    let res = sqlx::query(
        "INSERT INTO Slip (PublicId, UserId, CourseId, ComponentId, WeakSpotId, Type, Description, OccurredAt, Confidence, LoggedBy, MockExamId, \
         MockQuestionOrder, QuestionAttemptId, FlashcardReviewId) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(s.public_id)
    .bind(s.user_id)
    .bind(s.course_id)
    .bind(s.component_id)
    .bind(s.weak_spot_id)
    .bind(s.stype)
    .bind(s.description)
    .bind(s.occurred_at)
    .bind(s.confidence)
    .bind(s.logged_by)
    .bind(s.mock_id)
    .bind(s.mock_order)
    .bind(s.attempt_id)
    .bind(s.review_id)
    .execute(conn)
    .await?;
    Ok(res.last_insert_id() as i64)
}

pub async fn update_slip(
    conn: &mut MySqlConnection,
    id: i64,
    stype: &str,
    description: &str,
    weak_spot_id: Option<i64>,
    resolved_at: Option<DateTime<Utc>>,
) -> RepoResult<()> {
    sqlx::query("UPDATE Slip SET Type = ?, Description = ?, WeakSpotId = ?, ResolvedAt = ? WHERE Id = ?")
        .bind(stype)
        .bind(description)
        .bind(weak_spot_id)
        .bind(resolved_at)
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

/// (mock order) pairs that already have a slip, for proposals.
pub async fn slipped_orders(conn: &mut MySqlConnection, user_id: i64, mock_id: i64) -> RepoResult<Vec<i32>> {
    Ok(sqlx::query_scalar("SELECT MockQuestionOrder FROM Slip WHERE UserId = ? AND MockExamId = ? AND MockQuestionOrder IS NOT NULL")
        .bind(user_id)
        .bind(mock_id)
        .fetch_all(conn)
        .await?)
}

/// Spec 5.6: a slip is resolved when a later answer to the same question is right.
pub async fn resolve_slips_for_question(conn: &mut MySqlConnection, user_id: i64, question_id: i64, at: DateTime<Utc>) -> RepoResult<u64> {
    let res = sqlx::query(
        "UPDATE Slip s LEFT JOIN MockQuestion mq ON mq.MockExamId = s.MockExamId AND mq.QuestionOrder = s.MockQuestionOrder \
         LEFT JOIN QuestionAttempt a ON a.Id = s.QuestionAttemptId \
         SET s.ResolvedAt = ? WHERE s.UserId = ? AND s.ResolvedAt IS NULL AND s.OccurredAt <= ? AND (mq.QuestionId = ? OR a.QuestionId = ?)",
    )
    .bind(at)
    .bind(user_id)
    .bind(at)
    .bind(question_id)
    .bind(question_id)
    .execute(conn)
    .await?;
    Ok(res.rows_affected())
}

/// Open slips with the concept of their source question, for weak-spot proposals:
/// (concept id, concept PublicId, concept name, OccurredAt).
pub async fn slip_concepts(
    conn: &mut MySqlConnection,
    user_id: i64,
    course_id: i64,
) -> RepoResult<Vec<(i64, Option<String>, String, DateTime<Utc>)>> {
    Ok(sqlx::query_as(
        "SELECT c.Id, c.PublicId, c.Name, s.OccurredAt FROM Slip s \
         LEFT JOIN MockQuestion mq ON mq.MockExamId = s.MockExamId AND mq.QuestionOrder = s.MockQuestionOrder \
         LEFT JOIN QuestionAttempt a ON a.Id = s.QuestionAttemptId \
         JOIN QuestionConcept qc ON qc.QuestionId = COALESCE(mq.QuestionId, a.QuestionId) \
         JOIN Concept c ON c.Id = qc.ConceptId \
         WHERE s.UserId = ? AND s.CourseId = ? AND s.ResolvedAt IS NULL",
    )
    .bind(user_id)
    .bind(course_id)
    .fetch_all(conn)
    .await?)
}

/// Open slips grouped by type across courses (spec 5.6 cross-course patterns).
pub async fn patterns(conn: &mut MySqlConnection, user_id: i64) -> RepoResult<Vec<(String, String, String, i64)>> {
    Ok(sqlx::query_as(
        "SELECT CAST(s.Type AS CHAR), c.PublicId, c.Code, COUNT(*) FROM Slip s JOIN Course c ON c.Id = s.CourseId \
         WHERE s.UserId = ? AND s.ResolvedAt IS NULL GROUP BY s.Type, c.Id ORDER BY s.Type, COUNT(*) DESC",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await?)
}

// ---------- weak spots ----------

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct WeakSpotRow {
    pub id: i64,
    pub public_id: String,
    pub course_id: i64,
    pub concept_id: Option<i64>,
    pub concept_ref: Option<String>,
    pub topic: String,
    pub detail: Option<String>,
    pub key_rule: Option<String>,
    pub severity: String,
    pub status: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub cold_passes_needed: i8,
    pub misses_at_level: i8,
    pub cold_pass_run: i8,
    pub row_version: i32,
}

const W_SELECT: &str = "SELECT w.Id, w.PublicId, w.CourseId, w.ConceptId, c.PublicId AS ConceptRef, w.Topic, w.Detail, w.KeyRule, \
    CAST(w.Severity AS CHAR) AS Severity, CAST(w.Status AS CHAR) AS Status, w.FirstSeenAt, w.LastSeenAt, w.ResolvedAt, w.ColdPassesNeeded, w.MissesAtLevel, w.ColdPassRun, w.RowVersion \
    FROM WeakSpot w LEFT JOIN Concept c ON c.Id = w.ConceptId";

pub async fn weak_spots(conn: &mut MySqlConnection, user_id: i64, course_id: i64) -> RepoResult<Vec<WeakSpotRow>> {
    Ok(sqlx::query_as(&format!("{W_SELECT} WHERE w.UserId = ? AND w.CourseId = ? ORDER BY w.LastSeenAt DESC"))
        .bind(user_id)
        .bind(course_id)
        .fetch_all(conn)
        .await?)
}

pub async fn weak_spot(conn: &mut MySqlConnection, user_id: i64, public_id: &str) -> RepoResult<Option<WeakSpotRow>> {
    Ok(sqlx::query_as(&format!("{W_SELECT} WHERE w.UserId = ? AND w.PublicId = ?"))
        .bind(user_id)
        .bind(public_id)
        .fetch_optional(conn)
        .await?)
}

/// Open or reopened spots of the user in the course whose concept is one of `concepts`.
pub async fn open_spots_for_concepts(
    conn: &mut MySqlConnection,
    user_id: i64,
    course_id: i64,
    concepts: &[i64],
) -> RepoResult<Vec<WeakSpotRow>> {
    if concepts.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "{W_SELECT} WHERE w.UserId = ? AND w.CourseId = ? AND w.ConceptId IN ({}) FOR UPDATE OF w",
        concepts.iter().map(|c| c.to_string()).collect::<Vec<_>>().join(",")
    );
    Ok(sqlx::query_as(&sql).bind(user_id).bind(course_id).fetch_all(conn).await?)
}

pub struct NewWeakSpot<'a> {
    pub public_id: &'a str,
    pub user_id: i64,
    pub course_id: i64,
    pub concept_id: Option<i64>,
    pub topic: &'a str,
    pub detail: Option<&'a str>,
    pub key_rule: Option<&'a str>,
    pub severity: &'a str,
    pub cold_passes_needed: i8,
    pub at: DateTime<Utc>,
}

pub async fn insert_weak_spot(conn: &mut MySqlConnection, w: &NewWeakSpot<'_>) -> RepoResult<i64> {
    let res = sqlx::query(
        "INSERT INTO WeakSpot (PublicId, UserId, CourseId, ConceptId, Topic, Detail, KeyRule, Severity, FirstSeenAt, LastSeenAt, ColdPassesNeeded) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(w.public_id)
    .bind(w.user_id)
    .bind(w.course_id)
    .bind(w.concept_id)
    .bind(w.topic)
    .bind(w.detail)
    .bind(w.key_rule)
    .bind(w.severity)
    .bind(w.at)
    .bind(w.at)
    .bind(w.cold_passes_needed)
    .execute(conn)
    .await?;
    Ok(res.last_insert_id() as i64)
}

pub async fn update_weak_spot_fields(
    conn: &mut MySqlConnection,
    id: i64,
    expected: i32,
    topic: &str,
    detail: Option<&str>,
    key_rule: Option<&str>,
    cold: i8,
) -> RepoResult<()> {
    let res = sqlx::query("UPDATE WeakSpot SET Topic = ?, Detail = ?, KeyRule = ?, ColdPassesNeeded = ?, RowVersion = RowVersion + 1 WHERE Id = ? AND RowVersion = ?")
        .bind(topic)
        .bind(detail)
        .bind(key_rule)
        .bind(cold)
        .bind(id)
        .bind(expected)
        .execute(conn)
        .await?;
    if res.rows_affected() == 0 {
        return Err(RepoError::VersionConflict);
    }
    Ok(())
}

/// The only writer of WeakSpot.Status/Severity/ResolvedAt and the rule counters (after
/// crux_domain::weakspot::apply).
#[allow(clippy::too_many_arguments)]
pub async fn set_weak_spot_state(
    conn: &mut MySqlConnection,
    id: i64,
    status: &str,
    severity: &str,
    resolved_at: Option<DateTime<Utc>>,
    misses_at_level: i8,
    cold_run: i8,
    last_seen: Option<DateTime<Utc>>,
) -> RepoResult<()> {
    sqlx::query(
        "UPDATE WeakSpot SET Status = ?, Severity = ?, ResolvedAt = ?, MissesAtLevel = ?, ColdPassRun = ?,          LastSeenAt = GREATEST(LastSeenAt, COALESCE(?, LastSeenAt)), RowVersion = RowVersion + 1 WHERE Id = ?",
    )
    .bind(status)
    .bind(severity)
    .bind(resolved_at)
    .bind(misses_at_level)
    .bind(cold_run)
    .bind(last_seen)
    .bind(id)
    .execute(conn)
    .await?;
    Ok(())
}

/// Manual severity change by the user: the miss count restarts at the new level.
pub async fn set_weak_spot_severity(conn: &mut MySqlConnection, id: i64, severity: &str) -> RepoResult<()> {
    sqlx::query("UPDATE WeakSpot SET Severity = ?, MissesAtLevel = 0 WHERE Id = ?").bind(severity).bind(id).execute(conn).await?;
    Ok(())
}

pub async fn insert_check(
    conn: &mut MySqlConnection,
    weak_spot_id: i64,
    at: DateTime<Utc>,
    passed: bool,
    cold: bool,
    attempt_id: Option<i64>,
    mock: Option<(i64, i32)>,
) -> RepoResult<()> {
    sqlx::query("INSERT INTO WeakSpotCheck (WeakSpotId, CheckedAt, Passed, Cold, QuestionAttemptId, MockExamId, MockQuestionOrder) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind(weak_spot_id)
        .bind(at)
        .bind(passed)
        .bind(cold)
        .bind(attempt_id)
        .bind(mock.map(|m| m.0))
        .bind(mock.map(|m| m.1))
        .execute(conn)
        .await?;
    Ok(())
}

/// (CheckedAt, Passed, Cold) for a spot.
pub async fn checks(conn: &mut MySqlConnection, weak_spot_id: i64) -> RepoResult<Vec<(DateTime<Utc>, bool, bool)>> {
    Ok(sqlx::query_as("SELECT CheckedAt, Passed, Cold FROM WeakSpotCheck WHERE WeakSpotId = ? ORDER BY CheckedAt, Id")
        .bind(weak_spot_id)
        .fetch_all(conn)
        .await?)
}

// ---------- traps ----------

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct TrapRow {
    pub id: i64,
    pub public_id: Option<String>,
    pub text: String,
    pub weak_spot_ref: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub async fn traps(conn: &mut MySqlConnection, user_id: i64, course_id: i64) -> RepoResult<Vec<TrapRow>> {
    Ok(sqlx::query_as(
        "SELECT t.Id, t.PublicId, t.Text, w.PublicId AS WeakSpotRef, t.CreatedAt FROM Trap t LEFT JOIN WeakSpot w ON w.Id = t.WeakSpotId \
         WHERE t.UserId = ? AND t.CourseId = ? AND t.RetiredAt IS NULL ORDER BY t.CreatedAt",
    )
    .bind(user_id)
    .bind(course_id)
    .fetch_all(conn)
    .await?)
}

pub async fn insert_trap(
    conn: &mut MySqlConnection,
    public_id: &str,
    user_id: i64,
    course_id: i64,
    text: &str,
    weak_spot_id: Option<i64>,
) -> RepoResult<i64> {
    let res = sqlx::query("INSERT INTO Trap (PublicId, UserId, CourseId, Text, WeakSpotId) VALUES (?, ?, ?, ?, ?)")
        .bind(public_id)
        .bind(user_id)
        .bind(course_id)
        .bind(text)
        .bind(weak_spot_id)
        .execute(conn)
        .await?;
    Ok(res.last_insert_id() as i64)
}

pub async fn retire_trap(conn: &mut MySqlConnection, user_id: i64, course_id: i64, public_id: &str, at: DateTime<Utc>) -> RepoResult<bool> {
    let res = sqlx::query("UPDATE Trap SET RetiredAt = ? WHERE UserId = ? AND CourseId = ? AND PublicId = ? AND RetiredAt IS NULL")
        .bind(at)
        .bind(user_id)
        .bind(course_id)
        .bind(public_id)
        .execute(conn)
        .await?;
    Ok(res.rows_affected() == 1)
}
