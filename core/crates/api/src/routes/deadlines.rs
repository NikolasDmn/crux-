//! Exams and deadlines (spec 5.10, 15).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use crux_service::deadlines::{self, DeadlineInput, DeadlineView, LayoutInput};

use crate::error::{ApiResult, ErrorBody};
use crate::extract::{Auth, IfMatch, JsonBody};
use crate::AppState;

#[utoipa::path(get, path = "/courses/{id}/deadlines", tag = "deadlines", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = Vec<DeadlineView>), (status = 404, body = ErrorBody)))]
pub async fn list(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<Vec<DeadlineView>>> {
    Ok(Json(deadlines::list(&s.ctx, &a, &id).await?))
}

/// Add an exam, resit or deadline to your subscription. The plan rebuilds.
#[utoipa::path(post, path = "/courses/{id}/deadlines", tag = "deadlines", security(("bearer" = [])), request_body = DeadlineInput,
    params(("id" = String, Path)), responses((status = 201, body = DeadlineView), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn create(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    JsonBody(i): JsonBody<DeadlineInput>,
) -> ApiResult<(StatusCode, Json<DeadlineView>)> {
    Ok((StatusCode::CREATED, Json(deadlines::create(&s.ctx, &a, &id, i).await?)))
}

/// Edit, or record the check-in result (status, grade, note). A pass archives the course for you.
#[utoipa::path(patch, path = "/deadlines/{id}", tag = "deadlines", security(("bearer" = [])), request_body = DeadlineInput,
    params(("id" = String, Path), ("If-Match" = i32, Header)),
    responses((status = 200, body = DeadlineView), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn patch(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    IfMatch(v): IfMatch,
    JsonBody(i): JsonBody<DeadlineInput>,
) -> ApiResult<Json<DeadlineView>> {
    Ok(Json(deadlines::patch(&s.ctx, &a, &id, v, i).await?))
}

/// Which lectures the exam covers, and how many questions each.
#[utoipa::path(put, path = "/deadlines/{id}/layout", tag = "deadlines", security(("bearer" = [])), request_body = LayoutInput,
    params(("id" = String, Path)), responses((status = 200, body = DeadlineView), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn layout(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    JsonBody(i): JsonBody<LayoutInput>,
) -> ApiResult<Json<DeadlineView>> {
    Ok(Json(deadlines::set_layout(&s.ctx, &a, &id, i).await?))
}
