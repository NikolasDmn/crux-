//! AI course import, applied by core (backend-spec 7.5): one transaction, idempotent on the import
//! ref, proposals in topological order (Course → Component → Concept → Material → Paper → Question
//! → Flashcard → Deadline → DeadlineComponent → PlanEntry → ExamAids) with TempKeys resolved.
//! Every proposal is checked like the matching API call would; a bad one fails alone (with the
//! proposals that depend on it), the rest still apply.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use crux_domain::catalog::CourseFields;
use crux_domain::question::{QuestionType, VersionInput};
use crux_repo::MySqlConnection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::courses::{editable, readable};
use crate::{Actor, AppError, AppResult, Ctx};

pub const ORDER: [&str; 11] = [
    "Course",
    "Component",
    "Concept",
    "Material",
    "Paper",
    "Question",
    "Flashcard",
    "Deadline",
    "DeadlineComponent",
    "PlanEntry",
    "ExamAids",
];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalIn {
    #[serde(rename = "ref")]
    pub proposal_ref: String,
    pub kind: String,
    pub temp_key: String,
    pub parent_temp_key: Option<String>,
    /// The proposal's payload with the user's edits applied.
    pub payload: Value,
    pub source_file_ref: Option<String>,
    pub source_page: Option<i32>,
    /// An existing row of the course this proposal matches (ai's duplicate check). An accepted
    /// Component, Concept or Paper duplicate links to that row instead of creating another, so its
    /// children land on it; other kinds are created as the user accepted them.
    #[serde(default)]
    pub duplicate_of: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyInput {
    pub user: String,
    pub import_ref: String,
    /// An existing course (Owner or Editor), or none: the Course proposal creates one.
    pub course: Option<String>,
    pub proposals: Vec<ProposalIn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Created {
    pub proposal_ref: String,
    pub entity_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Failed {
    pub proposal_ref: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub course_ref: String,
    pub created: Vec<Created>,
    pub failed: Vec<Failed>,
}

/// What a TempKey stands for once created: (kind, internal id, PublicId).
type Keys = HashMap<String, (String, i64, String)>;
/// Exam layouts gathered per deadline: (component, question count, open count).
type Layouts = HashMap<i64, Vec<(i64, Option<i32>, Option<i32>)>>;

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str).map(str::trim).filter(|x| !x.is_empty())
}

fn parent(keys: &Keys, p: &ProposalIn, field: &str, kind: &str) -> Result<i64, String> {
    let key = p.payload.get(field).and_then(Value::as_str).map(String::from).or_else(|| p.parent_temp_key.clone());
    match key.and_then(|k| keys.get(&k).cloned()) {
        Some((k, id, _)) if k == kind => Ok(id),
        _ => Err(format!("parent_{}", kind.to_lowercase())),
    }
}

pub async fn apply(ctx: &Ctx, input: ApplyInput) -> AppResult<ApplyResult> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let actor = crate::ai::actor_of(&mut tx, &input.user).await?;
    if let Some(done) = crux_repo::courses::import_result(&mut tx, &input.import_ref).await? {
        return Ok(serde_json::from_value(done.0).map_err(anyhow::Error::from)?);
    }
    let now = ctx.clock.now();
    let mut keys: Keys = HashMap::new();
    let mut created = Vec::new();
    let mut failed = Vec::new();

    // The course: an existing one the user can edit, or the Course proposal.
    let mut course_id = match &input.course {
        Some(r) => Some(editable(&mut tx, &actor, r).await?.id),
        None => None,
    };
    let mut proposals = input.proposals.clone();
    proposals.sort_by_key(|p| ORDER.iter().position(|k| *k == p.kind).unwrap_or(ORDER.len()));
    let mut layouts: Layouts = HashMap::new();
    for p in &proposals {
        if !ORDER.contains(&p.kind.as_str()) {
            failed.push(Failed { proposal_ref: p.proposal_ref.clone(), reason: "unknown_kind".into() });
            continue;
        }
        if p.kind != "Course" && course_id.is_none() {
            failed.push(Failed { proposal_ref: p.proposal_ref.clone(), reason: "no_course".into() });
            continue;
        }
        let linked = match (&p.duplicate_of, course_id) {
            (Some(r), Some(cid)) => existing(&mut tx, p.kind.as_str(), r, cid).await?,
            _ => None,
        };
        let outcome = match linked {
            Some(id) => Ok(Some((id, p.duplicate_of.clone().unwrap_or_default()))),
            None => apply_one(&mut tx, ctx, &actor, p, &mut course_id, &keys, &mut layouts, now).await,
        };
        match outcome {
            Ok(Some((id, entity_ref))) => {
                keys.insert(p.temp_key.clone(), (p.kind.clone(), id, entity_ref.clone()));
                created.push(Created { proposal_ref: p.proposal_ref.clone(), entity_ref });
            }
            Ok(None) => {}
            Err(reason) => failed.push(Failed { proposal_ref: p.proposal_ref.clone(), reason }),
        }
    }
    for (deadline_id, rows) in layouts {
        crux_repo::deadlines::set_layout(&mut tx, deadline_id, &rows).await?;
    }
    let course_id = course_id
        .ok_or_else(|| AppError::from(crux_domain::ValidationErrors::single("course", "required", "The import has no course.")))?;
    let course_ref = crux_repo::courses::public_id_of(&mut tx, course_id).await?;
    let result = ApplyResult { course_ref, created, failed };
    crux_repo::courses::save_import_result(
        &mut tx,
        &input.import_ref,
        actor.user_id,
        Some(course_id),
        &serde_json::to_value(&result).map_err(anyhow::Error::from)?,
        now,
    )
    .await?;
    crate::plan::rebuild_in(&mut tx, ctx, actor.user_id).await?;
    tx.commit().await?;
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
async fn apply_one(
    tx: &mut MySqlConnection,
    ctx: &Ctx,
    actor: &Actor,
    p: &ProposalIn,
    course_id: &mut Option<i64>,
    keys: &Keys,
    layouts: &mut Layouts,
    now: DateTime<Utc>,
) -> Result<Option<(i64, String)>, String> {
    let v = &p.payload;
    let db = |e: AppError| match e {
        AppError::Validation(v) => v.0.first().map(|f| format!("{}: {}", f.field, f.code)).unwrap_or_else(|| "invalid".into()),
        other => format!("{other}"),
    };
    match p.kind.as_str() {
        "Course" => {
            if let Some(id) = *course_id {
                // Importing into an existing course: the Course proposal stands for it.
                let r = crux_repo::courses::public_id_of(tx, id).await.map_err(|e| e.to_string())?;
                return Ok(Some((id, r)));
            }
            let f = CourseFields {
                name: s(v, "name").map(String::from),
                code: s(v, "code").map(String::from),
                institution: s(v, "institution").map(String::from),
                term: s(v, "term").map(String::from),
                color: None,
                grade_scale_id: None,
            };
            let id = crate::courses::create_in(tx, ctx, actor, f).await.map_err(db)?;
            *course_id = Some(id);
            let r = crux_repo::courses::public_id_of(tx, id).await.map_err(|e| e.to_string())?;
            Ok(Some((id, r)))
        }
        "Component" => {
            let cid = course_id.expect("checked");
            let (code, name) = (s(v, "code").ok_or("code: required")?, s(v, "name").ok_or("name: required")?);
            let public_id = ctx.ids.ulid();
            let sort = crux_repo::catalog::next_component_sort(tx, cid).await.map_err(|e| e.to_string())?;
            let id = crux_repo::catalog::insert_component(
                tx,
                &public_id,
                cid,
                &crux_repo::catalog::ComponentWrite {
                    code: &code.chars().take(16).collect::<String>(),
                    name: &name.chars().take(255).collect::<String>(),
                    description: s(v, "description"),
                    sort_order: sort,
                    est_hours: v.get("estHours").and_then(Value::as_f64).filter(|h| (0.0..=500.0).contains(h)),
                },
            )
            .await
            .map_err(|e| e.to_string())?;
            Ok(Some((id, public_id)))
        }
        "Concept" => {
            let public_id = ctx.ids.ulid();
            let name = s(v, "name").ok_or("name: required")?;
            let id = crux_repo::catalog::insert_concept(
                tx,
                &public_id,
                course_id.expect("checked"),
                &name.chars().take(255).collect::<String>(),
            )
            .await
            .map_err(|e| e.to_string())?;
            Ok(Some((id, public_id)))
        }
        "Material" => {
            let component_id = parent(keys, p, "componentTempKey", "Component")?;
            let course = course_row(tx, actor, course_id.expect("checked")).await.map_err(db)?;
            let file_ref = s(v, "fileRef").or(p.source_file_ref.as_deref());
            let file = match file_ref {
                Some(r) => Some(crate::catalog::usable_file(ctx, actor, &course, r, "fileRef").await.map_err(db)?),
                None => None,
            };
            let content = s(v, "content");
            if file.is_none() && content.is_none() {
                return Err("content: required".into());
            }
            let kind =
                s(v, "kind").filter(|k| ["Slides", "Textbook", "Exercise", "PastExam", "Notes", "Other"].contains(k)).unwrap_or("Other");
            let public_id = ctx.ids.ulid();
            let sort = crux_repo::catalog::next_material_sort(tx, component_id).await.map_err(|e| e.to_string())?;
            let id = crux_repo::catalog::insert_material(
                tx,
                &public_id,
                component_id,
                &crux_repo::catalog::MaterialWrite {
                    kind,
                    title: &s(v, "title").unwrap_or("Imported material").chars().take(255).collect::<String>(),
                    content: if file.is_some() { None } else { content },
                    file_ref: file.as_ref().map(|f| f.id.as_str()),
                    file_sha256: file.as_ref().map(|f| f.sha256.as_str()),
                    original_status: if file.is_some() { "Present" } else { "None" },
                    page_from: v.get("pageFrom").and_then(Value::as_i64).map(|x| x as i32),
                    page_to: v.get("pageTo").and_then(Value::as_i64).map(|x| x as i32),
                    excluded: false,
                    excluded_note: None,
                    ai_allowed: true,
                    sort_order: sort,
                },
            )
            .await
            .map_err(|e| e.to_string())?;
            Ok(Some((id, public_id)))
        }
        "Paper" => {
            let public_id = ctx.ids.ulid();
            let key = s(v, "paperKey").ok_or("paperKey: required")?;
            // The payload (and the user's edits to it) are untrusted: a fileRef must pass the same
            // check as `POST /courses/{id}/papers`, or a paper would grant its readers any file (audit H3).
            let course = course_row(tx, actor, course_id.expect("checked")).await.map_err(db)?;
            let file = match s(v, "fileRef").or(p.source_file_ref.as_deref()) {
                Some(r) => Some(crate::catalog::usable_file(ctx, actor, &course, r, "fileRef").await.map_err(db)?),
                None => None,
            };
            let id = crux_repo::catalog::insert_paper(
                tx,
                &public_id,
                course_id.expect("checked"),
                &crux_repo::catalog::PaperWrite {
                    paper_key: &key.chars().take(32).collect::<String>(),
                    label: &s(v, "label").unwrap_or(key).chars().take(255).collect::<String>(),
                    exam_date: s(v, "examDate").and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()),
                    file_ref: file.as_ref().map(|f| f.id.as_str()),
                    total_questions: v.get("totalQuestions").and_then(Value::as_i64).map(|x| x as i32),
                    key_status: s(v, "keyStatus").filter(|k| ["Official", "Partial", "Derived", "None"].contains(k)).unwrap_or("None"),
                    note: s(v, "note"),
                },
            )
            .await
            .map_err(|e| e.to_string())?;
            Ok(Some((id, public_id)))
        }
        "Question" => {
            let component_id = parent(keys, p, "componentTempKey", "Component")?;
            let qtype: QuestionType = serde_json::from_value(v.get("type").cloned().unwrap_or(Value::Null)).map_err(|_| "type: invalid")?;
            let mut content: VersionInput =
                serde_json::from_value(v.get("content").cloned().unwrap_or(Value::Null)).map_err(|_| "content: invalid")?;
            content.image_file_id = None; // model output never names a file (audit M2)
            let mut paper = v
                .get("paperTempKey")
                .and_then(Value::as_str)
                .and_then(|k| keys.get(k))
                .filter(|(k, ..)| k == "Paper")
                .map(|(_, id, _)| *id);
            if let (None, Some(r)) = (paper, s(v, "paperId")) {
                // An existing paper of this course (past exam -> questions, spec 7.5).
                let found = crux_repo::catalog::paper(tx, r).await.map_err(|e| e.to_string())?;
                paper = found.filter(|p| Some(p.course_id) == *course_id).map(|p| p.id);
                if paper.is_none() {
                    return Err("paperId: unknown".into());
                }
            }
            // Official only for a key found in the same paper's answer file (ai says so with a paper link).
            if content.key_reliability.as_deref() == Some("Official") && paper.is_none() {
                content.key_reliability = Some("Derived".into());
            }
            let source = if paper.is_some() { "PastExam" } else { "Imported" };
            let errors = crux_domain::question::validate_version(qtype, &content, source);
            if !errors.is_empty() {
                return Err(errors.0.first().map(|e| format!("{}: {}", e.field, e.code)).unwrap_or_default());
            }
            let public_id = ctx.ids.ulid();
            let qid = crux_repo::bank::insert_question(
                tx,
                &crux_repo::bank::NewQuestion {
                    public_id: &public_id,
                    course_id: course_id.expect("checked"),
                    component_id,
                    qtype: qtype.as_db(),
                    owner_user_id: None,
                    source,
                    source_label: Some("AI import"),
                    paper_id: paper,
                    paper_question_no: s(v, "paperQuestionNo"),
                    family_id: None,
                    review_state: "Approved",
                    created_by: actor.user_id,
                },
            )
            .await
            .map_err(|e| e.to_string())?;
            let vid = crate::bank::write_version(tx, ctx, actor, qid, qtype, &content).await.map_err(db)?;
            crux_repo::bank::set_current_version(tx, qid, vid).await.map_err(|e| e.to_string())?;
            let concepts: Vec<(i64, bool)> = v
                .get("conceptTempKeys")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .filter_map(|k| keys.get(k))
                        .filter(|(k, ..)| k == "Concept")
                        .enumerate()
                        .map(|(i, (_, id, _))| (*id, i == 0))
                        .collect()
                })
                .unwrap_or_default();
            crux_repo::bank::set_concepts(tx, qid, &concepts).await.map_err(|e| e.to_string())?;
            Ok(Some((qid, public_id)))
        }
        "Flashcard" => {
            let component_id = parent(keys, p, "componentTempKey", "Component")?;
            let cid = course_id.expect("checked");
            let (front, back) = (s(v, "front").ok_or("front: required")?, s(v, "back").ok_or("back: required")?);
            let course = course_row(tx, actor, cid).await.map_err(db)?;
            crux_repo::flash::lock_course(tx, cid).await.map_err(|e| e.to_string())?;
            let code = crux_domain::fsrs::next_code(&course.code, &crux_repo::flash::codes(tx, cid).await.map_err(|e| e.to_string())?);
            let public_id = ctx.ids.ulid();
            let id = crux_repo::flash::insert(
                tx,
                &crux_repo::flash::NewCard {
                    public_id: &public_id,
                    course_id: cid,
                    component_id,
                    code: &code,
                    front,
                    back,
                    source: "Lecture",
                    owner_user_id: None,
                    slip_id: None,
                },
            )
            .await
            .map_err(|e| e.to_string())?;
            Ok(Some((id, public_id)))
        }
        "Deadline" => {
            let cid = course_id.expect("checked");
            let (sub, _) =
                crux_repo::deadlines::subscription_id(tx, actor.user_id, cid).await.map_err(|e| e.to_string())?.ok_or("not_subscribed")?;
            let starts_at = s(v, "startsAt")
                .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                .map(|t| t.with_timezone(&Utc))
                .ok_or("startsAt: required")?;
            let dtype = s(v, "type").filter(|t| ["Exam", "Resit", "Deadline"].contains(t)).unwrap_or("Deadline");
            let public_id = ctx.ids.ulid();
            let id = crux_repo::deadlines::insert(
                tx,
                &public_id,
                sub,
                &crux_repo::deadlines::DeadlineWrite {
                    dtype,
                    title: s(v, "title"),
                    resit_of: None,
                    starts_at,
                    duration_min: v.get("durationMin").and_then(Value::as_i64).map(|x| x as i32).filter(|d| (1..=1440).contains(d)),
                    format: s(v, "format"),
                    pass_mark: v.get("passMark").and_then(Value::as_f64),
                    status: if starts_at < now { "Done" } else { "Upcoming" },
                    grade_value: None,
                    grade_text: None,
                    grade_provisional: false,
                    check_in_note: None,
                },
            )
            .await
            .map_err(|e| e.to_string())?;
            Ok(Some((id, public_id)))
        }
        "DeadlineComponent" => {
            let deadline_id = parent(keys, p, "deadlineTempKey", "Deadline")?;
            let component_id = match v.get("componentTempKey").and_then(Value::as_str).and_then(|k| keys.get(k)) {
                Some((k, id, _)) if k == "Component" => *id,
                _ => return Err("parent_component".into()),
            };
            let q = v.get("questionCount").and_then(Value::as_i64).map(|x| x as i32).filter(|x| *x >= 0);
            let o = v.get("openCount").and_then(Value::as_i64).map(|x| x as i32).filter(|x| *x >= 0 && q.is_none_or(|q| *x <= q));
            layouts.entry(deadline_id).or_default().push((component_id, q, o));
            Ok(None)
        }
        "PlanEntry" => {
            let cid = course_id.expect("checked");
            let start = s(v, "startAt")
                .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                .map(|t| t.with_timezone(&Utc))
                .ok_or("startAt: required")?;
            let end =
                s(v, "endAt").and_then(|t| DateTime::parse_from_rfc3339(t).ok()).map(|t| t.with_timezone(&Utc)).ok_or("endAt: required")?;
            if end <= start || end - start > chrono::Duration::hours(16) {
                return Err("endAt: order".into());
            }
            let component = v
                .get("componentTempKey")
                .and_then(Value::as_str)
                .and_then(|k| keys.get(k))
                .filter(|(k, ..)| k == "Component")
                .map(|(_, id, _)| *id);
            let public_id = ctx.ids.ulid();
            let id = crux_repo::plan::insert(
                tx,
                &crux_repo::plan::NewEntry {
                    public_id: &public_id,
                    user_id: actor.user_id,
                    course_id: cid,
                    component_id: component,
                    kind: s(v, "kind").filter(|k| ["Lecture", "Revision", "Mock", "Other"].contains(k)).unwrap_or("Other"),
                    description: &s(v, "description").unwrap_or("Imported plan entry").chars().take(2000).collect::<String>(),
                    start_at: start,
                    end_at: end,
                    status: "Planned",
                    created_by: "AI",
                    pinned_at: Some(now),
                },
            )
            .await
            .map_err(|e| e.to_string())?;
            Ok(Some((id, public_id)))
        }
        "ExamAids" => {
            crux_repo::courses::set_aids(
                tx,
                actor.user_id,
                course_id.expect("checked"),
                s(v, "calculator"),
                s(v, "cheatSheet"),
                s(v, "other"),
            )
            .await
            .map_err(|e| e.to_string())?;
            Ok(None)
        }
        _ => Err("unknown_kind".into()),
    }
}

/// The internal id of an existing Component, Concept or Paper of this course, by PublicId.
async fn existing(tx: &mut MySqlConnection, kind: &str, public_id: &str, course_id: i64) -> AppResult<Option<i64>> {
    Ok(match kind {
        "Component" => crux_repo::catalog::component(tx, public_id).await?.filter(|k| k.course_id == course_id).map(|k| k.id),
        "Concept" => {
            crux_repo::catalog::concepts(tx, course_id).await?.into_iter().find(|c| c.public_id.as_deref() == Some(public_id)).map(|c| c.id)
        }
        "Paper" => crux_repo::catalog::paper(tx, public_id).await?.filter(|p| p.course_id == course_id).map(|p| p.id),
        _ => None,
    })
}

async fn course_row(tx: &mut MySqlConnection, actor: &Actor, course_id: i64) -> AppResult<crux_repo::courses::CourseRow> {
    let r = crux_repo::courses::public_id_of(tx, course_id).await?;
    readable(tx, actor, &r).await
}
