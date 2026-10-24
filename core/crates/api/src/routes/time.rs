//! Events, sessions, progress, week and course stats (spec 5.7, 5.11, 15).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use crux_service::activity::{self, EventBatch, EventBatchResult, SessionInput, SessionQuery, SessionView};
use crux_service::stats::{self, CourseSummary, LectureProgress, ProgressPatch, WeekQuery, WeekView};

use crate::error::{ApiResult, ErrorBody};
use crate::extract::{Auth, JsonBody};
use crate::AppState;

/// A batch of up to 200 activity events from the client (types from the registry).
#[utoipa::path(post, path = "/events", tag = "time", security(("bearer" = [])), request_body = EventBatch,
    responses((status = 200, body = EventBatchResult), (status = 422, body = ErrorBody)))]
pub async fn events(State(s): State<AppState>, Auth(a): Auth, JsonBody(b): JsonBody<EventBatch>) -> ApiResult<Json<EventBatchResult>> {
    Ok(Json(activity::ingest(&s.ctx, &a, b).await?))
}

/// Study sessions: timer, manual, and derived from your activity.
#[utoipa::path(get, path = "/sessions", tag = "time", security(("bearer" = [])), params(SessionQuery),
    responses((status = 200, body = Vec<SessionView>), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn list_sessions(State(s): State<AppState>, Auth(a): Auth, Query(q): Query<SessionQuery>) -> ApiResult<Json<Vec<SessionView>>> {
    Ok(Json(activity::list(&s.ctx, &a, q).await?))
}

/// Log a timer or manual session; it wins over derived sessions it overlaps.
#[utoipa::path(post, path = "/sessions", tag = "time", security(("bearer" = [])), request_body = SessionInput,
    responses((status = 201, body = SessionView), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn create_session(
    State(s): State<AppState>,
    Auth(a): Auth,
    JsonBody(i): JsonBody<SessionInput>,
) -> ApiResult<(StatusCode, Json<SessionView>)> {
    Ok((StatusCode::CREATED, Json(activity::create(&s.ctx, &a, i).await?)))
}

/// Your progress per lecture, with hours studied.
#[utoipa::path(get, path = "/courses/{id}/progress", tag = "time", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = Vec<LectureProgress>), (status = 404, body = ErrorBody)))]
pub async fn progress(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<Vec<LectureProgress>>> {
    Ok(Json(stats::progress(&s.ctx, &a, &id).await?))
}

/// Set a lecture's status or steps. Finishing one updates the plan.
#[utoipa::path(patch, path = "/courses/{id}/progress", tag = "time", security(("bearer" = [])), request_body = ProgressPatch,
    params(("id" = String, Path)), responses((status = 200, body = LectureProgress), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn patch_progress(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    JsonBody(p): JsonBody<ProgressPatch>,
) -> ApiResult<Json<LectureProgress>> {
    Ok(Json(stats::patch_progress(&s.ctx, &a, &id, p).await?))
}

/// One ISO week: planned vs actual per day and course, steps, slips, first-try MC rate.
#[utoipa::path(get, path = "/week", tag = "time", security(("bearer" = [])), params(WeekQuery),
    responses((status = 200, body = WeekView), (status = 422, body = ErrorBody)))]
pub async fn week(State(s): State<AppState>, Auth(a): Auth, Query(q): Query<WeekQuery>) -> ApiResult<Json<WeekView>> {
    Ok(Json(stats::week(&s.ctx, &a, q).await?))
}

/// For the course page: lectures, hours, remaining range, MC trend, top weak spots.
#[utoipa::path(get, path = "/courses/{id}/stats/summary", tag = "time", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = CourseSummary), (status = 404, body = ErrorBody)))]
pub async fn summary(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<CourseSummary>> {
    Ok(Json(stats::summary(&s.ctx, &a, &id).await?))
}
