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
