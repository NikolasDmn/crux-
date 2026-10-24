//! Question, QuestionVersion (immutable), MCQuestion/MCOption, NumericQuestion, OpenQuestion,
//! QuestionConcept. Visibility (spec 4.2 can_see_question) is applied in SQL.

use chrono::{DateTime, Utc};
use sqlx::types::Json;
use sqlx::MySqlConnection;

use crate::{RepoError, RepoResult};

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct QuestionRow {
    pub id: i64,
    pub public_id: String,
    pub course_id: i64,
    pub component_id: i64,
    pub component_ref: String,
    pub qtype: String,
    pub owner_user_id: Option<i64>,
    pub source: String,
    pub source_label: Option<String>,
    pub paper_id: Option<i64>,
    pub paper_ref: Option<String>,
    pub paper_question_no: Option<String>,
    pub family_id: Option<i64>,
    pub current_version_id: Option<i64>,
    pub review_state: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
    pub row_version: i32,
}

const Q_SELECT: &str = "SELECT q.Id, q.PublicId, q.CourseId, q.CourseComponentId AS ComponentId, k.PublicId AS ComponentRef, \
    CAST(q.Type AS CHAR) AS Qtype, q.OwnerUserId, CAST(q.Source AS CHAR) AS Source, q.SourceLabel, q.PaperId, p.PublicId AS PaperRef, \
    q.PaperQuestionNo, q.FamilyId, q.CurrentVersionId, CAST(q.ReviewState AS CHAR) AS ReviewState, q.CreatedAt, q.UpdatedAt, \
    q.ArchivedAt, q.RowVersion FROM Question q JOIN CourseComponent k ON k.Id = q.CourseComponentId LEFT JOIN Paper p ON p.Id = q.PaperId";

pub async fn question(conn: &mut MySqlConnection, public_id: &str) -> RepoResult<Option<QuestionRow>> {
    Ok(sqlx::query_as(&format!("{Q_SELECT} WHERE q.PublicId = ?")).bind(public_id).fetch_optional(conn).await?)
}

pub async fn question_by_id(conn: &mut MySqlConnection, id: i64) -> RepoResult<Option<QuestionRow>> {
    Ok(sqlx::query_as(&format!("{Q_SELECT} WHERE q.Id = ?")).bind(id).fetch_optional(conn).await?)
}

#[derive(Debug, Clone, Default)]
pub struct BankFilter {
    pub component_id: Option<i64>,
    pub qtype: Option<String>,
    pub source: Option<String>,
    pub paper_id: Option<i64>,
    pub key_reliability: Option<String>,
    pub review_state: Option<String>,
    /// Never answered right by the actor (`UserQuestionStat.FirstRightAt IS NULL`).
    pub untested_only: bool,
    pub search: Option<String>,
    pub include_archived: bool,
}

/// The course's questions the actor can see (spec 4.2): bank questions Approved (or any review
/// state for editors), plus the actor's own private questions.
pub async fn list(
    conn: &mut MySqlConnection,
    course_id: i64,
    actor: i64,
    editor: bool,
    f: &BankFilter,
    after_id: Option<i64>,
    limit: u32,
) -> RepoResult<Vec<QuestionRow>> {
    let like = f.search.as_deref().map(|q| format!("%{}%", q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")));
    let sql = format!(
        "{Q_SELECT} JOIN QuestionVersion v ON v.Id = q.CurrentVersionId \
         LEFT JOIN UserQuestionStat s ON s.QuestionId = q.Id AND s.UserId = ? \
         WHERE q.CourseId = ? \
           AND ((q.OwnerUserId IS NULL AND (q.ReviewState = 'Approved' OR ?)) OR q.OwnerUserId = ?) \
           AND (? OR q.ArchivedAt IS NULL) \
           AND (? IS NULL OR q.CourseComponentId = ?) \
           AND (? IS NULL OR q.Type = ?) \
           AND (? IS NULL OR q.Source = ?) \
           AND (? IS NULL OR q.PaperId = ?) \
           AND (? IS NULL OR v.KeyReliability = ?) \
           AND (? IS NULL OR q.ReviewState = ?) \
           AND (NOT ? OR s.FirstRightAt IS NULL) \
           AND (? IS NULL OR v.Prompt LIKE ?) \
           AND (? IS NULL OR q.Id > ?) \
         ORDER BY q.Id LIMIT ?"
    );
    Ok(sqlx::query_as(&sql)
        .bind(actor)
        .bind(course_id)
        .bind(editor)
        .bind(actor)
        .bind(f.include_archived)
        .bind(f.component_id)
        .bind(f.component_id)
        .bind(&f.qtype)
        .bind(&f.qtype)
        .bind(&f.source)
        .bind(&f.source)
        .bind(f.paper_id)
        .bind(f.paper_id)
        .bind(&f.key_reliability)
        .bind(&f.key_reliability)
        .bind(&f.review_state)
        .bind(&f.review_state)
        .bind(f.untested_only)
        .bind(&like)
        .bind(&like)
        .bind(after_id)
        .bind(after_id)
        .bind(limit)
        .fetch_all(conn)
        .await?)
}

pub struct NewQuestion<'a> {
    pub public_id: &'a str,
    pub course_id: i64,
    pub component_id: i64,
    pub qtype: &'a str,
    pub owner_user_id: Option<i64>,
    pub source: &'a str,
    pub source_label: Option<&'a str>,
    pub paper_id: Option<i64>,
    pub paper_question_no: Option<&'a str>,
    pub family_id: Option<i64>,
    pub review_state: &'a str,
    pub created_by: i64,
}

pub async fn insert_question(conn: &mut MySqlConnection, q: &NewQuestion<'_>) -> RepoResult<i64> {
    let res = sqlx::query(
        "INSERT INTO Question (PublicId, CourseId, CourseComponentId, Type, OwnerUserId, Source, SourceLabel, PaperId, PaperQuestionNo, \
         FamilyId, ReviewState, CreatedBy) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(q.public_id)
    .bind(q.course_id)
    .bind(q.component_id)
    .bind(q.qtype)
    .bind(q.owner_user_id)
    .bind(q.source)
    .bind(q.source_label)
    .bind(q.paper_id)
    .bind(q.paper_question_no)
    .bind(q.family_id)
    .bind(q.review_state)
    .bind(q.created_by)
    .execute(conn)
    .await?;
    Ok(res.last_insert_id() as i64)
}

pub struct QuestionMeta<'a> {
    pub component_id: i64,
    pub source_label: Option<&'a str>,
    pub paper_id: Option<i64>,
    pub paper_question_no: Option<&'a str>,
    pub family_id: Option<i64>,
}

/// Points the question at a new current version and bumps RowVersion (If-Match guarded).
pub async fn update_question(
    conn: &mut MySqlConnection,
    id: i64,
    expected: i32,
    current_version: i64,
    m: &QuestionMeta<'_>,
) -> RepoResult<()> {
    let res = sqlx::query(
        "UPDATE Question SET CurrentVersionId = ?, CourseComponentId = ?, SourceLabel = ?, PaperId = ?, PaperQuestionNo = ?, FamilyId = ?, \
         RowVersion = RowVersion + 1 WHERE Id = ? AND RowVersion = ?",
    )
    .bind(current_version)
    .bind(m.component_id)
    .bind(m.source_label)
    .bind(m.paper_id)
    .bind(m.paper_question_no)
    .bind(m.family_id)
    .bind(id)
    .bind(expected)
    .execute(conn)
    .await?;
    if res.rows_affected() == 0 {
        return Err(RepoError::VersionConflict);
    }
    Ok(())
}

pub async fn set_current_version(conn: &mut MySqlConnection, id: i64, version_id: i64) -> RepoResult<()> {
    sqlx::query("UPDATE Question SET CurrentVersionId = ? WHERE Id = ?").bind(version_id).bind(id).execute(conn).await?;
    Ok(())
}

pub async fn set_review_state(conn: &mut MySqlConnection, id: i64, state: &str) -> RepoResult<()> {
    sqlx::query("UPDATE Question SET ReviewState = ?, RowVersion = RowVersion + 1 WHERE Id = ?").bind(state).bind(id).execute(conn).await?;
    Ok(())
}

pub async fn set_owner(conn: &mut MySqlConnection, id: i64, owner: Option<i64>) -> RepoResult<()> {
    sqlx::query("UPDATE Question SET OwnerUserId = ?, RowVersion = RowVersion + 1 WHERE Id = ?").bind(owner).bind(id).execute(conn).await?;
    Ok(())
}

pub async fn archive(conn: &mut MySqlConnection, id: i64, at: DateTime<Utc>) -> RepoResult<()> {
    sqlx::query("UPDATE Question SET ArchivedAt = COALESCE(ArchivedAt, ?), RowVersion = RowVersion + 1 WHERE Id = ?")
        .bind(at)
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

// ---------- versions ----------

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct VersionRow {
    pub id: i64,
    pub public_id: Option<String>,
    pub question_id: i64,
    pub version_no: i32,
    pub prompt: String,
    pub image_file_ref: Option<String>,
    pub explanation: Option<String>,
    pub key_reliability: String,
    pub key_note: Option<String>,
    pub change_note: Option<String>,
    pub created_at: DateTime<Utc>,
}

const V_SELECT: &str =
    "SELECT Id, PublicId, QuestionId, VersionNo, Prompt, ImageFileRef, Explanation, CAST(KeyReliability AS CHAR) AS KeyReliability, \
    KeyNote, ChangeNote, CreatedAt FROM QuestionVersion";

pub async fn version(conn: &mut MySqlConnection, id: i64) -> RepoResult<Option<VersionRow>> {
    Ok(sqlx::query_as(&format!("{V_SELECT} WHERE Id = ?")).bind(id).fetch_optional(conn).await?)
}

pub async fn version_by_public_id(conn: &mut MySqlConnection, public_id: &str) -> RepoResult<Option<VersionRow>> {
    Ok(sqlx::query_as(&format!("{V_SELECT} WHERE PublicId = ?")).bind(public_id).fetch_optional(conn).await?)
}

pub async fn versions(conn: &mut MySqlConnection, question_id: i64) -> RepoResult<Vec<VersionRow>> {
    Ok(sqlx::query_as(&format!("{V_SELECT} WHERE QuestionId = ? ORDER BY VersionNo DESC")).bind(question_id).fetch_all(conn).await?)
}

pub async fn versions_by_ids(conn: &mut MySqlConnection, ids: &[i64]) -> RepoResult<Vec<VersionRow>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!("{V_SELECT} WHERE Id IN ({})", vec!["?"; ids.len()].join(","));
    let mut q = sqlx::query_as(&sql);
    for id in ids {
        q = q.bind(id);
    }
    Ok(q.fetch_all(conn).await?)
}

pub struct NewVersion<'a> {
    pub public_id: &'a str,
    pub question_id: i64,
    pub qtype: &'a str,
    pub prompt: &'a str,
    pub image_file_ref: Option<&'a str>,
    pub explanation: Option<&'a str>,
    pub key_reliability: &'a str,
    pub key_note: Option<&'a str>,
    pub change_note: Option<&'a str>,
    pub created_by: i64,
}

/// Inserts VersionNo = max + 1 (versions are immutable; review rule 6).
pub async fn insert_version(conn: &mut MySqlConnection, v: &NewVersion<'_>) -> RepoResult<i64> {
    let next: i32 = sqlx::query_scalar("SELECT CAST(COALESCE(MAX(VersionNo), 0) + 1 AS SIGNED) FROM QuestionVersion WHERE QuestionId = ?")
        .bind(v.question_id)
        .fetch_one(&mut *conn)
        .await
        .map(|n: i64| n as i32)?;
    let res = sqlx::query(
        "INSERT INTO QuestionVersion (PublicId, QuestionId, Type, VersionNo, Prompt, ImageFileRef, Explanation, KeyReliability, KeyNote, ChangeNote, CreatedBy) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(v.public_id)
    .bind(v.question_id)
    .bind(v.qtype)
    .bind(next)
    .bind(v.prompt)
    .bind(v.image_file_ref)
    .bind(v.explanation)
    .bind(v.key_reliability)
    .bind(v.key_note)
    .bind(v.change_note)
    .bind(v.created_by)
    .execute(conn)
    .await?;
    Ok(res.last_insert_id() as i64)
}

// ---------- typed keys ----------

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct OptionRow {
    pub id: i64,
    pub public_id: Option<String>,
    pub question_version_id: i64,
    pub position: i8,
    pub text: String,
    pub is_correct: bool,
    pub distractor_note: Option<String>,
}

pub async fn insert_mc(
    conn: &mut MySqlConnection,
    version_id: i64,
    shuffle: bool,
    options: &[(String, &str, bool, Option<&str>)],
) -> RepoResult<()> {
    sqlx::query("INSERT INTO MCQuestion (QuestionVersionId, ShuffleOptions) VALUES (?, ?)")
        .bind(version_id)
        .bind(shuffle)
        .execute(&mut *conn)
        .await?;
    for (pos, (public_id, text, correct, note)) in options.iter().enumerate() {
        sqlx::query(
            "INSERT INTO MCOption (PublicId, QuestionVersionId, Position, Text, IsCorrect, DistractorNote) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(public_id)
        .bind(version_id)
        .bind(pos as i8)
        .bind(text)
        .bind(correct)
        .bind(note)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

pub async fn options(conn: &mut MySqlConnection, version_ids: &[i64]) -> RepoResult<Vec<OptionRow>> {
    if version_ids.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT Id, PublicId, QuestionVersionId, Position, Text, IsCorrect, DistractorNote FROM MCOption WHERE QuestionVersionId IN ({}) \
         ORDER BY QuestionVersionId, Position",
        vec!["?"; version_ids.len()].join(",")
    );
    let mut q = sqlx::query_as(&sql);
    for id in version_ids {
        q = q.bind(id);
    }
    Ok(q.fetch_all(conn).await?)
}

pub async fn shuffle_flag(conn: &mut MySqlConnection, version_id: i64) -> RepoResult<bool> {
    Ok(sqlx::query_scalar("SELECT ShuffleOptions FROM MCQuestion WHERE QuestionVersionId = ?")
        .bind(version_id)
        .fetch_optional(conn)
        .await?
        .unwrap_or(true))
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct NumericRow {
    pub question_version_id: i64,
    pub value: f64,
    pub tolerance: f64,
    pub tolerance_kind: String,
    pub unit: Option<String>,
    pub unit_required: bool,
    pub model_answer: Option<String>,
}

pub async fn insert_numeric(conn: &mut MySqlConnection, version_id: i64, n: &NumericRow) -> RepoResult<()> {
    sqlx::query("INSERT INTO NumericQuestion (QuestionVersionId, Value, Tolerance, ToleranceKind, Unit, UnitRequired, ModelAnswer) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind(version_id)
        .bind(n.value)
        .bind(n.tolerance)
        .bind(&n.tolerance_kind)
        .bind(&n.unit)
        .bind(n.unit_required)
        .bind(&n.model_answer)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn numeric_keys(conn: &mut MySqlConnection, version_ids: &[i64]) -> RepoResult<Vec<NumericRow>> {
    if version_ids.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT QuestionVersionId, Value, Tolerance, CAST(ToleranceKind AS CHAR) AS ToleranceKind, Unit, UnitRequired, ModelAnswer \
         FROM NumericQuestion WHERE QuestionVersionId IN ({})",
        vec!["?"; version_ids.len()].join(",")
    );
    let mut q = sqlx::query_as(&sql);
    for id in version_ids {
        q = q.bind(id);
    }
    Ok(q.fetch_all(conn).await?)
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct OpenRow {
    pub question_version_id: i64,
    pub model_answer: Option<String>,
    pub rubric_note: Option<String>,
    pub rubric_json: Option<Json<serde_json::Value>>,
}

pub async fn insert_open(
    conn: &mut MySqlConnection,
    version_id: i64,
    model: Option<&str>,
    note: Option<&str>,
    rubric: Option<serde_json::Value>,
) -> RepoResult<()> {
    sqlx::query("INSERT INTO OpenQuestion (QuestionVersionId, ModelAnswer, RubricNote, RubricJson) VALUES (?, ?, ?, ?)")
        .bind(version_id)
        .bind(model)
        .bind(note)
        .bind(rubric.map(Json))
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn open_keys(conn: &mut MySqlConnection, version_ids: &[i64]) -> RepoResult<Vec<OpenRow>> {
    if version_ids.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT QuestionVersionId, ModelAnswer, RubricNote, RubricJson FROM OpenQuestion WHERE QuestionVersionId IN ({})",
        vec!["?"; version_ids.len()].join(",")
    );
    let mut q = sqlx::query_as(&sql);
    for id in version_ids {
        q = q.bind(id);
    }
    Ok(q.fetch_all(conn).await?)
}

// ---------- concepts ----------

/// (QuestionId, concept PublicId, IsPrimary) for the given questions.
pub async fn question_concepts(conn: &mut MySqlConnection, question_ids: &[i64]) -> RepoResult<Vec<(i64, Option<String>, i64, bool)>> {
    if question_ids.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT qc.QuestionId, c.PublicId, c.Id, qc.IsPrimary FROM QuestionConcept qc JOIN Concept c ON c.Id = qc.ConceptId \
         WHERE qc.QuestionId IN ({}) ORDER BY qc.IsPrimary DESC, c.Name",
        vec!["?"; question_ids.len()].join(",")
    );
    let mut q = sqlx::query_as(&sql);
    for id in question_ids {
        q = q.bind(id);
    }
    Ok(q.fetch_all(conn).await?)
}

pub async fn set_concepts(conn: &mut MySqlConnection, question_id: i64, concepts: &[(i64, bool)]) -> RepoResult<()> {
    sqlx::query("DELETE FROM QuestionConcept WHERE QuestionId = ?").bind(question_id).execute(&mut *conn).await?;
    for (cid, primary) in concepts {
        sqlx::query("INSERT INTO QuestionConcept (QuestionId, ConceptId, IsPrimary) VALUES (?, ?, ?)")
            .bind(question_id)
            .bind(cid)
            .bind(primary)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

/// Concept ids by PublicId within the course (review rule 3: concept in the question's course).
pub async fn concept_ids(conn: &mut MySqlConnection, course_id: i64, refs: &[String]) -> RepoResult<Vec<(i64, String)>> {
    if refs.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!("SELECT Id, PublicId FROM Concept WHERE CourseId = ? AND PublicId IN ({})", vec!["?"; refs.len()].join(","));
    let mut q = sqlx::query_as(&sql).bind(course_id);
    for r in refs {
        q = q.bind(r);
    }
    Ok(q.fetch_all(conn).await?)
}

pub async fn paper_id(conn: &mut MySqlConnection, course_id: i64, paper_ref: &str) -> RepoResult<Option<i64>> {
    Ok(sqlx::query_scalar("SELECT Id FROM Paper WHERE CourseId = ? AND PublicId = ?")
        .bind(course_id)
        .bind(paper_ref)
        .fetch_optional(conn)
        .await?)
}

/// The family root of a question in the same course (review rule 3).
pub async fn family_root(conn: &mut MySqlConnection, course_id: i64, question_ref: &str) -> RepoResult<Option<i64>> {
    Ok(sqlx::query_scalar("SELECT COALESCE(FamilyId, Id) FROM Question WHERE CourseId = ? AND PublicId = ?")
        .bind(course_id)
        .bind(question_ref)
        .fetch_optional(conn)
        .await?)
}

/// Generated questions remember the question they were seeded from (spec 7.5).
pub async fn set_seed(conn: &mut MySqlConnection, question_id: i64, seed_id: i64) -> RepoResult<()> {
    sqlx::query("UPDATE Question SET SeedQuestionId = ? WHERE Id = ?").bind(seed_id).bind(question_id).execute(conn).await?;
    Ok(())
}

/// (prompt, type) of questions in a course the user can see, newest first (generation dedupe/style).
/// `(PublicId, prompt)` of the course questions the user can see, newest first (import duplicates).
pub async fn prompts_with_ids(conn: &mut MySqlConnection, user_id: i64, course_id: i64, limit: i64) -> RepoResult<Vec<(String, String)>> {
    Ok(sqlx::query_as(
        "SELECT q.PublicId, v.Prompt FROM Question q JOIN QuestionVersion v ON v.Id = q.CurrentVersionId          WHERE q.CourseId = ? AND q.ArchivedAt IS NULL AND (q.OwnerUserId IS NULL OR q.OwnerUserId = ?) ORDER BY q.Id DESC LIMIT ?",
    )
    .bind(course_id)
    .bind(user_id)
    .bind(limit)
    .fetch_all(conn)
    .await?)
}

pub async fn prompts_for(
    conn: &mut MySqlConnection,
    user_id: i64,
    course_id: i64,
    component_id: Option<i64>,
    limit: i64,
) -> RepoResult<Vec<(String, String)>> {
    Ok(sqlx::query_as(
        "SELECT v.Prompt, CAST(q.Type AS CHAR) FROM Question q JOIN QuestionVersion v ON v.Id = q.CurrentVersionId \
         WHERE q.CourseId = ? AND q.ArchivedAt IS NULL AND (q.OwnerUserId IS NULL OR q.OwnerUserId = ?) AND (? IS NULL OR q.CourseComponentId = ?) \
         ORDER BY q.Id DESC LIMIT ?",
    )
    .bind(course_id)
    .bind(user_id)
    .bind(component_id)
    .bind(component_id)
    .bind(limit)
    .fetch_all(conn)
    .await?)
}

/// Per lecture: questions the user can see and how many of them aren't answered right yet
/// (`UserQuestionStat.FirstRightAt IS NULL`, spec 7.5 "untested").
pub async fn untested_by_component(conn: &mut MySqlConnection, user_id: i64, course_id: i64) -> RepoResult<Vec<(i64, i64)>> {
    Ok(sqlx::query_as(
        "SELECT q.CourseComponentId, CAST(SUM(s.FirstRightAt IS NULL) AS SIGNED) FROM Question q \
         LEFT JOIN UserQuestionStat s ON s.QuestionId = q.Id AND s.UserId = ? \
         WHERE q.CourseId = ? AND q.ArchivedAt IS NULL AND q.ReviewState = 'Approved' AND (q.OwnerUserId IS NULL OR q.OwnerUserId = ?) \
         GROUP BY q.CourseComponentId",
    )
    .bind(user_id)
    .bind(course_id)
    .bind(user_id)
    .fetch_all(conn)
    .await?)
}

// ---------- check questions and quizzes ----------

/// A material's check questions: (question id, position) in reading order. Position is the page
/// (PDF) or section (notes) the check comes after; 0 means at the end.
pub async fn material_checks(conn: &mut MySqlConnection, material_id: i64) -> RepoResult<Vec<(i64, i32)>> {
    Ok(sqlx::query_as("SELECT QuestionId, Position FROM MaterialQuestion WHERE MaterialId = ? ORDER BY Position = 0, Position, QuestionId")
        .bind(material_id)
        .fetch_all(conn)
        .await?)
}

/// Replaces a material's check questions.
pub async fn set_material_checks(conn: &mut MySqlConnection, material_id: i64, checks: &[(i64, i32)]) -> RepoResult<()> {
    sqlx::query("DELETE FROM MaterialQuestion WHERE MaterialId = ?").bind(material_id).execute(&mut *conn).await?;
    for (question_id, position) in checks {
        sqlx::query("INSERT INTO MaterialQuestion (MaterialId, QuestionId, Position) VALUES (?, ?, ?)")
            .bind(material_id)
            .bind(question_id)
            .bind(position)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

/// The user's last outcome per question, for the ones given.
pub async fn last_outcomes(conn: &mut MySqlConnection, user_id: i64, question_ids: &[i64]) -> RepoResult<Vec<(i64, Option<String>)>> {
    if question_ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids = question_ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
    Ok(sqlx::query_as(&format!(
        "SELECT QuestionId, CAST(LastOutcome AS CHAR) FROM UserQuestionStat WHERE UserId = ? AND QuestionId IN ({ids})"
    ))
    .bind(user_id)
    .fetch_all(conn)
    .await?)
}

/// "Quiz me": questions you can answer in a course, optionally one lecture or one concept. Never
/// tried first, then the ones you got wrong, then the longest unseen; random within a tie.
#[allow(clippy::too_many_arguments)]
pub async fn quiz_questions(
    conn: &mut MySqlConnection,
    user_id: i64,
    course_id: i64,
    component_id: Option<i64>,
    concept_ref: Option<&str>,
    exclude: &[i64],
    limit: i64,
) -> RepoResult<Vec<i64>> {
    let not_in = if exclude.is_empty() {
        String::new()
    } else {
        format!("AND q.Id NOT IN ({})", exclude.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(","))
    };
    Ok(sqlx::query_scalar(&format!(
        "SELECT q.Id FROM Question q JOIN CourseComponent k ON k.Id = q.CourseComponentId \
         LEFT JOIN UserQuestionStat s ON s.QuestionId = q.Id AND s.UserId = ? \
         WHERE q.CourseId = ? AND q.ArchivedAt IS NULL AND k.ArchivedAt IS NULL AND q.CurrentVersionId IS NOT NULL \
           AND (q.OwnerUserId IS NULL OR q.OwnerUserId = ?) AND q.ReviewState = 'Approved' \
           AND (? IS NULL OR q.CourseComponentId = ?) \
           AND (? IS NULL OR EXISTS (SELECT 1 FROM QuestionConcept qc JOIN Concept c ON c.Id = qc.ConceptId \
                WHERE qc.QuestionId = q.Id AND c.PublicId = ?)) {not_in} \
         ORDER BY s.LastAnsweredAt IS NOT NULL, s.LastOutcome = 'Right', s.LastAnsweredAt, RAND() LIMIT ?"
    ))
    .bind(user_id)
    .bind(course_id)
    .bind(user_id)
    .bind(component_id)
    .bind(component_id)
    .bind(concept_ref)
    .bind(concept_ref)
    .bind(limit)
    .fetch_all(conn)
    .await?)
}
