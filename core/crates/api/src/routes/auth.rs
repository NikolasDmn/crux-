use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use crux_service::auth::{self, LoginInput, Session, SignupInput};
use crux_service::time::iso;
use crux_service::views::UserView;
use crux_service::AppError;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::{ApiResult, ErrorBody};
use crate::extract::{Auth, Client, JsonBody};
use crate::AppState;

pub const REFRESH_COOKIE: &str = "crux_refresh";
/// The browser sees core under `/api/v1`; the gateway strips it before core.
pub const REFRESH_COOKIE_PATH: &str = "/api/v1/auth";

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionResponse {
    pub access_token: String,
    pub expires_at: String,
    pub user: UserView,
}

fn refresh_cookie(state: &AppState, s: &Session) -> Cookie<'static> {
    let max_age = (s.refresh_expires_at - s.access_expires_at + chrono::Duration::minutes(15)).num_seconds();
    Cookie::build((REFRESH_COOKIE, s.refresh_token.clone()))
        .path(REFRESH_COOKIE_PATH)
        .http_only(true)
        .secure(state.cookie_secure)
        .same_site(SameSite::Strict)
        .max_age(cookie::time::Duration::seconds(max_age))
        .build()
}

fn cleared_cookie(state: &AppState) -> Cookie<'static> {
    Cookie::build((REFRESH_COOKIE, ""))
        .path(REFRESH_COOKIE_PATH)
        .http_only(true)
        .secure(state.cookie_secure)
        .same_site(SameSite::Strict)
        .build()
}

fn respond(state: &AppState, jar: CookieJar, s: Session) -> (CookieJar, Json<SessionResponse>) {
    let jar = jar.add(refresh_cookie(state, &s));
    (jar, Json(SessionResponse { access_token: s.access_token, expires_at: iso(s.access_expires_at), user: s.user }))
}

/// Sign up with an invite code.
#[utoipa::path(post, path = "/auth/signup", tag = "auth", request_body = SignupInput,
    responses((status = 200, body = SessionResponse), (status = 403, body = ErrorBody, description = "invite_invalid"), (status = 422, body = ErrorBody)))]
pub async fn signup(
    State(state): State<AppState>,
    jar: CookieJar,
    Client(client): Client,
    JsonBody(input): JsonBody<SignupInput>,
) -> ApiResult<(CookieJar, Json<SessionResponse>)> {
    let s = auth::signup(&state.ctx, input, &client).await?;
    Ok(respond(&state, jar, s))
}

/// Log in with email or username + password.
#[utoipa::path(post, path = "/auth/login", tag = "auth", request_body = LoginInput,
    responses((status = 200, body = SessionResponse), (status = 401, body = ErrorBody), (status = 429, body = ErrorBody)))]
pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Client(client): Client,
    JsonBody(input): JsonBody<LoginInput>,
) -> ApiResult<(CookieJar, Json<SessionResponse>)> {
    let s = auth::login(&state.ctx, input, &client).await?;
    Ok(respond(&state, jar, s))
}

/// Trade the refresh cookie for a new access token. Rotates the cookie.
#[utoipa::path(post, path = "/auth/refresh", tag = "auth",
    responses((status = 200, body = SessionResponse), (status = 401, body = ErrorBody)))]
pub async fn refresh(
    State(state): State<AppState>,
    jar: CookieJar,
    Client(client): Client,
) -> ApiResult<(CookieJar, Json<SessionResponse>)> {
    let token = jar.get(REFRESH_COOKIE).map(|c| c.value().to_string()).filter(|v| !v.is_empty()).ok_or(AppError::Unauthenticated)?;
    match auth::refresh(&state.ctx, &token, &client).await {
        Ok(s) => Ok(respond(&state, jar, s)),
        Err(e) => Err(e.into()),
    }
}

/// Revoke this session and clear the cookie.
#[utoipa::path(post, path = "/auth/logout", tag = "auth", responses((status = 204)))]
pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> ApiResult<(CookieJar, StatusCode)> {
    let token = jar.get(REFRESH_COOKIE).map(|c| c.value().to_string());
    auth::logout(&state.ctx, token.as_deref()).await?;
    Ok((jar.add(cleared_cookie(&state)), StatusCode::NO_CONTENT))
}

/// Send a new verification link to the signed-in user's email.
#[utoipa::path(post, path = "/auth/verify-email", tag = "auth", security(("bearer" = [])),
    responses((status = 202), (status = 401, body = ErrorBody), (status = 429, body = ErrorBody)))]
pub async fn request_verify_email(State(state): State<AppState>, Auth(actor): Auth) -> ApiResult<StatusCode> {
    auth::request_verify_email(&state.ctx, &actor).await?;
    Ok(StatusCode::ACCEPTED)
}

#[derive(Deserialize, ToSchema)]
pub struct TokenInput {
    pub token: String,
}

/// Confirm an email address with the token from the link.
#[utoipa::path(post, path = "/auth/verify-email/confirm", tag = "auth", request_body = TokenInput,
    responses((status = 200, body = UserView), (status = 422, body = ErrorBody)))]
pub async fn confirm_verify_email(State(state): State<AppState>, JsonBody(input): JsonBody<TokenInput>) -> ApiResult<Json<UserView>> {
    Ok(Json(auth::confirm_verify_email(&state.ctx, &input.token).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct ForgotInput {
    pub email: String,
}

/// Email a reset link. Always 202, whether or not the address has an account.
#[utoipa::path(post, path = "/auth/password/forgot", tag = "auth", request_body = ForgotInput,
    responses((status = 202), (status = 429, body = ErrorBody)))]
pub async fn forgot_password(
    State(state): State<AppState>,
    Client(client): Client,
    JsonBody(input): JsonBody<ForgotInput>,
) -> ApiResult<StatusCode> {
    auth::forgot_password(&state.ctx, &input.email, &client).await?;
    Ok(StatusCode::ACCEPTED)
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResetInput {
    pub token: String,
    pub password: String,
    /// The user saw the "breached password" warning and chose to keep it.
    #[serde(default)]
    pub accept_breached_password: bool,
}

/// Set a new password with the token from the link. Signs out every session.
#[utoipa::path(post, path = "/auth/password/reset", tag = "auth", request_body = ResetInput,
    responses((status = 204), (status = 422, body = ErrorBody)))]
pub async fn reset_password(State(state): State<AppState>, JsonBody(input): JsonBody<ResetInput>) -> ApiResult<StatusCode> {
    auth::reset_password(&state.ctx, &input.token, &input.password, input.accept_breached_password).await?;
    Ok(StatusCode::NO_CONTENT)
}
