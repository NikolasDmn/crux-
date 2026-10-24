//! Slips, weak spots, traps, insight, confidence view (spec 15).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use crux_service::mistakes::{
    self, LectureConfidence, Pattern, SlipInput, SlipPatch, SlipQuery, SlipView, TrapInput, TrapView, WeakSpotInput, WeakSpotPatch,
    WeakSpotView, WeakSpotsView,
};

use crate::error::{ApiResult, ErrorBody};
use crate::extract::{Auth, IfMatch, JsonBody};
use crate::AppState;

#[utoipa::path(get, path = "/slips", tag = "mistakes", security(("bearer" = [])), params(SlipQuery),
    responses((status = 200, body = Vec<SlipView>)))]
pub async fn list_slips(State(s): State<AppState>, Auth(a): Auth, Query(q): Query<SlipQuery>) -> ApiResult<Json<Vec<SlipView>>> {
    Ok(Json(mistakes::list_slips(&s.ctx, &a, q).await?))
}

/// Log a slip, or confirm a proposed one (give its mock and question number).
#[utoipa::path(post, path = "/slips", tag = "mistakes", security(("bearer" = [])), request_body = SlipInput,
    responses((status = 201, body = SlipView), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn create_slip(
    State(s): State<AppState>,
    Auth(a): Auth,
    JsonBody(i): JsonBody<SlipInput>,
) -> ApiResult<(StatusCode, Json<SlipView>)> {
    Ok((StatusCode::CREATED, Json(mistakes::create_slip(&s.ctx, &a, i).await?)))
}

#[utoipa::path(patch, path = "/slips/{id}", tag = "mistakes", security(("bearer" = [])), request_body = SlipPatch, params(("id" = String, Path)),
    responses((status = 200, body = SlipView), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn patch_slip(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    JsonBody(p): JsonBody<SlipPatch>,
) -> ApiResult<Json<SlipView>> {
    Ok(Json(mistakes::patch_slip(&s.ctx, &a, &id, p).await?))
}

/// The OPEN line, resolved spots, and spots worth creating.
#[utoipa::path(get, path = "/courses/{id}/weak-spots", tag = "mistakes", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = WeakSpotsView), (status = 404, body = ErrorBody)))]
pub async fn list_weak_spots(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<WeakSpotsView>> {
    Ok(Json(mistakes::list_weak_spots(&s.ctx, &a, &id).await?))
}

#[utoipa::path(post, path = "/courses/{id}/weak-spots", tag = "mistakes", security(("bearer" = [])), request_body = WeakSpotInput,
    params(("id" = String, Path)), responses((status = 201, body = WeakSpotView), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn create_weak_spot(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    JsonBody(i): JsonBody<WeakSpotInput>,
) -> ApiResult<(StatusCode, Json<WeakSpotView>)> {
    Ok((StatusCode::CREATED, Json(mistakes::create_weak_spot(&s.ctx, &a, &id, i).await?)))
}

#[utoipa::path(patch, path = "/weak-spots/{id}", tag = "mistakes", security(("bearer" = [])), request_body = WeakSpotPatch,
    params(("id" = String, Path), ("If-Match" = i32, Header)),
    responses((status = 200, body = WeakSpotView), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn patch_weak_spot(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    IfMatch(v): IfMatch,
    JsonBody(p): JsonBody<WeakSpotPatch>,
) -> ApiResult<Json<WeakSpotView>> {
    Ok(Json(mistakes::patch_weak_spot(&s.ctx, &a, &id, v, p).await?))
}

/// Shown on the mock start screen.
#[utoipa::path(get, path = "/courses/{id}/traps", tag = "mistakes", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = Vec<TrapView>), (status = 404, body = ErrorBody)))]
pub async fn list_traps(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<Vec<TrapView>>> {
    Ok(Json(mistakes::list_traps(&s.ctx, &a, &id).await?))
}

#[utoipa::path(post, path = "/courses/{id}/traps", tag = "mistakes", security(("bearer" = [])), request_body = TrapInput, params(("id" = String, Path)),
    responses((status = 201, body = TrapView), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn create_trap(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    JsonBody(i): JsonBody<TrapInput>,
) -> ApiResult<(StatusCode, Json<TrapView>)> {
    Ok((StatusCode::CREATED, Json(mistakes::create_trap(&s.ctx, &a, &id, i).await?)))
}

/// Retire a trap.
#[utoipa::path(delete, path = "/courses/{id}/traps/{trapId}", tag = "mistakes", security(("bearer" = [])),
    params(("id" = String, Path), ("trapId" = String, Path)), responses((status = 204), (status = 404, body = ErrorBody)))]
pub async fn retire_trap(State(s): State<AppState>, Auth(a): Auth, Path((id, trap)): Path<(String, String)>) -> ApiResult<StatusCode> {
    mistakes::retire_trap(&s.ctx, &a, &id, &trap).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Open slips by type across courses.
#[utoipa::path(get, path = "/insight/patterns", tag = "mistakes", security(("bearer" = [])), responses((status = 200, body = Vec<Pattern>)))]
pub async fn patterns(State(s): State<AppState>, Auth(a): Auth) -> ApiResult<Json<Vec<Pattern>>> {
    Ok(Json(mistakes::patterns(&s.ctx, &a).await?))
}

/// Per lecture: right rate for sure, unsure and guess answers; flags a real gap (spec 5.4).
#[utoipa::path(get, path = "/courses/{id}/stats/confidence", tag = "mistakes", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = Vec<LectureConfidence>), (status = 404, body = ErrorBody)))]
pub async fn confidence(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<Vec<LectureConfidence>>> {
    Ok(Json(mistakes::confidence(&s.ctx, &a, &id).await?))
}
