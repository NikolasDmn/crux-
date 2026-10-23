use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use crux_service::invites::{self, CreateInviteInput};
use crux_service::views::{CreatedInvite, InviteView, Page};

use crate::error::{ApiResult, ErrorBody};
use crate::extract::{Auth, JsonBody};
use crate::routes::PageQuery;
use crate::AppState;

/// Create an invite (needs a verified email). The code is in this response only.
#[utoipa::path(post, path = "/invites", tag = "invites", security(("bearer" = [])), request_body = CreateInviteInput,
    params(("Idempotency-Key" = Option<String>, Header)),
    responses((status = 201, body = CreatedInvite), (status = 403, body = ErrorBody, description = "email_unverified"), (status = 422, body = ErrorBody)))]
pub async fn create(
    State(state): State<AppState>,
    Auth(actor): Auth,
    JsonBody(input): JsonBody<CreateInviteInput>,
) -> ApiResult<(StatusCode, Json<CreatedInvite>)> {
    Ok((StatusCode::CREATED, Json(invites::create(&state.ctx, &actor, input, false).await?)))
}

/// My invites, newest first.
#[utoipa::path(get, path = "/invites", tag = "invites", security(("bearer" = [])), params(PageQuery),
    responses((status = 200, body = Page<InviteView>)))]
pub async fn list(State(state): State<AppState>, Auth(actor): Auth, Query(q): Query<PageQuery>) -> ApiResult<Json<Page<InviteView>>> {
    Ok(Json(invites::list(&state.ctx, &actor, q.cursor.as_deref(), q.limit, false).await?))
}

/// Revoke one of my invites.
#[utoipa::path(delete, path = "/invites/{id}", tag = "invites", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = ErrorBody)))]
pub async fn revoke(State(state): State<AppState>, Auth(actor): Auth, Path(id): Path<String>) -> ApiResult<StatusCode> {
    invites::revoke(&state.ctx, &actor, &id, false).await?;
    Ok(StatusCode::NO_CONTENT)
}
