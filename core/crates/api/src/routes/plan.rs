//! Study plan, "Take me there" and the warm-up (spec 5.8, 5.9, 15).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use crux_service::next::{self, AdvanceInput, NextView, WarmupItem};
use crux_service::plan::{self, EntryInput, EntryPatch, PlanEntryView, PlanQuery, RebuildView, ShortView};

use crate::error::{ApiResult, ErrorBody};
use crate::extract::{Auth, IfMatch, JsonBody};
use crate::AppState;

#[utoipa::path(get, path = "/plan", tag = "plan", security(("bearer" = [])), params(PlanQuery),
    responses((status = 200, body = Vec<PlanEntryView>), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn list(State(s): State<AppState>, Auth(a): Auth, Query(q): Query<PlanQuery>) -> ApiResult<Json<Vec<PlanEntryView>>> {
    Ok(Json(plan::list(&s.ctx, &a, q).await?))
}

/// Re-plan from now on; pinned entries stay where they are.
#[utoipa::path(post, path = "/plan/rebuild", tag = "plan", security(("bearer" = [])), responses((status = 200, body = RebuildView)))]
pub async fn rebuild(State(s): State<AppState>, Auth(a): Auth) -> ApiResult<Json<RebuildView>> {
    Ok(Json(plan::rebuild(&s.ctx, &a).await?))
}

/// Courses whose lectures don't fit before their final week, as a rebuild now would say (no change).
#[utoipa::path(get, path = "/plan/short", tag = "plan", security(("bearer" = [])), responses((status = 200, body = Vec<ShortView>)))]
pub async fn short(State(s): State<AppState>, Auth(a): Auth) -> ApiResult<Json<Vec<ShortView>>> {
    Ok(Json(plan::short(&s.ctx, &a).await?))
}

/// Add your own entry (pinned).
#[utoipa::path(post, path = "/plan/entries", tag = "plan", security(("bearer" = [])), request_body = EntryInput,
    responses((status = 201, body = PlanEntryView), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn create_entry(
    State(s): State<AppState>,
    Auth(a): Auth,
    JsonBody(i): JsonBody<EntryInput>,
) -> ApiResult<(StatusCode, Json<PlanEntryView>)> {
    Ok((StatusCode::CREATED, Json(plan::create_entry(&s.ctx, &a, i).await?)))
}

/// Edit, move (new time) or mark done/skipped. Edits pin the entry.
#[utoipa::path(patch, path = "/plan/entries/{id}", tag = "plan", security(("bearer" = [])), request_body = EntryPatch,
    params(("id" = String, Path), ("If-Match" = i32, Header)),
    responses((status = 200, body = PlanEntryView), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn patch_entry(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    IfMatch(v): IfMatch,
    JsonBody(p): JsonBody<EntryPatch>,
) -> ApiResult<Json<PlanEntryView>> {
    Ok(Json(plan::patch_entry(&s.ctx, &a, &id, v, p).await?))
}

/// Delete your entry; a planner entry is skipped instead (and stays skipped through rebuilds).
#[utoipa::path(delete, path = "/plan/entries/{id}", tag = "plan", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = ErrorBody)))]
pub async fn delete_entry(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<StatusCode> {
    plan::delete_entry(&s.ctx, &a, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The steps for right now: warm-up, the main item, due flashcards.
#[utoipa::path(get, path = "/next", tag = "plan", security(("bearer" = [])), responses((status = 200, body = NextView)))]
pub async fn next(State(s): State<AppState>, Auth(a): Auth) -> ApiResult<Json<NextView>> {
    Ok(Json(next::next(&s.ctx, &a).await?))
}

/// A step is done (or skipped).
#[utoipa::path(post, path = "/next/advance", tag = "plan", security(("bearer" = [])), request_body = AdvanceInput,
    responses((status = 204), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn advance(State(s): State<AppState>, Auth(a): Auth, JsonBody(i): JsonBody<AdvanceInput>) -> ApiResult<StatusCode> {
    next::advance(&s.ctx, &a, i).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 2-3 cold items: flagged cards, then one question per open weak spot.
#[utoipa::path(get, path = "/courses/{id}/warmup", tag = "plan", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = Vec<WarmupItem>), (status = 404, body = ErrorBody)))]
pub async fn warmup(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<Vec<WarmupItem>>> {
    Ok(Json(next::warmup(&s.ctx, &a, &id).await?))
}
