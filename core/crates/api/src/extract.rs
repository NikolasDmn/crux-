//! Extractors: the signed-in user, `If-Match`, client info, service tokens, and JSON bodies whose
//! errors come out as `validation_failed`.

use axum::extract::{FromRef, FromRequest, FromRequestParts, Request};
use axum::http::header::{AUTHORIZATION, USER_AGENT};
use axum::http::request::Parts;
use crux_domain::ValidationErrors;
use crux_service::auth::{actor_from_token, ClientInfo};
use crux_service::{Actor, AppError};
use serde::de::DeserializeOwned;

use crate::error::ApiError;
use crate::reqlog::LogUser;
use crate::AppState;

pub fn bearer(parts: &Parts) -> Option<&str> {
    parts.headers.get(AUTHORIZATION)?.to_str().ok()?.strip_prefix("Bearer ").map(str::trim)
}

/// A signed-in user (access token in `Authorization: Bearer`).
pub struct Auth(pub Actor);

impl<S> FromRequestParts<S> for Auth
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let state = AppState::from_ref(state);
        let token = bearer(parts).ok_or(AppError::Unauthenticated)?;
        let actor = actor_from_token(&state.ctx, token).await?;
        if let Some(log) = parts.extensions.get::<LogUser>() {
            log.set(&actor.public_id);
        }
        Ok(Auth(actor))
    }
}

/// `If-Match: <version>` (spec 3.1). Quotes (ETag style) are accepted.
pub struct IfMatch(pub i32);

impl<S: Send + Sync> FromRequestParts<S> for IfMatch {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let raw = parts.headers.get("if-match").and_then(|v| v.to_str().ok()).map(|s| s.trim().trim_matches('"').to_string());
        raw.and_then(|s| s.parse().ok()).map(IfMatch).ok_or_else(|| {
            ApiError(ValidationErrors::single("If-Match", "required", "Send the resource's version in If-Match.").into())
        })
    }
}

/// Caller IP (first `X-Forwarded-For` hop, set by the gateway) and user agent.
pub struct Client(pub ClientInfo);

impl<S: Send + Sync> FromRequestParts<S> for Client {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let ip = parts
            .headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unknown".into());
        let user_agent = parts.headers.get(USER_AGENT).and_then(|v| v.to_str().ok()).map(String::from);
        Ok(Client(ClientInfo { ip, user_agent }))
    }
}

/// A service token for an internal route (aud = core; spec 3.2). Holds the calling service name.
pub struct Service(pub String);

impl<S> FromRequestParts<S> for Service
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let state = AppState::from_ref(state);
        let token = bearer(parts).ok_or(AppError::Unauthenticated)?;
        let claims = state.ctx.keys.verify_service(token, "core", state.ctx.clock.now().timestamp()).map_err(|_| AppError::Unauthenticated)?;
        Ok(Service(claims.sub))
    }
}

/// `axum::Json`, but a bad body is `422 validation_failed` in our error format.
pub struct JsonBody<T>(pub T);

impl<T, S> FromRequest<S> for JsonBody<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match axum::Json::<T>::from_request(req, state).await {
            Ok(axum::Json(v)) => Ok(JsonBody(v)),
            Err(rejection) => Err(ApiError(ValidationErrors::single("body", "invalid", &rejection.body_text()).into())),
        }
    }
}
