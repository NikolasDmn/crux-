//! CourseComponent (lectures) + prerequisites, Concept, ComponentMaterial, Paper + PaperOmission.
//! Callers check course access first; every query here is also scoped by CourseId.

use chrono::{DateTime, NaiveDate, Utc};
use sqlx::MySqlConnection;

use crate::{map_unique, RepoError, RepoResult};

// ---------- components ----------

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct ComponentRow {
    pub id: i64,
    pub public_id: String,
    pub course_id: i64,
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub sort_order: i32,
    pub est_hours: Option<f64>,
    pub updated_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
    pub row_version: i32,
}

const COMPONENT_SELECT: &str = "SELECT Id, PublicId, CourseId, Code, Name, Description, SortOrder, CAST(EstHours AS DOUBLE) AS EstHours, \
    UpdatedAt, ArchivedAt, RowVersion FROM CourseComponent";

pub async fn components(conn: &mut MySqlConnection, course_id: i64, include_archived: bool) -> RepoResult<Vec<ComponentRow>> {
    Ok(sqlx::query_as(&format!("{COMPONENT_SELECT} WHERE CourseId = ? AND (? OR ArchivedAt IS NULL) ORDER BY SortOrder, Id"))
        .bind(course_id)
        .bind(include_archived)
        .fetch_all(conn)
        .await?)
}

/// A component by PublicId, with its course id (for the access check).
pub async fn component(conn: &mut MySqlConnection, public_id: &str) -> RepoResult<Option<ComponentRow>> {
    Ok(sqlx::query_as(&format!("{COMPONENT_SELECT} WHERE PublicId = ?")).bind(public_id).fetch_optional(conn).await?)
}

pub async fn component_ref(conn: &mut MySqlConnection, id: i64) -> RepoResult<Option<String>> {
    Ok(sqlx::query_scalar("SELECT PublicId FROM CourseComponent WHERE Id = ?").bind(id).fetch_optional(conn).await?)
}

pub async fn component_ids_by_public(conn: &mut MySqlConnection, course_id: i64, refs: &[String]) -> RepoResult<Vec<(i64, String)>> {
    if refs.is_empty() {
        return Ok(Vec::new());
    }
    let marks = vec!["?"; refs.len()].join(",");
    let sql = format!("SELECT Id, PublicId FROM CourseComponent WHERE CourseId = ? AND PublicId IN ({marks})");
    let mut q = sqlx::query_as::<_, (i64, String)>(&sql).bind(course_id);
    for r in refs {
        q = q.bind(r);
    }
    Ok(q.fetch_all(conn).await?)
}

pub struct ComponentWrite<'a> {
    pub code: &'a str,
    pub name: &'a str,
    pub description: Option<&'a str>,
    pub sort_order: i32,
    pub est_hours: Option<f64>,
}

pub async fn insert_component(conn: &mut MySqlConnection, public_id: &str, course_id: i64, c: &ComponentWrite<'_>) -> RepoResult<i64> {
    let res = sqlx::query(
        "INSERT INTO CourseComponent (PublicId, CourseId, Code, Name, Description, SortOrder, EstHours) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(public_id)
    .bind(course_id)
    .bind(c.code)
    .bind(c.name)
    .bind(c.description)
    .bind(c.sort_order)
    .bind(c.est_hours)
    .execute(conn)
    .await
    .map_err(map_unique)?;
    Ok(res.last_insert_id() as i64)
}

pub async fn update_component(conn: &mut MySqlConnection, id: i64, version: i32, c: &ComponentWrite<'_>) -> RepoResult<()> {
    let res = sqlx::query(
        "UPDATE CourseComponent SET Code = ?, Name = ?, Description = ?, SortOrder = ?, EstHours = ?, RowVersion = RowVersion + 1 \
         WHERE Id = ? AND RowVersion = ?",
    )
    .bind(c.code)
    .bind(c.name)
    .bind(c.description)
    .bind(c.sort_order)
    .bind(c.est_hours)
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

pub async fn archive_component(conn: &mut MySqlConnection, id: i64, at: DateTime<Utc>) -> RepoResult<()> {
    sqlx::query("UPDATE CourseComponent SET ArchivedAt = COALESCE(ArchivedAt, ?), RowVersion = RowVersion + 1 WHERE Id = ?")
        .bind(at)
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn next_component_sort(conn: &mut MySqlConnection, course_id: i64) -> RepoResult<i32> {
    Ok(sqlx::query_scalar("SELECT CAST(COALESCE(MAX(SortOrder), 0) + 10 AS SIGNED) FROM CourseComponent WHERE CourseId = ?")
        .bind(course_id)
        .fetch_one(conn)
        .await
        .map(|v: i64| v as i32)?)
}

/// All prerequisite edges within a course: (prerequisite, dependant) internal ids.
pub async fn prerequisite_edges(conn: &mut MySqlConnection, course_id: i64) -> RepoResult<Vec<(i64, i64)>> {
    Ok(sqlx::query_as(
        "SELECT d.PrerequisiteId, d.DependantId FROM ComponentDependency d JOIN CourseComponent c ON c.Id = d.DependantId WHERE c.CourseId = ?",
    )
    .bind(course_id)
    .fetch_all(conn)
    .await?)
}

pub async fn set_prerequisites(conn: &mut MySqlConnection, component_id: i64, prereqs: &[i64]) -> RepoResult<()> {
    sqlx::query("DELETE FROM ComponentDependency WHERE DependantId = ?").bind(component_id).execute(&mut *conn).await?;
    for p in prereqs {
        sqlx::query("INSERT IGNORE INTO ComponentDependency (PrerequisiteId, DependantId) VALUES (?, ?)")
            .bind(p)
            .bind(component_id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

// ---------- concepts ----------

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct ConceptRow {
    pub id: i64,
    pub public_id: Option<String>,
    pub name: String,
}

pub async fn concepts(conn: &mut MySqlConnection, course_id: i64) -> RepoResult<Vec<ConceptRow>> {
    Ok(sqlx::query_as("SELECT Id, PublicId, Name FROM Concept WHERE CourseId = ? ORDER BY Name").bind(course_id).fetch_all(conn).await?)
}

pub async fn insert_concept(conn: &mut MySqlConnection, public_id: &str, course_id: i64, name: &str) -> RepoResult<i64> {
    let res = sqlx::query("INSERT INTO Concept (PublicId, CourseId, Name) VALUES (?, ?, ?)")
        .bind(public_id)
        .bind(course_id)
        .bind(name)
        .execute(conn)
        .await
        .map_err(map_unique)?;
    Ok(res.last_insert_id() as i64)
}

// ---------- materials ----------

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct MaterialRow {
    pub id: i64,
    pub public_id: String,
    pub component_id: i64,
    pub component_ref: String,
    pub course_id: i64,
    pub kind: String,
    pub title: String,
    pub content: Option<String>,
    pub file_ref: Option<String>,
    pub file_sha256: Option<String>,
    pub original_status: String,
    pub original_purge_at: Option<DateTime<Utc>>,
    pub page_from: Option<i32>,
    pub page_to: Option<i32>,
    pub excluded: bool,
    pub excluded_note: Option<String>,
    pub ai_allowed: bool,
    pub sort_order: i32,
    pub updated_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
    pub row_version: i32,
}

const MATERIAL_SELECT: &str = "SELECT m.Id, m.PublicId, m.CourseComponentId AS ComponentId, c.PublicId AS ComponentRef, c.CourseId, \
    CAST(m.Kind AS CHAR) AS Kind, m.Title, m.Content, m.FileRef, m.FileSha256, CAST(m.OriginalStatus AS CHAR) AS OriginalStatus, \
    m.OriginalPurgeAt, m.PageFrom, m.PageTo, m.Excluded, m.ExcludedNote, m.AiAllowed, m.SortOrder, m.UpdatedAt, m.ArchivedAt, m.RowVersion \
    FROM ComponentMaterial m JOIN CourseComponent c ON c.Id = m.CourseComponentId";

pub async fn materials(conn: &mut MySqlConnection, component_id: i64) -> RepoResult<Vec<MaterialRow>> {
    Ok(sqlx::query_as(&format!("{MATERIAL_SELECT} WHERE m.CourseComponentId = ? AND m.ArchivedAt IS NULL ORDER BY m.SortOrder, m.Id"))
        .bind(component_id)
        .fetch_all(conn)
        .await?)
}

pub async fn material(conn: &mut MySqlConnection, public_id: &str) -> RepoResult<Option<MaterialRow>> {
    Ok(sqlx::query_as(&format!("{MATERIAL_SELECT} WHERE m.PublicId = ? AND m.ArchivedAt IS NULL"))
        .bind(public_id)
        .fetch_optional(conn)
        .await?)
}

pub struct MaterialWrite<'a> {
    pub kind: &'a str,
    pub title: &'a str,
    pub content: Option<&'a str>,
    pub file_ref: Option<&'a str>,
    pub file_sha256: Option<&'a str>,
    /// 'None' without a file, 'Present' with one.
    pub original_status: &'a str,
    pub page_from: Option<i32>,
    pub page_to: Option<i32>,
    pub excluded: bool,
    pub excluded_note: Option<&'a str>,
    pub ai_allowed: bool,
    pub sort_order: i32,
}

pub async fn insert_material(conn: &mut MySqlConnection, public_id: &str, component_id: i64, m: &MaterialWrite<'_>) -> RepoResult<i64> {
    let res = sqlx::query(
        "INSERT INTO ComponentMaterial (PublicId, CourseComponentId, Kind, Title, Content, FileRef, FileSha256, OriginalStatus, PageFrom, PageTo, \
         Excluded, ExcludedNote, AiAllowed, SortOrder) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(public_id)
    .bind(component_id)
    .bind(m.kind)
    .bind(m.title)
    .bind(m.content)
    .bind(m.file_ref)
    .bind(m.file_sha256)
    .bind(m.original_status)
    .bind(m.page_from)
    .bind(m.page_to)
    .bind(m.excluded)
    .bind(m.excluded_note)
    .bind(m.ai_allowed)
    .bind(m.sort_order)
    .execute(conn)
    .await?;
    Ok(res.last_insert_id() as i64)
}

pub async fn update_material(conn: &mut MySqlConnection, id: i64, version: i32, m: &MaterialWrite<'_>) -> RepoResult<()> {
    let res = sqlx::query(
        "UPDATE ComponentMaterial SET Kind = ?, Title = ?, Content = ?, FileRef = ?, FileSha256 = ?, OriginalStatus = ?, PageFrom = ?, PageTo = ?, \
         Excluded = ?, ExcludedNote = ?, AiAllowed = ?, SortOrder = ?, RowVersion = RowVersion + 1 WHERE Id = ? AND RowVersion = ?",
    )
    .bind(m.kind)
    .bind(m.title)
    .bind(m.content)
    .bind(m.file_ref)
    .bind(m.file_sha256)
    .bind(m.original_status)
    .bind(m.page_from)
    .bind(m.page_to)
    .bind(m.excluded)
    .bind(m.excluded_note)
    .bind(m.ai_allowed)
    .bind(m.sort_order)
    .bind(id)
    .bind(version)
    .execute(conn)
    .await?;
    if res.rows_affected() == 0 {
        return Err(RepoError::VersionConflict);
    }
    Ok(())
}

pub async fn archive_material(conn: &mut MySqlConnection, id: i64, at: DateTime<Utc>) -> RepoResult<()> {
    sqlx::query("UPDATE ComponentMaterial SET ArchivedAt = COALESCE(ArchivedAt, ?), RowVersion = RowVersion + 1 WHERE Id = ?")
        .bind(at)
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn next_material_sort(conn: &mut MySqlConnection, component_id: i64) -> RepoResult<i32> {
    Ok(sqlx::query_scalar("SELECT CAST(COALESCE(MAX(SortOrder), 0) + 10 AS SIGNED) FROM ComponentMaterial WHERE CourseComponentId = ?")
        .bind(component_id)
        .fetch_one(conn)
        .await
        .map(|v: i64| v as i32)?)
}

/// Is this file referenced by a live material, paper or question of a course the user can read?
/// (spec 6.1 step 7; question images since audit M2: any version, so old mocks keep theirs, of a
/// bank question or the user's own private one.)
pub async fn user_can_read_file_ref(conn: &mut MySqlConnection, user_id: i64, file_ref: &str) -> RepoResult<bool> {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM Course c \
         LEFT JOIN CourseMember cm ON cm.CourseId = c.Id AND cm.UserId = ? AND cm.Status = 'Accepted' \
         WHERE c.ArchivedAt IS NULL AND (c.OwnerUserId = ? OR cm.UserId IS NOT NULL) AND ( \
           EXISTS (SELECT 1 FROM ComponentMaterial m JOIN CourseComponent k ON k.Id = m.CourseComponentId \
                   WHERE k.CourseId = c.Id AND m.FileRef = ? AND m.ArchivedAt IS NULL) \
           OR EXISTS (SELECT 1 FROM Paper p WHERE p.CourseId = c.Id AND p.FileRef = ?) \
           OR EXISTS (SELECT 1 FROM QuestionVersion v JOIN Question q ON q.Id = v.QuestionId \
                      WHERE q.CourseId = c.Id AND v.ImageFileRef = ? AND (q.OwnerUserId IS NULL OR q.OwnerUserId = ?)))",
    )
    .bind(user_id)
    .bind(user_id)
    .bind(file_ref)
    .bind(file_ref)
    .bind(file_ref)
    .bind(user_id)
    .fetch_one(conn)
    .await?;
    Ok(n > 0)
}

// ---------- papers ----------

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct PaperRow {
    pub id: i64,
    pub public_id: Option<String>,
    pub course_id: i64,
    pub paper_key: String,
    pub label: String,
    pub exam_date: Option<NaiveDate>,
    pub file_ref: Option<String>,
    pub total_questions: Option<i32>,
    pub key_status: String,
    pub note: Option<String>,
    pub row_version: i32,
}

const PAPER_SELECT: &str =
    "SELECT Id, PublicId, CourseId, PaperKey, Label, ExamDate, FileRef, TotalQuestions, CAST(KeyStatus AS CHAR) AS KeyStatus, \
    Note, RowVersion FROM Paper";

pub async fn papers(conn: &mut MySqlConnection, course_id: i64) -> RepoResult<Vec<PaperRow>> {
    Ok(sqlx::query_as(&format!("{PAPER_SELECT} WHERE CourseId = ? ORDER BY ExamDate IS NULL, ExamDate DESC, PaperKey"))
        .bind(course_id)
        .fetch_all(conn)
        .await?)
}

pub async fn paper(conn: &mut MySqlConnection, public_id: &str) -> RepoResult<Option<PaperRow>> {
    Ok(sqlx::query_as(&format!("{PAPER_SELECT} WHERE PublicId = ?")).bind(public_id).fetch_optional(conn).await?)
}

pub struct PaperWrite<'a> {
    pub paper_key: &'a str,
    pub label: &'a str,
    pub exam_date: Option<NaiveDate>,
    pub file_ref: Option<&'a str>,
    pub total_questions: Option<i32>,
    pub key_status: &'a str,
    pub note: Option<&'a str>,
}

pub async fn insert_paper(conn: &mut MySqlConnection, public_id: &str, course_id: i64, p: &PaperWrite<'_>) -> RepoResult<i64> {
    let res = sqlx::query(
        "INSERT INTO Paper (PublicId, CourseId, PaperKey, Label, ExamDate, FileRef, TotalQuestions, KeyStatus, Note) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(public_id)
    .bind(course_id)
    .bind(p.paper_key)
    .bind(p.label)
    .bind(p.exam_date)
    .bind(p.file_ref)
    .bind(p.total_questions)
    .bind(p.key_status)
    .bind(p.note)
    .execute(conn)
    .await
    .map_err(map_unique)?;
    Ok(res.last_insert_id() as i64)
}

pub async fn update_paper(conn: &mut MySqlConnection, id: i64, version: i32, p: &PaperWrite<'_>) -> RepoResult<()> {
    let res = sqlx::query(
        "UPDATE Paper SET PaperKey = ?, Label = ?, ExamDate = ?, FileRef = ?, TotalQuestions = ?, KeyStatus = ?, Note = ?, RowVersion = RowVersion + 1 \
         WHERE Id = ? AND RowVersion = ?",
    )
    .bind(p.paper_key)
    .bind(p.label)
    .bind(p.exam_date)
    .bind(p.file_ref)
    .bind(p.total_questions)
    .bind(p.key_status)
    .bind(p.note)
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

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct OmissionRow {
    pub paper_id: i64,
    pub question_number: String,
    pub reason: String,
}

pub async fn omissions(conn: &mut MySqlConnection, paper_ids: &[i64]) -> RepoResult<Vec<OmissionRow>> {
    if paper_ids.is_empty() {
        return Ok(Vec::new());
    }
    let marks = vec!["?"; paper_ids.len()].join(",");
    let sql =
        format!("SELECT PaperId, QuestionNumber, Reason FROM PaperOmission WHERE PaperId IN ({marks}) ORDER BY PaperId, QuestionNumber");
    let mut q = sqlx::query_as::<_, OmissionRow>(&sql);
    for id in paper_ids {
        q = q.bind(id);
    }
    Ok(q.fetch_all(conn).await?)
}

pub async fn replace_omissions(conn: &mut MySqlConnection, paper_id: i64, items: &[(String, String)]) -> RepoResult<()> {
    sqlx::query("DELETE FROM PaperOmission WHERE PaperId = ?").bind(paper_id).execute(&mut *conn).await?;
    for (number, reason) in items {
        sqlx::query("INSERT INTO PaperOmission (PaperId, QuestionNumber, Reason) VALUES (?, ?, ?)")
            .bind(paper_id)
            .bind(number)
            .bind(reason)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

// ---------- storage saver (spec 6.2) ----------

/// Every material that uses the file (one file can back several materials; it's purged once).
/// `status` is None/Present/PendingPurge/Purged; `purge_at` is set only for PendingPurge.
pub async fn set_original_status_for_file(
    conn: &mut MySqlConnection,
    file_ref: &str,
    from: &[&str],
    status: &str,
    purge_at: Option<DateTime<Utc>>,
) -> RepoResult<u64> {
    let from = from.iter().map(|s| format!("'{s}'")).collect::<Vec<_>>().join(",");
    let res = sqlx::query(&format!(
        "UPDATE ComponentMaterial SET OriginalStatus = ?, OriginalPurgeAt = ?, RowVersion = RowVersion + 1 \
         WHERE FileRef = ? AND OriginalStatus IN ({from})"
    ))
    .bind(status)
    .bind(purge_at)
    .bind(file_ref)
    .execute(conn)
    .await?;
    Ok(res.rows_affected())
}

/// The course's materials with an original still present (course free-space).
pub async fn course_materials_with_file(conn: &mut MySqlConnection, course_id: i64) -> RepoResult<Vec<MaterialRow>> {
    Ok(sqlx::query_as(&format!(
        "{MATERIAL_SELECT} WHERE c.CourseId = ? AND m.ArchivedAt IS NULL AND m.FileRef IS NOT NULL AND m.OriginalStatus = 'Present'"
    ))
    .bind(course_id)
    .fetch_all(conn)
    .await?)
}

/// Past-paper files of a course (their questions are in the bank; spec 6.2 per course).
pub async fn course_paper_files(conn: &mut MySqlConnection, course_id: i64) -> RepoResult<Vec<String>> {
    Ok(sqlx::query_scalar("SELECT FileRef FROM Paper WHERE CourseId = ? AND FileRef IS NOT NULL").bind(course_id).fetch_all(conn).await?)
}
