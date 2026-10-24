//! Mocks and practice attempts (spec 15).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use crux_service::attempts::{self, AttemptInput, AttemptResult};
use crux_service::mocks::{self, AiDecisionInput, AnswerInput, ItemView, MockInput, MockSummary, MockView, SelfGradeInput};

use crate::error::{ApiResult, ErrorBody};
use crate::extract::{Auth, JsonBody};
use crate::AppState;

/// Build a mock (spec 5.3). Needs a subscription to the course. `warnings` lists lectures with
/// too few questions.
#[utoipa::path(post, path = "/courses/{id}/mocks", tag = "mocks", security(("bearer" = [])), request_body = MockInput,
    params(("id" = String, Path), ("Idempotency-Key" = Option<String>, Header)),
    responses((status = 201, body = MockView), (status = 403, body = ErrorBody, description = "ai_disabled"), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn build(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    JsonBody(i): JsonBody<MockInput>,
) -> ApiResult<(StatusCode, Json<MockView>)> {
    Ok((StatusCode::CREATED, Json(mocks::build(&s.ctx, &a, &id, i).await?)))
}

/// My mocks for this course, newest first.
#[utoipa::path(get, path = "/courses/{id}/mocks", tag = "mocks", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = Vec<MockSummary>), (status = 404, body = ErrorBody)))]
pub async fn list(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<Vec<MockSummary>>> {
    Ok(Json(mocks::list(&s.ctx, &a, &id).await?))
}

/// The taking view: questions and your saved answers, never keys.
#[utoipa::path(get, path = "/mocks/{id}", tag = "mocks", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = MockView), (status = 404, body = ErrorBody)))]
pub async fn get(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<MockView>> {
    Ok(Json(mocks::get(&s.ctx, &a, &id).await?))
}

#[utoipa::path(post, path = "/mocks/{id}/start", tag = "mocks", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = MockView), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn start(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<MockView>> {
    Ok(Json(mocks::start(&s.ctx, &a, &id).await?))
}

/// Auto-grades MC, numeric and blanks; open answers wait for your self-grade.
#[utoipa::path(post, path = "/mocks/{id}/submit", tag = "mocks", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = MockView), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn submit(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<MockView>> {
    Ok(Json(mocks::submit(&s.ctx, &a, &id).await?))
}

/// Save one answer (the whole answer each time). After the time limit + 2 minutes: 409 mock_time_over.
#[utoipa::path(put, path = "/mocks/{id}/questions/{order}/answer", tag = "mocks", security(("bearer" = [])), request_body = AnswerInput,
    params(("id" = String, Path), ("order" = i32, Path)),
    responses((status = 200, body = ItemView), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn answer(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path((id, order)): Path<(String, i32)>,
    JsonBody(i): JsonBody<AnswerInput>,
) -> ApiResult<Json<ItemView>> {
    Ok(Json(mocks::answer(&s.ctx, &a, &id, order, i).await?))
}

/// Self-grade an open answer: Right, Partial (0.5) or Wrong.
#[utoipa::path(post, path = "/mocks/{id}/questions/{order}/grade", tag = "mocks", security(("bearer" = [])), request_body = SelfGradeInput,
    params(("id" = String, Path), ("order" = i32, Path)),
    responses((status = 200, body = MockView), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn grade(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path((id, order)): Path<(String, i32)>,
    JsonBody(i): JsonBody<SelfGradeInput>,
) -> ApiResult<Json<MockView>> {
    Ok(Json(mocks::self_grade(&s.ctx, &a, &id, order, i).await?))
}

/// Peek at the key for one item (marks it self-shown).
#[utoipa::path(post, path = "/mocks/{id}/questions/{order}/show-answer", tag = "mocks", security(("bearer" = [])),
    params(("id" = String, Path), ("order" = i32, Path)), responses((status = 200, body = MockView), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn show_answer(State(s): State<AppState>, Auth(a): Auth, Path((id, order)): Path<(String, i32)>) -> ApiResult<Json<MockView>> {
    Ok(Json(mocks::review_flag(&s.ctx, &a, &id, order, true).await?))
}

/// "I fixed this" in review.
#[utoipa::path(post, path = "/mocks/{id}/questions/{order}/fixed", tag = "mocks", security(("bearer" = [])),
    params(("id" = String, Path), ("order" = i32, Path)), responses((status = 200, body = MockView), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn fixed(State(s): State<AppState>, Auth(a): Auth, Path((id, order)): Path<(String, i32)>) -> ApiResult<Json<MockView>> {
    Ok(Json(mocks::review_flag(&s.ctx, &a, &id, order, false).await?))
}

/// Result + review. Keys only for revealed, self-shown or (once reviewed) all items.
#[utoipa::path(get, path = "/mocks/{id}/result", tag = "mocks", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = MockView), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn result(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<MockView>> {
    Ok(Json(mocks::result(&s.ctx, &a, &id).await?))
}

/// A practice, notes, warm-up or review answer. Idempotent on `clientId`.
#[utoipa::path(post, path = "/attempts", tag = "practice", security(("bearer" = [])), request_body = AttemptInput,
    responses((status = 200, body = AttemptResult), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn attempt(State(s): State<AppState>, Auth(a): Auth, JsonBody(i): JsonBody<AttemptInput>) -> ApiResult<Json<AttemptResult>> {
    Ok(Json(attempts::record(&s.ctx, &a, i).await?))
}

/// Accept the AI's suggested grade for this answer.
#[utoipa::path(post, path = "/mocks/{id}/questions/{order}/ai-grade/accept", tag = "mocks", security(("bearer" = [])),
    request_body = AiDecisionInput, params(("id" = String, Path), ("order" = i32, Path)),
    responses((status = 200, body = MockView), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn ai_accept(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path((id, order)): Path<(String, i32)>,
    JsonBody(i): JsonBody<AiDecisionInput>,
) -> ApiResult<Json<MockView>> {
    Ok(Json(mocks::ai_decision(&s.ctx, &a, &id, order, "Accepted", i).await?))
}

/// Grade it yourself instead of the AI's suggestion (`outcome`).
#[utoipa::path(post, path = "/mocks/{id}/questions/{order}/ai-grade/override", tag = "mocks", security(("bearer" = [])),
    request_body = AiDecisionInput, params(("id" = String, Path), ("order" = i32, Path)),
    responses((status = 200, body = MockView), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn ai_override(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path((id, order)): Path<(String, i32)>,
    JsonBody(i): JsonBody<AiDecisionInput>,
) -> ApiResult<Json<MockView>> {
    Ok(Json(mocks::ai_decision(&s.ctx, &a, &id, order, "Overridden", i).await?))
}

/// Dismiss the suggestion; the answer waits for your own grade.
#[utoipa::path(post, path = "/mocks/{id}/questions/{order}/ai-grade/reject", tag = "mocks", security(("bearer" = [])),
    request_body = AiDecisionInput, params(("id" = String, Path), ("order" = i32, Path)),
    responses((status = 200, body = MockView), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn ai_reject(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path((id, order)): Path<(String, i32)>,
    JsonBody(i): JsonBody<AiDecisionInput>,
) -> ApiResult<Json<MockView>> {
    Ok(Json(mocks::ai_decision(&s.ctx, &a, &id, order, "Rejected", i).await?))
}

/// Accept every pending AI suggestion of this mock.
#[utoipa::path(post, path = "/mocks/{id}/ai-grades/accept-all", tag = "mocks", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = MockView), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn ai_accept_all(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<MockView>> {
    Ok(Json(mocks::accept_all_ai(&s.ctx, &a, &id).await?))
}
