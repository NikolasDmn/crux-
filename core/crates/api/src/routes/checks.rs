//! Check questions after a material, and "quiz me" on a lecture or concept.

use axum::extract::{Path, Query, State};
use axum::Json;
use crux_service::checks::{self, ChecksInput, MaterialCheck, QuizQuery};
use crux_service::next::PracticeQuestion;

use crate::error::{ApiResult, ErrorBody};
use crate::extract::{Auth, JsonBody};
use crate::AppState;

/// The material's check questions you can answer, in reading order (no keys: POST /attempts with
/// context Notes and the material returns it).
#[utoipa::path(get, path = "/materials/{id}/checks", tag = "materials", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = Vec<MaterialCheck>), (status = 404, body = ErrorBody)))]
pub async fn list(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<Vec<MaterialCheck>>> {
    Ok(Json(checks::list(&s.ctx, &a, &id).await?))
}

/// Replace the material's check questions (Owner or Editor): course-bank questions, each after a
/// page or section (0 = at the end).
#[utoipa::path(put, path = "/materials/{id}/checks", tag = "materials", security(("bearer" = [])), request_body = ChecksInput,
    params(("id" = String, Path)),
    responses((status = 200, body = Vec<MaterialCheck>), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn set(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    JsonBody(i): JsonBody<ChecksInput>,
) -> ApiResult<Json<Vec<MaterialCheck>>> {
    Ok(Json(checks::set(&s.ctx, &a, &id, i).await?))
}

/// A couple of questions on a lecture or concept: never tried first, then missed, then the longest
/// unseen (no keys: POST /attempts with context Practice returns it).
#[utoipa::path(get, path = "/courses/{id}/quiz", tag = "questions", security(("bearer" = [])), params(("id" = String, Path), QuizQuery),
    responses((status = 200, body = Vec<PracticeQuestion>), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn quiz(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    Query(q): Query<QuizQuery>,
) -> ApiResult<Json<Vec<PracticeQuestion>>> {
    Ok(Json(checks::quiz(&s.ctx, &a, &id, q).await?))
}
