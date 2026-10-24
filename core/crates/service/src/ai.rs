//! Core's side of AI (backend-spec 7): the gate through the ai service, and the internal answers
//! ai needs about materials. Core never trusts ai for authorization (spec 9.2): every answer here
//! re-checks the user's access.

use crux_repo::MySqlConnection;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::courses::readable;
use crate::{Actor, AppError, AppResult, Ctx};

pub const FEATURES: [&str; 5] = ["MaterialChat", "Grading", "CourseImport", "PastExamImport", "Generation"];

fn token(ctx: &Ctx) -> String {
    ctx.keys.sign_service("core", "ai", ctx.clock.now().timestamp())
}

/// `None` = allowed, else the reason (`service_unavailable` when ai can't be reached).
pub async fn reason(ctx: &Ctx, actor: &Actor, feature: &str, course_ref: Option<&str>, materials: &[String]) -> Option<String> {
    match ctx.ai.allowed(&token(ctx), &actor.public_id, feature, course_ref, materials).await {
        Ok(a) if a.allowed => None,
        Ok(a) => Some(a.reason.unwrap_or_else(|| "feature_off".into())),
        Err(e) => {
            tracing::warn!(error = %e, "ai gate unreachable");
            Some("service_unavailable".into())
        }
    }
}

fn static_reason(r: &str) -> &'static str {
    match r {
        "user_off" => "user_off",
        "feature_off" => "feature_off",
        "course_off" => "course_off",
        "material_blocked" => "material_blocked",
        "no_key" => "no_key",
        _ => "service_unavailable",
    }
}

/// Fails with `403 ai_disabled {reason}` unless the gate allows it.
pub async fn gate(ctx: &Ctx, actor: &Actor, feature: &str, course_ref: Option<&str>) -> AppResult<()> {
    match reason(ctx, actor, feature, course_ref, &[]).await {
        None => Ok(()),
        Some(r) => Err(AppError::AiDisabled { reason: static_reason(&r) }),
    }
}

#[derive(Debug, Clone, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct AllowedQuery {
    /// MaterialChat, Grading, CourseImport, PastExamImport, Generation.
    pub feature: String,
    pub course_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AllowedView {
    pub allowed: bool,
    /// user_off, feature_off, course_off, material_blocked, no_key, service_unavailable.
    pub reason: Option<String>,
}

/// For the frontend: show or hide AI controls (spec 7.1: `service_unavailable` hides them).
pub async fn allowed(ctx: &Ctx, actor: &Actor, q: AllowedQuery) -> AppResult<AllowedView> {
    if !FEATURES.contains(&q.feature.as_str()) {
        return Err(crux_domain::ValidationErrors::single("feature", "invalid", "Unknown AI feature.").into());
    }
    if let Some(c) = &q.course_id {
        let mut conn = ctx.pool.acquire().await?;
        readable(&mut conn, actor, c).await?;
    }
    let r = reason(ctx, actor, &q.feature, q.course_id.as_deref(), &[]).await;
    Ok(AllowedView { allowed: r.is_none(), reason: r })
}

/// The actor for a user ref named by another service (`?user=`). 404 for unknown users.
pub async fn actor_of(conn: &mut MySqlConnection, user_ref: &str) -> AppResult<Actor> {
    let u = crux_repo::users::by_public_id(conn, user_ref).await?.ok_or(AppError::NotFound)?;
    Ok(Actor {
        user_id: u.id,
        public_id: u.public_id.clone(),
        role: crux_domain::user::Role::from_db(&u.role),
        email_verified: u.email_verified_at.is_some(),
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMaterial {
    pub material_ref: String,
    pub course_ref: String,
    pub component_ref: String,
    pub title: String,
    pub ai_allowed: bool,
    pub excluded: bool,
    pub file_ref: Option<String>,
    pub page_from: Option<i32>,
    pub page_to: Option<i32>,
}

/// `GET /internal/v1/ai/materials?user=&refs=`: only materials of courses the user can read.
pub async fn materials(ctx: &Ctx, user_ref: &str, refs: &[String]) -> AppResult<Vec<AiMaterial>> {
    let mut conn = ctx.pool.acquire().await?;
    let actor = actor_of(&mut conn, user_ref).await?;
    let mut out = Vec::new();
    for r in refs.iter().take(100) {
        let Some(m) = crux_repo::catalog::material(&mut conn, r).await? else { continue };
        let course_ref = crux_repo::courses::public_id_of(&mut conn, m.course_id).await?;
        if readable(&mut conn, &actor, &course_ref).await.is_err() {
            continue;
        }
        out.push(AiMaterial {
            material_ref: m.public_id.clone(),
            course_ref,
            component_ref: m.component_ref.clone(),
            title: m.title.clone(),
            ai_allowed: m.ai_allowed,
            excluded: m.excluded,
            file_ref: m.file_ref.clone(),
            page_from: m.page_from,
            page_to: m.page_to,
        });
    }
    Ok(out)
}

// ---------- context for the AI context builder (spec 7.4) ----------

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Anchor {
    pub material_id: Option<String>,
    pub component_id: Option<String>,
    pub question_id: Option<String>,
    pub mock_id: Option<String>,
    pub order: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextRequest {
    pub user: String,
    pub course: String,
    /// progress, mistakes, answers: the user's share scopes (spec 7.4 block 6).
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub anchor: Anchor,
}

/// `POST /internal/v1/context/user`: course facts, the materials AI may read, and the user's data
/// within the share scopes. A hidden answer key is never included (spec 7.4).
pub async fn context(ctx: &Ctx, req: ContextRequest) -> AppResult<serde_json::Value> {
    use serde_json::json;
    let mut conn = ctx.pool.acquire().await?;
    let actor = actor_of(&mut conn, &req.user).await?;
    let course = readable(&mut conn, &actor, &req.course).await?;
    let now = ctx.clock.now();
    let lectures = crux_repo::study::progress(&mut conn, actor.user_id, course.id).await?;
    let exam = crux_repo::deadlines::upcoming_exams(&mut conn, actor.user_id, now).await?.into_iter().find(|d| d.course_id == course.id);
    let mut materials = Vec::new();
    let mut exclusions = Vec::new();
    for k in crux_repo::catalog::components(&mut conn, course.id, false).await? {
        for m in crux_repo::catalog::materials(&mut conn, k.id).await? {
            if m.archived_at.is_some() {
                continue;
            }
            if m.excluded {
                exclusions.push(json!({ "materialId": m.public_id, "title": m.title, "note": m.excluded_note }));
            } else if m.ai_allowed {
                materials.push(json!({
                    "materialId": m.public_id, "componentId": k.public_id, "componentCode": k.code, "title": m.title,
                    "fileRef": m.file_ref, "pageFrom": m.page_from, "pageTo": m.page_to,
                    "content": m.content.as_ref().map(|c| c.chars().take(4000).collect::<String>()),
                }));
            }
        }
    }
    let mut out = json!({
        "course": {
            "id": course.public_id, "name": course.name, "code": course.code, "institution": course.institution,
            "exam": exam.map(|d| json!({ "startsAt": crate::time::iso(d.starts_at), "format": d.format, "durationMin": d.duration_min, "title": d.title })),
            "lectures": lectures.iter().map(|l| json!({ "id": l.component_ref, "code": l.code, "name": l.name })).collect::<Vec<_>>(),
            "exclusions": exclusions,
        },
        "materials": materials,
    });
    let has = |s: &str| req.scopes.iter().any(|x| x == s);
    if has("progress") {
        let day = crux_domain::fsrs::local_day_start(now, chrono_tz::Europe::Amsterdam);
        let plan = crux_repo::plan::between(&mut conn, actor.user_id, day, day + chrono::Duration::days(1)).await?;
        out["progress"] = json!({
            "lectures": lectures.iter().map(|l| json!({ "code": l.code, "status": l.status.clone().unwrap_or_else(|| "Todo".into()) })).collect::<Vec<_>>(),
            "planToday": plan.iter().filter(|e| e.course_id == course.id && e.status == "Planned").map(|e| e.description.clone()).collect::<Vec<_>>(),
        });
    }
    if has("mistakes") {
        let spots: Vec<_> = crux_repo::mistakes::weak_spots(&mut conn, actor.user_id, course.id)
            .await?
            .into_iter()
            .filter(|w| w.status != "Resolved")
            .map(|w| json!({ "id": w.public_id, "topic": w.topic, "severity": w.severity, "keyRule": w.key_rule }))
            .collect();
        let slips: Vec<_> = crux_repo::mistakes::slips(&mut conn, actor.user_id, Some(course.id), None, None)
            .await?
            .into_iter()
            .take(10)
            .map(|s| json!({ "id": s.public_id, "type": s.stype, "description": s.description }))
            .collect();
        let traps: Vec<_> = crux_repo::mistakes::traps(&mut conn, actor.user_id, course.id)
            .await?
            .into_iter()
            .map(|t| json!({ "id": t.public_id, "text": t.text }))
            .collect();
        out["mistakes"] = json!({ "weakSpots": spots, "slips": slips, "traps": traps });
    }
    drop(conn);
    if has("answers") {
        if let (Some(mock), Some(order)) = (&req.anchor.mock_id, req.anchor.order) {
            // The mock view already hides keys until they're visible to this user (spec 5.4).
            if let Ok(m) = crate::mocks::get(ctx, &actor, mock).await {
                let v = serde_json::to_value(&m).map_err(anyhow::Error::from)?;
                if let Some(item) = v["items"].as_array().and_then(|items| items.iter().find(|i| i["order"] == order)) {
                    out["answers"] = json!({ "mockItem": item });
                }
            }
        } else if let Some(q) = &req.anchor.question_id {
            if let Ok(view) = crate::bank::get(ctx, &actor, q).await {
                let mut v = serde_json::to_value(&view).map_err(anyhow::Error::from)?;
                let mut conn = ctx.pool.acquire().await?;
                let qrow = crux_repo::bank::question(&mut conn, q).await?;
                let answered = match qrow {
                    Some(r) => crux_repo::mocks::stat_for_update(&mut conn, actor.user_id, r.id).await?.is_some_and(|s| s.attempts > 0),
                    None => false,
                };
                if !answered {
                    strip_key(&mut v["current"]);
                }
                out["answers"] = json!({ "question": v["current"], "keyRevealed": answered });
            }
        }
    }
    Ok(out)
}

/// Removes everything that would give the answer away.
fn strip_key(v: &mut serde_json::Value) {
    if let Some(opts) = v.get_mut("options").and_then(|o| o.as_array_mut()) {
        for o in opts {
            if let Some(obj) = o.as_object_mut() {
                obj.remove("isCorrect");
                obj.remove("distractorNote");
            }
        }
    }
    if let Some(obj) = v.as_object_mut() {
        obj.remove("explanation");
        obj.remove("numeric");
        obj.remove("open");
        obj.remove("keyNote");
    }
}
