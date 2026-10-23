use axum::extract::State;
use axum::Json;
use crux_domain::user::{ProfilePatch, SettingsPatch};
use crux_service::me;
use crux_service::views::{SettingsView, TierView, UserView};

use crate::error::{ApiResult, ErrorBody};
use crate::extract::{Auth, IfMatch, JsonBody};
use crate::AppState;

#[utoipa::path(get, path = "/me", tag = "me", security(("bearer" = [])),
    responses((status = 200, body = UserView), (status = 401, body = ErrorBody)))]
pub async fn get_me(State(state): State<AppState>, Auth(actor): Auth) -> ApiResult<Json<UserView>> {
    Ok(Json(me::get(&state.ctx, &actor).await?))
}

/// Profile and time zone. Needs `If-Match: <version>`.
#[utoipa::path(patch, path = "/me", tag = "me", security(("bearer" = [])), request_body = ProfilePatch,
    params(("If-Match" = i32, Header, description = "The version you edited")),
    responses((status = 200, body = UserView), (status = 409, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn patch_me(
    State(state): State<AppState>,
    Auth(actor): Auth,
    IfMatch(version): IfMatch,
    JsonBody(patch): JsonBody<ProfilePatch>,
) -> ApiResult<Json<UserView>> {
    Ok(Json(me::patch(&state.ctx, &actor, version, patch).await?))
}

#[utoipa::path(get, path = "/me/settings", tag = "me", security(("bearer" = [])),
    responses((status = 200, body = SettingsView), (status = 401, body = ErrorBody)))]
pub async fn get_settings(State(state): State<AppState>, Auth(actor): Auth) -> ApiResult<Json<SettingsView>> {
    Ok(Json(me::get_settings(&state.ctx, &actor).await?))
}

#[utoipa::path(patch, path = "/me/settings", tag = "me", security(("bearer" = [])), request_body = SettingsPatch,
    params(("If-Match" = i32, Header, description = "The version you edited")),
    responses((status = 200, body = SettingsView), (status = 409, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn patch_settings(
    State(state): State<AppState>,
    Auth(actor): Auth,
    IfMatch(version): IfMatch,
    JsonBody(patch): JsonBody<SettingsPatch>,
) -> ApiResult<Json<SettingsView>> {
    Ok(Json(me::patch_settings(&state.ctx, &actor, version, patch).await?))
}

/// The user's membership tier and its limits.
#[utoipa::path(get, path = "/me/tier", tag = "me", security(("bearer" = [])),
    responses((status = 200, body = TierView), (status = 401, body = ErrorBody)))]
pub async fn get_tier(State(state): State<AppState>, Auth(actor): Auth) -> ApiResult<Json<TierView>> {
    Ok(Json(me::tier(&state.ctx, &actor).await?))
}
