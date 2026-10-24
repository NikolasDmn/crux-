//! MockExam, MockQuestion, QuestionAttempt, UserQuestionStat, and the builder's candidate pool.
//! Status and answer columns are written only through functions the service calls after
//! crux_domain::mock decided the transition (spec 2.3 state module).

use chrono::{DateTime, Utc};
use sqlx::types::Json;
use sqlx::MySqlConnection;

use crate::RepoResult;

// ---------- builder pool ----------

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct PoolRow {
    pub question_id: i64,
    pub family: i64,
    pub component_id: i64,
    pub primary_concept: Option<i64>,
    pub is_open: bool,
    pub attempts: i32,
    pub rights: i32,
    pub last_outcome: Option<String>,
    pub last_confidence: Option<String>,
}

/// Spec 5.3 step 1: questions the actor can see, not archived, Approved (plus own drafts when
/// `include_drafts`), in live lectures (optionally only `components`).
pub async fn pool(
    conn: &mut MySqlConnection,
    course_id: i64,
    actor: i64,
    include_drafts: bool,
    components: Option<&[i64]>,
) -> RepoResult<Vec<PoolRow>> {
    let comp_filter = match components {
        Some(ids) if !ids.is_empty() => {
            format!("AND q.CourseComponentId IN ({})", ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(","))
        }
        _ => String::new(),
    };
    let sql = format!(
        "SELECT q.Id AS QuestionId, COALESCE(q.FamilyId, q.Id) AS Family, q.CourseComponentId AS ComponentId, \
         (SELECT qc.ConceptId FROM QuestionConcept qc WHERE qc.QuestionId = q.Id AND qc.IsPrimary LIMIT 1) AS PrimaryConcept, \
         (q.Type = 'Open') AS IsOpen, COALESCE(s.Attempts, 0) AS Attempts, COALESCE(s.Rights, 0) AS Rights, \
         CAST(s.LastOutcome AS CHAR) AS LastOutcome, CAST(s.LastConfidence AS CHAR) AS LastConfidence \
         FROM Question q JOIN CourseComponent k ON k.Id = q.CourseComponentId \
         LEFT JOIN UserQuestionStat s ON s.QuestionId = q.Id AND s.UserId = ? \
         WHERE q.CourseId = ? AND q.ArchivedAt IS NULL AND k.ArchivedAt IS NULL AND q.CurrentVersionId IS NOT NULL \
           AND (q.OwnerUserId IS NULL OR q.OwnerUserId = ?) \
           AND (q.ReviewState = 'Approved' OR (? AND q.OwnerUserId = ? AND q.ReviewState = 'Draft')) {comp_filter}"
    );
    Ok(sqlx::query_as(&sql).bind(actor).bind(course_id).bind(actor).bind(include_drafts).bind(actor).fetch_all(conn).await?)
}

/// (ComponentId, QuestionCount, OpenCount) for a deadline of the actor's (spec 5.3 real layout).
pub async fn deadline_layout(
    conn: &mut MySqlConnection,
    user_id: i64,
    course_id: i64,
    deadline_ref_id: i64,
) -> RepoResult<Vec<(i64, i32, i32)>> {
    Ok(sqlx::query_as(
        "SELECT dc.ComponentId, COALESCE(dc.QuestionCount, 0), COALESCE(dc.OpenCount, 0) FROM DeadlineComponent dc \
         JOIN Deadline d ON d.Id = dc.DeadlineId JOIN CourseSubscription s ON s.Id = d.CourseSubscriptionId \
         WHERE d.Id = ? AND s.UserId = ? AND s.CourseId = ? ORDER BY dc.ComponentId",
    )
    .bind(deadline_ref_id)
    .bind(user_id)
    .bind(course_id)
    .fetch_all(conn)
    .await?)
}

/// The actor's subscription status on a course, if any.
pub async fn subscription_status(conn: &mut MySqlConnection, user_id: i64, course_id: i64) -> RepoResult<Option<String>> {
    Ok(sqlx::query_scalar("SELECT CAST(Status AS CHAR) FROM CourseSubscription WHERE UserId = ? AND CourseId = ?")
        .bind(user_id)
        .bind(course_id)
        .fetch_optional(conn)
        .await?)
}

/// Current version id + MC option PublicIds (stored order) for the questions being put on a mock.
pub async fn current_versions(conn: &mut MySqlConnection, question_ids: &[i64]) -> RepoResult<Vec<(i64, i64)>> {
    if question_ids.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!("SELECT Id, CurrentVersionId FROM Question WHERE Id IN ({})", vec!["?"; question_ids.len()].join(","));
    let mut q = sqlx::query_as(&sql);
    for id in question_ids {
        q = q.bind(id);
    }
    Ok(q.fetch_all(conn).await?)
}

// ---------- mocks ----------

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct MockRow {
    pub id: i64,
    pub public_id: String,
    pub user_id: i64,
    pub course_id: i64,
    pub course_ref: String,
    pub deadline_id: Option<i64>,
    pub name: String,
    pub time_limit_min: Option<i32>,
    pub build_options: Option<Json<serde_json::Value>>,
    pub seed: Option<i64>,
    pub grading_mode: String,
    pub status: String,
    pub reveal_all: bool,
    pub score: Option<f64>,
    pub max_score: Option<f64>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub row_version: i32,
}

const M_SELECT: &str = "SELECT m.Id, m.PublicId, m.UserId, m.CourseId, c.PublicId AS CourseRef, m.DeadlineId, m.Name, m.TimeLimitMin, m.BuildOptions, \
    m.Seed, CAST(m.GradingMode AS CHAR) AS GradingMode, CAST(m.Status AS CHAR) AS Status, m.RevealAll, CAST(m.Score AS DOUBLE) AS Score, \
    CAST(m.MaxScore AS DOUBLE) AS MaxScore, m.CreatedAt, m.StartedAt, m.FinishedAt, m.RowVersion FROM MockExam m JOIN Course c ON c.Id = m.CourseId";

/// The actor's own mock (spec 4.2 can_open_mock: `m.user == actor`); others' are never returned.
pub async fn mock_for_user(conn: &mut MySqlConnection, user_id: i64, public_id: &str, lock: bool) -> RepoResult<Option<MockRow>> {
    let sql = format!("{M_SELECT} WHERE m.PublicId = ? AND m.UserId = ?{}", if lock { " FOR UPDATE OF m" } else { "" });
    Ok(sqlx::query_as(&sql).bind(public_id).bind(user_id).fetch_optional(conn).await?)
}

pub async fn mock_by_id(conn: &mut MySqlConnection, id: i64) -> RepoResult<Option<MockRow>> {
    Ok(sqlx::query_as(&format!("{M_SELECT} WHERE m.Id = ?")).bind(id).fetch_optional(conn).await?)
}

/// A deadline of the user's subscription to the course, by PublicId.
pub async fn deadline_id_for(conn: &mut MySqlConnection, user_id: i64, course_id: i64, deadline_ref: &str) -> RepoResult<Option<i64>> {
    Ok(sqlx::query_scalar(
        "SELECT d.Id FROM Deadline d JOIN CourseSubscription s ON s.Id = d.CourseSubscriptionId WHERE d.PublicId = ? AND s.UserId = ? AND s.CourseId = ?",
    )
    .bind(deadline_ref)
    .bind(user_id)
    .bind(course_id)
    .fetch_optional(conn)
    .await?)
}

pub async fn mocks_for_course(conn: &mut MySqlConnection, user_id: i64, course_id: i64) -> RepoResult<Vec<MockRow>> {
    Ok(sqlx::query_as(&format!("{M_SELECT} WHERE m.UserId = ? AND m.CourseId = ? ORDER BY m.CreatedAt DESC, m.Id DESC"))
        .bind(user_id)
        .bind(course_id)
        .fetch_all(conn)
        .await?)
}

pub struct NewMock<'a> {
    pub public_id: &'a str,
    pub user_id: i64,
    pub course_id: i64,
    pub deadline_id: Option<i64>,
    pub name: &'a str,
    pub time_limit_min: Option<i32>,
    pub build_options: serde_json::Value,
    pub seed: i64,
    pub grading_mode: &'a str,
}

pub async fn insert_mock(conn: &mut MySqlConnection, m: &NewMock<'_>) -> RepoResult<i64> {
    let res = sqlx::query(
        "INSERT INTO MockExam (PublicId, UserId, CourseId, DeadlineId, Name, TimeLimitMin, BuildOptions, Seed, GradingMode) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(m.public_id)
    .bind(m.user_id)
    .bind(m.course_id)
    .bind(m.deadline_id)
    .bind(m.name)
    .bind(m.time_limit_min)
    .bind(Json(&m.build_options))
    .bind(m.seed)
    .bind(m.grading_mode)
    .execute(conn)
    .await?;
    Ok(res.last_insert_id() as i64)
}

pub async fn insert_item(
    conn: &mut MySqlConnection,
    mock_id: i64,
    order: i32,
    question_id: i64,
    version_id: i64,
    option_order: Option<serde_json::Value>,
) -> RepoResult<()> {
    sqlx::query("INSERT INTO MockQuestion (MockExamId, QuestionOrder, QuestionId, QuestionVersionId, OptionOrder) VALUES (?, ?, ?, ?, ?)")
        .bind(mock_id)
        .bind(order)
        .bind(question_id)
        .bind(version_id)
        .bind(option_order.map(Json))
        .execute(conn)
        .await?;
    Ok(())
}

/// The only writer of MockExam.Status (after crux_domain::mock::transition).
pub async fn set_status(
    conn: &mut MySqlConnection,
    mock_id: i64,
    status: &str,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
) -> RepoResult<()> {
    sqlx::query(
        "UPDATE MockExam SET Status = ?, StartedAt = COALESCE(?, StartedAt), FinishedAt = COALESCE(?, FinishedAt), RowVersion = RowVersion + 1 WHERE Id = ?",
    )
    .bind(status)
    .bind(started_at)
    .bind(finished_at)
    .bind(mock_id)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn set_score(conn: &mut MySqlConnection, mock_id: i64, score: f64, max: f64) -> RepoResult<()> {
    sqlx::query("UPDATE MockExam SET Score = ?, MaxScore = ? WHERE Id = ?").bind(score).bind(max).bind(mock_id).execute(conn).await?;
    Ok(())
}

pub async fn set_reveal_all(conn: &mut MySqlConnection, mock_id: i64) -> RepoResult<()> {
    sqlx::query("UPDATE MockExam SET RevealAll = TRUE WHERE Id = ?").bind(mock_id).execute(conn).await?;
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct ItemRow {
    pub mock_exam_id: i64,
    pub question_order: i32,
    pub question_id: i64,
    pub question_ref: String,
    pub qtype: String,
    pub component_id: i64,
    pub question_version_id: i64,
    pub option_order: Option<Json<serde_json::Value>>,
    pub max_grade: f64,
    pub chosen_option_id: Option<i64>,
    pub answer_text: Option<String>,
    pub answer_numeric: Option<f64>,
    pub answer_unit: Option<String>,
    pub confidence: Option<String>,
    pub flagged: bool,
    pub answered_at: Option<DateTime<Utc>>,
    pub outcome: Option<String>,
    pub grade: Option<f64>,
    pub graded_by: Option<String>,
    pub accepted_ai_grade_ref: Option<String>,
    pub revealed: bool,
    pub self_shown: bool,
    pub fixed_in_review: bool,
}

pub async fn items(conn: &mut MySqlConnection, mock_id: i64) -> RepoResult<Vec<ItemRow>> {
    Ok(sqlx::query_as(
        "SELECT mq.MockExamId, mq.QuestionOrder, mq.QuestionId, q.PublicId AS QuestionRef, CAST(q.Type AS CHAR) AS Qtype, q.CourseComponentId AS ComponentId, \
         mq.QuestionVersionId, mq.OptionOrder, CAST(mq.MaxGrade AS DOUBLE) AS MaxGrade, mq.ChosenOptionId, mq.AnswerText, mq.AnswerNumeric, mq.AnswerUnit, \
         CAST(mq.Confidence AS CHAR) AS Confidence, mq.Flagged, mq.AnsweredAt, CAST(mq.Outcome AS CHAR) AS Outcome, CAST(mq.Grade AS DOUBLE) AS Grade, \
         CAST(mq.GradedBy AS CHAR) AS GradedBy, mq.AcceptedAiGradeRef, mq.Revealed, mq.SelfShown, mq.FixedInReview \
         FROM MockQuestion mq JOIN Question q ON q.Id = mq.QuestionId WHERE mq.MockExamId = ? ORDER BY mq.QuestionOrder",
    )
    .bind(mock_id)
    .fetch_all(conn)
    .await?)
}

pub struct Answer<'a> {
    pub chosen_option_id: Option<i64>,
    pub text: Option<&'a str>,
    pub numeric: Option<f64>,
    pub unit: Option<&'a str>,
    pub confidence: Option<&'a str>,
    pub flagged: bool,
    pub at: DateTime<Utc>,
}

pub async fn save_answer(conn: &mut MySqlConnection, mock_id: i64, order: i32, a: &Answer<'_>) -> RepoResult<bool> {
    let res = sqlx::query(
        "UPDATE MockQuestion SET ChosenOptionId = ?, AnswerText = ?, AnswerNumeric = ?, AnswerUnit = ?, Confidence = ?, Flagged = ?, AnsweredAt = ? \
         WHERE MockExamId = ? AND QuestionOrder = ?",
    )
    .bind(a.chosen_option_id)
    .bind(a.text)
    .bind(a.numeric)
    .bind(a.unit)
    .bind(a.confidence)
    .bind(a.flagged)
    .bind(a.at)
    .bind(mock_id)
    .bind(order)
    .execute(conn)
    .await?;
    Ok(res.rows_affected() == 1)
}

#[allow(clippy::too_many_arguments)]
pub async fn set_grade(
    conn: &mut MySqlConnection,
    mock_id: i64,
    order: i32,
    outcome: &str,
    grade: f64,
    by: &str,
    ai_grade_ref: Option<&str>,
    revealed: bool,
) -> RepoResult<()> {
    sqlx::query(
        "UPDATE MockQuestion SET Outcome = ?, Grade = ?, GradedBy = ?, AcceptedAiGradeRef = ?, Revealed = (Revealed OR ?) WHERE MockExamId = ? AND QuestionOrder = ?",
    )
    .bind(outcome)
    .bind(grade)
    .bind(by)
    .bind(ai_grade_ref)
    .bind(revealed)
    .bind(mock_id)
    .bind(order)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn set_review_flag(conn: &mut MySqlConnection, mock_id: i64, order: i32, self_shown: bool, fixed: bool) -> RepoResult<bool> {
    let res = sqlx::query("UPDATE MockQuestion SET SelfShown = (SelfShown OR ?), FixedInReview = (FixedInReview OR ?) WHERE MockExamId = ? AND QuestionOrder = ?")
        .bind(self_shown)
        .bind(fixed)
        .bind(mock_id)
        .bind(order)
        .execute(conn)
        .await?;
    Ok(res.rows_affected() == 1)
}

/// MC option internal id for a PublicId within a version.
pub async fn option_id(conn: &mut MySqlConnection, version_id: i64, public_id: &str) -> RepoResult<Option<(i64, bool)>> {
    Ok(sqlx::query_as("SELECT Id, IsCorrect FROM MCOption WHERE QuestionVersionId = ? AND PublicId = ?")
        .bind(version_id)
        .bind(public_id)
        .fetch_optional(conn)
        .await?)
}

pub async fn option_correct(conn: &mut MySqlConnection, option_id: i64) -> RepoResult<Option<bool>> {
    Ok(sqlx::query_scalar("SELECT IsCorrect FROM MCOption WHERE Id = ?").bind(option_id).fetch_optional(conn).await?)
}

pub async fn option_public_id(conn: &mut MySqlConnection, option_id: i64) -> RepoResult<Option<String>> {
    Ok(sqlx::query_scalar("SELECT PublicId FROM MCOption WHERE Id = ?").bind(option_id).fetch_optional(conn).await?.flatten())
}

// ---------- stats ----------

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct StatRow {
    pub attempts: i32,
    pub rights: i32,
    pub sure_wrongs: i32,
    pub last_outcome: Option<String>,
    pub last_confidence: Option<String>,
    pub last_answered_at: Option<DateTime<Utc>>,
    pub first_right_at: Option<DateTime<Utc>>,
}

pub async fn stat_for_update(conn: &mut MySqlConnection, user_id: i64, question_id: i64) -> RepoResult<Option<StatRow>> {
    Ok(sqlx::query_as(
        "SELECT Attempts, Rights, SureWrongs, CAST(LastOutcome AS CHAR) AS LastOutcome, CAST(LastConfidence AS CHAR) AS LastConfidence, \
         LastAnsweredAt, FirstRightAt FROM UserQuestionStat WHERE UserId = ? AND QuestionId = ? FOR UPDATE",
    )
    .bind(user_id)
    .bind(question_id)
    .fetch_optional(conn)
    .await?)
}

pub async fn upsert_stat(conn: &mut MySqlConnection, user_id: i64, question_id: i64, s: &StatRow) -> RepoResult<()> {
    sqlx::query(
        "INSERT INTO UserQuestionStat (UserId, QuestionId, Attempts, Rights, SureWrongs, LastOutcome, LastConfidence, LastAnsweredAt, FirstRightAt) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) ON DUPLICATE KEY UPDATE Attempts = VALUES(Attempts), Rights = VALUES(Rights), SureWrongs = VALUES(SureWrongs), \
         LastOutcome = VALUES(LastOutcome), LastConfidence = VALUES(LastConfidence), LastAnsweredAt = VALUES(LastAnsweredAt), FirstRightAt = VALUES(FirstRightAt)",
    )
    .bind(user_id)
    .bind(question_id)
    .bind(s.attempts)
    .bind(s.rights)
    .bind(s.sure_wrongs)
    .bind(&s.last_outcome)
    .bind(&s.last_confidence)
    .bind(s.last_answered_at)
    .bind(s.first_right_at)
    .execute(conn)
    .await?;
    Ok(())
}

/// Was the lecture studied (any answer to its questions) in the window, outside this mock?
/// Stand-in for "the spot's lecture wasn't studied in the previous 24 h" until study events land
/// (milestone 5).
pub async fn studied_between(
    conn: &mut MySqlConnection,
    user_id: i64,
    component_id: i64,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    except_mock: Option<i64>,
) -> RepoResult<bool> {
    let n: i64 = sqlx::query_scalar(
        "SELECT (SELECT COUNT(*) FROM QuestionAttempt a JOIN Question q ON q.Id = a.QuestionId \
                 WHERE a.UserId = ? AND q.CourseComponentId = ? AND a.AnsweredAt >= ? AND a.AnsweredAt < ?) \
              + (SELECT COUNT(*) FROM MockQuestion mq JOIN MockExam m ON m.Id = mq.MockExamId JOIN Question q ON q.Id = mq.QuestionId \
                 WHERE m.UserId = ? AND q.CourseComponentId = ? AND mq.AnsweredAt >= ? AND mq.AnsweredAt < ? AND (? IS NULL OR m.Id <> ?))",
    )
    .bind(user_id)
    .bind(component_id)
    .bind(from)
    .bind(to)
    .bind(user_id)
    .bind(component_id)
    .bind(from)
    .bind(to)
    .bind(except_mock)
    .bind(except_mock)
    .fetch_one(conn)
    .await?;
    Ok(n > 0)
}

// ---------- attempts ----------

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct AttemptRow {
    pub id: i64,
    pub client_id: String,
    pub question_id: i64,
    pub question_version_id: i64,
    pub outcome: String,
    pub answered_at: DateTime<Utc>,
}

pub async fn attempt_by_client(conn: &mut MySqlConnection, user_id: i64, client_id: &str) -> RepoResult<Option<AttemptRow>> {
    Ok(sqlx::query_as("SELECT Id, ClientId, QuestionId, QuestionVersionId, CAST(Outcome AS CHAR) AS Outcome, AnsweredAt FROM QuestionAttempt WHERE UserId = ? AND ClientId = ?")
        .bind(user_id)
        .bind(client_id)
        .fetch_optional(conn)
        .await?)
}

pub struct NewAttempt<'a> {
    pub user_id: i64,
    pub client_id: &'a str,
    pub question_id: i64,
    pub version_id: i64,
    pub material_id: Option<i64>,
    pub context: &'a str,
    pub try_no: i8,
    pub chosen_option_id: Option<i64>,
    pub text: Option<&'a str>,
    pub numeric: Option<f64>,
    pub confidence: Option<&'a str>,
    pub outcome: &'a str,
    pub cold: bool,
    pub at: DateTime<Utc>,
}

pub async fn insert_attempt(conn: &mut MySqlConnection, a: &NewAttempt<'_>) -> RepoResult<i64> {
    let res = sqlx::query(
        "INSERT INTO QuestionAttempt (UserId, ClientId, QuestionId, QuestionVersionId, MaterialId, Context, TryNo, ChosenOptionId, AnswerText, AnswerNumeric, \
         Confidence, Outcome, Cold, AnsweredAt) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(a.user_id)
    .bind(a.client_id)
    .bind(a.question_id)
    .bind(a.version_id)
    .bind(a.material_id)
    .bind(a.context)
    .bind(a.try_no)
    .bind(a.chosen_option_id)
    .bind(a.text)
    .bind(a.numeric)
    .bind(a.confidence)
    .bind(a.outcome)
    .bind(a.cold)
    .bind(a.at)
    .execute(conn)
    .await?;
    Ok(res.last_insert_id() as i64)
}

/// Per lecture, per confidence: (component id, confidence, answered, right) over graded mock items
/// and attempts (spec 5.4 confidence view).
pub async fn confidence_counts(conn: &mut MySqlConnection, user_id: i64, course_id: i64) -> RepoResult<Vec<(i64, String, i64, i64)>> {
    Ok(sqlx::query_as(
        "SELECT ComponentId, Conf, CAST(SUM(1) AS SIGNED), CAST(SUM(Outcome = 'Right') AS SIGNED) FROM ( \
           SELECT q.CourseComponentId AS ComponentId, CAST(mq.Confidence AS CHAR) AS Conf, CAST(mq.Outcome AS CHAR) AS Outcome \
             FROM MockQuestion mq JOIN MockExam m ON m.Id = mq.MockExamId JOIN Question q ON q.Id = mq.QuestionId \
             WHERE m.UserId = ? AND m.CourseId = ? AND mq.Outcome IS NOT NULL AND mq.Confidence IS NOT NULL \
           UNION ALL \
           SELECT q.CourseComponentId, CAST(a.Confidence AS CHAR), CAST(a.Outcome AS CHAR) \
             FROM QuestionAttempt a JOIN Question q ON q.Id = a.QuestionId \
             WHERE a.UserId = ? AND q.CourseId = ? AND a.Confidence IS NOT NULL) t \
         GROUP BY ComponentId, Conf ORDER BY ComponentId, Conf",
    )
    .bind(user_id)
    .bind(course_id)
    .bind(user_id)
    .bind(course_id)
    .fetch_all(conn)
    .await?)
}
