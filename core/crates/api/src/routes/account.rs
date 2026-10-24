//! Account export and deletion (spec 11, 15).

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use crux_service::account::{self, DeletionInput, DeletionView, ExportView};

use crate::error::{ApiResult, ErrorBody};
use crate::extract::{Auth, JsonBody};
use crate::AppState;

/// Everything of yours as a zip (JSON per service + your PDFs). You get a notification when it's
/// ready; the download works for 24 hours.
#[utoipa::path(post, path = "/account/export", tag = "account", security(("bearer" = [])), responses((status = 202, body = ExportView)))]
pub async fn export(State(s): State<AppState>, Auth(a): Auth) -> ApiResult<(StatusCode, Json<ExportView>)> {
    Ok((StatusCode::ACCEPTED, Json(account::request_export(&s.ctx, &a).await?)))
}

#[utoipa::path(get, path = "/account/exports", tag = "account", security(("bearer" = [])), responses((status = 200, body = Vec<ExportView>)))]
pub async fn exports(State(s): State<AppState>, Auth(a): Auth) -> ApiResult<Json<Vec<ExportView>>> {
    Ok(Json(account::list_exports(&s.ctx, &a).await?))
}

/// The deletion status, and the owned courses others use (each needs a decision).
#[utoipa::path(get, path = "/account/deletion", tag = "account", security(("bearer" = [])), responses((status = 200, body = DeletionView)))]
pub async fn deletion(State(s): State<AppState>, Auth(a): Auth) -> ApiResult<Json<DeletionView>> {
    Ok(Json(account::deletion_status(&s.ctx, &a).await?))
}

/// Delete your account in 14 days (cancellable until then). Needs your password.
#[utoipa::path(post, path = "/account/delete", tag = "account", security(("bearer" = [])), request_body = DeletionInput,
    responses((status = 200, body = DeletionView), (status = 422, body = ErrorBody)))]
pub async fn delete(State(s): State<AppState>, Auth(a): Auth, JsonBody(i): JsonBody<DeletionInput>) -> ApiResult<Json<DeletionView>> {
    Ok(Json(account::request_deletion(&s.ctx, &a, i).await?))
}

#[utoipa::path(post, path = "/account/delete/cancel", tag = "account", security(("bearer" = [])),
    responses((status = 200, body = DeletionView), (status = 409, body = ErrorBody)))]
pub async fn cancel(State(s): State<AppState>, Auth(a): Auth) -> ApiResult<Json<DeletionView>> {
    Ok(Json(account::cancel_deletion(&s.ctx, &a).await?))
}
