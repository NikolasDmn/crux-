//! Flashcards (spec 5.5, 15).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use crux_service::flash::{
    self, CardInput, CardPatch, CardQuery, CardView, CodeQuery, CustomInput, FlagInput, QueueQuery, QueueView, ReviewBatch,
    ReviewBatchView, SuspendInput,
};

use crate::error::{ApiResult, ErrorBody};
use crate::extract::{Auth, IfMatch, JsonBody};
use crate::AppState;

/// Course cards plus your personal cards, with your progress on each.
#[utoipa::path(get, path = "/courses/{id}/flashcards", tag = "flashcards", security(("bearer" = [])), params(("id" = String, Path), CardQuery),
    responses((status = 200, body = Vec<CardView>), (status = 404, body = ErrorBody)))]
pub async fn list(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    Query(q): Query<CardQuery>,
) -> ApiResult<Json<Vec<CardView>>> {
    Ok(Json(flash::list(&s.ctx, &a, &id, q).await?))
}

/// A personal card (anyone on the course), or a course card (`shared`, Owner or Editor).
#[utoipa::path(post, path = "/courses/{id}/flashcards", tag = "flashcards", security(("bearer" = [])), request_body = CardInput,
    params(("id" = String, Path)), responses((status = 201, body = CardView), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn create(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    JsonBody(i): JsonBody<CardInput>,
) -> ApiResult<(StatusCode, Json<CardView>)> {
    Ok((StatusCode::CREATED, Json(flash::create(&s.ctx, &a, &id, i).await?)))
}

#[utoipa::path(patch, path = "/flashcards/{id}", tag = "flashcards", security(("bearer" = [])), request_body = CardPatch,
    params(("id" = String, Path), ("If-Match" = i32, Header)),
    responses((status = 200, body = CardView), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn patch(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    IfMatch(v): IfMatch,
    JsonBody(p): JsonBody<CardPatch>,
) -> ApiResult<Json<CardView>> {
    Ok(Json(flash::patch(&s.ctx, &a, &id, v, p).await?))
}

/// Put the card in (or take it out of) your next warm-up.
#[utoipa::path(post, path = "/flashcards/{id}/warmup-flag", tag = "flashcards", security(("bearer" = [])), request_body = FlagInput,
    params(("id" = String, Path)), responses((status = 200, body = CardView), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn warmup_flag(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    JsonBody(i): JsonBody<FlagInput>,
) -> ApiResult<Json<CardView>> {
    Ok(Json(flash::set_warmup_flag(&s.ctx, &a, &id, i).await?))
}

/// Suspend or resume the card for you.
#[utoipa::path(post, path = "/flashcards/{id}/suspend", tag = "flashcards", security(("bearer" = [])), request_body = SuspendInput,
    params(("id" = String, Path)), responses((status = 200, body = CardView), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn suspend(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    JsonBody(i): JsonBody<SuspendInput>,
) -> ApiResult<Json<CardView>> {
    Ok(Json(flash::set_suspended(&s.ctx, &a, &id, i).await?))
}

/// Today's queue: due cards, then new cards up to your daily limit.
#[utoipa::path(get, path = "/flash/queue", tag = "flashcards", security(("bearer" = [])), params(QueueQuery),
    responses((status = 200, body = QueueView), (status = 404, body = ErrorBody)))]
pub async fn queue(State(s): State<AppState>, Auth(a): Auth, Query(q): Query<QueueQuery>) -> ApiResult<Json<QueueView>> {
    Ok(Json(flash::queue(&s.ctx, &a, q).await?))
}

/// A custom set: slips, forgotten, unseen or all, per lecture.
#[utoipa::path(post, path = "/flash/custom", tag = "flashcards", security(("bearer" = [])), request_body = CustomInput,
    responses((status = 200, body = Vec<CardView>), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn custom(State(s): State<AppState>, Auth(a): Auth, JsonBody(i): JsonBody<CustomInput>) -> ApiResult<Json<Vec<CardView>>> {
    Ok(Json(flash::custom(&s.ctx, &a, i).await?))
}

/// Sync reviews (up to 500, e.g. from a phone that was offline). Retries are safe.
#[utoipa::path(post, path = "/flash/reviews", tag = "flashcards", security(("bearer" = [])), request_body = ReviewBatch,
    responses((status = 200, body = ReviewBatchView), (status = 422, body = ErrorBody)))]
pub async fn reviews(State(s): State<AppState>, Auth(a): Auth, JsonBody(b): JsonBody<ReviewBatch>) -> ApiResult<Json<ReviewBatchView>> {
    Ok(Json(flash::review(&s.ctx, &a, b).await?))
}

/// Find a card by its code across your courses.
#[utoipa::path(get, path = "/flash/cards", tag = "flashcards", security(("bearer" = [])), params(CodeQuery),
    responses((status = 200, body = Vec<CardView>)))]
pub async fn by_code(State(s): State<AppState>, Auth(a): Auth, Query(q): Query<CodeQuery>) -> ApiResult<Json<Vec<CardView>>> {
    Ok(Json(flash::by_code(&s.ctx, &a, q).await?))
}
