//! Question bank routes (spec 15).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use crux_service::bank::{self, BankQuery, CropInput, CroppedImage, QuestionInput, QuestionPatch, QuestionView, VersionView};
use crux_service::views::Page;

use crate::error::{ApiResult, ErrorBody};
use crate::extract::{Auth, IfMatch, JsonBody};
use crate::AppState;

/// The bank, with filters (spec 5.2). Course-bank questions plus your private ones.
#[utoipa::path(get, path = "/courses/{id}/questions", tag = "bank", security(("bearer" = [])), params(("id" = String, Path), BankQuery),
    responses((status = 200, body = Page<QuestionView>), (status = 404, body = ErrorBody)))]
pub async fn list(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    Query(q): Query<BankQuery>,
) -> ApiResult<Json<Page<QuestionView>>> {
    Ok(Json(bank::list(&s.ctx, &a, &id, q).await?))
}

/// Add a question. Bank questions need Editor; `private: true` only needs read access.
#[utoipa::path(post, path = "/courses/{id}/questions", tag = "bank", security(("bearer" = [])), request_body = QuestionInput,
    params(("id" = String, Path), ("Idempotency-Key" = Option<String>, Header)),
    responses((status = 201, body = QuestionView), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn create(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    JsonBody(i): JsonBody<QuestionInput>,
) -> ApiResult<(StatusCode, Json<QuestionView>)> {
    Ok((StatusCode::CREATED, Json(bank::create(&s.ctx, &a, &id, i).await?)))
}

#[utoipa::path(get, path = "/questions/{id}", tag = "bank", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = QuestionView), (status = 404, body = ErrorBody)))]
pub async fn get(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<QuestionView>> {
    Ok(Json(bank::get(&s.ctx, &a, &id).await?))
}

/// `content` makes a new immutable version; other fields change the question itself.
#[utoipa::path(patch, path = "/questions/{id}", tag = "bank", security(("bearer" = [])), request_body = QuestionPatch,
    params(("id" = String, Path), ("If-Match" = i32, Header)),
    responses((status = 200, body = QuestionView), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn patch(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    IfMatch(v): IfMatch,
    JsonBody(p): JsonBody<QuestionPatch>,
) -> ApiResult<Json<QuestionView>> {
    Ok(Json(bank::patch(&s.ctx, &a, &id, v, p).await?))
}

#[utoipa::path(get, path = "/questions/{id}/versions", tag = "bank", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = Vec<VersionView>), (status = 404, body = ErrorBody)))]
pub async fn versions(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<Vec<VersionView>>> {
    Ok(Json(bank::versions(&s.ctx, &a, &id).await?))
}

#[utoipa::path(post, path = "/questions/{id}/archive", tag = "bank", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = QuestionView), (status = 404, body = ErrorBody)))]
pub async fn archive(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<QuestionView>> {
    Ok(Json(bank::archive(&s.ctx, &a, &id).await?))
}

#[utoipa::path(post, path = "/questions/{id}/approve", tag = "bank", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = QuestionView), (status = 404, body = ErrorBody)))]
pub async fn approve(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<QuestionView>> {
    Ok(Json(bank::approve(&s.ctx, &a, &id).await?))
}

/// Your private question → the course bank (you must be able to edit the course).
#[utoipa::path(post, path = "/questions/{id}/promote", tag = "bank", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = QuestionView), (status = 404, body = ErrorBody)))]
pub async fn promote(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<QuestionView>> {
    Ok(Json(bank::promote(&s.ctx, &a, &id).await?))
}

/// A question image cut from a page of one of the course's PDFs, for a question being edited (save
/// the question with it as `imageFileId`).
#[utoipa::path(post, path = "/courses/{id}/question-images/crop", tag = "bank", security(("bearer" = [])), request_body = CropInput,
    params(("id" = String, Path)), responses((status = 200, body = CroppedImage), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn crop_for_course(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    JsonBody(i): JsonBody<CropInput>,
) -> ApiResult<Json<CroppedImage>> {
    Ok(Json(bank::crop_for_course(&s.ctx, &a, &id, i).await?))
}

/// Cut the question image from a page of a course PDF; saves a new version with it.
#[utoipa::path(post, path = "/questions/{id}/image-crop", tag = "bank", security(("bearer" = [])), request_body = CropInput,
    params(("id" = String, Path)), responses((status = 200, body = QuestionView), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn image_crop(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    JsonBody(i): JsonBody<CropInput>,
) -> ApiResult<Json<QuestionView>> {
    Ok(Json(bank::image_crop(&s.ctx, &a, &id, i).await?))
}
