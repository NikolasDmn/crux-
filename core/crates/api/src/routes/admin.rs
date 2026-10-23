//! `/admin/*`: Role = Admin only; everyone else gets 404 (spec 4.2). Audited in the service.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use crux_service::admin::{self, AdminUserPatch, TierInput};
use crux_service::invites::{self, CreateInviteInput};
use crux_service::views::{CreatedInvite, InviteView, Page, TierView, UserView};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::error::{ApiResult, ErrorBody};
use crate::extract::{Admin, JsonBody};
use crate::routes::PageQuery;
use crate::AppState;

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct UserQuery {
    /// Matches email or username.
    pub q: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[utoipa::path(get, path = "/admin/users", tag = "admin", security(("bearer" = [])), params(UserQuery),
    responses((status = 200, body = Page<UserView>), (status = 404, body = ErrorBody)))]
pub async fn list_users(State(state): State<AppState>, Admin(actor): Admin, Query(q): Query<UserQuery>) -> ApiResult<Json<Page<UserView>>> {
    Ok(Json(admin::list_users(&state.ctx, &actor, q.q.as_deref(), q.cursor.as_deref(), q.limit).await?))
}

/// Change a user's tier, role or status.
#[utoipa::path(patch, path = "/admin/users/{id}", tag = "admin", security(("bearer" = [])), params(("id" = String, Path)),
    request_body = AdminUserPatch, responses((status = 200, body = UserView), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn patch_user(
    State(state): State<AppState>,
    Admin(actor): Admin,
    Path(id): Path<String>,
    JsonBody(patch): JsonBody<AdminUserPatch>,
) -> ApiResult<Json<UserView>> {
    Ok(Json(admin::patch_user(&state.ctx, &actor, &id, patch).await?))
}

#[utoipa::path(get, path = "/admin/tiers", tag = "admin", security(("bearer" = [])),
    responses((status = 200, body = Vec<TierView>), (status = 404, body = ErrorBody)))]
pub async fn list_tiers(State(state): State<AppState>, Admin(actor): Admin) -> ApiResult<Json<Vec<TierView>>> {
    Ok(Json(admin::list_tiers(&state.ctx, &actor).await?))
}

#[utoipa::path(post, path = "/admin/tiers", tag = "admin", security(("bearer" = [])), request_body = TierInput,
    responses((status = 201, body = TierView), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn create_tier(
    State(state): State<AppState>,
    Admin(actor): Admin,
    JsonBody(input): JsonBody<TierInput>,
) -> ApiResult<(StatusCode, Json<TierView>)> {
    Ok((StatusCode::CREATED, Json(admin::create_tier(&state.ctx, &actor, input).await?)))
}

/// Tiers are addressed by `code` (they have no PublicId in the schema).
#[utoipa::path(patch, path = "/admin/tiers/{code}", tag = "admin", security(("bearer" = [])), params(("code" = String, Path)),
    request_body = TierInput, responses((status = 200, body = TierView), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn update_tier(
    State(state): State<AppState>,
    Admin(actor): Admin,
    Path(code): Path<String>,
    JsonBody(input): JsonBody<TierInput>,
) -> ApiResult<Json<TierView>> {
    Ok(Json(admin::update_tier(&state.ctx, &actor, &code, input).await?))
}

#[utoipa::path(get, path = "/admin/invites", tag = "admin", security(("bearer" = [])), params(PageQuery),
    responses((status = 200, body = Page<InviteView>), (status = 404, body = ErrorBody)))]
pub async fn list_invites(
    State(state): State<AppState>,
    Admin(actor): Admin,
    Query(q): Query<PageQuery>,
) -> ApiResult<Json<Page<InviteView>>> {
    Ok(Json(invites::list(&state.ctx, &actor, q.cursor.as_deref(), q.limit, true).await?))
}

/// Admin invites can carry any tier and more uses.
#[utoipa::path(post, path = "/admin/invites", tag = "admin", security(("bearer" = [])), request_body = CreateInviteInput,
    params(("Idempotency-Key" = Option<String>, Header)),
    responses((status = 201, body = CreatedInvite), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn create_invite(
    State(state): State<AppState>,
    Admin(actor): Admin,
    JsonBody(input): JsonBody<CreateInviteInput>,
) -> ApiResult<(StatusCode, Json<CreatedInvite>)> {
    Ok((StatusCode::CREATED, Json(invites::create(&state.ctx, &actor, input, true).await?)))
}

#[utoipa::path(delete, path = "/admin/invites/{id}", tag = "admin", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = ErrorBody)))]
pub async fn revoke_invite(State(state): State<AppState>, Admin(actor): Admin, Path(id): Path<String>) -> ApiResult<StatusCode> {
    invites::revoke(&state.ctx, &actor, &id, true).await?;
    Ok(StatusCode::NO_CONTENT)
}
