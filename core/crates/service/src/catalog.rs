//! Lectures (components), concepts, materials, papers (backend-spec 5.1, 5.2 papers).
//! Reads need `can_read_course`, writes `can_edit_course`; every lookup re-checks the course.

use std::collections::HashMap;

use crux_clients::files::FileInfo;
use crux_domain::catalog::{self, ComponentFields, MaterialFields, PaperFields};
use crux_domain::{topics, ValidationErrors};
use crux_repo::catalog::{self as repo, ComponentRow, MaterialRow, PaperRow};
use crux_repo::courses::CourseRow;
use crux_repo::{MySqlConnection, RepoError};
use serde::Serialize;
use serde_json::json;
use utoipa::ToSchema;

use crate::courses::{editable, readable};
use crate::events::emit;
use crate::time::{iso, iso_opt};
use crate::{Actor, AppError, AppResult, Ctx};

// ---------- components ----------

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComponentView {
    pub id: String,
    pub course_id: String,
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub sort_order: i32,
    pub est_hours: Option<f64>,
    pub prerequisite_ids: Vec<String>,
    pub archived_at: Option<String>,
    pub version: i32,
}

fn component_view(c: &ComponentRow, course_ref: &str, prereqs: &HashMap<i64, Vec<String>>) -> ComponentView {
    ComponentView {
        id: c.public_id.clone(),
        course_id: course_ref.to_string(),
        code: c.code.clone(),
        name: c.name.clone(),
        description: c.description.clone(),
        sort_order: c.sort_order,
        est_hours: c.est_hours,
        prerequisite_ids: prereqs.get(&c.id).cloned().unwrap_or_default(),
        archived_at: iso_opt(c.archived_at),
        version: c.row_version,
    }
}

/// dependant id → prerequisite PublicIds, for a course.
async fn prerequisite_map(conn: &mut MySqlConnection, course_id: i64) -> AppResult<HashMap<i64, Vec<String>>> {
    let all = repo::components(conn, course_id, true).await?;
    let refs: HashMap<i64, String> = all.iter().map(|c| (c.id, c.public_id.clone())).collect();
    let mut out: HashMap<i64, Vec<String>> = HashMap::new();
    for (pre, dep) in repo::prerequisite_edges(conn, course_id).await? {
        if let Some(r) = refs.get(&pre) {
            out.entry(dep).or_default().push(r.clone());
        }
    }
    Ok(out)
}

/// A component and its course, if the actor may read the course.
async fn component_for(conn: &mut MySqlConnection, actor: &Actor, component_ref: &str, edit: bool) -> AppResult<(ComponentRow, CourseRow)> {
    let comp = repo::component(conn, component_ref).await?.ok_or(AppError::NotFound)?;
    let course_ref = crux_repo::courses::public_id_of(conn, comp.course_id).await?;
    let course = if edit { editable(conn, actor, &course_ref).await? } else { readable(conn, actor, &course_ref).await? };
    Ok((comp, course))
}

pub async fn list_components(ctx: &Ctx, actor: &Actor, course_ref: &str) -> AppResult<Vec<ComponentView>> {
    let mut conn = ctx.pool.acquire().await?;
    let course = readable(&mut conn, actor, course_ref).await?;
    let prereqs = prerequisite_map(&mut conn, course.id).await?;
    Ok(repo::components(&mut conn, course.id, false).await?.iter().map(|c| component_view(c, &course.public_id, &prereqs)).collect())
}

fn code_taken(e: RepoError, field: &str, msg: &str) -> AppError {
    match e {
        RepoError::Duplicate(_) => ValidationErrors::single(field, "taken", msg).into(),
        other => other.into(),
    }
}

/// Resolves prerequisite PublicIds within the course and rejects cycles (review section 6 rule 8).
async fn check_prerequisites(conn: &mut MySqlConnection, course_id: i64, component_id: i64, refs: &[String]) -> AppResult<Vec<i64>> {
    let found = repo::component_ids_by_public(conn, course_id, refs).await?;
    if found.len() != refs.iter().collect::<std::collections::HashSet<_>>().len() {
        return Err(ValidationErrors::single("prerequisiteIds", "unknown", "Prerequisites must be lectures of the same course.").into());
    }
    let ids: Vec<i64> = found.into_iter().map(|(id, _)| id).collect();
    if ids.contains(&component_id) {
        return Err(ValidationErrors::single("prerequisiteIds", "self", "A lecture can't be its own prerequisite.").into());
    }
    let edges = catalog::replace_prerequisites(&repo::prerequisite_edges(conn, course_id).await?, component_id, &ids);
    if catalog::has_cycle(&edges) {
        return Err(ValidationErrors::single("prerequisiteIds", "cycle", "That would make the prerequisites go in a circle.").into());
    }
    Ok(ids)
}

pub async fn create_component(ctx: &Ctx, actor: &Actor, course_ref: &str, f: ComponentFields) -> AppResult<ComponentView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let course = editable(&mut tx, actor, course_ref).await?;
    catalog::validate_component(&f, true).into_result()?;
    let sort = match f.sort_order {
        Some(s) => s,
        None => repo::next_component_sort(&mut tx, course.id).await?,
    };
    let public_id = ctx.ids.ulid();
    let id = repo::insert_component(
        &mut tx,
        &public_id,
        course.id,
        &repo::ComponentWrite {
            code: f.code.as_deref().unwrap_or_default().trim(),
            name: f.name.as_deref().unwrap_or_default().trim(),
            description: f.description.as_deref(),
            sort_order: sort,
            est_hours: f.est_hours,
        },
    )
    .await
    .map_err(|e| code_taken(e, "code", "A lecture with this code exists in the course."))?;
    if let Some(refs) = &f.prerequisite_ids {
        let ids = check_prerequisites(&mut tx, course.id, id, refs).await?;
        repo::set_prerequisites(&mut tx, id, &ids).await?;
    }
    let prereqs = prerequisite_map(&mut tx, course.id).await?;
    let row = repo::component(&mut tx, &public_id).await?.ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(component_view(&row, &course.public_id, &prereqs))
}

pub async fn patch_component(ctx: &Ctx, actor: &Actor, component_ref: &str, version: i32, f: ComponentFields) -> AppResult<ComponentView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let (cur, course) = component_for(&mut tx, actor, component_ref, true).await?;
    catalog::validate_component(&f, false).into_result()?;
    if cur.archived_at.is_some() {
        return Err(AppError::NotFound);
    }
    if cur.row_version != version {
        let prereqs = prerequisite_map(&mut tx, course.id).await?;
        return Err(AppError::Conflict {
            message: "Changed somewhere else.".into(),
            current: serde_json::to_value(component_view(&cur, &course.public_id, &prereqs)).ok(),
        });
    }
    repo::update_component(
        &mut tx,
        cur.id,
        version,
        &repo::ComponentWrite {
            code: f.code.as_deref().map(str::trim).unwrap_or(&cur.code),
            name: f.name.as_deref().map(str::trim).unwrap_or(&cur.name),
            description: if f.description.is_some() { f.description.as_deref() } else { cur.description.as_deref() },
            sort_order: f.sort_order.unwrap_or(cur.sort_order),
            est_hours: f.est_hours.or(cur.est_hours),
        },
    )
    .await
    .map_err(|e| code_taken(e, "code", "A lecture with this code exists in the course."))?;
    if let Some(refs) = &f.prerequisite_ids {
        let ids = check_prerequisites(&mut tx, course.id, cur.id, refs).await?;
        repo::set_prerequisites(&mut tx, cur.id, &ids).await?;
    }
    let prereqs = prerequisite_map(&mut tx, course.id).await?;
    let row = repo::component(&mut tx, component_ref).await?.ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(component_view(&row, &course.public_id, &prereqs))
}

/// Spec 15: delete = archive. Lectures are never hard-deleted once anything points at them.
pub async fn archive_component(ctx: &Ctx, actor: &Actor, component_ref: &str) -> AppResult<()> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let (cur, _) = component_for(&mut tx, actor, component_ref, true).await?;
    repo::archive_component(&mut tx, cur.id, ctx.clock.now()).await?;
    tx.commit().await?;
    Ok(())
}

// ---------- concepts ----------

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConceptView {
    pub id: String,
    pub name: String,
}

pub async fn list_concepts(ctx: &Ctx, actor: &Actor, course_ref: &str) -> AppResult<Vec<ConceptView>> {
    let mut conn = ctx.pool.acquire().await?;
    let course = readable(&mut conn, actor, course_ref).await?;
    Ok(repo::concepts(&mut conn, course.id)
        .await?
        .into_iter()
        .map(|c| ConceptView { id: c.public_id.unwrap_or_default(), name: c.name })
        .collect())
}

pub async fn create_concept(ctx: &Ctx, actor: &Actor, course_ref: &str, name: &str) -> AppResult<ConceptView> {
    let name = name.trim();
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let course = editable(&mut tx, actor, course_ref).await?;
    if !(1..=255).contains(&name.chars().count()) {
        return Err(ValidationErrors::single("name", "invalid", "Enter 1-255 characters.").into());
    }
    let public_id = ctx.ids.ulid();
    repo::insert_concept(&mut tx, &public_id, course.id, name)
        .await
        .map_err(|e| code_taken(e, "name", "This concept exists in the course."))?;
    tx.commit().await?;
    Ok(ConceptView { id: public_id, name: name.to_string() })
}

// ---------- materials ----------

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MaterialView {
    pub id: String,
    pub component_id: String,
    pub kind: String,
    pub title: String,
    pub content: Option<String>,
    pub file_id: Option<String>,
    /// None (markdown), Present, PendingPurge, Purged (storage saver, milestone 7).
    pub original_status: String,
    pub original_purge_at: Option<String>,
    pub page_from: Option<i32>,
    pub page_to: Option<i32>,
    pub excluded: bool,
    pub excluded_note: Option<String>,
    pub ai_allowed: bool,
    pub sort_order: i32,
    pub updated_at: String,
    pub version: i32,
}

impl From<&MaterialRow> for MaterialView {
    fn from(m: &MaterialRow) -> Self {
        Self {
            id: m.public_id.clone(),
            component_id: m.component_ref.clone(),
            kind: m.kind.clone(),
            title: m.title.clone(),
            content: m.content.clone(),
            file_id: m.file_ref.clone(),
            original_status: m.original_status.clone(),
            original_purge_at: iso_opt(m.original_purge_at),
            page_from: m.page_from,
            page_to: m.page_to,
            excluded: m.excluded,
            excluded_note: m.excluded_note.clone(),
            ai_allowed: m.ai_allowed,
            sort_order: m.sort_order,
            updated_at: iso(m.updated_at),
            version: m.row_version,
        }
    }
}

/// Spec 5.1: the file must be Active and scanned Clean in files, and belong to this course (an
/// upload for it) or to the course owner (D2-4: course files count against the owner).
pub(crate) async fn usable_file(ctx: &Ctx, actor: &Actor, course: &CourseRow, file_ref: &str, field: &str) -> AppResult<FileInfo> {
    let token = ctx.keys.sign_service("core", "files", ctx.clock.now().timestamp());
    let info = ctx.files.get_file(&token, &actor.public_id, file_ref).await.map_err(|e| anyhow::anyhow!("files service: {e}"))?;
    let unknown = || AppError::from(ValidationErrors::single(field, "unknown", "No such file."));
    let info = info.ok_or_else(unknown)?;
    if !info.usable_in(&course.public_id, &actor.public_id) {
        return Err(unknown());
    }
    if !info.usable_original() {
        let msg = if info.scan_status == "Pending" {
            "The file is still being checked. Try again in a moment."
        } else {
            "This file can't be used."
        };
        return Err(ValidationErrors::single(field, "not_ready", msg).into());
    }
    Ok(info)
}

pub(crate) async fn material_for(
    conn: &mut MySqlConnection,
    actor: &Actor,
    material_ref: &str,
    edit: bool,
) -> AppResult<(MaterialRow, CourseRow)> {
    let m = repo::material(conn, material_ref).await?.ok_or(AppError::NotFound)?;
    let course_ref = crux_repo::courses::public_id_of(conn, m.course_id).await?;
    let course = if edit { editable(conn, actor, &course_ref).await? } else { readable(conn, actor, &course_ref).await? };
    Ok((m, course))
}

async fn emit_material(conn: &mut MySqlConnection, ctx: &Ctx, topic: &str, m: &MaterialRow, course_ref: &str) -> AppResult<()> {
    emit(
        conn,
        ctx,
        topic,
        Some(&m.public_id),
        json!({ "materialRef": m.public_id, "courseRef": course_ref, "fileRef": m.file_ref, "aiAllowed": m.ai_allowed, "excluded": m.excluded }),
    )
    .await
}

pub async fn list_materials(ctx: &Ctx, actor: &Actor, component_ref: &str) -> AppResult<Vec<MaterialView>> {
    let mut conn = ctx.pool.acquire().await?;
    let (comp, _) = component_for(&mut conn, actor, component_ref, false).await?;
    Ok(repo::materials(&mut conn, comp.id).await?.iter().map(MaterialView::from).collect())
}

pub async fn create_material(ctx: &Ctx, actor: &Actor, component_ref: &str, f: MaterialFields) -> AppResult<MaterialView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let (comp, course) = component_for(&mut tx, actor, component_ref, true).await?;
    catalog::validate_material(&f, true, false).into_result()?;
    if comp.archived_at.is_some() {
        return Err(AppError::NotFound);
    }
    let file = match &f.file_id {
        Some(r) => Some(usable_file(ctx, actor, &course, r, "fileId").await?),
        None => None,
    };
    let sort = match f.sort_order {
        Some(s) => s,
        None => repo::next_material_sort(&mut tx, comp.id).await?,
    };
    let public_id = ctx.ids.ulid();
    repo::insert_material(
        &mut tx,
        &public_id,
        comp.id,
        &repo::MaterialWrite {
            kind: f.kind.as_deref().unwrap_or("Other"),
            title: f.title.as_deref().unwrap_or_default().trim(),
            content: f.content.as_deref(),
            file_ref: file.as_ref().map(|i| i.id.as_str()),
            file_sha256: file.as_ref().map(|i| i.sha256.as_str()),
            original_status: if file.is_some() { "Present" } else { "None" },
            page_from: f.page_from,
            page_to: f.page_to,
            excluded: f.excluded.unwrap_or(false),
            excluded_note: f.excluded_note.as_deref(),
            ai_allowed: f.ai_allowed.unwrap_or(true),
            sort_order: sort,
        },
    )
    .await?;
    let row = repo::material(&mut tx, &public_id).await?.ok_or(AppError::NotFound)?;
    emit_material(&mut tx, ctx, topics::MATERIAL_CREATED, &row, &course.public_id).await?;
    tx.commit().await?;
    Ok(MaterialView::from(&row))
}

pub async fn patch_material(ctx: &Ctx, actor: &Actor, material_ref: &str, version: i32, f: MaterialFields) -> AppResult<MaterialView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let (cur, course) = material_for(&mut tx, actor, material_ref, true).await?;
    catalog::validate_material(&f, false, cur.content.is_some() || cur.file_ref.is_some()).into_result()?;
    if cur.row_version != version {
        return Err(AppError::Conflict {
            message: "Changed somewhere else.".into(),
            current: serde_json::to_value(MaterialView::from(&cur)).ok(),
        });
    }
    // Switching between markdown and a file replaces the other source.
    let (content, file_ref, sha, status) = match (&f.content, &f.file_id) {
        (Some(c), None) => (Some(c.clone()), None, None, "None".to_string()),
        (None, Some(r)) if Some(r) != cur.file_ref.as_ref() => {
            let info = usable_file(ctx, actor, &course, r, "fileId").await?;
            (None, Some(info.id), Some(info.sha256), "Present".to_string())
        }
        _ => (cur.content.clone(), cur.file_ref.clone(), cur.file_sha256.clone(), cur.original_status.clone()),
    };
    repo::update_material(
        &mut tx,
        cur.id,
        version,
        &repo::MaterialWrite {
            kind: f.kind.as_deref().unwrap_or(&cur.kind),
            title: f.title.as_deref().map(str::trim).unwrap_or(&cur.title),
            content: content.as_deref(),
            file_ref: file_ref.as_deref(),
            file_sha256: sha.as_deref(),
            original_status: &status,
            page_from: f.page_from.or(cur.page_from),
            page_to: f.page_to.or(cur.page_to),
            excluded: f.excluded.unwrap_or(cur.excluded),
            excluded_note: if f.excluded_note.is_some() { f.excluded_note.as_deref() } else { cur.excluded_note.as_deref() },
            ai_allowed: f.ai_allowed.unwrap_or(cur.ai_allowed),
            sort_order: f.sort_order.unwrap_or(cur.sort_order),
        },
    )
    .await?;
    let row = repo::material(&mut tx, material_ref).await?.ok_or(AppError::NotFound)?;
    emit_material(&mut tx, ctx, topics::MATERIAL_UPDATED, &row, &course.public_id).await?;
    tx.commit().await?;
    Ok(MaterialView::from(&row))
}

pub async fn archive_material(ctx: &Ctx, actor: &Actor, material_ref: &str) -> AppResult<()> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let (cur, _) = material_for(&mut tx, actor, material_ref, true).await?;
    repo::archive_material(&mut tx, cur.id, ctx.clock.now()).await?;
    tx.commit().await?;
    Ok(())
}

// ---------- papers ----------

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OmissionView {
    pub question_number: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaperView {
    pub id: String,
    pub paper_key: String,
    pub label: String,
    /// `YYYY-MM-DD`.
    pub exam_date: Option<String>,
    pub file_id: Option<String>,
    pub total_questions: Option<i32>,
    pub key_status: String,
    pub note: Option<String>,
    pub omissions: Vec<OmissionView>,
    pub version: i32,
}

fn paper_view(p: &PaperRow, omissions: Vec<OmissionView>) -> PaperView {
    PaperView {
        id: p.public_id.clone().unwrap_or_default(),
        paper_key: p.paper_key.clone(),
        label: p.label.clone(),
        exam_date: p.exam_date.map(|d| d.format("%Y-%m-%d").to_string()),
        file_id: p.file_ref.clone(),
        total_questions: p.total_questions,
        key_status: p.key_status.clone(),
        note: p.note.clone(),
        omissions,
        version: p.row_version,
    }
}

async fn papers_with_omissions(conn: &mut MySqlConnection, rows: &[PaperRow]) -> AppResult<Vec<PaperView>> {
    let ids: Vec<i64> = rows.iter().map(|p| p.id).collect();
    let mut by_paper: HashMap<i64, Vec<OmissionView>> = HashMap::new();
    for o in repo::omissions(conn, &ids).await? {
        by_paper.entry(o.paper_id).or_default().push(OmissionView { question_number: o.question_number, reason: o.reason });
    }
    Ok(rows.iter().map(|p| paper_view(p, by_paper.remove(&p.id).unwrap_or_default())).collect())
}

pub async fn list_papers(ctx: &Ctx, actor: &Actor, course_ref: &str) -> AppResult<Vec<PaperView>> {
    let mut conn = ctx.pool.acquire().await?;
    let course = readable(&mut conn, actor, course_ref).await?;
    let rows = repo::papers(&mut conn, course.id).await?;
    papers_with_omissions(&mut conn, &rows).await
}

fn omission_pairs(f: &PaperFields) -> Option<Vec<(String, String)>> {
    f.omissions.as_ref().map(|om| om.iter().map(|o| (o.question_number.trim().to_string(), o.reason.trim().to_string())).collect())
}

pub async fn create_paper(ctx: &Ctx, actor: &Actor, course_ref: &str, f: PaperFields) -> AppResult<PaperView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let course = editable(&mut tx, actor, course_ref).await?;
    catalog::validate_paper(&f, true).into_result()?;
    let file = match &f.file_id {
        Some(r) => Some(usable_file(ctx, actor, &course, r, "fileId").await?),
        None => None,
    };
    let public_id = ctx.ids.ulid();
    let id = repo::insert_paper(
        &mut tx,
        &public_id,
        course.id,
        &repo::PaperWrite {
            paper_key: f.paper_key.as_deref().unwrap_or_default().trim(),
            label: f.label.as_deref().unwrap_or_default().trim(),
            exam_date: f.exam_date,
            file_ref: file.as_ref().map(|i| i.id.as_str()),
            total_questions: f.total_questions,
            key_status: f.key_status.as_deref().unwrap_or("None"),
            note: f.note.as_deref(),
        },
    )
    .await
    .map_err(|e| code_taken(e, "paperKey", "A paper with this key exists in the course."))?;
    if let Some(items) = omission_pairs(&f) {
        repo::replace_omissions(&mut tx, id, &items).await?;
    }
    let row = repo::paper(&mut tx, &public_id).await?.ok_or(AppError::NotFound)?;
    let view = papers_with_omissions(&mut tx, std::slice::from_ref(&row)).await?.remove(0);
    tx.commit().await?;
    Ok(view)
}

pub async fn patch_paper(ctx: &Ctx, actor: &Actor, paper_ref: &str, version: i32, f: PaperFields) -> AppResult<PaperView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let cur = repo::paper(&mut tx, paper_ref).await?.ok_or(AppError::NotFound)?;
    let course_ref = crux_repo::courses::public_id_of(&mut tx, cur.course_id).await?;
    let course = editable(&mut tx, actor, &course_ref).await?;
    catalog::validate_paper(&f, false).into_result()?;
    if cur.row_version != version {
        let view = papers_with_omissions(&mut tx, std::slice::from_ref(&cur)).await?.remove(0);
        return Err(AppError::Conflict { message: "Changed somewhere else.".into(), current: serde_json::to_value(view).ok() });
    }
    let file_ref = match &f.file_id {
        Some(r) if Some(r) != cur.file_ref.as_ref() => Some(usable_file(ctx, actor, &course, r, "fileId").await?.id),
        _ => cur.file_ref.clone(),
    };
    repo::update_paper(
        &mut tx,
        cur.id,
        version,
        &repo::PaperWrite {
            paper_key: f.paper_key.as_deref().map(str::trim).unwrap_or(&cur.paper_key),
            label: f.label.as_deref().map(str::trim).unwrap_or(&cur.label),
            exam_date: f.exam_date.or(cur.exam_date),
            file_ref: file_ref.as_deref(),
            total_questions: f.total_questions.or(cur.total_questions),
            key_status: f.key_status.as_deref().unwrap_or(&cur.key_status),
            note: if f.note.is_some() { f.note.as_deref() } else { cur.note.as_deref() },
        },
    )
    .await
    .map_err(|e| code_taken(e, "paperKey", "A paper with this key exists in the course."))?;
    if let Some(items) = omission_pairs(&f) {
        repo::replace_omissions(&mut tx, cur.id, &items).await?;
    }
    let row = repo::paper(&mut tx, paper_ref).await?.ok_or(AppError::NotFound)?;
    let view = papers_with_omissions(&mut tx, std::slice::from_ref(&row)).await?.remove(0);
    tx.commit().await?;
    Ok(view)
}

// ---------- internal access answers (files, ai; spec 4.2) ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessAnswer {
    pub course_ref: String,
    pub owner_ref: String,
    /// Owner, Editor, Viewer; null = no access.
    pub role: Option<String>,
}

/// `GET /internal/v1/access?user=&course=`. 404 when the course doesn't exist (or is trashed).
pub async fn access(ctx: &Ctx, user_ref: &str, course_ref: &str) -> AppResult<AccessAnswer> {
    let mut conn = ctx.pool.acquire().await?;
    let user = crux_repo::users::by_public_id(&mut conn, user_ref).await?;
    let course = match &user {
        Some(u) => crux_repo::courses::for_actor(&mut conn, u.id, course_ref, false).await?,
        None => None,
    };
    if let Some(c) = course {
        return Ok(AccessAnswer { course_ref: c.public_id.clone(), owner_ref: c.owner_ref.clone(), role: Some(c.my_role.clone()) });
    }
    let (owner_ref, _) = crux_repo::courses::owner_of_live_course(&mut conn, course_ref).await?.ok_or(AppError::NotFound)?;
    Ok(AccessAnswer { course_ref: course_ref.to_string(), owner_ref, role: None })
}

/// `GET /internal/v1/access/file-ref?user=&ref=`: may this user read the file through a course?
pub async fn file_ref_access(ctx: &Ctx, user_ref: &str, file_ref: &str) -> AppResult<bool> {
    let mut conn = ctx.pool.acquire().await?;
    let Some(user) = crux_repo::users::by_public_id(&mut conn, user_ref).await? else { return Ok(false) };
    Ok(repo::user_can_read_file_ref(&mut conn, user.id, file_ref).await?)
}
