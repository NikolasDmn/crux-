//! Question bank (backend-spec 5.2): questions with immutable versions and typed keys.

use std::collections::HashMap;

use crux_domain::page::{clamp_limit, decode_cursor, encode_cursor};
use crux_domain::policy::{can_edit_course, can_read_course};
use crux_domain::question::{self, OpenKey, QuestionType, VersionInput};
use crux_domain::ValidationErrors;
use crux_repo::bank::{self as repo, BankFilter, QuestionRow, VersionRow};
use crux_repo::courses::CourseRow;
use crux_repo::MySqlConnection;
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::{IntoParams, ToSchema};

use crate::courses::{access_of, readable};
use crate::time::{iso, iso_opt};
use crate::views::Page;
use crate::{Actor, AppError, AppResult, Ctx};

// ---------- views ----------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OptionView {
    pub id: String,
    pub text: String,
    pub is_correct: bool,
    pub distractor_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NumericView {
    pub value: f64,
    pub tolerance: f64,
    pub tolerance_kind: String,
    pub unit: Option<String>,
    pub unit_required: bool,
    pub model_answer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpenView {
    pub model_answer: Option<String>,
    pub rubric_note: Option<String>,
    #[schema(value_type = Option<Object>)]
    pub rubric: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VersionView {
    pub id: String,
    pub version_no: i32,
    pub prompt: String,
    pub explanation: Option<String>,
    pub image_file_id: Option<String>,
    pub key_reliability: String,
    pub key_note: Option<String>,
    pub change_note: Option<String>,
    pub created_at: String,
    /// MC: in their stored order.
    pub options: Option<Vec<OptionView>>,
    pub shuffle_options: Option<bool>,
    pub numeric: Option<NumericView>,
    pub open: Option<OpenView>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConceptLink {
    pub id: String,
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuestionView {
    pub id: String,
    pub course_id: String,
    pub component_id: String,
    #[schema(value_type = String)]
    pub r#type: String,
    /// Private to you (e.g. generated from your weak spots); `promote` puts it in the course bank.
    pub private: bool,
    pub source: String,
    pub source_label: Option<String>,
    pub paper_id: Option<String>,
    pub paper_question_no: Option<String>,
    pub review_state: String,
    pub concepts: Vec<ConceptLink>,
    pub current: VersionView,
    pub archived_at: Option<String>,
    pub updated_at: String,
    pub version: i32,
}

/// Loads every version's typed key in batches.
pub(crate) async fn version_views(
    conn: &mut MySqlConnection,
    versions: &[VersionRow],
    qtypes: &HashMap<i64, String>,
) -> AppResult<HashMap<i64, VersionView>> {
    let ids: Vec<i64> = versions.iter().map(|v| v.id).collect();
    let mut options: HashMap<i64, Vec<OptionView>> = HashMap::new();
    for o in repo::options(conn, &ids).await? {
        options.entry(o.question_version_id).or_default().push(OptionView {
            id: o.public_id.unwrap_or_default(),
            text: o.text,
            is_correct: o.is_correct,
            distractor_note: o.distractor_note,
        });
    }
    let numerics: HashMap<i64, NumericView> = repo::numeric_keys(conn, &ids)
        .await?
        .into_iter()
        .map(|n| {
            (
                n.question_version_id,
                NumericView {
                    value: n.value,
                    tolerance: n.tolerance,
                    tolerance_kind: n.tolerance_kind,
                    unit: n.unit,
                    unit_required: n.unit_required,
                    model_answer: n.model_answer,
                },
            )
        })
        .collect();
    let opens: HashMap<i64, OpenView> = repo::open_keys(conn, &ids)
        .await?
        .into_iter()
        .map(|o| {
            (
                o.question_version_id,
                OpenView { model_answer: o.model_answer, rubric_note: o.rubric_note, rubric: o.rubric_json.map(|j| j.0) },
            )
        })
        .collect();
    let mut out = HashMap::new();
    for v in versions {
        let qt = qtypes.get(&v.question_id).map(String::as_str).unwrap_or("Open");
        let shuffle = if qt == "MC" { Some(repo::shuffle_flag(conn, v.id).await?) } else { None };
        out.insert(
            v.id,
            VersionView {
                id: v.public_id.clone().unwrap_or_default(),
                version_no: v.version_no,
                prompt: v.prompt.clone(),
                explanation: v.explanation.clone(),
                image_file_id: v.image_file_ref.clone(),
                key_reliability: v.key_reliability.clone(),
                key_note: v.key_note.clone(),
                change_note: v.change_note.clone(),
                created_at: iso(v.created_at),
                options: (qt == "MC").then(|| options.remove(&v.id).unwrap_or_default()),
                shuffle_options: shuffle,
                numeric: numerics.get(&v.id).cloned(),
                open: opens.get(&v.id).cloned(),
            },
        );
    }
    Ok(out)
}

pub(crate) async fn question_views(
    conn: &mut MySqlConnection,
    actor: i64,
    course_ref: &str,
    rows: &[QuestionRow],
) -> AppResult<Vec<QuestionView>> {
    let version_ids: Vec<i64> = rows.iter().filter_map(|q| q.current_version_id).collect();
    let versions = repo::versions_by_ids(conn, &version_ids).await?;
    let qtypes: HashMap<i64, String> = rows.iter().map(|q| (q.id, q.qtype.clone())).collect();
    let mut vviews = version_views(conn, &versions, &qtypes).await?;
    let ids: Vec<i64> = rows.iter().map(|q| q.id).collect();
    let mut concepts: HashMap<i64, Vec<ConceptLink>> = HashMap::new();
    for (qid, cref, _, primary) in repo::question_concepts(conn, &ids).await? {
        concepts.entry(qid).or_default().push(ConceptLink { id: cref.unwrap_or_default(), primary });
    }
    Ok(rows
        .iter()
        .filter_map(|q| {
            let current = vviews.remove(&q.current_version_id?)?;
            Some(QuestionView {
                id: q.public_id.clone(),
                course_id: course_ref.to_string(),
                component_id: q.component_ref.clone(),
                r#type: q.qtype.clone(),
                private: q.owner_user_id == Some(actor),
                source: q.source.clone(),
                source_label: q.source_label.clone(),
                paper_id: q.paper_ref.clone(),
                paper_question_no: q.paper_question_no.clone(),
                review_state: q.review_state.clone(),
                concepts: concepts.remove(&q.id).unwrap_or_default(),
                current,
                archived_at: iso_opt(q.archived_at),
                updated_at: iso(q.updated_at),
                version: q.row_version,
            })
        })
        .collect())
}

// ---------- access ----------

/// Spec 4.2 can_see_question, with one build choice: course editors also see the course bank's
/// drafts (someone has to approve them).
async fn visible(conn: &mut MySqlConnection, actor: &Actor, question_ref: &str) -> AppResult<(QuestionRow, CourseRow)> {
    let q = repo::question(conn, question_ref).await?.ok_or(AppError::NotFound)?;
    let course_ref = crux_repo::courses::public_id_of(conn, q.course_id).await?;
    let course = readable(conn, actor, &course_ref).await?;
    let access = access_of(&course);
    let mine = q.owner_user_id == Some(actor.user_id);
    let ok = can_read_course(&access)
        && (q.owner_user_id.is_none() || mine)
        && (q.review_state == "Approved" || mine || (q.owner_user_id.is_none() && can_edit_course(&access)));
    if ok {
        Ok((q, course))
    } else {
        Err(AppError::NotFound)
    }
}

/// Bank questions: course editors. Private questions: their owner.
async fn writable(conn: &mut MySqlConnection, actor: &Actor, question_ref: &str) -> AppResult<(QuestionRow, CourseRow)> {
    let (q, course) = visible(conn, actor, question_ref).await?;
    let ok = match q.owner_user_id {
        Some(owner) => owner == actor.user_id,
        None => can_edit_course(&access_of(&course)),
    };
    if ok {
        Ok((q, course))
    } else {
        Err(AppError::NotFound)
    }
}

// ---------- reads ----------

#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct BankQuery {
    /// Lecture id.
    pub component_id: Option<String>,
    #[serde(rename = "type")]
    #[param(rename = "type")]
    pub qtype: Option<String>,
    pub source: Option<String>,
    pub paper_id: Option<String>,
    pub key_reliability: Option<String>,
    pub review_state: Option<String>,
    /// Only questions you haven't answered right yet.
    #[serde(default)]
    pub untested: bool,
    /// Text search in the prompt.
    pub q: Option<String>,
    #[serde(default)]
    pub include_archived: bool,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

pub async fn list(ctx: &Ctx, actor: &Actor, course_ref: &str, q: BankQuery) -> AppResult<Page<QuestionView>> {
    let mut conn = ctx.pool.acquire().await?;
    let course = readable(&mut conn, actor, course_ref).await?;
    let component_id = match &q.component_id {
        Some(r) => Some(
            crux_repo::catalog::component_ids_by_public(&mut conn, course.id, std::slice::from_ref(r))
                .await?
                .first()
                .map(|c| c.0)
                .ok_or(AppError::NotFound)?,
        ),
        None => None,
    };
    let paper_id = match &q.paper_id {
        Some(r) => Some(repo::paper_id(&mut conn, course.id, r).await?.ok_or(AppError::NotFound)?),
        None => None,
    };
    let filter = BankFilter {
        component_id,
        qtype: q.qtype.clone(),
        source: q.source.clone(),
        paper_id,
        key_reliability: q.key_reliability.clone(),
        review_state: q.review_state.clone(),
        untested_only: q.untested,
        search: q.q.clone().filter(|s| !s.trim().is_empty()),
        include_archived: q.include_archived,
    };
    let limit = clamp_limit(q.limit);
    let editor = can_edit_course(&access_of(&course));
    let rows =
        repo::list(&mut conn, course.id, actor.user_id, editor, &filter, q.cursor.as_deref().and_then(decode_cursor), limit + 1).await?;
    let next_cursor = (rows.len() > limit as usize).then(|| encode_cursor(rows[limit as usize - 1].id));
    let rows: Vec<QuestionRow> = rows.into_iter().take(limit as usize).collect();
    Ok(Page { items: question_views(&mut conn, actor.user_id, &course.public_id, &rows).await?, next_cursor })
}

pub async fn get(ctx: &Ctx, actor: &Actor, question_ref: &str) -> AppResult<QuestionView> {
    let mut conn = ctx.pool.acquire().await?;
    let (q, course) = visible(&mut conn, actor, question_ref).await?;
    question_views(&mut conn, actor.user_id, &course.public_id, &[q]).await?.pop().ok_or(AppError::NotFound)
}

pub async fn versions(ctx: &Ctx, actor: &Actor, question_ref: &str) -> AppResult<Vec<VersionView>> {
    let mut conn = ctx.pool.acquire().await?;
    let (q, _) = visible(&mut conn, actor, question_ref).await?;
    let rows = repo::versions(&mut conn, q.id).await?;
    let qtypes = HashMap::from([(q.id, q.qtype.clone())]);
    let mut views = version_views(&mut conn, &rows, &qtypes).await?;
    Ok(rows.iter().filter_map(|v| views.remove(&v.id)).collect())
}

// ---------- writes ----------

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuestionInput {
    pub component_id: String,
    /// MC, Numeric or Open. Fixed for life.
    #[serde(rename = "type")]
    #[schema(rename = "type")]
    pub qtype: QuestionType,
    /// Manual (default), Lecture, Tutorial, PastExam, Practice, Imported, Unknown. `Generated` comes from AI.
    pub source: Option<String>,
    pub source_label: Option<String>,
    pub paper_id: Option<String>,
    pub paper_question_no: Option<String>,
    /// Keep it to yourself instead of adding it to the course bank.
    #[serde(default)]
    pub private: bool,
    /// The same past-paper question as this one (shares its family; spec 5.2).
    pub family_of_id: Option<String>,
    #[serde(default)]
    pub concept_ids: Vec<String>,
    pub primary_concept_id: Option<String>,
    pub content: VersionInput,
}

#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuestionPatch {
    pub component_id: Option<String>,
    pub source_label: Option<String>,
    pub paper_id: Option<String>,
    pub paper_question_no: Option<String>,
    pub family_of_id: Option<String>,
    pub concept_ids: Option<Vec<String>>,
    pub primary_concept_id: Option<String>,
    /// New content = a new immutable version.
    pub content: Option<VersionInput>,
}

/// Inserts a version and its typed key. Returns the version id.
pub(crate) async fn write_version(
    conn: &mut MySqlConnection,
    ctx: &Ctx,
    actor: &Actor,
    q_id: i64,
    qt: QuestionType,
    v: &VersionInput,
) -> AppResult<i64> {
    let public_id = ctx.ids.ulid();
    let vid = repo::insert_version(
        conn,
        &repo::NewVersion {
            public_id: &public_id,
            question_id: q_id,
            qtype: qt.as_db(),
            prompt: v.prompt.trim(),
            image_file_ref: v.image_file_id.as_deref(),
            explanation: v.explanation.as_deref(),
            key_reliability: v.key_reliability.as_deref().unwrap_or("Derived"),
            key_note: v.key_note.as_deref(),
            change_note: v.change_note.as_deref(),
            created_by: actor.user_id,
        },
    )
    .await?;
    match qt {
        QuestionType::MC => {
            let opts: Vec<(String, &str, bool, Option<&str>)> = v
                .options
                .as_deref()
                .unwrap_or(&[])
                .iter()
                .map(|o| (ctx.ids.ulid(), o.text.trim(), o.is_correct, o.distractor_note.as_deref()))
                .collect();
            repo::insert_mc(conn, vid, v.shuffle_options, &opts).await?;
        }
        QuestionType::Numeric => {
            let n = v.numeric.as_ref().ok_or_else(|| ValidationErrors::single("numeric", "required", "Give the answer value."))?;
            repo::insert_numeric(
                conn,
                vid,
                &repo::NumericRow {
                    question_version_id: vid,
                    value: n.value,
                    tolerance: n.tolerance,
                    tolerance_kind: n.tolerance_kind.clone(),
                    unit: n.unit.clone().filter(|u| !u.trim().is_empty()),
                    unit_required: n.unit_required,
                    model_answer: n.model_answer.clone(),
                },
            )
            .await?;
        }
        QuestionType::Open => {
            let o = v.open.clone().unwrap_or_default();
            let rubric =
                o.rubric.map(|rows| json!(rows.iter().map(|r| json!({"criterion": r.criterion, "points": r.points})).collect::<Vec<_>>()));
            repo::insert_open(conn, vid, o.model_answer.as_deref(), o.rubric_note.as_deref(), rubric).await?;
        }
    }
    Ok(vid)
}

async fn concept_links(conn: &mut MySqlConnection, course_id: i64, refs: &[String], primary: Option<&str>) -> AppResult<Vec<(i64, bool)>> {
    let found = repo::concept_ids(conn, course_id, refs).await?;
    let unique: std::collections::HashSet<&String> = refs.iter().collect();
    if found.len() != unique.len() {
        return Err(ValidationErrors::single("conceptIds", "unknown", "Concepts must belong to this course.").into());
    }
    if primary.is_some_and(|p| !refs.iter().any(|r| r == p)) {
        return Err(ValidationErrors::single("primaryConceptId", "unknown", "The primary concept must be one of conceptIds.").into());
    }
    Ok(found.into_iter().map(|(id, r)| (id, primary == Some(r.as_str()))).collect())
}

async fn resolve_component(conn: &mut MySqlConnection, course_id: i64, r: &str) -> AppResult<i64> {
    let found = crux_repo::catalog::component_ids_by_public(conn, course_id, &[r.to_string()]).await?;
    found.first().map(|c| c.0).ok_or_else(|| ValidationErrors::single("componentId", "unknown", "Pick a lecture of this course.").into())
}

async fn resolve_paper(conn: &mut MySqlConnection, course_id: i64, r: Option<&str>) -> AppResult<Option<i64>> {
    match r {
        Some(r) => Ok(Some(
            repo::paper_id(conn, course_id, r)
                .await?
                .ok_or_else(|| ValidationErrors::single("paperId", "unknown", "Pick a paper of this course."))?,
        )),
        None => Ok(None),
    }
}

async fn resolve_family(conn: &mut MySqlConnection, course_id: i64, r: Option<&str>) -> AppResult<Option<i64>> {
    match r {
        Some(r) => Ok(Some(
            repo::family_root(conn, course_id, r)
                .await?
                .ok_or_else(|| ValidationErrors::single("familyOfId", "unknown", "Pick a question of this course."))?,
        )),
        None => Ok(None),
    }
}

pub async fn create(ctx: &Ctx, actor: &Actor, course_ref: &str, input: QuestionInput) -> AppResult<QuestionView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let course = readable(&mut tx, actor, course_ref).await?;
    // Bank questions need edit rights; a private question only read access.
    if !input.private && !can_edit_course(&access_of(&course)) {
        return Err(AppError::NotFound);
    }
    let source = input.source.clone().unwrap_or_else(|| "Manual".into());
    let mut e = ValidationErrors::new();
    if !question::SOURCES.contains(&source.as_str()) || source == "Generated" {
        e.push("source", "invalid", "Unknown source.");
    }
    e.merge(question::validate_version(input.qtype, &input.content, &source));
    e.merge(question::validate_paper_link(input.paper_id.as_deref(), input.paper_question_no.as_deref()));
    e.into_result()?;
    let component_id = resolve_component(&mut tx, course.id, &input.component_id).await?;
    let paper_id = resolve_paper(&mut tx, course.id, input.paper_id.as_deref()).await?;
    let family_id = resolve_family(&mut tx, course.id, input.family_of_id.as_deref()).await?;
    let concepts = concept_links(&mut tx, course.id, &input.concept_ids, input.primary_concept_id.as_deref()).await?;
    let public_id = ctx.ids.ulid();
    let qid = repo::insert_question(
        &mut tx,
        &repo::NewQuestion {
            public_id: &public_id,
            course_id: course.id,
            component_id,
            qtype: input.qtype.as_db(),
            owner_user_id: input.private.then_some(actor.user_id),
            source: &source,
            source_label: input.source_label.as_deref(),
            paper_id,
            paper_question_no: input.paper_question_no.as_deref(),
            family_id,
            review_state: "Approved",
            created_by: actor.user_id,
        },
    )
    .await?;
    let vid = write_version(&mut tx, ctx, actor, qid, input.qtype, &input.content).await?;
    repo::set_current_version(&mut tx, qid, vid).await?;
    repo::set_concepts(&mut tx, qid, &concepts).await?;
    let row = repo::question_by_id(&mut tx, qid).await?.ok_or(AppError::NotFound)?;
    let view = question_views(&mut tx, actor.user_id, &course.public_id, &[row]).await?.pop().ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(view)
}

/// Spec 5.2: every edit inserts a new version; metadata changes ride along.
pub async fn patch(ctx: &Ctx, actor: &Actor, question_ref: &str, version: i32, p: QuestionPatch) -> AppResult<QuestionView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let (q, course) = writable(&mut tx, actor, question_ref).await?;
    let qt = QuestionType::from_db(&q.qtype);
    let mut e = ValidationErrors::new();
    if let Some(c) = &p.content {
        e.merge(question::validate_version(qt, c, &q.source));
    }
    let paper_ref = p.paper_id.clone().or(q.paper_ref.clone());
    let number = p.paper_question_no.clone().or(q.paper_question_no.clone());
    e.merge(question::validate_paper_link(paper_ref.as_deref(), number.as_deref()));
    e.into_result()?;
    if q.row_version != version {
        let current = question_views(&mut tx, actor.user_id, &course.public_id, std::slice::from_ref(&q)).await?.pop();
        return Err(AppError::Conflict {
            message: "Changed somewhere else.".into(),
            current: current.and_then(|c| serde_json::to_value(c).ok()),
        });
    }
    let component_id = match &p.component_id {
        Some(r) => resolve_component(&mut tx, course.id, r).await?,
        None => q.component_id,
    };
    let paper_id = resolve_paper(&mut tx, course.id, paper_ref.as_deref()).await?;
    let family_id = match &p.family_of_id {
        Some(r) => resolve_family(&mut tx, course.id, Some(r)).await?,
        None => q.family_id,
    };
    let current_version = match &p.content {
        Some(c) => write_version(&mut tx, ctx, actor, q.id, qt, c).await?,
        None => q.current_version_id.ok_or(AppError::NotFound)?,
    };
    repo::update_question(
        &mut tx,
        q.id,
        version,
        current_version,
        &repo::QuestionMeta {
            component_id,
            source_label: if p.source_label.is_some() { p.source_label.as_deref() } else { q.source_label.as_deref() },
            paper_id,
            paper_question_no: number.as_deref(),
            family_id,
        },
    )
    .await?;
    if let Some(refs) = &p.concept_ids {
        let concepts = concept_links(&mut tx, course.id, refs, p.primary_concept_id.as_deref()).await?;
        repo::set_concepts(&mut tx, q.id, &concepts).await?;
    }
    let row = repo::question_by_id(&mut tx, q.id).await?.ok_or(AppError::NotFound)?;
    let view = question_views(&mut tx, actor.user_id, &course.public_id, &[row]).await?.pop().ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(view)
}

pub async fn archive(ctx: &Ctx, actor: &Actor, question_ref: &str) -> AppResult<QuestionView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let (q, course) = writable(&mut tx, actor, question_ref).await?;
    repo::archive(&mut tx, q.id, ctx.clock.now()).await?;
    let row = repo::question_by_id(&mut tx, q.id).await?.ok_or(AppError::NotFound)?;
    let view = question_views(&mut tx, actor.user_id, &course.public_id, &[row]).await?.pop().ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(view)
}

/// Draft → Approved. Bank drafts need an editor; private drafts their owner.
pub async fn approve(ctx: &Ctx, actor: &Actor, question_ref: &str) -> AppResult<QuestionView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let (q, course) = writable(&mut tx, actor, question_ref).await?;
    if q.review_state != "Approved" {
        repo::set_review_state(&mut tx, q.id, "Approved").await?;
    }
    let row = repo::question_by_id(&mut tx, q.id).await?.ok_or(AppError::NotFound)?;
    let view = question_views(&mut tx, actor.user_id, &course.public_id, &[row]).await?.pop().ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(view)
}

/// Review section 2: a private question reaches the course bank only by this explicit action, by
/// its owner, who must also be able to edit the course.
pub async fn promote(ctx: &Ctx, actor: &Actor, question_ref: &str) -> AppResult<QuestionView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let (q, course) = visible(&mut tx, actor, question_ref).await?;
    if q.owner_user_id != Some(actor.user_id) || !can_edit_course(&access_of(&course)) {
        return Err(AppError::NotFound);
    }
    repo::set_owner(&mut tx, q.id, None).await?;
    let row = repo::question_by_id(&mut tx, q.id).await?.ok_or(AppError::NotFound)?;
    let view = question_views(&mut tx, actor.user_id, &course.public_id, &[row]).await?.pop().ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(view)
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CropBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CropInput {
    /// A PDF of this course (an uploaded original).
    pub file_id: String,
    /// 1-based page.
    pub page: i32,
    /// In PDF points from the page's top-left.
    #[serde(rename = "box")]
    #[schema(rename = "box")]
    pub crop_box: CropBox,
}

/// Spec 5.2: cut a question image from a page (files renders and stores it), then save a new
/// version that shows it. The crop never goes away with its original (spec 6.2).
pub async fn image_crop(ctx: &Ctx, actor: &Actor, question_ref: &str, input: CropInput) -> AppResult<QuestionView> {
    let b = &input.crop_box;
    if input.page < 1 || !(b.w > 0.0 && b.h > 0.0 && b.x >= 0.0 && b.y >= 0.0) {
        return Err(ValidationErrors::single("box", "invalid", "Give a page ≥ 1 and a box with positive size.").into());
    }
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let (q, course) = writable(&mut tx, actor, question_ref).await?;
    let token = ctx.keys.sign_service("core", "files", ctx.clock.now().timestamp());
    let source = ctx.files.get_file(&token, &actor.public_id, &input.file_id).await.map_err(|e| anyhow::anyhow!("files: {e}"))?;
    let belongs =
        source.as_ref().is_some_and(|f| f.course_ref.as_deref() == Some(course.public_id.as_str()) || f.owner_ref == course.owner_ref);
    if !belongs {
        return Err(ValidationErrors::single("fileId", "unknown", "No such file in this course.").into());
    }
    let crop = ctx
        .files
        .crop(&token, &actor.public_id, &input.file_id, input.page, [b.x, b.y, b.w, b.h], &course.public_id)
        .await
        .map_err(|e| anyhow::anyhow!("files crop: {e}"))?;
    let vid = q.current_version_id.ok_or(AppError::NotFound)?;
    let current = repo::version(&mut tx, vid).await?.ok_or(AppError::NotFound)?;
    let qtypes = HashMap::from([(q.id, q.qtype.clone())]);
    let view = version_views(&mut tx, std::slice::from_ref(&current), &qtypes).await?.remove(&vid).ok_or(AppError::NotFound)?;
    let mut input_v = version_input_from(&view);
    input_v.image_file_id = Some(crop.id);
    input_v.change_note = Some("Question image cropped from a page".into());
    let new_vid = write_version(&mut tx, ctx, actor, q.id, QuestionType::from_db(&q.qtype), &input_v).await?;
    repo::update_question(
        &mut tx,
        q.id,
        q.row_version,
        new_vid,
        &repo::QuestionMeta {
            component_id: q.component_id,
            source_label: q.source_label.as_deref(),
            paper_id: q.paper_id,
            paper_question_no: q.paper_question_no.as_deref(),
            family_id: q.family_id,
        },
    )
    .await?;
    let row = repo::question_by_id(&mut tx, q.id).await?.ok_or(AppError::NotFound)?;
    let out = question_views(&mut tx, actor.user_id, &course.public_id, &[row]).await?.pop().ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(out)
}

/// A stored version as input for the next one (same content, new id).
pub(crate) fn version_input_from(v: &VersionView) -> VersionInput {
    VersionInput {
        prompt: v.prompt.clone(),
        explanation: v.explanation.clone(),
        image_file_id: v.image_file_id.clone(),
        key_reliability: Some(v.key_reliability.clone()),
        key_note: v.key_note.clone(),
        change_note: None,
        options: v.options.as_ref().map(|os| {
            os.iter()
                .map(|o| question::OptionInput {
                    text: o.text.clone(),
                    is_correct: o.is_correct,
                    distractor_note: o.distractor_note.clone(),
                })
                .collect()
        }),
        shuffle_options: v.shuffle_options.unwrap_or(true),
        numeric: v.numeric.as_ref().map(|n| question::NumericKey {
            value: n.value,
            tolerance: n.tolerance,
            tolerance_kind: n.tolerance_kind.clone(),
            unit: n.unit.clone(),
            unit_required: n.unit_required,
            model_answer: n.model_answer.clone(),
        }),
        open: v.open.as_ref().map(|o| OpenKey {
            model_answer: o.model_answer.clone(),
            rubric_note: o.rubric_note.clone(),
            rubric: o.rubric.as_ref().and_then(|r| serde_json::from_value(r.clone()).ok()),
        }),
    }
}

// ---------- generated questions (from ai, spec 7.5) ----------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedQuestion {
    #[serde(rename = "type")]
    pub qtype: QuestionType,
    pub content: VersionInput,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedInput {
    pub user: String,
    pub course: String,
    pub component: String,
    pub concept: Option<String>,
    pub seed_question: Option<String>,
    /// UserAiSettings.AutoApproveGenerated: Approved, else Draft.
    #[serde(default)]
    pub auto_approve: bool,
    pub questions: Vec<GeneratedQuestion>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedResult {
    pub created: Vec<String>,
    /// Index into `questions` and why it was refused.
    pub rejected: Vec<(usize, String)>,
}

/// `POST /internal/v1/questions/generated`. Core re-checks access for the user (spec 9.2) and the
/// rules: Source Generated, KeyReliability Derived, the user's own question (spec 7.5).
pub async fn create_generated(ctx: &Ctx, input: GeneratedInput) -> AppResult<GeneratedResult> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let actor = crate::ai::actor_of(&mut tx, &input.user).await?;
    let course = readable(&mut tx, &actor, &input.course).await?;
    let component_id = resolve_component(&mut tx, course.id, &input.component).await?;
    let concepts = match &input.concept {
        Some(c) => concept_links(&mut tx, course.id, std::slice::from_ref(c), Some(c)).await?,
        None => Vec::new(),
    };
    let seed_id = match &input.seed_question {
        Some(r) => repo::question(&mut tx, r).await?.filter(|q| q.course_id == course.id).map(|q| q.id),
        None => None,
    };
    let mut out = GeneratedResult { created: Vec::new(), rejected: Vec::new() };
    for (i, g) in input.questions.into_iter().enumerate().take(50) {
        let mut content = g.content;
        content.key_reliability = Some("Derived".into());
        let errors = question::validate_version(g.qtype, &content, "Generated");
        if !errors.is_empty() {
            out.rejected.push((i, errors.0.first().map(|e| e.code.clone()).unwrap_or_default()));
            continue;
        }
        let public_id = ctx.ids.ulid();
        let qid = repo::insert_question(
            &mut tx,
            &repo::NewQuestion {
                public_id: &public_id,
                course_id: course.id,
                component_id,
                qtype: g.qtype.as_db(),
                owner_user_id: Some(actor.user_id),
                source: "Generated",
                source_label: Some("AI"),
                paper_id: None,
                paper_question_no: None,
                family_id: None,
                review_state: if input.auto_approve { "Approved" } else { "Draft" },
                created_by: actor.user_id,
            },
        )
        .await?;
        let vid = write_version(&mut tx, ctx, &actor, qid, g.qtype, &content).await?;
        repo::set_current_version(&mut tx, qid, vid).await?;
        repo::set_concepts(&mut tx, qid, &concepts).await?;
        if let Some(seed) = seed_id {
            repo::set_seed(&mut tx, qid, seed).await?;
        }
        out.created.push(public_id);
    }
    tx.commit().await?;
    Ok(out)
}
