//! Flashcard, FlashcardProgress, FlashcardReview (schema section 8). Progress status columns are
//! written only after crux_domain::fsrs decided them (spec 2.3).

use chrono::{DateTime, Utc};
use sqlx::MySqlConnection;

use crate::{RepoError, RepoResult};

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct CardRow {
    pub id: i64,
    pub public_id: String,
    pub course_id: i64,
    pub course_ref: String,
    pub component_id: i64,
    pub component_ref: String,
    pub code: String,
    pub front_text: String,
    pub back_text: String,
    pub source: String,
    pub owner_user_id: Option<i64>,
    pub slip_ref: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
    pub row_version: i32,
    // The caller's progress (LEFT JOIN):
    pub state: Option<String>,
    pub stability: Option<f64>,
    pub difficulty: Option<f64>,
    pub due_at: Option<DateTime<Utc>>,
    pub last_reviewed_at: Option<DateTime<Utc>>,
    pub reps: Option<i32>,
    pub lapses: Option<i32>,
    pub suspended: Option<bool>,
    pub suspend_reason: Option<String>,
    pub warmup_flagged_at: Option<DateTime<Utc>>,
}

/// `?` placeholders: user id (progress join), then filters.
const SELECT: &str = "SELECT f.Id, f.PublicId, f.CourseId, c.PublicId AS CourseRef, f.CourseComponentId AS ComponentId, k.PublicId AS ComponentRef, \
    f.Code, f.FrontText, f.BackText, CAST(f.Source AS CHAR) AS Source, f.OwnerUserId, s.PublicId AS SlipRef, f.UpdatedAt, f.ArchivedAt, f.RowVersion, \
    CAST(p.State AS CHAR) AS State, p.Stability, p.Difficulty, p.DueAt, p.LastReviewedAt, p.Reps, p.Lapses, p.Suspended, \
    CAST(p.SuspendReason AS CHAR) AS SuspendReason, p.WarmupFlaggedAt \
    FROM Flashcard f JOIN Course c ON c.Id = f.CourseId JOIN CourseComponent k ON k.Id = f.CourseComponentId \
    LEFT JOIN Slip s ON s.Id = f.SlipId \
    LEFT JOIN FlashcardProgress p ON p.FlashcardId = f.Id AND p.UserId = ?";

/// Cards of a course the user can see: course cards + their own personal ones.
pub async fn cards(
    conn: &mut MySqlConnection,
    user_id: i64,
    course_id: i64,
    component_id: Option<i64>,
    code: Option<&str>,
) -> RepoResult<Vec<CardRow>> {
    Ok(sqlx::query_as(&format!(
        "{SELECT} WHERE f.CourseId = ? AND f.ArchivedAt IS NULL AND (f.OwnerUserId IS NULL OR f.OwnerUserId = ?) \
         AND (? IS NULL OR f.CourseComponentId = ?) AND (? IS NULL OR f.Code = ?) ORDER BY f.Code"
    ))
    .bind(user_id)
    .bind(course_id)
    .bind(user_id)
    .bind(component_id)
    .bind(component_id)
    .bind(code)
    .bind(code)
    .fetch_all(conn)
    .await?)
}

pub async fn card(conn: &mut MySqlConnection, user_id: i64, public_id: &str) -> RepoResult<Option<CardRow>> {
    Ok(sqlx::query_as(&format!("{SELECT} WHERE f.PublicId = ? AND f.ArchivedAt IS NULL"))
        .bind(user_id)
        .bind(public_id)
        .fetch_optional(conn)
        .await?)
}

/// Lookup by code across the courses the user can read (spec 5.5 "GET /flash/cards?code=").
pub async fn by_code(conn: &mut MySqlConnection, user_id: i64, code: &str) -> RepoResult<Vec<CardRow>> {
    Ok(sqlx::query_as(&format!(
        "{SELECT} LEFT JOIN CourseMember cm ON cm.CourseId = f.CourseId AND cm.UserId = ? AND cm.Status = 'Accepted' \
         WHERE f.Code = ? AND f.ArchivedAt IS NULL AND c.ArchivedAt IS NULL AND (c.OwnerUserId = ? OR cm.UserId IS NOT NULL) \
         AND (f.OwnerUserId IS NULL OR f.OwnerUserId = ?)"
    ))
    .bind(user_id)
    .bind(user_id)
    .bind(code)
    .bind(user_id)
    .bind(user_id)
    .fetch_all(conn)
    .await?)
}

/// All codes used in a course (the course row must be locked by the caller: codes are assigned in
/// the insert transaction, spec 5.5).
pub async fn codes(conn: &mut MySqlConnection, course_id: i64) -> RepoResult<Vec<String>> {
    Ok(sqlx::query_scalar("SELECT Code FROM Flashcard WHERE CourseId = ?").bind(course_id).fetch_all(conn).await?)
}

pub async fn lock_course(conn: &mut MySqlConnection, course_id: i64) -> RepoResult<()> {
    sqlx::query("SELECT Id FROM Course WHERE Id = ? FOR UPDATE").bind(course_id).execute(conn).await?;
    Ok(())
}

pub struct NewCard<'a> {
    pub public_id: &'a str,
    pub course_id: i64,
    pub component_id: i64,
    pub code: &'a str,
    pub front: &'a str,
    pub back: &'a str,
    pub source: &'a str,
    pub owner_user_id: Option<i64>,
    pub slip_id: Option<i64>,
}

pub async fn insert(conn: &mut MySqlConnection, c: &NewCard<'_>) -> RepoResult<i64> {
    let res = sqlx::query(
        "INSERT INTO Flashcard (PublicId, CourseId, CourseComponentId, Code, FrontText, BackText, Source, OwnerUserId, SlipId) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(c.public_id)
    .bind(c.course_id)
    .bind(c.component_id)
    .bind(c.code)
    .bind(c.front)
    .bind(c.back)
    .bind(c.source)
    .bind(c.owner_user_id)
    .bind(c.slip_id)
    .execute(conn)
    .await?;
    Ok(res.last_insert_id() as i64)
}

pub async fn update(conn: &mut MySqlConnection, id: i64, expected: i32, component_id: i64, front: &str, back: &str) -> RepoResult<()> {
    let res = sqlx::query("UPDATE Flashcard SET CourseComponentId = ?, FrontText = ?, BackText = ?, RowVersion = RowVersion + 1 WHERE Id = ? AND RowVersion = ?")
        .bind(component_id)
        .bind(front)
        .bind(back)
        .bind(id)
        .bind(expected)
        .execute(conn)
        .await?;
    if res.rows_affected() == 0 {
        return Err(RepoError::VersionConflict);
    }
    Ok(())
}

// ---------- progress ----------

pub async fn ensure_progress(conn: &mut MySqlConnection, user_id: i64, card_id: i64) -> RepoResult<()> {
    sqlx::query("INSERT IGNORE INTO FlashcardProgress (UserId, FlashcardId) VALUES (?, ?)")
        .bind(user_id)
        .bind(card_id)
        .execute(conn)
        .await?;
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct ProgressRow {
    pub flashcard_id: i64,
    pub state: String,
    pub stability: Option<f64>,
    pub difficulty: Option<f64>,
    pub due_at: Option<DateTime<Utc>>,
    pub last_reviewed_at: Option<DateTime<Utc>>,
    pub reps: i32,
    pub lapses: i32,
    pub suspended: bool,
}

pub async fn progress_for_update(conn: &mut MySqlConnection, user_id: i64, card_id: i64) -> RepoResult<ProgressRow> {
    ensure_progress(conn, user_id, card_id).await?;
    Ok(sqlx::query_as(
        "SELECT FlashcardId, CAST(State AS CHAR) AS State, Stability, Difficulty, DueAt, LastReviewedAt, Reps, Lapses, Suspended \
         FROM FlashcardProgress WHERE UserId = ? AND FlashcardId = ? FOR UPDATE",
    )
    .bind(user_id)
    .bind(card_id)
    .fetch_one(conn)
    .await?)
}

/// The only writer of FlashcardProgress.State (after crux_domain::fsrs::review).
#[allow(clippy::too_many_arguments)]
pub async fn set_progress(
    conn: &mut MySqlConnection,
    user_id: i64,
    card_id: i64,
    state: &str,
    stability: Option<f64>,
    difficulty: Option<f64>,
    due_at: Option<DateTime<Utc>>,
    last_reviewed_at: Option<DateTime<Utc>>,
    reps: i32,
    lapses: i32,
    params_version: &str,
) -> RepoResult<()> {
    sqlx::query(
        "UPDATE FlashcardProgress SET State = ?, Stability = ?, Difficulty = ?, DueAt = ?, LastReviewedAt = ?, Reps = ?, Lapses = ?, \
         FsrsParamsVersion = ?, RowVersion = RowVersion + 1 WHERE UserId = ? AND FlashcardId = ?",
    )
    .bind(state)
    .bind(stability)
    .bind(difficulty)
    .bind(due_at)
    .bind(last_reviewed_at)
    .bind(reps)
    .bind(lapses)
    .bind(params_version)
    .bind(user_id)
    .bind(card_id)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn set_suspended(conn: &mut MySqlConnection, user_id: i64, card_id: i64, suspended: bool) -> RepoResult<()> {
    ensure_progress(conn, user_id, card_id).await?;
    sqlx::query("UPDATE FlashcardProgress SET Suspended = ?, SuspendReason = IF(?, 'Manual', NULL) WHERE UserId = ? AND FlashcardId = ?")
        .bind(suspended)
        .bind(suspended)
        .bind(user_id)
        .bind(card_id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn set_warmup_flag(conn: &mut MySqlConnection, user_id: i64, card_id: i64, at: Option<DateTime<Utc>>) -> RepoResult<()> {
    ensure_progress(conn, user_id, card_id).await?;
    sqlx::query("UPDATE FlashcardProgress SET WarmupFlaggedAt = ? WHERE UserId = ? AND FlashcardId = ?")
        .bind(at)
        .bind(user_id)
        .bind(card_id)
        .execute(conn)
        .await?;
    Ok(())
}

/// Spec 5.5: archiving a course suspends that user's cards (CourseArchived); unarchiving lifts
/// only those, never a manual suspension.
pub async fn suspend_for_archive(conn: &mut MySqlConnection, user_id: i64, course_id: i64, archived: bool) -> RepoResult<u64> {
    if archived {
        sqlx::query(
            "INSERT IGNORE INTO FlashcardProgress (UserId, FlashcardId) SELECT ?, f.Id FROM Flashcard f \
             WHERE f.CourseId = ? AND (f.OwnerUserId IS NULL OR f.OwnerUserId = ?)",
        )
        .bind(user_id)
        .bind(course_id)
        .bind(user_id)
        .execute(&mut *conn)
        .await?;
        let res = sqlx::query(
            "UPDATE FlashcardProgress p JOIN Flashcard f ON f.Id = p.FlashcardId SET p.Suspended = TRUE, p.SuspendReason = 'CourseArchived' \
             WHERE p.UserId = ? AND f.CourseId = ? AND NOT p.Suspended",
        )
        .bind(user_id)
        .bind(course_id)
        .execute(conn)
        .await?;
        Ok(res.rows_affected())
    } else {
        let res = sqlx::query(
            "UPDATE FlashcardProgress p JOIN Flashcard f ON f.Id = p.FlashcardId SET p.Suspended = FALSE, p.SuspendReason = NULL \
             WHERE p.UserId = ? AND f.CourseId = ? AND p.SuspendReason = 'CourseArchived'",
        )
        .bind(user_id)
        .bind(course_id)
        .execute(conn)
        .await?;
        Ok(res.rows_affected())
    }
}

// ---------- queue ----------

/// Due cards (spec 5.5 queue step 1): DueAt ≤ now, not suspended, seen before, in the given
/// courses (live, subscribed Active), oldest due first.
pub async fn due(conn: &mut MySqlConnection, user_id: i64, course_ids: &[i64], now: DateTime<Utc>, limit: i64) -> RepoResult<Vec<CardRow>> {
    if course_ids.is_empty() || limit <= 0 {
        return Ok(Vec::new());
    }
    let ids = course_ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
    Ok(sqlx::query_as(&format!(
        "{SELECT} WHERE f.CourseId IN ({ids}) AND f.ArchivedAt IS NULL AND (f.OwnerUserId IS NULL OR f.OwnerUserId = ?) \
         AND p.State <> 'New' AND NOT p.Suspended AND p.DueAt <= ? ORDER BY p.DueAt LIMIT ?"
    ))
    .bind(user_id)
    .bind(user_id)
    .bind(now)
    .bind(limit)
    .fetch_all(conn)
    .await?)
}

/// New cards (never reviewed, not suspended), in code order.
pub async fn new_cards(conn: &mut MySqlConnection, user_id: i64, course_ids: &[i64], limit: i64) -> RepoResult<Vec<CardRow>> {
    if course_ids.is_empty() || limit <= 0 {
        return Ok(Vec::new());
    }
    let ids = course_ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
    Ok(sqlx::query_as(&format!(
        "{SELECT} WHERE f.CourseId IN ({ids}) AND f.ArchivedAt IS NULL AND (f.OwnerUserId IS NULL OR f.OwnerUserId = ?) \
         AND (p.FlashcardId IS NULL OR (p.State = 'New' AND NOT p.Suspended)) ORDER BY f.CourseId, f.Code LIMIT ?"
    ))
    .bind(user_id)
    .bind(user_id)
    .bind(limit)
    .fetch_all(conn)
    .await?)
}

/// New cards first reviewed since `since` (the user's local midnight).
pub async fn new_seen_since(conn: &mut MySqlConnection, user_id: i64, since: DateTime<Utc>) -> RepoResult<i64> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(DISTINCT FlashcardId) FROM FlashcardReview WHERE UserId = ? AND StateBefore = 'New' AND ReviewedAt >= ?",
    )
    .bind(user_id)
    .bind(since)
    .fetch_one(conn)
    .await?)
}

/// Spec 5.5 custom sets: `which` = slips (cards made from slips), forgotten (lapsed), unseen, all.
pub async fn custom(
    conn: &mut MySqlConnection,
    user_id: i64,
    course_id: i64,
    components: Option<&[i64]>,
    which: &str,
    limit: i64,
) -> RepoResult<Vec<CardRow>> {
    let comp = match components {
        Some(ids) if !ids.is_empty() => {
            format!("AND f.CourseComponentId IN ({})", ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(","))
        }
        _ => String::new(),
    };
    let filter = match which {
        "slips" => "AND f.Source = 'Slip'",
        "forgotten" => "AND p.Lapses > 0",
        "unseen" => "AND (p.FlashcardId IS NULL OR p.State = 'New')",
        _ => "",
    };
    Ok(sqlx::query_as(&format!(
        "{SELECT} WHERE f.CourseId = ? AND f.ArchivedAt IS NULL AND (f.OwnerUserId IS NULL OR f.OwnerUserId = ?) {comp} {filter} \
         ORDER BY COALESCE(p.Lapses, 0) DESC, p.DueAt IS NULL, p.DueAt, f.Code LIMIT ?"
    ))
    .bind(user_id)
    .bind(course_id)
    .bind(user_id)
    .bind(limit)
    .fetch_all(conn)
    .await?)
}

// ---------- reviews ----------

pub async fn review_exists(conn: &mut MySqlConnection, user_id: i64, client_id: &str) -> RepoResult<Option<i64>> {
    Ok(sqlx::query_scalar("SELECT Id FROM FlashcardReview WHERE UserId = ? AND ClientId = ?")
        .bind(user_id)
        .bind(client_id)
        .fetch_optional(conn)
        .await?)
}

pub struct NewReview<'a> {
    pub user_id: i64,
    pub client_id: &'a str,
    pub card_id: i64,
    pub reviewed_at: DateTime<Utc>,
    pub rating: u8,
    pub confidence: Option<&'a str>,
    pub state_before: &'a str,
    pub elapsed_days: Option<f64>,
    pub duration_ms: Option<i32>,
}

pub async fn insert_review(conn: &mut MySqlConnection, r: &NewReview<'_>) -> RepoResult<i64> {
    let res = sqlx::query(
        "INSERT INTO FlashcardReview (UserId, ClientId, FlashcardId, ReviewedAt, Rating, Confidence, StateBefore, ElapsedDays, DurationMs) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(r.user_id)
    .bind(r.client_id)
    .bind(r.card_id)
    .bind(r.reviewed_at)
    .bind(r.rating)
    .bind(r.confidence)
    .bind(r.state_before)
    .bind(r.elapsed_days)
    .bind(r.duration_ms)
    .execute(conn)
    .await?;
    Ok(res.last_insert_id() as i64)
}

/// The next upcoming exam of the user for a course (exam cap, spec 5.5).
pub async fn next_exam(conn: &mut MySqlConnection, user_id: i64, course_id: i64, now: DateTime<Utc>) -> RepoResult<Option<DateTime<Utc>>> {
    Ok(sqlx::query_scalar(
        "SELECT MIN(d.StartsAt) FROM Deadline d JOIN CourseSubscription s ON s.Id = d.CourseSubscriptionId \
         WHERE s.UserId = ? AND s.CourseId = ? AND d.Status = 'Upcoming' AND d.Type IN ('Exam','Resit') AND d.StartsAt > ?",
    )
    .bind(user_id)
    .bind(course_id)
    .bind(now)
    .fetch_one(conn)
    .await?)
}

/// Courses the user studies flashcards in: live, subscription Active.
pub async fn active_course_ids(conn: &mut MySqlConnection, user_id: i64) -> RepoResult<Vec<i64>> {
    Ok(sqlx::query_scalar(
        "SELECT s.CourseId FROM CourseSubscription s JOIN Course c ON c.Id = s.CourseId WHERE s.UserId = ? AND s.Status = 'Active' AND c.ArchivedAt IS NULL",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await?)
}

/// (review id, course id, component id) of one of the user's reviews (slip source, spec 5.5).
pub async fn review_by_client(conn: &mut MySqlConnection, user_id: i64, client_id: &str) -> RepoResult<Option<(i64, i64, i64)>> {
    Ok(sqlx::query_as(
        "SELECT r.Id, f.CourseId, f.CourseComponentId FROM FlashcardReview r JOIN Flashcard f ON f.Id = r.FlashcardId WHERE r.UserId = ? AND r.ClientId = ?",
    )
    .bind(user_id)
    .bind(client_id)
    .fetch_optional(conn)
    .await?)
}
